//! A limit on the threads left on a disk that does not answer (review R25). A reading nobody waits for
//! any more (Esc, a panel gone elsewhere) cannot always be broken while its call hangs on a dead share:
//! its thread stays until the call returns. Each Ctrl+R, look of the information panel, quick view or
//! count would leave one more; once [`LIMIT`] threads left on one drive, share or plugin session have
//! hung there past [`HUNG`], the next reading of it fails at once ("does not answer") instead of leaving
//! another. Readings waited for are never counted, nor those let go that end soon (a disk that answers):
//! any number may run at once.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use crate::vfs::VfsPath;

/// How many threads left hung on one disk may still run before its next reading is refused.
pub const LIMIT: usize = 4;

/// How long a thread let go may still run before it counts as hung on its disk.
pub const HUNG: Duration = Duration::from_secs(3);

/// The threads let go on each disk, with when.
type Left = HashMap<VfsPath, Vec<(Arc<Shared>, Instant)>>;

/// Threads left on each disk and not ended yet, each with when it was let go (a disk is the root of its
/// paths). Every change of a thread's state is made under this lock.
fn left_on_disks() -> MutexGuard<'static, Left> {
    static LEFT: OnceLock<Mutex<Left>> = OnceLock::new();
    LEFT.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
struct Shared {
    disk: VfsPath,
    /// Its owner let it go before it ended.
    left: AtomicBool,
    ended: AtomicBool,
}

/// The thread's side: dropped when its work is over.
#[derive(Debug)]
pub struct Slot(Arc<Shared>);

/// The side of whoever waits for the thread: dropped when nobody does any more.
#[derive(Debug)]
pub struct Waited(Arc<Shared>);

impl Drop for Slot {
    fn drop(&mut self) {
        let mut left = left_on_disks();
        self.0.ended.store(true, Ordering::Relaxed);
        if self.0.left.load(Ordering::Relaxed)
            && let Some(threads) = left.get_mut(&self.0.disk)
        {
            threads.retain(|(thread, _)| !Arc::ptr_eq(thread, &self.0));
            if threads.is_empty() {
                left.remove(&self.0.disk);
            }
        }
    }
}

impl Drop for Waited {
    fn drop(&mut self) {
        let mut left = left_on_disks();
        if !self.0.ended.load(Ordering::Relaxed) {
            self.0.left.store(true, Ordering::Relaxed);
            let disk = self.0.disk.clone();
            let thread = (Arc::clone(&self.0), Instant::now());
            left.entry(disk).or_default().push(thread);
        }
    }
}

/// A thread for work on `path`'s disk: its side and the waiting one's; an error when [`LIMIT`] threads
/// let go on that disk have not ended within [`HUNG`] of it (a reading let go is stopped and its call
/// broken: on a disk that answers it ends at once, however long it had read).
pub fn take(path: &VfsPath) -> io::Result<(Slot, Waited)> {
    let disk = path.root();
    let left = left_on_disks();
    let hung = (left.get(&disk).into_iter().flatten())
        .filter(|(_, let_go)| let_go.elapsed() >= HUNG)
        .count();
    if hung >= LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("{disk} does not answer: its earlier readings have not ended"),
        ));
    }
    let shared = Arc::new(Shared {
        disk,
        left: AtomicBool::new(false),
        ended: AtomicBool::new(false),
    });
    Ok((Slot(Arc::clone(&shared)), Waited(shared)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn left(path: &VfsPath) -> usize {
        left_on_disks().get(&path.root()).map_or(0, Vec::len)
    }

    /// A thread let go on `path`'s disk; `hung`: let go long enough ago to count as hung there.
    fn left_one(path: &VfsPath, hung: bool) -> Slot {
        let (slot, waited) = take(path).unwrap();
        drop(waited);
        if hung && let Some(threads) = left_on_disks().get_mut(&path.root()) {
            for (thread, let_go) in threads.iter_mut() {
                if Arc::ptr_eq(thread, &slot.0) {
                    *let_go -= HUNG;
                }
            }
        }
        slot
    }

    #[test]
    fn readings_waited_for_are_never_refused() {
        let path = VfsPath::in_session("disk-threads-test", "waited", "/a");
        let running: Vec<_> = (0..LIMIT * 3).map(|_| take(&path).unwrap()).collect();
        assert_eq!(left(&path), 0);
        drop(running);
        assert_eq!(left(&path), 0, "ended or let go after: nothing left");
    }

    #[test]
    fn readings_let_go_that_end_soon_are_not_counted() {
        let path = VfsPath::in_session("disk-threads-test", "answers", "/a");
        let ending_soon: Vec<Slot> = (0..LIMIT * 2).map(|_| left_one(&path, false)).collect();
        assert!(take(&path).is_ok(), "let go just now: a disk that answers");
        drop(ending_soon);
        assert_eq!(left(&path), 0);
    }

    #[test]
    fn a_disk_with_so_many_threads_hung_refuses_the_next_until_one_ends() {
        let path = VfsPath::in_session("disk-threads-test", "dead", "/a/b");
        let mut hanging: Vec<Slot> = (0..LIMIT).map(|_| left_one(&path, true)).collect();
        assert_eq!(left(&path), LIMIT);
        let refused = take(&VfsPath::in_session("disk-threads-test", "dead", "/c")).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::TimedOut, "the same disk");
        assert!(
            take(&VfsPath::in_session("disk-threads-test", "alive", "/")).is_ok(),
            "another disk is not held by it"
        );
        hanging.pop();
        assert!(take(&path).is_ok(), "one ended: its place free");
        drop(hanging);
        assert_eq!(left(&path), 0, "forgotten once all ended");
    }
}
