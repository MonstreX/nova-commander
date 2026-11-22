//! A file's blocks read on a thread of its own as a view asks for them (ADR-0015 §1, owner: the screen
//! never waits for the disk): the view shows what it has and asks for what it lacks; the thread reads
//! by offset and, while asked for nothing, looks every second whether the file grew or shrank.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::vfs::{RandomRead, VfsPath, provider_for};

/// How often the length is looked at while nothing is asked.
const LOOK_AGAIN: Duration = Duration::from_secs(1);

/// What came since the last [`FileReader::take`].
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Taken {
    /// The file's length, when it is new (the first time, or it changed).
    pub len: Option<u64>,
    /// Blocks read: their numbers and bytes (fewer than a block at the end).
    pub blocks: Vec<(u64, Vec<u8>)>,
    /// The file could not be opened or read.
    pub failed: Option<String>,
    /// Another file took its path and was opened in its place: what was read is of the old one.
    pub reopened: bool,
}

#[derive(Default)]
struct State {
    asked: VecDeque<u64>,
    taken: Taken,
    stop: bool,
    /// The file open, shared with the document's save (`open_kept`).
    held: Option<HeldFile>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("State")
            .field("asked", &self.asked)
            .field("taken", &self.taken)
            .field("stop", &self.stop)
            .finish_non_exhaustive()
    }
}

/// A file kept open for a document's life: what its unread pieces are read from, its save too.
pub type HeldFile = Arc<Mutex<Box<dyn RandomRead>>>;

/// The file kept open read by another thread (the save) beside the reader's.
pub struct HeldRead(pub HeldFile);

impl RandomRead for HeldRead {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        lock(&self.0).read_at(offset, buf)
    }

    fn len(&mut self) -> std::io::Result<u64> {
        lock(&self.0).len()
    }
}

#[derive(Debug, Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The thread reading one file; dropping it stops the thread.
#[derive(Debug)]
pub struct FileReader(Arc<Shared>, Option<crate::jobs::disk_threads::Waited>);

impl FileReader {
    /// Opens `path` to read blocks of `block` bytes; a file put at its path in its place is opened
    /// again (a log rotated, a file saved over).
    pub fn open(path: VfsPath, block: u64) -> Self {
        Self::start(path, block, false)
    }

    /// [`FileReader::open`] for an editor's document (review C2): the file opened first stays the one
    /// read for the document's life, whatever is put at its path - another editor's save of it too -
    /// and its save reads the bytes not edited from it (`held`), so that no document is ever a mixture
    /// of two files.
    pub fn open_kept(path: VfsPath, block: u64) -> Self {
        Self::start(path, block, true)
    }

    /// The file kept open (`open_kept`), once it is.
    pub fn held(&self) -> Option<HeldFile> {
        self.0.state().held.clone()
    }

    fn start(path: VfsPath, block: u64, keep: bool) -> Self {
        let shared = Arc::new(Shared::default());
        let reading = Arc::clone(&shared);
        // A disk with threads left hanging on it gets no more (review R25).
        let (slot, waited) = match crate::jobs::disk_threads::take(&path) {
            Ok(taken) => taken,
            Err(error) => {
                shared.state().taken.failed = Some(error.to_string());
                return Self(shared, None);
            }
        };
        let started = std::thread::Builder::new()
            .name("file reader".to_string())
            .spawn(move || {
                let _slot = slot;
                read(&path, (block, keep), &reading);
            });
        if let Err(error) = started {
            shared.state().taken.failed = Some(error.to_string());
        }
        Self(shared, Some(waited))
    }

    /// Asks for block `index` (once: asking again before it comes changes nothing).
    pub fn want(&self, index: u64) {
        let mut state = self.0.state();
        if !state.asked.contains(&index) {
            state.asked.push_back(index);
            self.0.wake.notify_one();
        }
    }

    pub fn take(&self) -> Taken {
        std::mem::take(&mut self.0.state().taken)
    }
}

