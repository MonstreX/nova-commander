//! `ReadDirectoryChangesW` on a folder, as FAR's `filesystemwatcher.cpp`: one overlapped read at a time
//! into a small buffer (individual changes do not matter, an overflow is a change too), waited for on a
//! thread of its own that also notices when the owner stops watching.

use std::ffi::c_void;
use std::io;
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::thread;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_NOTIFY_ENUM_DIR, HANDLE, INVALID_HANDLE_VALUE,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_ATTRIBUTES, FILE_NOTIFY_CHANGE_CREATION, FILE_NOTIFY_CHANGE_DIR_NAME,
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_ACCESS, FILE_NOTIFY_CHANGE_LAST_WRITE,
    FILE_NOTIFY_CHANGE_SECURITY, FILE_NOTIFY_CHANGE_SIZE, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING, ReadDirectoryChangesW,
};
use windows_sys::Win32::System::IO::{
    CancelIoEx, GetOverlappedResult, GetOverlappedResultEx, OVERLAPPED,
};

use crate::platform::files::WatchFlags;

/// FAR's filter: every kind of change of the folder's entries.
const FILTER: u32 = FILE_NOTIFY_CHANGE_FILE_NAME
    | FILE_NOTIFY_CHANGE_DIR_NAME
    | FILE_NOTIFY_CHANGE_ATTRIBUTES
    | FILE_NOTIFY_CHANGE_SIZE
    | FILE_NOTIFY_CHANGE_LAST_WRITE
    | FILE_NOTIFY_CHANGE_LAST_ACCESS
    | FILE_NOTIFY_CHANGE_CREATION
    | FILE_NOTIFY_CHANGE_SECURITY;

/// How often the waiting thread looks whether the owner stopped watching.
const STOP_CHECK_MS: u32 = 200;

/// Opens `dir` and waits for the first read to be issued, so a folder that cannot be watched fails at
/// once. The reads are issued on the watch's own thread: the system cancels a thread's pending reads
/// when it ends, and the caller's thread (a listing) ends soon.
pub fn watch_dir(dir: &Path, flags: Arc<WatchFlags>) -> io::Result<()> {
    let mut watched = Watched::open(dir)?;
    let (started, start) = mpsc::channel();
    thread::Builder::new()
        .name("folder watch".to_string())
        .spawn(move || {
            let first = watched.read();
            let ok = first.is_ok();
            let _ = started.send(first);
            if ok {
                watched.wait_for_changes(&flags);
            }
        })?;
    start
        .recv()
        .unwrap_or_else(|_| Err(io::Error::other("the folder watch did not start")))
}

/// The folder's handle with its read in flight. The buffer and the `OVERLAPPED` are boxed: the system
/// writes to them until the read completes or is cancelled, wherever this value moves meanwhile.
struct Watched {
    handle: HANDLE,
    buffer: Box<[u32; 64]>,
    overlapped: Box<OVERLAPPED>,
    pending: bool,
}

// SAFETY: a file handle and the memory of its overlapped read may be used from any thread; only one
// thread owns this value at a time.
unsafe impl Send for Watched {}

