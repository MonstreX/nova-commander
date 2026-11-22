//! Reading a folder on a worker thread (ADR-0013 §5): a panel asks for a folder and gets a handle; the
//! entries come back through it. Dropping the handle stops a reading nobody waits for any more; stopping
//! it on purpose (Esc) keeps what was read, as FAR does. The folder read is also watched for changes
//! from then on (3b step 2), the watch opened here since opening it may take as long as reading.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use crate::jobs::disk_threads;
use crate::vfs::{self, Capabilities, Entry, PanelLook, VfsPath, Watch};

/// A folder being read.
pub struct Listing {
    dir: VfsPath,
    /// Its thread: a call hanging on a disk broken once nobody waits for it (ADR-0019 §4).
    thread: Option<thread::JoinHandle<()>>,
    /// Counts its thread as left on its disk once this is dropped before the thread ends (review R25).
    waited: Option<disk_threads::Waited>,
    result: Receiver<io::Result<Listed>>,
    read: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}

/// A folder read.
#[derive(Debug)]
pub struct Listed {
    /// The folder requested, or the nearest existing one above it (`Request::refresh`).
    pub dir: VfsPath,
    pub entries: Vec<Entry>,
    /// Changes of `dir` from before it was read on; `None` when its provider reports none, or it is on
    /// a network drive and `Request::watch_remote` is off.
    pub watch: Option<Box<dyn Watch>>,
    /// What `dir` allows (ADR-0013 §4), asked here so the panel knows it before a key acts.
    pub capabilities: Capabilities,
    /// The provider's title and modes for `dir` (a plugin's panel).
    pub look: PanelLook,
}

/// How a folder is read.
#[derive(Debug, Clone, Copy, Default)]
pub struct Request {
    /// The panel's own folder read again: one that is gone gives the nearest existing folder above it,
    /// as FAR's `SetCurPath` does.
    pub refresh: bool,
    /// FAR's "Network drives autorefresh": folders on network drives are watched too.
    pub watch_remote: bool,
    /// A share's root (`\\server\share`) has a ".." too (FAR's with its Network plugin there).
    pub share_parent: bool,
}

/// Starts reading `dir`; a ".." entry comes first unless `dir` is a root (a plugin's root has one).
pub fn start(dir: VfsPath, request: Request) -> Listing {
    let (sender, result) = mpsc::channel();
    let read = Arc::new(AtomicUsize::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    // A disk with readings left hanging on it gets no more (review R25).
    let (slot, waited) = match disk_threads::take(&dir) {
        Ok(taken) => taken,
        Err(error) => {
            let _ = sender.send(Err(error));
            return Listing {
                dir,
                thread: None,
                waited: None,
                result,
                read,
                cancel,
            };
        }
    };
    let worker = {
        let (dir, read, cancel, sender) =
            (dir.clone(), read.clone(), cancel.clone(), sender.clone());
        move || {
            let _slot = slot;
            // The panel may be gone; then nobody wants the result.
            let _ = sender.send(read_existing(dir, request, &read, &cancel));
        }
    };
    let thread = match thread::Builder::new()
        .name("listing".to_string())
        .spawn(worker)
    {
        Ok(thread) => Some(thread),
        Err(error) => {
            let _ = sender.send(Err(error));
            None
        }
    };
    Listing {
        dir,
        thread,
        waited: Some(waited),
        result,
        read,
        cancel,
    }
}

impl Listing {
    pub fn dir(&self) -> &VfsPath {
        &self.dir
    }

    /// Entries read so far (FAR's "Reading: N files").
    pub fn read_so_far(&self) -> usize {
        self.read.load(Ordering::Relaxed)
    }

    /// The result, if it comes within `timeout`. Taken once: the handle is done with afterwards.
    pub fn wait(&self, timeout: Duration) -> Option<io::Result<Listed>> {
        match self.result.recv_timeout(timeout) {
            Ok(result) => Some(result),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Err(io::Error::other(
                "the folder reading stopped unexpectedly",
            ))),
        }
    }

    /// Esc: stop reading; the result then holds what was read.
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for Listing {
    /// Nobody waits for the reading any more: it stops, and a call it hangs in on a disk that does not
    /// answer is broken, its thread freed (review R25).
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = &self.thread {
            crate::platform::process::break_thread_io(thread);
        }
        // Nobody waits for it any more: its thread, if still on its disk, is counted as left there.
        let _ = self.waited.take();
    }
}

/// Reads `dir`, or the nearest existing folder above it for a refresh, watching what it reads.
fn read_existing(
    mut dir: VfsPath,
    request: Request,
    read: &AtomicUsize,
    cancel: &AtomicBool,
) -> io::Result<Listed> {
    loop {
        let watch = start_watch(&dir, request);
        match read_all(&dir, request, read, cancel) {
            Ok(entries) => {
                let provider = vfs::provider_for(&dir).ok();
                let capabilities = (provider.as_ref())
                    .map(|provider| provider.capabilities(&dir))
                    .unwrap_or_default();
                let look =
                    (provider.map(|provider| provider.look(&dir, cancel))).unwrap_or_default();
                return Ok(Listed {
                    dir,
                    entries,
                    watch,
                    capabilities,
                    look,
                });
            }
            Err(error) if request.refresh && error.kind() == io::ErrorKind::NotFound => {
                match dir.parent() {
                    Some(parent) => dir = parent,
                    None => return Err(error),
                }
            }
            Err(error) => return Err(error),
        }
    }
}

