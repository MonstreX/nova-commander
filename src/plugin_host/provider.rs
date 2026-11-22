//! A plugin's panels as a `vfs` provider (ADR-0016 §13): `<plugin>://<instance>/<path>` paths, each call
//! sent to the plugin's own thread and waited for on the worker that asked (never the UI thread). A
//! reading asked to stop stops waiting at once and the plugin hears it (`cancelled()`, ADR-0019 §5); its
//! call ends on its own and its answer is dropped. A plugin no longer loaded answers that it is gone. Its
//! files (5c-2) are read, written, made, renamed and removed the same way (`plugin_files`), stopped by the
//! stop of the work that asks (`vfs::stop`, review R14: F7, a copy).

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

use super::Job;
use super::calls::Calls;
use super::ffi::{FileAnswer, FileCall};
use super::plugin_files;
use crate::platform::files::{Attributes, FileTimes};
use crate::vfs::{Entry, PanelLook, Provider, RandomRead, Removal, VfsPath, WriteMode};

/// How often a waiting reading looks whether it was asked to stop.
const LOOK: Duration = Duration::from_millis(20);

/// The way to one plugin's thread, as its provider and its open files use it.
#[derive(Clone)]
pub(super) struct Line {
    /// The plugin's name in its paths (lower case).
    id: Arc<str>,
    jobs: Arc<Calls>,
}

/// The paths of one plugin's panels.
pub(super) struct PluginProvider {
    line: Line,
}

impl PluginProvider {
    pub(super) fn new(name: &str, jobs: Arc<Calls>) -> Self {
        Self {
            line: Line {
                id: name.to_ascii_lowercase().into(),
                jobs,
            },
        }
    }
}

impl Line {
    /// A call of the plugin's for `dir`'s instance and path, its answer waited for until `cancel`.
    fn ask<T>(
        &self,
        dir: &VfsPath,
        cancel: &AtomicBool,
        job: impl FnOnce(String, String, std::sync::mpsc::Sender<T>) -> Job,
    ) -> io::Result<T> {
        let Some((instance, path)) = dir.session_path() else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("{dir} is not a plugin's panel"),
            ));
        };
        let (reply, answer) = channel();
        let Some(id) = self
            .jobs
            .send(job(instance.to_string(), path.to_string(), reply))
        else {
            return Err(gone(&self.id));
        };
        let answered = wait(&answer, cancel, &self.id);
        if answered
            .as_ref()
            .is_err_and(|error| error.kind() == io::ErrorKind::Interrupted)
        {
            // Nobody waits for it any more: the plugin hears it, and may end its call at once.
            self.jobs.state.stop(id);
        }
        answered
    }

    /// `call` on the file `path`, waited for until the stop of the work that asks (`vfs::stop`); a stop
    /// fails it with `vfs::stop::stopped` (not `Interrupted`, which readers and writers retry), and once
    /// the stop is set no call is sent at all.
    pub(super) fn file(&self, path: &VfsPath, call: FileCall) -> io::Result<FileAnswer> {
        let stop = crate::vfs::stop::current();
        if stop.load(Ordering::Relaxed) {
            return Err(crate::vfs::stop::stopped());
        }
        let answered = self.ask(path, &stop, |instance, path, reply| Job::File {
            instance,
            path,
            call,
            reply,
        });
        match answered {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                Err(crate::vfs::stop::stopped())
            }
            Err(error) => Err(error),
            Ok(answer) => answer,
        }
    }
}

/// The plugin's answer; `Interrupted` once `cancel` is set, gone when its thread drops the question.
pub(super) fn wait<T>(answer: &Receiver<T>, cancel: &AtomicBool, id: &str) -> io::Result<T> {
    loop {
        match answer.recv_timeout(LOOK) {
            Ok(answer) => return Ok(answer),
            Err(RecvTimeoutError::Disconnected) => return Err(gone(id)),
            Err(RecvTimeoutError::Timeout) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err(io::ErrorKind::Interrupted.into());
                }
            }
        }
    }
}

fn gone(id: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotConnected,
        format!("The plugin {id} is no longer loaded"),
    )
}

/// The plugin answered something else than the call asks for (never by the host's own calls).
pub(super) fn unexpected() -> io::Error {
    io::Error::other("The plugin's answer does not fit the call")
}

impl Provider for PluginProvider {
    fn id(&self) -> &str {
        &self.line.id
    }

    fn list(
        &self,
        dir: &VfsPath,
        cancel: &AtomicBool,
        found: &mut dyn FnMut(Entry),
    ) -> io::Result<()> {
        let listed = self
            .line
            .ask(dir, cancel, |instance, path, reply| Job::List {
                instance,
                path,
                reply,
            })??;
        // A name that would lead elsewhere ("..", "a/b") is not the folder's.
        (listed.into_iter())
            .filter(|entry| dir.accepts_name(&entry.name))
            .for_each(found);
        Ok(())
    }

    fn leads_back(&self) -> bool {
        true
    }

    fn look(&self, dir: &VfsPath, cancel: &AtomicBool) -> PanelLook {
        self.line
            .ask(dir, cancel, |instance, path, reply| Job::Look {
                instance,
                path,
                reply,
            })
            .unwrap_or_default()
    }

    fn stat(&self, path: &VfsPath) -> io::Result<Entry> {
        match self.line.file(path, FileCall::Stat)? {
            FileAnswer::Entry(entry) => Ok(entry),
            _ => Err(unexpected()),
        }
    }

    fn open_read(&self, path: &VfsPath) -> io::Result<Box<dyn io::Read + Send>> {
        Ok(Box::new(plugin_files::Reader::new(&self.line, path)))
    }

    fn open_random(&self, path: &VfsPath) -> io::Result<Box<dyn RandomRead>> {
        // Its length now: a file that is not there fails here, as a disk's does.
        let file = plugin_files::Random::new(&self.line, path);
        self.stat(path)?;
        Ok(Box::new(file))
    }

    fn open_write(&self, path: &VfsPath, mode: WriteMode) -> io::Result<Box<dyn io::Write + Send>> {
        if mode == WriteMode::New {
            // Made at once, empty: an existing entry fails before anything is written.
            let made = FileCall::Write {
                append: false,
                data: Vec::new(),
            };
            self.line.file(path, made)?;
        }
        Ok(Box::new(plugin_files::Writer::new(&self.line, path)))
    }

    fn make_dir(&self, path: &VfsPath) -> io::Result<()> {
        self.line.file(path, FileCall::MakeDir).map(drop)
    }

    fn rename(&self, from: &VfsPath, to: &VfsPath, replace: bool) -> io::Result<()> {
        let Some((_, to_path)) = to.session_path().filter(|_| to.same_mount(from)) else {
            return Err(io::Error::new(
                io::ErrorKind::CrossesDevices,
                format!("{from} cannot be renamed into another panel"),
            ));
        };
        let call = FileCall::Rename {
            to: to_path.to_string(),
            replace,
        };
        self.line.file(from, call).map(drop)
    }

    fn remove(&self, path: &VfsPath, how: Removal) -> io::Result<()> {
        let folder = match how {
            Removal::File => false,
            Removal::EmptyDir => true,
            Removal::Trash => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("{path} has no Recycle Bin"),
                ));
            }
        };
        self.line.file(path, FileCall::Remove { folder }).map(drop)
    }

    fn set_times(
        &self,
        path: &VfsPath,
        times: &FileTimes,
        attributes: Option<Attributes>,
    ) -> io::Result<()> {
        let call = FileCall::SetTimes {
            times: *times,
            attributes,
        };
        self.line.file(path, call).map(drop)
    }
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
