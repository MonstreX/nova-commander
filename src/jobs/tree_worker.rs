//! The tree panel's worker (ADR-0014): one thread per open tree reads its disk folder by folder, the most
//! wanted first, while the tree is in use. Wanted first: the folder under the cursor (`Focus`), the
//! folders from the root to the other panel's folder and those an operation touched (`Follow`,
//! `Check`), the other panel's folder's subtree, then the rest of the disk to its end. A folder read
//! before is listed again only when its last write time changed (a folder made, deleted or renamed in
//! it); a folder gone takes what was under it along. Links to folders are walked into as FAR's "Scan
//! symbolic links" does, never into a folder they are inside of. What is known goes to the screen as it
//! comes (laid out here) and to the disk's file (`tree_cache`) as it goes and when the worker stops.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use super::folder_size::identity;
use super::tree::{TreeItem, make_items};
use super::tree_cache::{TreeCache, Unsaved};
use super::tree_nodes::{Nodes, is_under};
use crate::platform::files::{exact_path, fold_case};
use crate::vfs::{VfsPath, provider_for};

/// How often what is known goes to the screen while the worker reads, at least.
const SHOW_EVERY: Duration = Duration::from_millis(300);
/// How often what is known goes to the file while the worker reads.
const SAVE_EVERY: Duration = Duration::from_secs(5);
/// How long the program's end waits for the last saves of the trees closed (a big disk's file takes
/// its time).
pub const LAST_SAVES: Duration = Duration::from_secs(3);
/// How long an idle worker waits for an order before it looks at its stop.
const IDLE: Duration = Duration::from_millis(200);
/// A folder's time this new is not kept: a folder made in it within the same tick of its file system's
/// clock (2 s on FAT) would not change it.
const FRESH: Duration = Duration::from_secs(3);

/// The queues, the most wanted first.
const FOCUS: usize = 0;
const PATH: usize = 1;
const UNDER: usize = 2;
const REST: usize = 3;

/// The trees' files and FAR's "Minimum number of folders" a file is kept for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keeping {
    pub cache: TreeCache,
    pub min: usize,
}

/// What the tree asks of its worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    /// The other panel's folder: it and the folders above it first, then its subtree.
    Follow(PathBuf),
    /// The folder under the cursor: its folders, when not known yet.
    Focus(PathBuf),
    /// Folders to list again now (an operation ended, a folder is gone).
    Check(Vec<PathBuf>),
    /// Ctrl+R: every folder listed again.
    Reread,
}

/// What the tree shows: its lines, whether the worker is still reading, how many folders are known,
/// and the file it could not save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub items: Vec<TreeItem>,
    pub reading: bool,
    pub count: usize,
    pub unsaved: Option<Unsaved>,
}

/// A tree's worker; dropping it stops it (its file saved).
pub struct TreeWorker {
    /// Dropped first when the worker is: an idle worker wakes at once.
    orders: Option<Sender<Order>>,
    snapshots: Receiver<Snapshot>,
    stop: Arc<AtomicBool>,
    done: Receiver<()>,
}

impl std::fmt::Debug for TreeWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("TreeWorker").finish_non_exhaustive()
    }
}

/// The workers of trees closed, still saving: the program's end waits for them (`wait_for_saves`).
static CLOSING: Mutex<Vec<Receiver<()>>> = Mutex::new(Vec::new());

impl Drop for TreeWorker {
    /// The worker stops and saves its file on its own: closing a tree does not hold the screen.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        drop(self.orders.take());
        let (_, nothing) = mpsc::channel();
        let done = std::mem::replace(&mut self.done, nothing);
        if let Ok(mut closing) = CLOSING.lock() {
            closing.retain(|other| matches!(other.try_recv(), Err(TryRecvError::Empty)));
            closing.push(done);
        }
    }
}

/// The program's end: the last saves of the trees closed, all within `limit` (a slow folder holding
/// a worker longer leaves its file as it was).
pub fn wait_for_saves(limit: Duration) {
    let deadline = Instant::now() + limit;
    let closing = CLOSING
        .lock()
        .map(|mut closing| std::mem::take(&mut *closing))
        .unwrap_or_default();
    for done in closing {
        let _ = done.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    }
}

