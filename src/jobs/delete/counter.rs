//! FAR's `calculate_total` (`DelShowTotal`), run beside the question and the deletion, never before
//! them (owner: a count never holds the operation up). The question before deleting starts a
//! [`Counter`] and shows its files and folders as they grow; Delete hands it to the deletion, whose
//! progress takes its total from there, so nothing is counted twice. To the Recycle Bin the count is
//! finished first: a folder held open inside cannot be renamed (NTFS: access denied), and the bin
//! moves an object by renaming it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::Item;
use super::engine::list;
use crate::vfs::{self, Entry, Provider, VfsPath};

/// The count is published after this many objects.
const COUNT_BATCH: u64 = 64;

/// What a count found so far: links count as what they look like, but nothing inside them; `bytes`
/// is the files' sizes (a link adds none: what it points to stays).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
}

impl Tally {
    /// Everything the deletion takes, one by one.
    pub fn objects(self) -> u64 {
        self.files + self.folders
    }
}

/// An object to count.
struct ToCount {
    provider: Option<Arc<dyn Provider>>,
    path: VfsPath,
    folder: bool,
    /// What is inside is counted too: a folder that is not a link.
    walk: bool,
    /// A file's size; nothing for a folder or a link.
    size: u64,
}

impl ToCount {
    fn new(provider: Option<Arc<dyn Provider>>, path: VfsPath, entry: &Entry) -> Self {
        let link = entry.points_elsewhere();
        Self {
            provider,
            path,
            folder: entry.is_dir,
            walk: entry.is_dir && !link,
            size: if entry.is_dir || link { 0 } else { entry.size },
        }
    }
}

/// Each selected object, and what is inside a folder that is not a link; the tally is published as it
/// grows (`known` at the end). A folder that cannot be read counts as itself. Stops once `stop` is
/// set, inside a folder's listing too, publishing nothing more.
pub fn count(items: &[Item], stop: &AtomicBool, publish: &mut dyn FnMut(Tally, bool)) {
    let mut pending: Vec<ToCount> = items
        .iter()
        .map(|item| {
            ToCount::new(
                vfs::provider_for(&item.path).ok(),
                item.path.clone(),
                &item.entry,
            )
        })
        .collect();
    let mut tally = Tally::default();
    while let Some(object) = pending.pop() {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        if object.folder {
            tally.folders += 1;
        } else {
            tally.files += 1;
            tally.bytes = tally.bytes.saturating_add(object.size);
        }
        if tally.objects().is_multiple_of(COUNT_BATCH) {
            publish(tally, false);
        }
        let (Some(provider), true) = (object.provider, object.walk) else {
            continue;
        };
        // What the deletion took already is simply not counted.
        let entries = list(provider.as_ref(), &object.path, stop);
        if stop.load(Ordering::Relaxed) {
            return;
        }
        for entry in entries.unwrap_or_default() {
            let path = object.path.join(&entry.name);
            pending.push(ToCount::new(Some(Arc::clone(&provider)), path, &entry));
        }
    }
    publish(tally, true);
}

/// The tally so far and whether it is complete, shared with the counting thread.
#[derive(Default)]
struct Shared {
    state: Mutex<(Tally, bool)>,
    changed: Condvar,
    stop: AtomicBool,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, (Tally, bool)> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A count on its own thread. Dropping it stops the count; [`Counter::finish`] waits for its end too.
pub struct Counter {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Counter {
    /// Starts counting `items`. Without a thread for it the tally stays unknown: the question and the
    /// deletion go on without it.
    pub fn start(items: Vec<Item>) -> Self {
        let shared = Arc::new(Shared::default());
        let counting = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("delete count".to_string())
            .spawn(move || {
                count(&items, &counting.stop, &mut |tally, known| {
                    *counting.state() = (tally, known);
                    counting.changed.notify_all();
                });
            })
            .ok();
        Self { shared, thread }
    }

    /// Stops the count and waits until its thread holds nothing open any more.
    pub fn finish(mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            // A count that panicked holds nothing either.
            let _ = thread.join();
        }
    }

    /// The tally so far, and whether the count is over.
    pub fn now(&self) -> (Tally, bool) {
        *self.shared.state()
    }

    /// The tally once it changes or `timeout` passes.
    pub fn wait(&self, last: (Tally, bool), timeout: Duration) -> (Tally, bool) {
        let state = self.shared.state();
        let (state, _) = self
            .shared
            .changed
            .wait_timeout_while(state, timeout, |state| *state == last)
            .unwrap_or_else(PoisonError::into_inner);
        *state
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

impl std::fmt::Debug for Counter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("Counter").field(&self.now()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_or_finishing_a_counter_stops_its_count() {
        let counter = Counter::start(Vec::new());
        let shared = Arc::clone(&counter.shared);
        drop(counter);
        assert!(shared.stop.load(Ordering::Relaxed));

        let counter = Counter::start(Vec::new());
        let shared = Arc::clone(&counter.shared);
        counter.finish();
        assert!(shared.stop.load(Ordering::Relaxed));
        assert_eq!(
            Arc::strong_count(&shared),
            1,
            "the count's thread has ended and let go of it"
        );
    }
}
