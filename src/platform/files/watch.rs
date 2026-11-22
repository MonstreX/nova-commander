//! Changes in a folder as the system reports them (FAR's `filesystemwatcher.cpp`): the folder itself, not
//! its subfolders; names, sizes, times, attributes and security. Which file changed does not matter: any
//! change means "read the folder again".

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A folder being watched; dropping it stops the watching (the system's side ends on its own shortly).
#[derive(Debug)]
pub struct DirWatch {
    flags: Arc<WatchFlags>,
}

/// What the system's side and the owner of a `DirWatch` tell each other.
#[derive(Debug, Default)]
pub struct WatchFlags {
    changed: AtomicBool,
    stopped: AtomicBool,
}

impl WatchFlags {
    /// The folder changed (or is gone: reading it again tells which).
    pub(in crate::platform) fn change(&self) {
        self.changed.store(true, Ordering::Relaxed);
    }

    /// The owner dropped its `DirWatch`.
    pub(in crate::platform) fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }
}

/// Starts watching `dir`. Fails when the folder cannot be opened or its file system does not report
/// changes.
pub fn watch_dir(dir: &Path) -> io::Result<DirWatch> {
    let flags = Arc::new(WatchFlags::default());
    super::super::os::watch_dir(dir, flags.clone())?;
    Ok(DirWatch { flags })
}

impl DirWatch {
    /// Whether the folder changed since the last call.
    pub fn take_change(&self) -> bool {
        self.flags.changed.swap(false, Ordering::Relaxed)
    }
}

impl Drop for DirWatch {
    fn drop(&mut self) {
        self.flags.stopped.store(true, Ordering::Relaxed);
    }
}
