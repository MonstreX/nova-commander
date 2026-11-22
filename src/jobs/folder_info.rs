//! Ctrl+Q on a folder: FAR's `GetDirInfo` as its quick view counts (`qview.cpp`, `dirinfo.cpp`), on a
//! thread of its own so the keys keep working (ADR-0015; FAR's is modal with a progress window): the
//! folders and files under it, their bytes, the space they take on the disk, the cluster's size, the
//! files' slack (space taken past their ends, a cluster or more) and what small files take in the
//! file system's table (MFT overhead: less than a cluster). Links to folders are walked as F3 walks
//! them (`folder_size`): the place a link leads to only once. A folder that is a link tells what it is
//! and where it leads first.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::folder_size::{children, first_visit, identity};
use crate::platform;
use crate::platform::files::LinkKind;
use crate::vfs::{Entry, VfsPath};

/// What is counted is published after this many objects.
const COUNT_BATCH: u64 = 256;

/// FAR's `DirInfoData`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FolderInfo {
    pub folders: u64,
    pub files: u64,
    pub bytes: u64,
    pub allocated: u64,
    /// The disk's cluster in bytes; none inside a provider (an archive) or when the system does not
    /// say: no cluster, slack or overhead then (FAR's `plugin_ok`).
    pub cluster: Option<u64>,
    pub slack: u64,
    pub mft_overhead: u64,
}

impl FolderInfo {
    /// A file found: its bytes and, when the disk tells, the space it takes.
    fn add_file(&mut self, entry: &Entry) {
        self.files += 1;
        if entry.points_elsewhere() {
            return;
        }
        self.bytes = self.bytes.saturating_add(entry.size);
        // FAR: a plugin's item without an allocation size takes its size.
        let allocated = entry.allocated.unwrap_or(entry.size);
        self.allocated = self.allocated.saturating_add(allocated);
        let past_end = allocated.saturating_sub(entry.size);
        if allocated >= self.cluster.unwrap_or(0) {
            self.slack = self.slack.saturating_add(past_end);
        } else {
            self.mft_overhead = self.mft_overhead.saturating_add(past_end);
        }
    }
}

/// A folder that is a link: its kind and, for a junction or a symbolic link, where it leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderLink {
    pub kind: LinkKind,
    pub target: Option<String>,
}

/// How far a count is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counted {
    pub info: FolderInfo,
    pub link: Option<FolderLink>,
    /// The whole folder walked.
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
pub struct InfoCount(Arc<Shared>, Option<crate::jobs::disk_threads::Waited>);

impl InfoCount {
    /// Counts `folder`. Without a thread for it the count is over at once, with nothing counted.
    pub fn start(folder: VfsPath) -> Self {
        let shared = Arc::new(Shared::default());
        let counting = Arc::clone(&shared);
        // A disk with threads left hanging on it gets no more (review R25): nothing counted.
        let Ok((slot, waited)) = crate::jobs::disk_threads::take(&folder) else {
            shared.counted().over = true;
            return Self(shared, None);
        };
        let started = std::thread::Builder::new()
            .name("folder info".to_string())
            .spawn(move || {
                let _slot = slot;
                count(&folder, &counting);
            });
        if started.is_err() {
            shared.counted().over = true;
        }
        Self(shared, Some(waited))
    }

    /// A count that stands where `counted` says (tests of the drawing).
    #[cfg(test)]
    pub fn at(counted: Counted) -> Self {
        let shared = Shared::default();
        *shared.counted() = counted;
        Self(Arc::new(shared), None)
    }

    pub fn now(&self) -> Counted {
        self.0.counted().clone()
    }
}

impl Drop for InfoCount {
    fn drop(&mut self) {
        self.0.stop.store(true, Ordering::Relaxed);
        // Nobody waits for it any more: its thread, if still on its disk, is counted as left there.
        let _ = self.1.take();
    }
}

/// The disk's cluster of a folder on a disk (FAR's `GetDiskFreeSpace` of its root).
fn cluster_of(folder: &VfsPath) -> Option<u64> {
    let local = folder.as_local()?;
    let root = local.ancestors().last()?;
    platform::volumes::cluster_size(&root.to_string_lossy())
}

/// What a folder on a disk is when it is a link (FAR's `GetReparsePointInfo`).
fn link_of(folder: &VfsPath) -> Option<FolderLink> {
    let local = folder.as_local()?;
    let kind = LinkKind::from_reparse_tag(platform::files::reparse_tag(local)?);
    let target = matches!(kind, LinkKind::Junction | LinkKind::Symlink)
        .then(|| platform::files::link_target(local).ok())
        .flatten()
        .map(|target| target.display().to_string());
    Some(FolderLink { kind, target })
}

fn count(root: &VfsPath, shared: &Shared) {
    shared.counted().link = link_of(root);
    let mut info = FolderInfo {
        cluster: cluster_of(root),
        ..FolderInfo::default()
    };
    let mut places: HashSet<(u32, u64)> = identity(root).into_iter().collect();
    let mut pending = vec![root.clone()];
    let mut objects = 0u64;
    while let Some(folder) = pending.pop() {
        if shared.stop.load(Ordering::Relaxed) {
            return;
        }
        for child in children(&folder, &shared.stop) {
            if child.is_dir {
                info.folders += 1;
                let path = folder.join(&child.name);
                if !child.points_elsewhere() || first_visit(&path, &mut places) {
                    pending.push(path);
                }
            } else {
                info.add_file(&child);
            }
            objects += 1;
            if objects.is_multiple_of(COUNT_BATCH) {
                shared.counted().info = info;
            }
        }
    }
    if !shared.stop.load(Ordering::Relaxed) {
        let mut counted = shared.counted();
        counted.info = info;
        counted.over = true;
    }
}

#[cfg(test)]
#[path = "folder_info_tests.rs"]
mod tests;