fn start_watch(dir: &VfsPath, request: Request) -> Option<Box<dyn Watch>> {
    if !request.watch_remote && dir.as_local().is_some_and(vfs::local::is_remote) {
        return None;
    }
    vfs::provider_for(dir).ok()?.watch(dir).ok()
}

fn read_all(
    dir: &VfsPath,
    request: Request,
    read: &AtomicUsize,
    cancel: &AtomicBool,
) -> io::Result<Vec<Entry>> {
    let provider = vfs::provider_for(dir)?;
    let mut entries = Vec::new();
    // A plugin's root has one too: it leads back to where the panel came from (FAR's plugin panels).
    // A share's root has one while a plugin goes up from it (`app::plugins::share_up`).
    let share_root =
        || request.share_parent && dir.as_local().and_then(vfs::local::share_of).is_some();
    if dir.parent().is_some() || provider.leads_back() || share_root() {
        entries.push(Entry::parent());
    }
    let listed = provider.list(dir, cancel, &mut |entry| {
        entries.push(entry);
        read.fetch_add(1, Ordering::Relaxed);
    });
    match listed {
        Ok(()) => Ok(entries),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(entries),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    const LONG: Duration = Duration::from_secs(10);

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn a_folder_is_read_with_its_parent_entry_first() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.txt"), b"hello").unwrap();
        let listing = start(VfsPath::local(temp.path()), Request::default());
        let entries = listing.wait(LONG).unwrap().unwrap().entries;
        assert_eq!(names(&entries), vec!["..", "a.txt"]);
        assert!(entries[0].is_parent);
        assert_eq!(listing.read_so_far(), 1);
    }

    #[test]
    fn a_folder_comes_with_what_its_provider_allows_there() {
        let root = vfs::test_tree::root("listing-capabilities");
        let listed = start(root, Request::default()).wait(LONG).unwrap().unwrap();
        assert!(listed.capabilities.trash);
        let plain = vfs::testing::put("changing://capabilities/", &[]);
        let listed = start(plain, Request::default())
            .wait(LONG)
            .unwrap()
            .unwrap();
        assert!(
            !listed.capabilities.trash,
            "a provider that says nothing has no bin"
        );
    }

    #[test]
    fn a_missing_folder_or_provider_is_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let missing = start(
            VfsPath::local(temp.path().join("missing")),
            Request::default(),
        );
        assert_eq!(
            missing.wait(LONG).unwrap().unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        let unknown = start(VfsPath::parse("nosuch://host/"), Request::default());
        assert_eq!(
            unknown.wait(LONG).unwrap().unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
    }

    #[test]
    fn a_refresh_of_a_folder_that_is_gone_reads_the_nearest_existing_one_above() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("kept")).unwrap();
        let gone = temp.path().join("kept").join("gone").join("deeper");
        let refresh = Request {
            refresh: true,
            ..Request::default()
        };
        let listed = start(VfsPath::local(&gone), refresh)
            .wait(LONG)
            .unwrap()
            .unwrap();
        assert_eq!(listed.dir, VfsPath::local(temp.path().join("kept")));
        assert_eq!(names(&listed.entries), vec![".."]);
        let entering = start(VfsPath::local(&gone), Request::default());
        assert_eq!(
            entering.wait(LONG).unwrap().unwrap_err().kind(),
            io::ErrorKind::NotFound,
            "entering a folder that is not there is an error"
        );
    }

    #[test]
    fn the_folder_read_comes_with_its_watch() {
        let dir = vfs::testing::put("changing://listing/", &["a"]);
        let listed = start(dir, Request::default()).wait(LONG).unwrap().unwrap();
        let watch = listed.watch.unwrap();
        assert!(!watch.take_change());
        vfs::testing::put("changing://listing/", &["a", "b"]);
        assert!(watch.take_change());
        let endless = start(vfs::testing::endless(), Request::default());
        endless.stop();
        assert!(
            endless.wait(LONG).unwrap().unwrap().watch.is_none(),
            "a provider that reports no changes"
        );
    }

    #[test]
    fn a_stopped_reading_keeps_what_was_read() {
        let listing = start(vfs::testing::endless(), Request::default());
        let deadline = Instant::now() + LONG;
        while listing.read_so_far() == 0 && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(listing.wait(Duration::ZERO).is_none(), "still reading");
        listing.stop();
        let entries = listing.wait(LONG).unwrap().unwrap().entries;
        assert_eq!(
            names(&entries),
            vec!["first", "second"],
            "a provider root has no .."
        );
    }

    #[test]
    fn a_dropped_handle_stops_the_worker() {
        let listing = start(vfs::testing::endless_named("dropped"), Request::default());
        let deadline = Instant::now() + LONG;
        while listing.read_so_far() == 0 && Instant::now() < deadline {
            thread::yield_now();
        }
        drop(listing);
        while !vfs::testing::stopped("dropped") && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(vfs::testing::stopped("dropped"), "the worker ended");
    }
}
