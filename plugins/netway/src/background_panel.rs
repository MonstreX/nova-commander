//! The program's file calls on a connected panel (API 9, S6.8b): its background tasks (a copy handed to
//! it, ADR-0023 addendum S6.8a) read, write, make, rename and remove through a connection of the panel's
//! own (WinSCP's queue opens its own too), opened at the first call with the user and password that
//! logged in - questions refused, as the panel's silent reopening does - so the panel stays free to
//! browse; its open files kept between the calls ([`Files`]). A connection the server dropped is opened
//! again at the next call (as the panel's, S6.6k); Esc stops the plugin's wait (its task's stop).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nc_plugin_sdk::{Error, ErrorKind, FileEntry, PutRequest, Times, Transfer, WriteMode};
use russh_sftp::protocol::FileAttributes;

use crate::NetWay;
use crate::ask::Asker;
use crate::background_files::{self, Files};
use crate::fs::Fs;
use crate::login;
use crate::reopen::FTP_IDLE;

/// A panel's connection for the program's tasks.
pub struct Background {
    /// The SSH connection behind `fs` (none for FTP and HTTP).
    connection: Option<crate::ssh::Connection>,
    pub fs: Fs,
    pub files: Files,
    /// When its last call ended.
    last: Instant,
    /// Its last call failed: whether it still answers is asked before the next.
    suspect: bool,
}

impl Background {
    /// Whether it may serve the next call: its SSH connection open; an FTP one asked whether it still
    /// answers after a pause of the calls or a failure - never while a file is written (its transfer
    /// goes on), a read left that long let go first (opened again where it is wanted).
    async fn usable(&mut self) -> bool {
        if (self.connection.as_ref()).is_some_and(crate::ssh::Connection::is_closed) {
            return false;
        }
        let quiet = self.last.elapsed() >= FTP_IDLE;
        if !(quiet || self.suspect) || self.files.writes() {
            return true;
        }
        self.files.let_go_reading();
        self.fs.alive(Duration::ZERO).await
    }
}

fn failed(error: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::Other, error.to_string())
}

/// The runtime's wait stopped: the task's own stop.
fn stopped(_: crate::runtime::Stopped) -> Error {
    Error::new(ErrorKind::Cancelled, "Cancelled")
}