impl Watched {
    fn open(dir: &Path) -> io::Result<Self> {
        let wide = super::wide_path(dir);
        // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call; the other arguments are
        // plain flags and null pointers allowed by the API.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle,
            buffer: Box::new([0; 64]),
            // SAFETY: OVERLAPPED is plain data for which all zeroes is the documented initial state.
            overlapped: Box::new(unsafe { std::mem::zeroed() }),
            pending: false,
        })
    }

    /// Issues the next read of changes.
    fn read(&mut self) -> io::Result<()> {
        let size = u32::try_from(std::mem::size_of_val(&*self.buffer)).unwrap_or(0);
        // SAFETY: the handle is open; the buffer and the OVERLAPPED are boxed, so they stay where they
        // are until the read completes or is cancelled in `Drop`; no completion routine.
        let ok = unsafe {
            ReadDirectoryChangesW(
                self.handle,
                self.buffer.as_mut_ptr().cast::<c_void>(),
                size,
                0,
                FILTER,
                std::ptr::null_mut(),
                &raw mut *self.overlapped,
                None,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        self.pending = true;
        Ok(())
    }

    /// Reports each completed read as a change until the owner stops watching or the folder cannot be
    /// watched any more.
    fn wait_for_changes(mut self, flags: &WatchFlags) {
        while !flags.is_stopped() {
            match self.completed() {
                Completed::Not => continue,
                Completed::Changes => flags.change(),
                Completed::Ended(gone) => {
                    // The handle goes first: a folder deleted while it is open is gone only then.
                    drop(self);
                    if gone {
                        flags.change();
                    }
                    return;
                }
            }
            if self.read().is_err() {
                return;
            }
        }
    }

    /// Waits a little for the read in flight.
    fn completed(&mut self) -> Completed {
        let mut bytes = 0u32;
        // SAFETY: the handle is open and the boxed OVERLAPPED belongs to the read in flight; `bytes` is
        // a valid out pointer.
        let ok = unsafe {
            GetOverlappedResultEx(
                self.handle,
                &raw const *self.overlapped,
                &raw mut bytes,
                STOP_CHECK_MS,
                0,
            )
        };
        if ok != 0 {
            self.pending = false;
            return Completed::Changes;
        }
        let error = io::Error::last_os_error().raw_os_error();
        match error.and_then(|code| u32::try_from(code).ok()) {
            Some(WAIT_TIMEOUT) => Completed::Not,
            Some(ERROR_NOTIFY_ENUM_DIR) => {
                self.pending = false;
                Completed::Changes
            }
            // The folder is being deleted (FAR's STATUS_DELETE_PENDING): its panel reads it again
            // and goes up.
            Some(ERROR_ACCESS_DENIED) => {
                self.pending = false;
                Completed::Ended(true)
            }
            _ => {
                self.pending = false;
                Completed::Ended(false)
            }
        }
    }
}

enum Completed {
    Not,
    Changes,
    /// The read failed; `true` when the folder is going away.
    Ended(bool),
}

impl Drop for Watched {
    fn drop(&mut self) {
        if self.pending {
            let mut bytes = 0u32;
            // SAFETY: the handle is open; the read in flight is cancelled and waited for before the
            // buffer and the OVERLAPPED are freed with this value.
            unsafe {
                CancelIoEx(self.handle, &raw const *self.overlapped);
                GetOverlappedResult(self.handle, &raw const *self.overlapped, &raw mut bytes, 1);
            }
        }
        // SAFETY: the handle was returned by CreateFileW and is closed exactly once, here. A failed close
        // leaves nothing to recover.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::platform::files::watch_dir;
    use std::time::{Duration, Instant};

    fn wait_for_change(watch: &crate::platform::files::DirWatch) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if watch.take_change() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn a_new_file_and_a_removed_one_are_changes_and_a_quiet_folder_has_none() {
        let temp = tempfile::tempdir().unwrap();
        let watch = watch_dir(temp.path()).unwrap();
        assert!(!watch.take_change());
        std::fs::write(temp.path().join("new.txt"), b"x").unwrap();
        assert!(wait_for_change(&watch));
        std::thread::sleep(Duration::from_millis(100));
        while watch.take_change() {}
        std::fs::remove_file(temp.path().join("new.txt")).unwrap();
        assert!(wait_for_change(&watch));
    }

    #[test]
    fn a_change_in_a_subfolder_is_not_the_folder_s() {
        let temp = tempfile::tempdir().unwrap();
        let sub = temp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let watch = watch_dir(temp.path()).unwrap();
        std::fs::write(sub.join("deep.txt"), b"x").unwrap();
        // Writing into `sub` may touch its own times, which the parent sees; nothing more. Quiet for
        // 300 ms first: on a slow machine that change comes later.
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(300));
            if !watch.take_change() {
                break;
            }
            while watch.take_change() {}
        }
        std::fs::write(sub.join("deep.txt"), b"xy").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(!watch.take_change(), "a file inside a subfolder");
    }

    #[test]
    fn the_watched_folder_deleted_is_a_change_and_is_gone() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("going");
        std::fs::create_dir(&dir).unwrap();
        let watch = watch_dir(&dir).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        assert!(wait_for_change(&watch));
        let deadline = Instant::now() + Duration::from_secs(10);
        while dir.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!dir.exists(), "the watch does not keep a deleted folder");
    }

    #[test]
    fn a_watch_outlives_the_thread_that_started_it() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().to_path_buf();
        let watch = std::thread::spawn(move || watch_dir(&dir).unwrap())
            .join()
            .unwrap();
        std::fs::write(temp.path().join("new.txt"), b"x").unwrap();
        assert!(wait_for_change(&watch));
    }

    #[test]
    fn a_missing_folder_cannot_be_watched() {
        let temp = tempfile::tempdir().unwrap();
        let error = watch_dir(&temp.path().join("missing")).err().unwrap();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }
}
