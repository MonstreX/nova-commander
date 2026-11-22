//! A disk's file opened by the plugins for Alt+F7's "Search in archives" (FAR's `OpenFilePlugin` with
//! `OFP_SEARCH` and `OPM_FIND`, ADR-0016 addendum S6.7): offered to the plugins whose masks take its name,
//! in their order, each asked on its own thread with `OPEN_SEARCH` (no window) while the search's thread
//! waits; the first panel opened is the search's, closed (`Job::Close`) when the search is done with it.
//! A stop of the search stops the wait at once, the plugin hearing it (`cancelled()`).

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{SendError, Sender, channel};

use nc_plugin_api::OPEN_SEARCH;

use super::calls::Calls;
use super::ffi::{KeyGo, Loaded};
use super::worker::Job;
use super::{PluginHost, provider};
use crate::vfs::{FileMask, FileOpener, OpenedFile, VfsPath};

/// A plugin that may take the file: its name, its masks, the way to its thread.
struct Taker {
    name: String,
    masks: Vec<FileMask>,
    jobs: Arc<Calls>,
}

/// The plugins' opener of a disk's files for the search.
struct PluginOpener {
    takers: Vec<Taker>,
}

impl PluginHost {
    /// An opener of a disk's files by the plugins `takers` (each with its masks, as the program reads
    /// them), in that order; plugins no longer loaded are left out.
    pub fn file_opener(&self, takers: Vec<(usize, Vec<FileMask>)>) -> Arc<dyn FileOpener> {
        let takers = (takers.into_iter())
            .filter_map(|(index, masks)| {
                let plugin = self.plugins.get(index)?;
                Some(Taker {
                    name: plugin.name.clone(),
                    masks,
                    jobs: Arc::clone(&plugin.worker.jobs),
                })
            })
            .collect();
        Arc::new(PluginOpener { takers })
    }
}

impl FileOpener for PluginOpener {
    fn open(&self, file: &Path, stop: &AtomicBool) -> Option<OpenedFile> {
        let name = file.file_name()?.to_string_lossy().into_owned();
        for taker in &self.takers {
            if !taker.masks.iter().any(|mask| mask.matches(&name)) {
                continue;
            }
            match taker.open(file, stop) {
                Ok(Some(opened)) => return Some(opened),
                Ok(None) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => return None,
                // Gone: the next plugin's.
                Err(_) => {}
            }
        }
        None
    }
}

impl Taker {
    /// `file` offered to this plugin: its panel's root, none when not taken.
    fn open(&self, file: &Path, stop: &AtomicBool) -> io::Result<Option<OpenedFile>> {
        let (reply, answer) = channel();
        let job = Job::SearchOpen {
            path: file.to_path_buf(),
            reply,
        };
        let Some(id) = self.jobs.send(job) else {
            return Ok(None);
        };
        let answered = provider::wait(&answer, stop, &self.name);
        if answered
            .as_ref()
            .is_err_and(|error| error.kind() == io::ErrorKind::Interrupted)
        {
            // Nobody waits for it any more: the plugin hears it (a panel it still opens is closed on
            // its thread, `hand_over`).
            self.jobs.state.stop(id);
        }
        let Some(instance) = answered? else {
            return Ok(None);
        };
        // R7 review A4: R47's check of the commands' panels - a name with / or \ is no path's part.
        if let Some(error) = super::bad_instance(&instance) {
            self.jobs.send(Job::Close(instance));
            return Err(io::Error::other(error));
        }
        let jobs = Arc::clone(&self.jobs);
        let closed = instance.clone();
        let close = Box::new(move || {
            jobs.send(Job::Close(closed));
        });
        let root = VfsPath::in_session(&self.name, &instance, "/");
        Ok(Some(OpenedFile::new(root, close)))
    }
}

/// The file `path` offered on the plugin's thread with `OPEN_SEARCH`, the answer sent by `reply`; a
/// panel opened once the search no longer waits (stopped, its results closed) is closed at once.
pub(super) fn answer(loaded: &Loaded, path: &Path, reply: &Sender<Option<String>>) {
    hand_over(opened(loaded, path), reply, |instance| {
        loaded.close_panel(instance);
    });
}

/// `opened` sent by `reply`; when nobody takes it, the panel it names closed by `close`.
fn hand_over(opened: Option<String>, reply: &Sender<Option<String>>, close: impl FnOnce(&str)) {
    if let Err(SendError(Some(instance))) = reply.send(opened) {
        close(&instance);
    }
}

/// The file `path` offered on the plugin's thread with `OPEN_SEARCH`: the instance of the panel it
/// opened; none when it passed, failed or answered anything else.
fn opened(loaded: &Loaded, path: &Path) -> Option<String> {
    let answer = loaded.open_file(path, OPEN_SEARCH, None).ok()?;
    match answer.go {
        Some(KeyGo::Open(instance)) if !answer.pass => Some(instance),
        _ => None,
    }
}

#[cfg(test)]
#[path = "search_open_tests.rs"]
mod tests;
