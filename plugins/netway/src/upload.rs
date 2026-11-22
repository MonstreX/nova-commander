//! A disk's files copied to the server (WinSCP's `CopyToRemote`, `TSFTPFileSystem::SFTPSource`): the
//! paths walked first (a folder with all its items, by name, what the transfer settings' file mask leaves
//! out passed by), each name changed as the settings say (`ChangeFileName`: the first level's masked, the
//! case, Windows' tokens read back); then the folders made on the server, the files written in pieces into
//! theirs (in the text mode CRLF as LF; no faster than the speed limit), their rights set and times kept
//! when asked (a failure passed by with "Ignore permission errors"); a moved file removed from the disk
//! after it, a moved folder after its items, else a source's "Archive" attribute cleared when asked.

use std::io::{Read as _, SeekFrom};
use std::path::{Path, PathBuf};

use russh_sftp::protocol::FileAttributes;

use crate::copy_params::PARTIAL_EXT;
use crate::copy_questions::Side;
use crate::copy_work::{
    Answers, Fault, Halted, Job, PIECE, Step, There, Write, fault, unix_seconds,
};
use crate::fs::FsFile;
use crate::sftp::child;
use crate::text_mode::{TextMode, Throttle};

type Upload = Step<PathBuf, String>;

/// A disk's item: a folder, or a file's size and last write; whether it is a link (a junction too).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Local {
    is_dir: bool,
    size: u64,
    modified: Option<u64>,
    link: bool,
}

impl Local {
    fn side(self) -> Side {
        Side {
            size: self.size,
            modified: self.modified,
        }
    }
}

/// The item at `path` as what it leads to (`meta`), and whether it is a link itself.
fn local_of(path: &Path, meta: &std::fs::Metadata) -> Local {
    Local {
        is_dir: meta.is_dir(),
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified: meta.modified().ok().and_then(unix_seconds),
        link: std::fs::symlink_metadata(path).is_ok_and(|own| own.file_type().is_symlink()),
    }
}

enum Visit {
    Item {
        source: PathBuf,
        target: String,
        local: Local,
        /// Inside a link to a folder.
        behind: bool,
    },
    Leave {
        source: PathBuf,
        behind: bool,
        link: bool,
    },
}

/// The name of a disk's item.
fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A folder's items by name.
fn local_listing(folder: &Path) -> Result<Vec<(PathBuf, Local)>, Fault> {
    let mut items = Vec::new();
    for item in std::fs::read_dir(folder).map_err(fault)? {
        let path = item.map_err(fault)?.path();
        let meta = std::fs::metadata(&path).map_err(fault)?;
        let local = local_of(&path, &meta);
        items.push((path, local));
    }
    items.sort_by(|(one, _), (other, _)| one.cmp(other));
    Ok(items)
}

/// A file sent: where to, how, what of it is kept.
struct Sending<'a> {
    target: &'a str,
    /// The temporary name it is written under first (WinSCP's resumable transfer).
    partial: Option<&'a str>,
    write: &'a Write,
    /// The source's bytes (SCP sends them first).
    size: u64,
    text: bool,
    /// The rights of the file it replaces through its temporary name.
    kept: Option<u32>,
}

/// WinSCP's `PRESERVE_TIME_PERM_ERROR3`.
fn attributes_error(target: &str) -> String {
    format!(
        "Upload of file '{target}' was successful, but error occurred while setting the permissions \
         and/or timestamp.\n\nIf the problem persists, turn off setting permissions or preserving \
         timestamp. Alternatively you can turn on 'Ignore permission errors' option."
    )
}

