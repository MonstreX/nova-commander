//! Breaking a thread's synchronous I/O (ADR-0019 §4): a call blocked on a share that does not answer, a
//! `CreateFile` on a dead network path, a read of a stuck device or a pipe, fails at once with "The I/O
//! operation has been aborted" instead of holding its thread for minutes.

use std::os::windows::io::AsRawHandle;
use std::thread::JoinHandle;

use windows_sys::Win32::System::IO::CancelSynchronousIo;

/// Breaks the synchronous call `thread` is blocked in; nothing when it is in none or has ended. Calls
/// it cannot break (WNet's, the shell's, `LoadLibrary`) go on: their thread is let go (review R30).
pub fn break_thread_io<T>(thread: &JoinHandle<T>) {
    if thread.is_finished() {
        return;
    }
    // SAFETY: the handle is the thread's own, kept open by `thread` for the call; the call only asks the
    // system to fail the I/O that thread waits on.
    unsafe {
        CancelSynchronousIo(thread.as_raw_handle());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::{Duration, Instant};

    #[test]
    fn a_read_that_never_ends_is_broken() {
        let (mut reader, writer) = std::io::pipe().unwrap();
        let reading = std::thread::spawn(move || reader.read(&mut [0u8; 16]));
        let deadline = Instant::now() + Duration::from_secs(10);
        // Until the read is under way: a break before it is nothing.
        while !reading.is_finished() {
            assert!(Instant::now() < deadline, "the read was not broken");
            std::thread::sleep(Duration::from_millis(20));
            break_thread_io(&reading);
        }
        assert!(reading.join().unwrap().is_err(), "the read failed");
        drop(writer);
    }

    #[test]
    fn a_thread_that_ended_is_let_be() {
        let ended = std::thread::spawn(|| 7);
        while !ended.is_finished() {
            std::thread::yield_now();
        }
        break_thread_io(&ended);
        assert_eq!(ended.join().unwrap(), 7);
    }
}
