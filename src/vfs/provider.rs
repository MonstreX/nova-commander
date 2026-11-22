//! The provider interface (ADR-0013 §2) and the registry that finds a path's provider.
//!
//! Providers are blocking and called only from worker threads (`jobs`); the UI thread never calls one.
//! The interface grows with the steps of 3b that use it (deletion, copy and move, properties); its
//! final shape is ADR-0013's.

use std::io::{self, Read, Write};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, PoisonError, RwLock};

use super::local::LocalProvider;
use super::{Entry, PanelLook, VfsPath};
use crate::platform::files::{Attributes, FileTimes};

pub trait Provider: Send + Sync {
    /// The name a `VfsPath` uses for this provider (`sftp` of `sftp://host/`).
    fn id(&self) -> &str;

    /// Reads `dir`'s entries (not ".."), handing each to `found`; stops with `Interrupted` once `cancel`
    /// is set.
    fn list(
        &self,
        dir: &VfsPath,
        cancel: &AtomicBool,
        found: &mut dyn FnMut(Entry),
    ) -> io::Result<()>;

    /// Whether its root lists ".." leading back to where the panel came from (FAR's plugin panels); a
    /// root has none otherwise.
    fn leads_back(&self) -> bool {
        false
    }

    /// How a panel showing `dir` looks (FAR's plugin `OpenPanelInfo`): its title and modes. Asked with
    /// the folder's reading; stops waiting (the default look) once `cancel` is set.
    fn look(&self, _dir: &VfsPath, _cancel: &AtomicBool) -> PanelLook {
        PanelLook::default()
    }

    /// What `path` allows (ADR-0013 §4). Asked by the job that reads a folder, so the panel knows it
    /// before a key acts.
    fn capabilities(&self, _path: &VfsPath) -> Capabilities {
        Capabilities::default()
    }

    /// One object as its folder lists it (ADR-0013 §2): a job checks again what the panel showed.
    fn stat(&self, path: &VfsPath) -> io::Result<Entry> {
        Err(unsupported(format!("{path} cannot be looked at alone")))
    }

    /// Reads a file from its start (ADR-0013 §2); the caller reads it in chunks and cancels between
    /// them.
    fn open_read(&self, path: &VfsPath) -> io::Result<Box<dyn Read + Send>> {
        Err(unsupported(format!("{path} cannot be read")))
    }

    /// Opens a file to be read anywhere in it (ADR-0015: the viewer reads only the blocks it shows). Other
    /// programs may go on writing, renaming or deleting it meanwhile.
    fn open_random(&self, path: &VfsPath) -> io::Result<Box<dyn RandomRead>> {
        Err(unsupported(format!("{path} cannot be read")))
    }

    /// Writes a file: a new one (never over an existing object: `AlreadyExists`), or onto the end of one
    /// that exists. A `flush` that succeeds means all of it is there: a move deletes the source next.
    fn open_write(&self, path: &VfsPath, _mode: WriteMode) -> io::Result<Box<dyn Write + Send>> {
        Err(unsupported(format!("{path} cannot be written")))
    }

    /// Creates one folder; its parent exists, an object of that name makes it fail (`AlreadyExists`).
    fn make_dir(&self, path: &VfsPath) -> io::Result<()> {
        Err(unsupported(format!("{path} cannot be created")))
    }

    /// Renames within this provider. An existing `to` is replaced in one step with `replace`, and is
    /// never touched without it (`AlreadyExists`).
    fn rename(&self, from: &VfsPath, _to: &VfsPath, _replace: bool) -> io::Result<()> {
        Err(unsupported(format!("{from} cannot be renamed")))
    }

    /// Removes one object (ADR-0013 §2): a file or a link (`File`), an empty folder or a link to a
    /// folder (`EmptyDir`), or a file or a whole folder into the Recycle Bin (`Trash`). A read-only
    /// object is removed too: the caller asked the user (FAR's "The following file is read only").
    fn remove(&self, path: &VfsPath, _how: Removal) -> io::Result<()> {
        Err(unsupported(format!("{path} cannot be deleted")))
    }

    /// Gives one object the times that are not `None` and, given, the hidden, system and read-only
    /// marks of `attributes` (an object a copy put in: a plugin's `set_times`, FAR's `PutFiles`).
    fn set_times(
        &self,
        path: &VfsPath,
        _times: &FileTimes,
        _attributes: Option<Attributes>,
    ) -> io::Result<()> {
        Err(unsupported(format!("{path} keeps no times given to it")))
    }

    /// Starts watching `dir` for changes (the "change notifications" capability, ADR-0013 §4); a
    /// provider without it answers `Unsupported` and its panels refresh on Ctrl+R only.
    fn watch(&self, dir: &VfsPath) -> io::Result<Box<dyn Watch>> {
        Err(unsupported(format!("{dir} reports no changes")))
    }
}

fn unsupported(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message)
}

/// How [`Provider::open_write`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    New,
    Append,
}

/// What a path allows; grows with the steps of 3b that ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// Deleting moves to a Recycle Bin (FAR asks `can_recycle` before its question).
    pub trash: bool,
}

/// How [`Provider::remove`] removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    File,
    EmptyDir,
    Trash,
}

/// A file opened by [`Provider::open_random`].
pub trait RandomRead: Send {
    /// Reads from `offset` into `buf`; fewer bytes than asked only at the file's end (0 past it).
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize>;
    /// The file's length now: it may grow or shrink while open.
    fn len(&mut self) -> io::Result<u64>;
    /// Another file is at its path now (saved over by a rename, a log rotated): the caller opens the
    /// path again. False where the provider cannot tell.
    fn replaced(&mut self) -> bool {
        false
    }
}

/// A folder being watched; dropping it stops the watching.
pub trait Watch: Send + std::fmt::Debug {
    /// Whether the folder changed since the last call (or is gone: reading it again tells which).
    fn take_change(&self) -> bool;
}

/// Providers of mounted paths: plugins register theirs in stage 5.
static MOUNTED: RwLock<Vec<Arc<dyn Provider>>> = RwLock::new(Vec::new());

/// The provider that serves `path`; `Unsupported` when none does (a plugin that is not installed).
pub fn provider_for(path: &VfsPath) -> io::Result<Arc<dyn Provider>> {
    match path {
        VfsPath::Local(_) => Ok(Arc::new(LocalProvider)),
        VfsPath::Mounted { provider, .. } => MOUNTED
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|candidate| candidate.id() == provider)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("No provider for \"{provider}:\" paths is installed"),
                )
            }),
    }
}

/// Adds a provider of mounted paths (the plugin host's, the tests'); one of the same id is replaced (a
/// plugin loaded again).
pub fn register(provider: Arc<dyn Provider>) {
    let mut mounted = MOUNTED.write().unwrap_or_else(PoisonError::into_inner);
    mounted.retain(|known| known.id() != provider.id());
    mounted.push(provider);
}

/// The provider of id `id` taken away (a plugin unloaded while the program runs): its paths have none.
pub fn unregister(id: &str) {
    let mut mounted = MOUNTED.write().unwrap_or_else(PoisonError::into_inner);
    mounted.retain(|known| known.id() != id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_paths_go_to_the_local_provider_and_unknown_ones_fail() {
        assert_eq!(provider_for(&VfsPath::local("C:\\")).unwrap().id(), "local");
        let error = provider_for(&VfsPath::parse("nosuch://host/"))
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }
}
