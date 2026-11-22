//! F3 on a folder: FAR's `GetDirInfo`, on its own thread so the window showing it stays alive. Links to folders
//! are walked, as FAR's default `ScanJunction`; the place a link leads to only once, so a link back up
//! cannot loop (FAR guards against loops only: two links to one folder count twice there).

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::platform::files::{exact_path, file_identity};
use crate::vfs::{Entry, VfsPath, provider_for};

/// The count is published after this many objects.
const COUNT_BATCH: u64 = 256;

/// A folder to count and the name of its row.
#[derive(Debug, Clone)]
pub struct Folder {
    pub name: String,
    pub path: VfsPath,
}

/// How far a count is: the folders begun, with their sizes so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counted {
    pub sizes: Vec<(String, u64)>,
    /// How many of `sizes`, from the first, are final.
    pub finished: usize,
    /// Files and folders seen, in all the folders.
    pub items: u64,
    pub over: bool,
}

#[derive(Debug, Default)]
struct Shared {
    counted: Mutex<Counted>,
    stop: AtomicBool,
}

impl Shared {
    fn counted(&self) -> MutexGuard<'_, Counted> {
        self.counted.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A count on its own thread; dropping it stops the count.
#[derive(Debug)]
pub struct SizeCount(Arc<Shared>, Option<crate::jobs::disk_threads::Waited>);

impl SizeCount {
    /// Counts `folders` one after another. Without a thread for it the count is over at once.
    pub fn start(folders: Vec<Folder>) -> Self {
        let shared = Arc::new(Shared::default());
        let counting = Arc::clone(&shared);
        // A disk with threads left hanging on it gets no more (review R25): nothing counted.
        let slot = folders
            .first()
            .map(|first| crate::jobs::disk_threads::take(&first.path));
        let Ok(taken) = slot.transpose() else {
            shared.counted().over = true;
            return Self(shared, None);
        };
        let (slot, waited) = taken.unzip();
        let started = std::thread::Builder::new()
            .name("folder size".to_string())
            .spawn(move || {
                let _slot = slot;
                count_folders(&folders, &counting);
            });
        if started.is_err() {
            shared.counted().over = true;
        }
        Self(shared, waited)
    }

    /// A count that never moves on.
    #[cfg(test)]
    pub fn stalled() -> Self {
        Self(Arc::new(Shared::default()), None)
    }

    /// A count that stands where `counted` says.
    #[cfg(test)]
    pub fn at(counted: Counted) -> Self {
        let count = Self::stalled();
        *count.0.counted() = counted;
        count
    }

    pub fn now(&self) -> Counted {
        self.0.counted().clone()
    }
}

impl Drop for SizeCount {
    fn drop(&mut self) {
        self.0.stop.store(true, Ordering::Relaxed);
        // Nobody waits for it any more: its thread, if still on its disk, is counted as left there.
        let _ = self.1.take();
    }
}

fn count_folders(folders: &[Folder], shared: &Shared) {
    for (index, folder) in folders.iter().enumerate() {
        let before = {
            let mut counted = shared.counted();
            counted.sizes.push((folder.name.clone(), 0));
            counted.items
        };
        let (bytes, items) = size_of(&folder.path, &shared.stop, &mut |bytes, items| {
            let mut counted = shared.counted();
            counted.sizes[index].1 = bytes;
            counted.items = before + items;
        });
        if shared.stop.load(Ordering::Relaxed) {
            return;
        }
        let mut counted = shared.counted();
        counted.sizes[index].1 = bytes;
        counted.items = before + items;
        counted.finished = index + 1;
    }
    shared.counted().over = true;
}

/// The bytes of all files under `root` and how many objects it holds, `publish` told as they grow.
/// What cannot be read counts for nothing, as in FAR. Stops once `stop` is set.
fn size_of(root: &VfsPath, stop: &AtomicBool, publish: &mut dyn FnMut(u64, u64)) -> (u64, u64) {
    let mut places: HashSet<(u32, u64)> = identity(root).into_iter().collect();
    let mut pending = vec![root.clone()];
    let (mut bytes, mut objects) = (0u64, 0u64);
    while let Some(folder) = pending.pop() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        for child in children(&folder, stop) {
            let path = folder.join(&child.name);
            if child.is_dir {
                if !child.points_elsewhere() || first_visit(&path, &mut places) {
                    pending.push(path);
                }
            } else if !child.points_elsewhere() {
                bytes = bytes.saturating_add(child.size);
            }
            objects += 1;
            if objects.is_multiple_of(COUNT_BATCH) {
                publish(bytes, objects);
            }
        }
    }
    (bytes, objects)
}

pub(in crate::jobs) fn children(folder: &VfsPath, stop: &AtomicBool) -> Vec<Entry> {
    let mut entries = Vec::new();
    if let Ok(provider) = provider_for(folder) {
        // A folder that cannot be read counts for what was listed of it.
        let _ = provider.list(folder, stop, &mut |entry| entries.push(entry));
    }
    entries
}

