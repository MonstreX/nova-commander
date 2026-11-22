//! A disk's file opened as a provider's folder, for Alt+F7's "Search in archives" (FAR's
//! `OpenFilePlugin` with `OFP_SEARCH`, ADR-0016 addendum S6.7): the file offered to the plugins that may
//! take it, without any window; the folder opened is walked like any provider's, then closed when the
//! [`OpenedFile`] goes. Called from worker threads only (the search's, a copy's).

use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::VfsPath;

/// Opens a disk's file as a provider's folder.
pub trait FileOpener: Send + Sync {
    /// `file` opened quietly by the first provider that takes it: its root; none when none takes it or
    /// `stop` is set.
    fn open(&self, file: &Path, stop: &AtomicBool) -> Option<OpenedFile>;
}

/// A file opened as a provider's folder, closed when it goes.
pub struct OpenedFile {
    root: VfsPath,
    close: Option<Box<dyn FnOnce() + Send>>,
}

impl OpenedFile {
    /// The folder `root` (a provider's session's root), `close` called once it is no longer needed.
    pub fn new(root: VfsPath, close: Box<dyn FnOnce() + Send>) -> Self {
        Self {
            root,
            close: Some(close),
        }
    }

    /// The opened file's root folder.
    pub fn root(&self) -> &VfsPath {
        &self.root
    }

    /// The provider that opened it.
    pub fn provider(&self) -> &str {
        self.root.provider().unwrap_or_default()
    }

    /// Where `path` (a path of this opened file's session) is inside `file`, the file opened:
    /// `provider:file!/path`, which stays after the session is closed.
    pub fn place_of(&self, file: &Path, path: &VfsPath) -> VfsPath {
        let inner = path.session_path().map_or("/", |(_, inner)| inner);
        VfsPath::in_file(self.provider(), VfsPath::local(file), inner)
    }

    /// The path of this session for `place`, a place inside the file opened (`provider:file!/path`).
    pub fn session_of(&self, place: &VfsPath) -> VfsPath {
        let inner = place.file_path().map_or("/", |(_, inner)| inner);
        let session = self.root.session_path().map_or("", |(session, _)| session);
        VfsPath::in_session(self.provider(), session, inner)
    }
}

impl Drop for OpenedFile {
    fn drop(&mut self) {
        if let Some(close) = self.close.take() {
            close();
        }
    }
}

impl std::fmt::Debug for OpenedFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("OpenedFile")
            .field(&self.root)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn a_session_and_the_places_inside_its_file_map_both_ways() {
        let closed = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&closed);
        let close = Box::new(move || {
            counted.fetch_add(1, Ordering::Relaxed);
        });
        let opened = OpenedFile::new(VfsPath::in_session("Archive", "7", "/"), close);
        assert_eq!(opened.provider(), "archive");
        let file = Path::new(r"D:\a.zip");
        let session = VfsPath::in_session("archive", "7", "/dir/x.txt");
        let place = opened.place_of(file, &session);
        assert_eq!(place, VfsPath::parse(r"archive:D:\a.zip!/dir/x.txt"));
        assert_eq!(opened.session_of(&place), session);
        assert_eq!(
            opened.place_of(file, opened.root()),
            VfsPath::parse(r"archive:D:\a.zip!/")
        );
        assert_eq!(closed.load(Ordering::Relaxed), 0, "open while it is kept");
        drop(opened);
        assert_eq!(closed.load(Ordering::Relaxed), 1, "closed once it goes");
    }
}