impl Drop for FileReader {
    fn drop(&mut self) {
        self.0.state().stop = true;
        self.0.wake.notify_one();
        // Nobody waits for it any more: its thread, if still on its disk, is counted as left there.
        let _ = self.1.take();
    }
}

fn read(path: &VfsPath, (block, keep): (u64, bool), shared: &Shared) {
    let opened = provider_for(path).and_then(|provider| provider.open_random(path));
    let mut file: HeldFile = match opened {
        Ok(file) => Arc::new(Mutex::new(file)),
        Err(error) => {
            shared.state().taken.failed = Some(error.to_string());
            return;
        }
    };
    if keep {
        shared.state().held = Some(Arc::clone(&file));
    }
    let mut known = None;
    // The length is looked at first, then at least every second (busy or not) and after a read
    // that ends before it: a file that shrank is never asked for blocks it no longer has.
    let mut look = true;
    let mut looked = Instant::now();
    loop {
        if look || looked.elapsed() >= LOOK_AGAIN {
            if !keep
                && lock(&file).replaced()
                && let Ok(again) =
                    provider_for(path).and_then(|provider| provider.open_random(path))
            {
                file = Arc::new(Mutex::new(again));
                known = None;
                // Blocks of the old file not taken yet go with it.
                let mut state = shared.state();
                state.taken.blocks.clear();
                state.taken.reopened = true;
            }
            if let Err(error) = look_at_len(lock(&file).as_mut(), &mut known, shared) {
                shared.state().taken.failed = Some(error.to_string());
                return;
            }
            looked = Instant::now();
        }
        let Some(asked) = next_asked(shared) else {
            return;
        };
        look = asked.is_none();
        let Some(index) = asked else {
            continue;
        };
        let mut bytes = vec![0; usize::try_from(block).unwrap_or(usize::MAX)];
        let read = lock(&file).read_at(index * block, &mut bytes);
        match read {
            Ok(read) => {
                bytes.truncate(read);
                look = known.is_some_and(|len| {
                    index * block + (read as u64) < len.min((index + 1) * block)
                });
                let mut state = shared.state();
                state.asked.retain(|&asked| asked != index);
                state.taken.blocks.push((index, bytes));
            }
            Err(error) => {
                shared.state().taken.failed = Some(error.to_string());
                return;
            }
        }
    }
}

/// The file, its other user (the save) done with it or not.
fn lock(file: &HeldFile) -> MutexGuard<'_, Box<dyn RandomRead>> {
    file.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Tells a length not told yet.
fn look_at_len(
    file: &mut dyn RandomRead,
    known: &mut Option<u64>,
    shared: &Shared,
) -> std::io::Result<()> {
    let len = file.len()?;
    if *known != Some(len) {
        *known = Some(len);
        shared.state().taken.len = Some(len);
    }
    Ok(())
}