impl NetWay {
    /// The panel `instance`'s connection for the program's tasks opened at its first call (the sessions'
    /// panel has none: its files are the plugin's own copies).
    fn open_background(&mut self, instance: &str) -> Result<(), Error> {
        let runtime = (self.runtime.as_ref()).ok_or_else(|| failed("No runtime"))?;
        let host = self.host;
        let remote = (self.remotes.get_mut(instance))
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "Not a connected panel"))?;
        if let Some(background) = remote.background.as_mut() {
            let usable = runtime.wait(background.usable(), &|| host.cancelled());
            if usable.map_err(stopped)? {
                return Ok(());
            }
            // Dropped by the server: opened again below, what it held let go.
            remote.background = None;
        }
        let plan = remote.plan.clone();
        // Questions go nowhere: answered as refused (the panel's login answered them already).
        let (asker, questions) = Asker::new();
        drop(questions);
        let work = async { login::start(&plan, &asker).await };
        let started = runtime.wait(work, &|| host.cancelled()).map_err(stopped)?;
        let started = started.map_err(|failure| failed(format!("{failure:?}")))?;
        remote.background = Some(Background {
            connection: started.connection,
            fs: started.fs,
            files: Files::default(),
            last: Instant::now(),
            suspect: false,
        });
        Ok(())
    }

    /// `work` on the panel's background connection, waited for on the plugin's thread until the
    /// task's stop.
    fn on_background<T>(
        &mut self,
        instance: &str,
        work: impl AsyncFnOnce(&Fs, &mut Files) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.open_background(instance)?;
        let runtime = (self.runtime.as_ref()).ok_or_else(|| failed("No runtime"))?;
        let host = self.host;
        let background = (self.remotes.get_mut(instance))
            .and_then(|remote| remote.background.as_mut())
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "Not a connected panel"))?;
        let Background { fs, files, .. } = background;
        let done = runtime.wait(work(fs, files), &|| host.cancelled());
        let Ok(done) = done else {
            // Stopped halfway: what the server still sends for it is nobody's - opened anew next time.
            if let Some(remote) = self.remotes.get_mut(instance) {
                remote.background = None;
            }
            return Err(Error::new(ErrorKind::Cancelled, "Cancelled"));
        };
        background.last = Instant::now();
        background.suspect = (done.as_ref()).is_err_and(|error| error.kind == ErrorKind::Other);
        done
    }

    /// API 9's `WRITE_ABORT`: what is kept for `path` let go (a spool removed), nothing sent.
    pub(crate) fn file_abort(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        if let Some(background) =
            (self.remotes.get_mut(instance)).and_then(|it| it.background.as_mut())
        {
            background.files.abort(path);
        }
        Ok(())
    }

    /// NetBox's "Transfer on background" (S6.8c): the entries `names` of the server's `folder` copied
    /// (moved) into the disk's folder `disk`, or (`upload`) the disk's entries into `folder`, handed to
    /// the program's background tasks.
    pub(crate) fn handed_over(
        &self,
        instance: &str,
        (folder, disk): (&str, &str),
        names: &[String],
        (upload, moving): (bool, bool),
    ) -> Result<(), Error> {
        let transfer = Transfer {
            instance: instance.to_string(),
            folder: folder.to_string(),
            names: names.to_vec(),
            disk: disk.to_string(),
            upload,
            moving,
        };
        match self.host.background_transfer(&transfer) {
            true => Ok(()),
            false => Err(failed("The copy could not be handed to the program")),
        }
    }

    /// An upload handed over as one task (one at a time is started): the disk's sources named from the
    /// folder that holds them all (a panel's selection: its folder); sources on several disks refused.
    pub(crate) fn handed_up(
        &self,
        instance: &str,
        target: &str,
        request: &PutRequest,
    ) -> Result<(), Error> {
        let Some((disk, names)) = common_folder(&request.sources) else {
            return Err(failed("The files to copy are on more than one disk"));
        };
        let disk = disk.display().to_string();
        self.handed_over(instance, (target, &disk), &names, (true, request.moving))
    }

    /// One entry, through the panel's own connection (opened again when the server dropped it): the
    /// quick view asks every second, and that is no reason to open another.
    pub(crate) fn file_stat(&mut self, instance: &str, path: &str) -> Result<FileEntry, Error> {
        let folder = (self.remotes.get(instance)).map(|remote| remote.folder.clone());
        let Some(folder) = folder else {
            return Err(Error::new(ErrorKind::Unsupported, "Not a connected panel"));
        };
        if !self.reopen_silently(instance, &folder) {
            return Err(failed(crate::reopen::LOST));
        }
        let (Some(remote), Some(runtime)) = (self.remotes.get(instance), self.runtime.as_ref())
        else {
            return Err(Error::new(ErrorKind::Unsupported, "Not a connected panel"));
        };
        let host = self.host;
        let work = background_files::stat(&remote.fs, path);
        let entry = runtime
            .wait(work, &|| host.cancelled())
            .map_err(stopped)??;
        Ok(crate::remote::entry(&entry))
    }

    pub(crate) fn file_read(
        &mut self,
        instance: &str,
        path: &str,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, Error> {
        self.on_background(instance, async |fs, files| {
            files.read(fs, path, offset, buffer).await
        })
    }

    pub(crate) fn file_write(
        &mut self,
        instance: &str,
        path: &str,
        mode: WriteMode,
        data: &[u8],
    ) -> Result<(), Error> {
        self.on_background(instance, async |fs, files| {
            files.write(fs, path, mode, data).await
        })
    }

    pub(crate) fn file_end(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        self.on_background(instance, async |fs, files| files.end(fs, path).await)
    }

    pub(crate) fn file_make_dir(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        self.on_background(instance, async |fs, _| {
            background_files::absent(fs, path).await?;
            fs.create_dir(path).await.map_err(failed)
        })
    }

    pub(crate) fn file_rename(
        &mut self,
        instance: &str,
        (from, to): (&str, &str),
        replace: bool,
    ) -> Result<(), Error> {
        self.on_background(instance, async |fs, _| {
            if let Ok(there) = fs.metadata(to).await {
                if !replace || there.is_dir() {
                    return Err(Error::new(ErrorKind::Exists, format!("{to} is there")));
                }
                // SFTP 3 renames over nothing: the file there goes first.
                fs.remove_file(to).await.map_err(failed)?;
            }
            fs.rename(from, to).await.map_err(failed)
        })
    }

    pub(crate) fn file_remove(
        &mut self,
        instance: &str,
        path: &str,
        folder: bool,
    ) -> Result<(), Error> {
        self.on_background(instance, async |fs, files| {
            // Whatever is still open for it goes first (never sent after it was removed).
            files.abort(path);
            // A link to a folder goes as the link it is (a server's `rmdir` refuses it; the program
            // removes a folder's link as a folder, R7 review C1).
            let link =
                folder && (fs.symlink_metadata(path).await).is_ok_and(|own| own.is_symlink());
            let removed = match folder && !link {
                true => fs.remove_dir(path).await,
                false => fs.remove_file(path).await,
            };
            removed.map_err(failed)
        })
    }

    /// The modification time kept where the protocol keeps one (WebDAV and S3: none, as WinSCP).
    pub(crate) fn file_set_times(
        &mut self,
        instance: &str,
        path: &str,
        times: &Times,
    ) -> Result<(), Error> {
        let Some(modified) = times.modified.and_then(crate::copy_work::unix_seconds) else {
            return Ok(());
        };
        let Ok(seconds) = u32::try_from(modified) else {
            return Ok(());
        };
        self.on_background(instance, async |fs, _| {
            let attributes = FileAttributes {
                mtime: Some(seconds),
                atime: Some(seconds),
                ..FileAttributes::empty()
            };
            (fs.set_metadata(path, attributes).await).map_err(failed)
        })
    }
}

/// The folder that holds all of `sources` and their names from it (`sub\name` for one deeper); none
/// when they are on more than one disk (no folder holds them all).
pub(crate) fn common_folder(sources: &[PathBuf]) -> Option<(PathBuf, Vec<String>)> {
    let mut folder: PathBuf = sources.first()?.parent()?.to_path_buf();
    for source in sources {
        while !source.starts_with(&folder) {
            folder = folder.parent()?.to_path_buf();
        }
    }
    let names = (sources.iter())
        .map(|source| {
            let name: &Path = source.strip_prefix(&folder).unwrap_or(source);
            name.display().to_string()
        })
        .collect();
    Some((folder, names))
}