impl Job<'_> {
    /// The disk's items `sources` copied into the server's folder `target`.
    pub async fn upload(&self, sources: &[PathBuf], target: &str) -> Result<(), Halted> {
        let mut answers = Answers::default();
        let steps = self.local_steps(&mut answers, sources, target).await?;
        self.walked(&steps);
        for step in steps {
            self.upload_step(&mut answers, step).await?;
        }
        Ok(())
    }

    /// The first level's items the file mask takes, last first for the walk's stack.
    async fn first_sources(
        &self,
        answers: &mut Answers,
        sources: &[PathBuf],
        target: &str,
    ) -> Result<Vec<Visit>, Halted> {
        let mut stack = Vec::new();
        for source in sources.iter().rev() {
            let shown = source.display().to_string();
            let read = async || std::fs::metadata(source).map_err(fault);
            let Some(meta) = self.retrying(answers, &shown, read).await? else {
                continue;
            };
            let local = local_of(source, &meta);
            if !self.allowed(&shown, true, local.is_dir, local.side()) {
                continue;
            }
            let name = self.params.change_file_name(&name_of(source), false, true);
            stack.push(Visit::Item {
                source: source.clone(),
                target: child(target, &name),
                local,
                behind: false,
            });
        }
        Ok(stack)
    }

    /// The steps of the items given, a folder's items after it.
    async fn local_steps(
        &self,
        answers: &mut Answers,
        sources: &[PathBuf],
        target: &str,
    ) -> Result<Vec<Upload>, Halted> {
        let mut stack = self.first_sources(answers, sources, target).await?;
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
                    local,
                    behind,
                } if local.is_dir => {
                    let shown = source.display().to_string();
                    self.progress.walking(&shown);
                    let read = async || local_listing(&source);
                    let Some(items) = self.retrying(answers, &shown, read).await? else {
                        continue;
                    };
                    stack.push(Visit::Leave {
                        source: source.clone(),
                        behind,
                        link: local.link,
                    });
                    let inside = behind || local.link;
                    for (path, local) in items.into_iter().rev() {
                        let shown = path.display().to_string();
                        if self.allowed(&shown, true, local.is_dir, local.side()) {
                            let name = self.params.change_file_name(&name_of(&path), false, false);
                            stack.push(Visit::Item {
                                source: path,
                                target: child(&target, &name),
                                local,
                                behind: inside,
                            });
                        }
                    }
                    steps.push(Step::Folder { source, target });
                }
                Visit::Item {
                    source,
                    target,
                    local,
                    behind,
                } => steps.push(Step::File {
                    source,
                    target,
                    size: local.size,
                    modified: local.modified,
                    rights: None,
                    behind,
                }),
            }
        }
        Ok(steps)
    }

    async fn upload_step(&self, answers: &mut Answers, step: Upload) -> Result<(), Halted> {
        match step {
            Step::Folder { source, target } => {
                let make = async || self.remote_folder(&target).await;
                self.retrying(answers, &source.display().to_string(), make)
                    .await?;
            }
            Step::File {
                source,
                target,
                size,
                modified,
                behind,
                ..
            } => {
                let new = Side { size, modified };
                self.upload_file(answers, &source, target, (new, behind))
                    .await?;
                self.progress.file_done();
            }
            // Nothing inside a link is removed by a move.
            Step::Leave { behind: true, .. } if self.moving => {}
            Step::Leave { source, link, .. } if self.moving => {
                // A folder with items left (skipped) stays; a link goes as a link (a junction or a
                // folder's link by `remove_dir`, a Unix one by `remove_file`).
                if std::fs::remove_dir(&source).is_err() && link {
                    let _ = std::fs::remove_file(&source);
                }
            }
            Step::Leave { source, .. } => self.clear_archive(&source),
        }
        Ok(())
    }

    /// WinSCP's `ClearArchive` of a source sent and not moved (silently, as WinSCP's folders).
    fn clear_archive(&self, source: &Path) {
        if self.params.clear_archive && !self.temporary {
            let _ = nc_plugin_sdk::system::clear_archive(source);
        }
    }

    /// The server's folder `target` made, unless it is there, with its rights when asked (WinSCP's
    /// `CreateDirectory` with the properties; a folder there is left as it is).
    async fn remote_folder(&self, target: &str) -> Result<(), Fault> {
        if (self.fs.metadata(target).await).is_ok_and(|meta| meta.is_dir()) {
            return Ok(());
        }
        self.fs.create_dir(target).await.map_err(fault)?;
        if self.params.preserve_rights {
            let attributes = FileAttributes {
                permissions: Some(u32::from(self.params.remote_rights(true))),
                ..FileAttributes::empty()
            };
            self.set_attributes(target, attributes).await?;
        }
        Ok(())
    }

    /// How the file goes over the server's file there (asked), or written anew when none is.
    async fn remote_write(
        &self,
        answers: &mut Answers,
        (shown, target): (&str, &str),
        (new, text): (Side, bool),
    ) -> Result<Write, Halted> {
        let meta = match self.fs.metadata(target).await {
            Ok(meta) if !meta.is_dir() => meta,
            _ => return Ok(Write::Overwrite),
        };
        let existing = Side {
            size: meta.size.unwrap_or(0),
            modified: meta.mtime.map(u64::from),
        };
        let name = target.rsplit('/').next().unwrap_or_default().to_string();
        let there = There {
            name: &name,
            source: shown,
            local: false,
            new,
            existing,
            text,
        };
        self.decide(answers, &there).await
    }

    /// The file `source` to `target`: asked over when one is there, then written, then (moved) removed
    /// or its archive attribute cleared.
    async fn upload_file(
        &self,
        answers: &mut Answers,
        source: &Path,
        mut target: String,
        (new, behind): (Side, bool),
    ) -> Result<(), Halted> {
        let shown = source.display().to_string();
        let folder = target
            .rsplit_once('/')
            .map_or("/", |(folder, _)| folder)
            .to_string();
        self.progress.file(&shown, &folder, new.size);
        let text = self.text_for(&shown, true, new);
        let mut there = self.fs.symlink_metadata(target.as_str()).await.ok();
        // WinSCP's: never resumable over a link (it would be deleted).
        let link = there.as_ref().is_some_and(|meta| meta.is_symlink());
        let mut name = target.rsplit('/').next().unwrap_or_default().to_string();
        let resumable = !self.temporary
            && !text
            && !link
            && self.fs.can(crate::fs::Capability::Resume)
            && self.params.allow_resume(new.size, &name);
        let resumed = match resumable {
            true => self.remote_partial(&target, &name, new.size).await?,
            false => None,
        };
        let write = match resumed {
            Some(offset) => Write::Resume(offset),
            None => {
                self.remote_write(answers, (&shown, &target), (new, text))
                    .await?
            }
        };
        let write = match write {
            Write::Skip => {
                self.progress.add(new.size);
                return Ok(());
            }
            Write::Renamed(renamed) => {
                target = child(&folder, &renamed);
                // What is under the new name (if anything) is what gets replaced, binned, its rights kept.
                there = self.fs.symlink_metadata(target.as_str()).await.ok();
                name = renamed;
                Write::Overwrite
            }
            other => other,
        };
        // WinSCP's `OverwrittenToRecycleBin` (SFTP): the file replaced goes into the bin first.
        if let Some(bin) = self.fs.recycle_bin(true)
            && write == Write::Overwrite
            && there.is_some()
        {
            let recycled =
                crate::remote_ops::target_of(&name, bin, &crate::remote_ops::recycle_mask());
            let recycle = async || self.fs.rename(&target, &recycled).await.map_err(fault);
            self.retrying(answers, &shown, recycle).await?;
        }
        let partial = (resumable && (resumed.is_some() || write == Write::Overwrite))
            .then(|| format!("{target}{PARTIAL_EXT}"));
        // WinSCP's `PreserveExistingRights`: a file replaced through its temporary name keeps its rights.
        let kept = there
            .and_then(|meta| meta.permissions)
            .filter(|_| partial.is_some());
        let sending = Sending {
            target: &target,
            partial: partial.as_deref(),
            write: &write,
            size: new.size,
            text,
            kept,
        };
        let send = async || self.send(source, &sending).await;
        let mut sent = self.retrying(answers, &shown, send).await?;
        if sent.is_some() {
            // Retried on its own: the file is in place already.
            let attributes = async || (self.sent_attributes(&target, new.modified)).await;
            sent = self.retrying(answers, &shown, attributes).await?;
        }
        // A file inside a link to a folder is copied, never removed.
        match (sent.is_some(), self.moving && !behind) {
            (true, true) => {
                let remove = async || std::fs::remove_file(source).map_err(fault);
                self.retrying(answers, &shown, remove).await?;
            }
            (true, false) => self.clear_archive(source),
            _ => {}
        }
        Ok(())
    }

    /// The file written to `target` as `write` says (its rights and time set after, on their own); one
    /// being written anew removed when the user stops it.
    async fn send(&self, source: &Path, sending: &Sending<'_>) -> Result<(), Fault> {
        let (target, write) = (sending.target, sending.write);
        // Written under the temporary name when resumable (kept when stopped, to be resumed).
        let written = sending.partial.unwrap_or(target);
        self.progress.restart_file();
        let mut local = std::fs::File::open(source).map_err(fault)?;
        let rights = match self.params.preserve_rights {
            true => self.params.remote_rights(false),
            false => crate::copy_params::DEFAULT_RIGHTS,
        };
        let mut remote = match write {
            Write::Append(_) | Write::Resume(_) => {
                (self.fs.open_for_writing(written)).await.map_err(fault)?
            }
            _ => (self.fs.create(written, sending.size, rights))
                .await
                .map_err(fault)?,
        };
        if let Write::Append(offset) | Write::Resume(offset) = write {
            remote.seek(SeekFrom::Start(*offset)).await.map_err(fault)?;
        }
        if let Write::Resume(offset) = write {
            std::io::Seek::seek(&mut local, SeekFrom::Start(*offset)).map_err(fault)?;
            self.progress.add(*offset);
        }
        let text = (sending.text && !self.fs.crlf()).then(TextMode::upload);
        let copied = self.pieces_up(&mut local, &mut remote, text).await;
        let closed = remote.shutdown().await.map_err(fault);
        if copied == Err(Fault::Stopped) && *write == Write::Overwrite && sending.partial.is_none()
        {
            let _ = self.fs.remove_file(target).await;
        }
        copied?;
        closed?;
        if let Some(partial) = sending.partial {
            self.put_remote_in_place(partial, target, sending.kept)
                .await?;
        }
        Ok(())
    }

    /// The file written under its temporary name put in place of `target`, with the `kept` rights of the
    /// file it replaces unless the settings set rights.
    async fn put_remote_in_place(
        &self,
        partial: &str,
        target: &str,
        kept: Option<u32>,
    ) -> Result<(), Fault> {
        // SFTP 3 renames over nothing: the file there goes first.
        let _ = self.fs.remove_file(target).await;
        self.fs.rename(partial, target).await.map_err(fault)?;
        let Some(rights) = kept.filter(|_| !self.params.preserve_rights) else {
            return Ok(());
        };
        let attributes = FileAttributes {
            permissions: Some(rights & 0o7777),
            ..FileAttributes::empty()
        };
        self.set_attributes(target, attributes).await
    }

    /// WinSCP's check of a partially transferred file of the target from an earlier transfer: the
    /// offset to resume from, or none (deleted when there was one).
    async fn remote_partial(
        &self,
        target: &str,
        name: &str,
        size: u64,
    ) -> Result<Option<u64>, Halted> {
        let partial = format!("{target}{PARTIAL_EXT}");
        let found = (self.fs.metadata(partial.as_str()).await).ok();
        let Some(meta) = found.filter(|meta| !meta.is_dir()) else {
            return Ok(None);
        };
        let resumed = (self.resume_partial(name, meta.size.unwrap_or(0), size)).await?;
        if resumed.is_none() {
            let _ = self.fs.remove_file(partial).await;
        }
        Ok(resumed)
    }

    /// WinSCP's attributes of a file sent: the rights and the time as the settings ask.
    async fn sent_attributes(&self, target: &str, modified: Option<u64>) -> Result<(), Fault> {
        let seconds = modified
            .filter(|_| self.params.preserve_time)
            .and_then(|seconds| u32::try_from(seconds).ok());
        let rights =
            (self.params.preserve_rights).then(|| u32::from(self.params.remote_rights(false)));
        if seconds.is_none() && rights.is_none() {
            return Ok(());
        }
        let attributes = FileAttributes {
            mtime: seconds,
            atime: seconds,
            permissions: rights,
            ..FileAttributes::empty()
        };
        self.set_attributes(target, attributes).await
    }

    /// The attributes set; a failure passed by with "Ignore permission errors".
    async fn set_attributes(&self, target: &str, attributes: FileAttributes) -> Result<(), Fault> {
        match self.fs.set_metadata(target, attributes).await {
            Ok(()) => Ok(()),
            Err(_) if self.params.ignore_perm_errors => Ok(()),
            Err(_) => Err(Fault::Failed(attributes_error(target))),
        }
    }

    async fn pieces_up(
        &self,
        local: &mut std::fs::File,
        remote: &mut FsFile,
        mut text: Option<TextMode>,
    ) -> Result<(), Fault> {
        let mut buffer = vec![0; PIECE];
        let mut throttle = Throttle::new(self.params.speed_limit);
        loop {
            self.check_stop()?;
            let read = local.read(&mut buffer).map_err(fault)?;
            if read == 0 {
                if let Some(held) = text.as_mut().map(TextMode::finish) {
                    remote.write_all(&held).await.map_err(fault)?;
                }
                return Ok(());
            }
            match text.as_mut() {
                Some(text) => remote.write_all(&text.convert(&buffer[..read])).await,
                None => remote.write_all(&buffer[..read]).await,
            }
            .map_err(fault)?;
            self.progress.add(read as u64);
            self.throttle(&mut throttle, read as u64).await;
        }
    }
}