/// The block asked first; `Some(None)` when nothing was asked for a while; none once stopped.
fn next_asked(shared: &Shared) -> Option<Option<u64>> {
    let state = shared.state();
    let (state, _) = shared
        .wake
        .wait_timeout_while(state, LOOK_AGAIN, |state| {
            !state.stop && state.asked.is_empty()
        })
        .unwrap_or_else(PoisonError::into_inner);
    if state.stop {
        return None;
    }
    Some(state.asked.front().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for(reader: &FileReader, done: impl Fn(&Taken) -> bool) -> Taken {
        let mut all = Taken::default();
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            let taken = reader.take();
            all.len = taken.len.or(all.len);
            all.blocks.extend(taken.blocks);
            all.failed = taken.failed.or(all.failed);
            all.reopened |= taken.reopened;
            if done(&all) {
                return all;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("nothing came: {all:?}");
    }

    #[test]
    fn blocks_asked_for_come_with_the_length() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("data.bin");
        std::fs::write(&path, b"0123456789").unwrap();
        let reader = FileReader::open(VfsPath::local(&path), 4);
        reader.want(2);
        reader.want(0);
        let taken = wait_for(&reader, |taken| taken.blocks.len() == 2);
        assert_eq!(taken.len, Some(10));
        assert!(
            taken.blocks.contains(&(2, b"89".to_vec())),
            "short at the end"
        );
        assert!(taken.blocks.contains(&(0, b"0123".to_vec())));
    }

    #[test]
    fn a_file_that_grows_tells_its_new_length() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("log.txt");
        std::fs::write(&path, b"ab").unwrap();
        let reader = FileReader::open(VfsPath::local(&path), 4);
        wait_for(&reader, |taken| taken.len == Some(2));
        std::fs::write(&path, b"abcdef").unwrap();
        wait_for(&reader, |taken| taken.len == Some(6));
    }

    #[test]
    fn a_short_read_looks_at_the_length_at_once() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rotated.log");
        std::fs::write(&path, b"0123456789").unwrap();
        let reader = FileReader::open(VfsPath::local(&path), 4);
        wait_for(&reader, |taken| taken.len == Some(10));
        std::fs::write(&path, b"01").unwrap();
        // Asked at once, before the second's look: the block past the new end tells it.
        reader.want(1);
        let started = Instant::now();
        let taken = wait_for(&reader, |taken| taken.len == Some(2));
        assert!(started.elapsed() < LOOK_AGAIN, "{:?}", started.elapsed());
        assert!(taken.blocks.contains(&(1, Vec::new())));
    }

    #[cfg(windows)]
    #[test]
    fn a_file_saved_over_by_a_rename_is_opened_again() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("notes.txt");
        std::fs::write(&path, b"old").unwrap();
        let reader = FileReader::open(VfsPath::local(&path), 16);
        wait_for(&reader, |taken| taken.len == Some(3));
        let saved = temp.path().join("notes.new");
        std::fs::write(&saved, b"newer").unwrap();
        std::fs::rename(&saved, &path).unwrap();
        let taken = wait_for(&reader, |taken| taken.reopened && taken.len == Some(5));
        assert!(taken.failed.is_none());
        reader.want(0);
        let taken = wait_for(&reader, |taken| !taken.blocks.is_empty());
        assert_eq!(taken.blocks, [(0, b"newer".to_vec())]);
    }

    /// Review C2: an editor's file stays the one read whatever is put at its path, and its save reads
    /// the same file through `held`; a plain reader keeps none.
    #[cfg(windows)]
    #[test]
    fn a_kept_file_stays_the_one_read_and_is_shared_with_the_save() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("notes.txt");
        std::fs::write(&path, b"old").unwrap();
        let reader = FileReader::open_kept(VfsPath::local(&path), 16);
        wait_for(&reader, |taken| taken.len == Some(3));
        let held = reader.held().expect("kept open");
        let saved = temp.path().join("notes.new");
        std::fs::write(&saved, b"newer").unwrap();
        std::fs::rename(&saved, &path).unwrap();
        // Past the reader's next look at the file: still the old one.
        let started = Instant::now();
        let mut taken = Taken::default();
        while started.elapsed() < 2 * LOOK_AGAIN + Duration::from_millis(500) {
            let more = reader.take();
            taken.len = more.len.or(taken.len);
            taken.reopened |= more.reopened;
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!taken.reopened && taken.len.is_none_or(|len| len == 3));
        reader.want(0);
        let taken = wait_for(&reader, |taken| !taken.blocks.is_empty());
        assert_eq!(taken.blocks, [(0, b"old".to_vec())]);
        let mut read = HeldRead(held);
        let mut buffer = [0; 8];
        assert_eq!(read.read_at(0, &mut buffer).unwrap(), 3);
        assert_eq!((&buffer[..3], read.len().unwrap()), (&b"old"[..], 3));
        let plain = FileReader::open(VfsPath::local(&path), 16);
        wait_for(&plain, |taken| taken.len == Some(5));
        assert!(plain.held().is_none());
    }

    #[test]
    fn a_file_not_there_fails() {
        let temp = tempfile::tempdir().unwrap();
        let reader = FileReader::open(VfsPath::local(temp.path().join("none")), 4);
        let taken = wait_for(&reader, |taken| taken.failed.is_some());
        assert!(taken.blocks.is_empty());
    }
}
