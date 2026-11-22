//! Files of the server copied to a disk (WinSCP's `CopyToLocal`, `TSFTPFileSystem::SFTPSink`): the items
//! named walked first (a link as what it points to, a folder with all its items, what the transfer
//! settings' file mask leaves out passed by), each name changed as the settings say (`ChangeFileName`:
//! the first level's masked, the case, Windows' invalid characters); then the folders made, the files read
//! in pieces into theirs (in the text mode LF as CRLF; no faster than the speed limit), their times kept,
//! a file without the owner's write right read only when asked; a moved file removed from the server after
//! it, a moved folder after its items.

use std::io::{Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};

use crate::copy_params::PARTIAL_EXT;
use crate::copy_questions::Side;
use crate::copy_work::{
    Answers, Fault, Halted, Job, PIECE, Step, There, Write, fault, system_time, unix_seconds,
};
use crate::fs::FsFile;
use crate::sftp::{RemoteEntry, child};
use crate::text_mode::{TextMode, Throttle};

type Download = Step<String, PathBuf>;

/// An item met in the walk, or the end of a folder's items.
enum Visit {
    Item {
        source: String,
        target: PathBuf,
        entry: RemoteEntry,
        /// Inside a link to a folder.
        behind: bool,
    },
    Leave {
        source: String,
        behind: bool,
        link: bool,
    },
}

/// A disk's file's temporary name while it is written resumably: `.filepart` after its name.
pub fn partial_of(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_owned();
    name.push(PARTIAL_EXT);
    PathBuf::from(name)
}