impl TreeWorker {
    /// Starts reading the tree of `root`, from its file when there is one.
    pub fn start(root: PathBuf, keeping: Option<Keeping>) -> Self {
        let (orders, ordered) = mpsc::channel();
        let (shown, snapshots) = mpsc::channel();
        let (finished, done) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let spawned = thread::Builder::new()
            .name("tree".to_string())
            .spawn(move || {
                let mut work = Work::new(root, keeping);
                work.run(&ordered, &shown, &stopped);
                let _ = finished.send(());
            });
        // A thread that cannot start reads nothing: the tree keeps its root alone.
        drop(spawned);
        Self {
            orders: Some(orders),
            snapshots,
            stop,
            done,
        }
    }

    pub fn send(&self, order: Order) {
        if let Some(orders) = &self.orders {
            // A worker gone has nothing left to do.
            let _ = orders.send(order);
        }
    }

    /// The last of what the worker sent since it was asked last, with an earlier one's "Cannot save"
    /// when the last has none (review R27: the notice is not lost in the merge).
    pub fn latest(&self) -> Option<Snapshot> {
        latest_of(self.snapshots.try_iter())
    }
}

/// The last of `snapshots`, keeping the first "Cannot save" of them.
fn latest_of(snapshots: impl Iterator<Item = Snapshot>) -> Option<Snapshot> {
    snapshots.reduce(|earlier, mut later| {
        later.unsaved = later.unsaved.or(earlier.unsaved);
        later
    })
}

/// The worker's state.
pub(super) struct Work {
    nodes: Nodes,
    queues: [VecDeque<(PathBuf, bool)>; 4],
    /// The folders looked at since the worker started (or since Ctrl+R).
    visited: HashSet<String>,
    /// The key and the path of the other panel's folder.
    follow: Option<(String, PathBuf)>,
    /// Whether each folder listed in its parent is a link (FAR's loop guard needs no more look).
    links: HashMap<String, bool>,
    keeping: Option<Keeping>,
    /// The file could not be written: said once, not tried again.
    unsaved: Option<Unsaved>,
    failed: bool,
    /// Since the last snapshot and the last save.
    shown: bool,
    saved: bool,
    /// What the last snapshot said of the reading.
    reading: bool,
    /// The folders listed (tests).
    pub(super) listed: usize,
    places: HashMap<PathBuf, Option<(u32, u64)>>,
}

impl Work {
    pub(super) fn new(root: PathBuf, keeping: Option<Keeping>) -> Self {
        let kept = keeping
            .as_ref()
            .and_then(|keeping| keeping.cache.load(&root));
        let nodes = kept.unwrap_or_else(|| Nodes::new(&root));
        let mut work = Self {
            nodes,
            queues: Default::default(),
            visited: HashSet::new(),
            follow: None,
            links: HashMap::new(),
            keeping,
            unsaved: None,
            failed: false,
            shown: false,
            saved: true,
            reading: false,
            listed: 0,
            places: HashMap::new(),
        };
        work.queue_all();
        work
    }

    /// The root first, then every folder known, the nearest first.
    fn queue_all(&mut self) {
        let root = self.nodes.root().to_path_buf();
        self.queues[PATH].push_back((root, false));
        for folder in self.nodes.by_depth() {
            self.queues[REST].push_back((folder, false));
        }
    }