/// Whether a link leads to a place not walked yet. Only a disk has links to follow.
pub(in crate::jobs) fn first_visit(link: &VfsPath, places: &mut HashSet<(u32, u64)>) -> bool {
    identity(link).is_some_and(|place| places.insert(place))
}

pub(in crate::jobs) fn identity(path: &VfsPath) -> Option<(u32, u64)> {
    file_identity(&exact_path(path.as_local()?)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::test_tree as tree;
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, Instant};

    fn size(root: &VfsPath) -> u64 {
        size_of(root, &AtomicBool::new(false), &mut |_, _| {}).0
    }

    fn junction(link: &Path, target: &Path) {
        let done = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(done.status.success());
    }

    fn local_file(path: &Path, bytes: usize) {
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    fn folder(name: &str, path: &VfsPath) -> Folder {
        Folder {
            name: name.to_string(),
            path: path.clone(),
        }
    }

    fn wait_over(count: &SizeCount) -> Counted {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let now = count.now();
            if now.over {
                return now;
            }
            assert!(Instant::now() < deadline, "the count did not end");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn every_file_below_is_added_up() {
        let root = tree::root("size-sum");
        let top = tree::dir(&root.join("top"));
        tree::write(&top.join("a"), b"12345");
        let inner = tree::dir(&top.join("inner"));
        tree::write(&inner.join("b"), b"123");
        assert_eq!(size(&top), 8);
    }

    #[test]
    fn a_link_behind_a_provider_adds_nothing() {
        let root = tree::root("size-link");
        let top = tree::dir(&root.join("top"));
        tree::write(&top.join("a"), b"12");
        tree::link(&top.join("l"));
        assert_eq!(size(&top), 2);
    }

    #[test]
    fn a_junction_is_walked_once_and_one_back_up_ends() {
        let temp = tempfile::tempdir().unwrap();
        let top = temp.path().join("top");
        let other = temp.path().join("other");
        fs::create_dir_all(&top).unwrap();
        fs::create_dir_all(&other).unwrap();
        local_file(&top.join("a"), 10);
        local_file(&other.join("b"), 100);
        junction(&top.join("first"), &other);
        junction(&top.join("second"), &other);
        junction(&other.join("back"), &top);

        assert_eq!(size(&VfsPath::local(&top)), 110);
    }

    #[test]
    fn folders_are_counted_in_turn_and_the_count_ends() {
        let root = tree::root("size-count");
        let (one, two) = (tree::dir(&root.join("1")), tree::dir(&root.join("2")));
        tree::write(&one.join("a"), b"1");
        tree::write(&two.join("b"), b"22");

        let count = SizeCount::start(vec![folder("1", &one), folder("2", &two)]);
        let done = wait_over(&count);

        assert_eq!(done.sizes, [("1".to_string(), 1), ("2".to_string(), 2)]);
        assert_eq!(done.finished, 2);
        assert_eq!(done.items, 2);
    }

    #[test]
    fn a_count_stopped_before_it_starts_adds_nothing() {
        let root = tree::root("size-stop");
        let top = tree::dir(&root.join("top"));
        tree::write(&top.join("a"), b"12");
        assert_eq!(
            size_of(&top, &AtomicBool::new(true), &mut |_, _| {}),
            (0, 0)
        );
    }

    #[test]
    fn a_count_stopped_midway_goes_no_deeper() {
        let root = tree::root("size-stop-midway");
        let top = tree::dir(&root.join("top"));
        for index in 0..COUNT_BATCH {
            tree::write(&top.join(&index.to_string()), b"1");
        }
        let inner = tree::dir(&top.join("inner"));
        tree::write(&inner.join("deep"), &[1; 1000]);
        let stop = AtomicBool::new(false);

        let (bytes, _) = size_of(&top, &stop, &mut |_, _| stop.store(true, Ordering::Relaxed));

        assert_eq!(bytes, COUNT_BATCH, "what is in `inner` was not reached");
    }

    #[test]
    fn a_stopped_thread_does_not_finish_its_folders() {
        let root = tree::root("size-stop-thread");
        let top = tree::dir(&root.join("top"));
        let shared = Shared::default();
        shared.stop.store(true, Ordering::Relaxed);

        count_folders(&[folder("top", &top)], &shared);

        let counted = shared.counted();
        assert_eq!((counted.finished, counted.over), (0, false));
    }

    #[test]
    fn the_size_is_told_as_it_grows() {
        let root = tree::root("size-grow");
        let top = tree::dir(&root.join("top"));
        for index in 0..COUNT_BATCH {
            tree::write(&top.join(&index.to_string()), b"1");
        }
        let mut told = Vec::new();
        size_of(&top, &AtomicBool::new(false), &mut |bytes, items| {
            told.push((bytes, items));
        });
        assert_eq!(told, [(COUNT_BATCH, COUNT_BATCH)]);
    }
}
