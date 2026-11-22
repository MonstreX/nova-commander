//! inotify on a folder, as FAR's `filesystemwatcher.cpp` on Windows: the folder's own entries (not its
//! subfolders), any change meaning "read the folder again", waited for on a thread of its own that also
//! notices when the owner stops watching.

use std::io;
use std::os::fd::OwnedFd;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::inotify::{self, CreateFlags};

use crate::platform::files::WatchFlags;

/// Windows' filter in inotify's terms: names made, removed and moved, data written, attributes (mode,
/// owner, times) changed, and the folder itself removed or moved. Not reads (`IN_ACCESS`): inotify tells
/// every read at once where Windows updates the access time lazily, and a viewer reading a file would
/// keep its panel re-reading.
const FILTER: inotify::WatchFlags = inotify::WatchFlags::CREATE
    .union(inotify::WatchFlags::DELETE)
    .union(inotify::WatchFlags::MOVED_FROM)
    .union(inotify::WatchFlags::MOVED_TO)
    .union(inotify::WatchFlags::MODIFY)
    .union(inotify::WatchFlags::CLOSE_WRITE)
    .union(inotify::WatchFlags::ATTRIB)
    .union(inotify::WatchFlags::DELETE_SELF)
    .union(inotify::WatchFlags::MOVE_SELF)
    .union(inotify::WatchFlags::ONLYDIR);

/// How often the waiting thread looks whether the owner stopped watching (Windows' too).
const STOP_CHECK: Timespec = Timespec {
    tv_sec: 0,
    tv_nsec: 200_000_000,
};

/// Room for a few events: which file changed does not matter, only that one did (an event takes at most
/// 16 bytes and a name of 255).
const BUFFER: usize = 4096;

/// Starts watching `dir`; a folder that cannot be watched (gone, not a folder, out of watches) fails at
/// once.
pub fn watch_dir(dir: &Path, flags: Arc<WatchFlags>) -> io::Result<()> {
    let watch = inotify::init(CreateFlags::CLOEXEC | CreateFlags::NONBLOCK)?;
    inotify::add_watch(&watch, dir, FILTER)?;
    thread::Builder::new()
        .name("folder watch".to_string())
        .spawn(move || wait_for_changes(&watch, &flags))?;
    Ok(())
}

/// Reports each batch of events as a change until the owner stops watching or the watch fails; the
/// descriptor closes with the thread.
fn wait_for_changes(watch: &OwnedFd, flags: &WatchFlags) {
    let mut buffer = [0_u8; BUFFER];
    while !flags.is_stopped() {
        let mut ready = [PollFd::new(watch, PollFlags::IN)];
        match poll(&mut ready, Some(&STOP_CHECK)) {
            Ok(0) | Err(rustix::io::Errno::INTR) => continue,
            Ok(_) => {}
            Err(_) => return,
        }
        // Everything waiting is read, so one burst of changes is one change.
        loop {
            match rustix::io::read(watch, &mut buffer[..]) {
                Ok(0) => break,
                Ok(_) => flags.change(),
                Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::AGAIN) => break,
                Err(_) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn changed_soon(watch: &crate::platform::files::DirWatch) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if watch.take_change() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn a_change_in_the_folder_is_told_once_and_a_read_is_not() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("a.txt");
        std::fs::write(&file, b"x").unwrap();
        let watch = crate::platform::files::watch_dir(temp.path()).unwrap();
        std::fs::write(temp.path().join("b.txt"), b"y").unwrap();
        assert!(changed_soon(&watch), "a new file");
        thread::sleep(Duration::from_millis(400));
        let _ = watch.take_change();
        let _ = std::fs::read(&file).unwrap();
        thread::sleep(Duration::from_millis(400));
        assert!(!watch.take_change(), "a read is no change");
        std::fs::remove_file(&file).unwrap();
        assert!(changed_soon(&watch), "a file removed");
    }

    #[test]
    fn a_subfolder_s_change_is_not_the_folder_s() {
        let temp = tempfile::tempdir().unwrap();
        let sub = temp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let watch = crate::platform::files::watch_dir(temp.path()).unwrap();
        std::fs::write(sub.join("inner.txt"), b"x").unwrap();
        thread::sleep(Duration::from_millis(400));
        assert!(!watch.take_change());
    }

    #[test]
    fn a_folder_that_is_not_there_cannot_be_watched() {
        let temp = tempfile::tempdir().unwrap();
        let missing = crate::platform::files::watch_dir(&temp.path().join("gone"));
        assert_eq!(missing.unwrap_err().kind(), io::ErrorKind::NotFound);
        let file = temp.path().join("file");
        std::fs::write(&file, b"").unwrap();
        assert!(
            crate::platform::files::watch_dir(&file).is_err(),
            "a file is no folder"
        );
    }
}