    fn run(&mut self, orders: &Receiver<Order>, shown: &Sender<Snapshot>, stop: &AtomicBool) {
        let _ = shown.send(self.snapshot(true));
        let mut last_shown = Instant::now();
        let mut last_saved = Instant::now();
        while !stop.load(Ordering::SeqCst) {
            match self.next() {
                Some((folder, force)) => {
                    while let Ok(order) = orders.try_recv() {
                        self.take(order);
                    }
                    self.visit(&folder, force, stop);
                }
                None => {
                    self.idle(shown);
                    match orders.recv_timeout(IDLE) {
                        Ok(order) => self.take(order),
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    continue;
                }
            }
            if self.shown && last_shown.elapsed() >= SHOW_EVERY {
                let started = Instant::now();
                let _ = shown.send(self.snapshot(true));
                // A big tree is laid out less often.
                last_shown = Instant::now() + started.elapsed() * 4;
            }
            if !self.saved && last_saved.elapsed() >= SAVE_EVERY {
                self.save();
                last_saved = Instant::now();
            }
        }
        self.save();
    }

    /// Nothing left to read: the screen and the file get what is known.
    fn idle(&mut self, shown: &Sender<Snapshot>) {
        self.save();
        if self.shown || self.reading {
            let _ = shown.send(self.snapshot(false));
        }
    }

    fn save(&mut self) {
        if self.saved || self.failed {
            return;
        }
        self.saved = true;
        if let Some(keeping) = &self.keeping
            && let Err(unsaved) = keeping.cache.store(&self.nodes, keeping.min)
        {
            self.unsaved = Some(unsaved);
            self.failed = true;
            self.shown = true;
        }
    }

    pub(super) fn snapshot(&mut self, reading: bool) -> Snapshot {
        self.shown = false;
        self.reading = reading;
        Snapshot {
            items: make_items(self.nodes.root(), self.nodes.folders()),
            reading,
            count: self.nodes.len() - 1,
            unsaved: self.unsaved.take(),
        }
    }

    /// The most wanted folder not looked at yet.
    pub(super) fn next(&mut self) -> Option<(PathBuf, bool)> {
        for queue in &mut self.queues {
            while let Some((folder, force)) = queue.pop_front() {
                let Some(key) = self.nodes.key(&folder) else {
                    continue;
                };
                if force || !self.visited.contains(&key) {
                    return Some((folder, force));
                }
            }
        }
        None
    }

    pub(super) fn take(&mut self, order: Order) {
        match order {
            Order::Follow(folder) => self.follow(folder),
            Order::Focus(folder) => self.queues[FOCUS].push_back((folder, false)),
            Order::Check(folders) => {
                for folder in folders {
                    self.queues[PATH].push_front((folder, true));
                }
            }
            Order::Reread => {
                self.nodes.forget_times();
                self.visited.clear();
                self.queues = Default::default();
                self.queue_all();
                // The other panel's folder first again, in the tree or not yet.
                if let Some((_, folder)) = self.follow.take() {
                    self.follow(folder);
                }
            }
        }
    }

    /// The other panel's folder: the folders from the root to it first, then those known under it and
    /// not looked at yet.
    fn follow(&mut self, folder: PathBuf) {
        let Some(key) = self.nodes.key(&folder) else {
            return;
        };
        let mut path: Vec<PathBuf> = (folder.ancestors())
            .take_while(|above| self.nodes.key(above).is_some())
            .map(Path::to_path_buf)
            .collect();
        path.reverse();
        for above in path {
            self.queues[PATH].push_back((above, false));
        }
        if self
            .follow
            .as_ref()
            .is_some_and(|(followed, _)| *followed == key)
        {
            return;
        }
        let visited = &self.visited;
        let under: Vec<PathBuf> = (self.nodes.under(&key).into_iter())
            .filter(|(known, _)| !visited.contains(known))
            .map(|(_, path)| path)
            .collect();
        self.queues[UNDER].extend(under.into_iter().map(|known| (known, false)));
        self.follow = Some((key, folder));
    }

    /// Looks at one folder: gone, it leaves; changed or not read, it is listed.
    pub(super) fn visit(&mut self, folder: &Path, force: bool, stop: &AtomicBool) {
        let Some(key) = self.nodes.key(folder) else {
            return;
        };
        self.visited.insert(key.clone());
        let time = match look(folder) {
            Look::There(time) => time,
            Look::Gone => {
                if self.nodes.remove(folder) {
                    self.changed();
                }
                return;
            }
            // Not to be read now (no rights, a device not ready): what is known stays.
            Look::Unknown => return,
        };
        if self.nodes.insert(folder) {
            self.changed();
        }
        // The folder as the tree spells it: what is listed in it hangs there.
        let Some((folder, read)) =
            (self.nodes.get(folder)).map(|node| (node.path.clone(), node.read))
        else {
            return;
        };
        let folder = folder.as_path();
        if !force && time.is_some() && read == time {
            self.queue_children(folder);
            return;
        }
        if self.leads_back(folder, &key) {
            return;
        }
        let Some(listing) = list(folder, stop) else {
            return;
        };
        self.listed += 1;
        self.take_listing(folder, &listing);
        // Of a listing cut short what is known stays, and the folder is read again next time.
        if listing.whole {
            let settled = time.filter(|time| (time.elapsed()).is_ok_and(|age| age >= FRESH));
            self.nodes.set_read(folder, settled);
        }
        self.changed();
        self.queue_children(folder);
    }

    /// The folders listed in `folder` in place of those known: one renamed in its case only takes its
    /// new name; of a listing cut short none known leaves.
    fn take_listing(&mut self, folder: &Path, listing: &Listing) {
        let listed: HashMap<String, &String> = (listing.folders.iter())
            .map(|(name, _)| (fold_case(name).into_owned(), name))
            .collect();
        for known in self.nodes.children(folder) {
            let own = known
                .file_name()
                .map(|name| name.to_string_lossy().into_owned());
            match own.as_ref().and_then(|own| listed.get(&*fold_case(own))) {
                Some(name) if own.as_ref() != Some(*name) => {
                    self.nodes.respell(&known, &folder.join(name));
                }
                Some(_) => {}
                None if listing.whole => {
                    self.nodes.remove(&known);
                }
                None => {}
            }
        }
        for (name, link) in &listing.folders {
            let path = folder.join(name);
            self.nodes.insert(&path);
            if let Some(key) = self.nodes.key(&path) {
                self.links.insert(key, *link);
            }
        }
    }

    /// The folders in `folder` not looked at yet: the other panel's subtree first.
    fn queue_children(&mut self, folder: &Path) {
        for child in self.nodes.children(folder) {
            let Some(key) = self.nodes.key(&child) else {
                continue;
            };
            if self.visited.contains(&key) {
                continue;
            }
            let under = (self.follow.as_ref()).is_some_and(|(follow, _)| is_under(&key, follow));
            let queue = if under { UNDER } else { REST };
            self.queues[queue].push_back((child, false));
        }
    }

    /// A link to a folder `folder` is inside of (FAR's loop guard): not walked into. Whether it is a
    /// link its parent's listing told, else it is looked at.
    fn leads_back(&mut self, folder: &Path, key: &str) -> bool {
        let link = self.links.get(key).copied().unwrap_or_else(|| {
            std::fs::symlink_metadata(exact_path(folder))
                .is_ok_and(|meta| meta.file_type().is_symlink())
        });
        if !link {
            return false;
        }
        let place = identity(&VfsPath::local(folder));
        if place.is_none() {
            return false;
        }
        let root = self.nodes.root().to_path_buf();
        (folder.ancestors().skip(1))
            .take_while(|above| above.starts_with(&root))
            .any(|above| {
                let above = above.to_path_buf();
                *self
                    .places
                    .entry(above.clone())
                    .or_insert_with(|| identity(&VfsPath::local(&above)))
                    == place
            })
    }

    fn changed(&mut self) {
        self.shown = true;
        self.saved = false;
    }
}

/// What a folder is now.
enum Look {
    /// A folder, with its last write time (none: the file system gives none).
    There(Option<SystemTime>),
    /// Not there, or no folder.
    Gone,
    /// Cannot be told now (no rights, a device not ready, a network error).
    Unknown,
}

/// The folder as its own name says, not as Win32 would trim it (`d.`, `d `).
fn look(folder: &Path) -> Look {
    match std::fs::metadata(exact_path(folder)) {
        Ok(meta) if meta.is_dir() => Look::There(meta.modified().ok()),
        Ok(_) => Look::Gone,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Look::Gone,
        Err(_) => Look::Unknown,
    }
}

/// The folders listed in a folder, each with whether it is a link to elsewhere, and whether the
/// listing went to its end.
struct Listing {
    folders: Vec<(String, bool)>,
    whole: bool,
}

/// The folders in `folder`; none when there is no provider or the worker stops.
fn list(folder: &Path, stop: &AtomicBool) -> Option<Listing> {
    let path = VfsPath::local(folder);
    let provider = provider_for(&path).ok()?;
    let mut folders = Vec::new();
    // A folder that cannot be read to its end gives what was listed of it.
    let whole = (provider.list(&path, stop, &mut |entry| {
        if entry.is_dir && !entry.is_parent {
            let link = entry.points_elsewhere();
            folders.push((entry.name, link));
        }
    }))
    .is_ok();
    (!stop.load(Ordering::SeqCst)).then_some(Listing { folders, whole })
}

#[cfg(test)]
#[path = "tree_worker_tests.rs"]
mod tests;