fn file_name(path: &Path) -> String {
    (path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What an entry tells the masks.
fn side_of(entry: &RemoteEntry) -> Side {
    Side {
        size: entry.size,
        modified: entry.modified.map(u64::from),
    }
}

/// A file received: where from and to, how, what of it is kept.
struct Receiving<'a> {
    source: &'a str,
    target: &'a Path,
    /// The temporary name it is written under first (WinSCP's resumable transfer).
    partial: Option<&'a Path>,
    write: &'a Write,
    modified: Option<u64>,
    text: bool,
    read_only: bool,
}

impl Job<'_> {
    /// The items `names` of the server's folder `folder` copied into the disk's folder `target`.
    pub async fn download(
        &self,
        folder: &str,
        names: &[String],
        target: &Path,
    ) -> Result<(), Halted> {
        let mut answers = Answers::default();
        let steps = self
            .remote_steps(&mut answers, folder, names, target)
            .await?;
        self.walked(&steps);
        for step in steps {
            self.download_step(&mut answers, step).await?;
        }
        Ok(())
    }

    /// The first level's items the file mask takes, last first for the walk's stack.
    async fn first_level(
        &self,
        answers: &mut Answers,
        folder: &str,
        names: &[String],
        target: &Path,
    ) -> Result<Vec<Visit>, Halted> {
        let mut stack = Vec::new();
        for name in names.iter().rev() {
            let source = child(folder, name);
            let Some(entry) = self.remote_entry(answers, &source, name).await? else {
                continue;
            };
            if !self.allowed(&source, false, entry.is_dir, side_of(&entry)) {
                continue;
            }
            let target = target.join(self.params.change_file_name(name, true, true));
            stack.push(Visit::Item {
                source,
                target,
                entry,
                behind: false,
            });
        }
        Ok(stack)
    }

    /// The steps of the items named, a folder's items after it.
    async fn remote_steps(
        &self,
        answers: &mut Answers,
        folder: &str,
        names: &[String],
        target: &Path,
    ) -> Result<Vec<Download>, Halted> {
        let mut stack = self.first_level(answers, folder, names, target).await?;
        let mut steps = Vec::new();
        while let Some(visit) = stack.pop() {
            self.halt_if_stopped()?;
            match visit {
                Visit::Leave {
                    source,
                    behind,
                    link,
                } => steps.push(Step::Leave {
                    source,
                    behind,
                    link,
                }),
                Visit::Item {
                    source,
                    target,
                    entry,
                    behind,
                } if entry.is_dir => {
                    self.progress.walking(&source);
                    let Some(items) = self.remote_listing(answers, &source).await? else {
                        continue;
                    };
                    stack.push(Visit::Leave {
                        source: source.clone(),
                        behind,
                        link: entry.is_link,
                    });
                    let inside = behind || entry.is_link;
                    for item in items.into_iter().rev() {
                        let path = child(&source, &item.name);
                        if self.allowed(&path, false, item.is_dir, side_of(&item)) {
                            let name = self.params.change_file_name(&item.name, true, false);
                            stack.push(Visit::Item {
                                source: path,
                                target: target.join(name),
                                entry: item,
                                behind: inside,
                            });
                        }
                    }
                    steps.push(Step::Folder { source, target });
                }
                Visit::Item {
                    source,
                    target,
                    entry,
                    behind,
                } => steps.push(Step::File {
                    source,
                    target,
                    size: entry.size,
                    modified: entry.modified.map(u64::from),
                    rights: entry.permissions,
                    behind,
                }),
            }
        }
        Ok(steps)
    }

    /// The item `source` as what it is (a link as what it points to, and that it is one); none when
    /// skipped after a failure.
    async fn remote_entry(
        &self,
        answers: &mut Answers,
        source: &str,
        name: &str,
    ) -> Result<Option<RemoteEntry>, Halted> {
        let sftp = self.fs;
        let read = async || sftp.metadata(source).await.map_err(fault);
        let Some(meta) = self.retrying(answers, source, read).await? else {
            return Ok(None);
        };
        let link = (sftp.symlink_metadata(source).await).is_ok_and(|own| own.is_symlink());
        Ok(Some(RemoteEntry {
            name: name.to_string(),
            size: meta.size.unwrap_or(0),
            is_dir: meta.is_dir(),
            is_link: link,
            modified: meta.mtime,
            permissions: meta.permissions,
            ..RemoteEntry::default()
        }))
    }

    /// The folder's items; none when skipped after a failure.
    async fn remote_listing(
        &self,
        answers: &mut Answers,
        source: &str,
    ) -> Result<Option<Vec<RemoteEntry>>, Halted> {
        let sftp = self.fs;
        let read = async || sftp.list(source).await.map_err(Fault::Failed);
        self.retrying(answers, source, read).await
    }

    async fn download_step(&self, answers: &mut Answers, step: Download) -> Result<(), Halted> {
        match step {
            Step::Folder { source, target } => {
                let make = async || std::fs::create_dir_all(&target).map_err(fault);
                self.retrying(answers, &source, make).await?;
            }
            Step::File {
                source,
                target,
                size,
                modified,
                rights,
                behind,
            } => {
                let new = Side { size, modified };
                // WinSCP's `LocalFileAttrs`: no write right of the owner, read only.
                let read_only = self.params.preserve_read_only
                    && rights.is_some_and(|rights| rights & 0o200 == 0);
                self.download_file(answers, &source, target, (new, read_only, behind))
                    .await?;
                self.progress.file_done();
            }
            // Nothing inside a link is removed by a move; a link goes as a link.
            Step::Leave { behind: true, .. } => {}
            Step::Leave { source, link, .. } if self.moving => {
                // A folder with items left (skipped) stays.
                let _ = match link {
                    true => self.fs.remove_file(source).await,
                    false => self.fs.remove_dir(source).await,
                };
            }
            Step::Leave { .. } => {}
        }
        Ok(())
    }

    /// How the file goes over the target there (asked), or written anew when none is.
    async fn local_write(
        &self,
        answers: &mut Answers,
        source: &str,
        target: &Path,
        (new, text): (Side, bool),
    ) -> Result<Write, Halted> {
        let meta = match std::fs::metadata(target) {
            Ok(meta) if meta.is_file() => meta,
            _ => return Ok(Write::Overwrite),
        };
        let existing = Side {
            size: meta.len(),
            modified: meta.modified().ok().and_then(unix_seconds),
        };
        let name = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let there = There {
            name: &name.unwrap_or_default(),
            source,
            local: true,
            new,
            existing,
            text,
        };
        self.decide(answers, &there).await
    }

    /// The file `source` into `target`: asked over when one is there, then read, then (moved) removed.
    async fn download_file(
        &self,
        answers: &mut Answers,
        source: &str,
        mut target: PathBuf,
        (new, read_only, behind): (Side, bool, bool),
    ) -> Result<(), Halted> {
        let folder = target.parent().map(|folder| folder.display().to_string());
        self.progress
            .file(source, &folder.unwrap_or_default(), new.size);
        let text = self.text_for(source, false, new);
        let name = file_name(&target);
        let resumable = !self.temporary
            && !text
            && self.fs.can(crate::fs::Capability::Resume)
            && self.params.allow_resume(new.size, &name);
        let resumed = match resumable {
            true => self.local_partial(&target, &name, new.size).await?,
            false => None,
        };
        let write = match resumed {
            Some(offset) => Write::Resume(offset),
            None => {
                self.local_write(answers, source, &target, (new, text))
                    .await?
            }
        };
        let write = match write {
            Write::Skip => {
                self.progress.add(new.size);
                return Ok(());
            }
            Write::Renamed(name) => {
                target.set_file_name(name);
                Write::Overwrite
            }
            other => other,
        };
        // WinSCP's resumable transfer: written under the temporary name, then put in place.
        let partial = (resumable && (resumed.is_some() || write == Write::Overwrite))
            .then(|| partial_of(&target));
        let receiving = Receiving {
            source,
            target: &target,
            partial: partial.as_deref(),
            write: &write,
            modified: new.modified,
            text,
            read_only,
        };
        let receive = async || self.receive(&receiving).await;
        let mut received = self.retrying(answers, source, receive).await?;
        if received.is_some() {
            let finish = async || self.finish(&receiving);
            received = self.retrying(answers, source, finish).await?;
        }
        // A file inside a link to a folder is copied, never removed.
        if received.is_some() && self.moving && !behind {
            let sftp = self.fs;
            let remove = async || sftp.remove_file(source).await.map_err(fault);
            self.retrying(answers, source, remove).await?;
        }
        Ok(())
    }

    /// WinSCP's check of a partially transferred file of the target `name` from an earlier transfer: the
    /// offset to resume from, or none (deleted when there was one).
    async fn local_partial(
        &self,
        target: &Path,
        name: &str,
        size: u64,
    ) -> Result<Option<u64>, Halted> {
        let partial = partial_of(target);
        let found = std::fs::metadata(&partial)
            .ok()
            .filter(std::fs::Metadata::is_file);
        let Some(meta) = found else {
            return Ok(None);
        };
        let resumed = self.resume_partial(name, meta.len(), size).await?;
        if resumed.is_none() {
            let _ = std::fs::remove_file(&partial);
        }
        Ok(resumed)
    }

    /// The file written under its temporary name put in its place (one there replaced).
    fn put_in_place(&self, written: &Path, target: &Path) -> Result<(), Fault> {
        std::fs::rename(written, target).map_err(fault)
    }

    /// The file read into its target as its write says, its time kept, made read only when asked; one
    /// being written anew removed when the user stops it.
    async fn receive(&self, receiving: &Receiving<'_>) -> Result<(), Fault> {
        let (target, write) = (receiving.target, receiving.write);
        // Written under the temporary name when resumable (kept when stopped, to be resumed).
        let written = receiving.partial.unwrap_or(target);
        self.progress.restart_file();
        let mut remote = self.fs.open(receiving.source).await.map_err(fault)?;
        let mut local = match write {
            // Cut back to where it stood first: a retried attempt writes its part again (WinSCP reads
            // the partial file's size anew each time).
            Write::Append(kept) | Write::Resume(kept) => {
                let mut local = (std::fs::OpenOptions::new().write(true))
                    .open(written)
                    .map_err(fault)?;
                local.set_len(*kept).map_err(fault)?;
                local.seek(SeekFrom::End(0)).map_err(fault)?;
                local
            }
            _ => (std::fs::OpenOptions::new().write(true).create(true))
                .truncate(true)
                .open(written)
                .map_err(fault)?,
        };
        if let Write::Resume(offset) = write {
            remote.seek(SeekFrom::Start(*offset)).await.map_err(fault)?;
            self.progress.add(*offset);
        }
        // A server whose lines end with CR/LF (the session's Environment) needs no conversion.
        let text = (receiving.text && !self.fs.crlf()).then(TextMode::download);
        let copied = self.pieces_down(&mut remote, &mut local, text).await;
        let partial = receiving.partial.is_some();
        if copied == Err(Fault::Stopped) && *write == Write::Overwrite && !partial {
            drop(local);
            let _ = std::fs::remove_file(target);
            return copied;
        }
        copied?;
        drop(local);
        if partial {
            self.put_in_place(written, target)?;
        }
        Ok(())
    }

    /// The received file's time kept, made read only when asked: retried on its own, the file being in
    /// place already.
    fn finish(&self, receiving: &Receiving<'_>) -> Result<(), Fault> {
        let target = receiving.target;
        if let (true, Some(seconds)) = (self.params.preserve_time, receiving.modified) {
            let time = system_time(seconds);
            let times = std::fs::FileTimes::new()
                .set_modified(time)
                .set_accessed(time);
            let local = (std::fs::OpenOptions::new().write(true))
                .open(target)
                .map_err(fault)?;
            local.set_times(times).map_err(fault)?;
        }
        if receiving.read_only {
            let mut permissions = std::fs::metadata(target).map_err(fault)?.permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(target, permissions).map_err(fault)?;
        }
        Ok(())
    }

    async fn pieces_down(
        &self,
        remote: &mut FsFile,
        local: &mut std::fs::File,
        mut text: Option<TextMode>,
    ) -> Result<(), Fault> {
        let mut buffer = vec![0; PIECE];
        let mut throttle = Throttle::new(self.params.speed_limit);
        loop {
            self.check_stop()?;
            let read = remote.read(&mut buffer).await.map_err(fault)?;
            if read == 0 {
                return Ok(());
            }
            match text.as_mut() {
                Some(text) => local.write_all(&text.convert(&buffer[..read])),
                None => local.write_all(&buffer[..read]),
            }
            .map_err(fault)?;
            self.progress.add(read as u64);
            self.throttle(&mut throttle, read as u64).await;
        }
    }
}

#[cfg(test)]
#[path = "copy_transfer_tests.rs"]
mod tests;
