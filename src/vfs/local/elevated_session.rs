//! The session's one elevated helper (ADR-0007 §3): started on the first approval, kept until the
//! program ends or the pipe breaks; and "Do not ask again in the current session".

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::Duration;

use super::elevated::{Client, ELEVATED_ARGUMENT, Request};
use crate::platform::elevation::{
    HelperProcess, PipeAccess, PipeListener, PipeStream, new_pipe_name, start_helper,
};
use crate::platform::files;

/// How long the helper gets to connect (FAR's `connect_pipe_to_process`).
const CONNECT_WAIT: Duration = Duration::from_secs(15);
/// How often a request waiting for the helper looks whether it was cancelled.
const BUSY_POLL: Duration = Duration::from_millis(20);

/// "Do not ask again in the current session", with the answer it was ticked with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Ask,
    Always,
    Never,
}

fn policy_code(policy: Policy) -> u8 {
    match policy {
        Policy::Ask => 0,
        Policy::Always => 1,
        Policy::Never => 2,
    }
}

fn policy_of(code: u8) -> Policy {
    match code {
        1 => Policy::Always,
        2 => Policy::Never,
        _ => Policy::Ask,
    }
}

#[cfg(not(test))]
static POLICY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

#[cfg(not(test))]
pub fn policy() -> Policy {
    policy_of(POLICY.load(std::sync::atomic::Ordering::Relaxed))
}

#[cfg(not(test))]
pub fn set_policy(policy: Policy) {
    POLICY.store(policy_code(policy), std::sync::atomic::Ordering::Relaxed);
}

// Tests run side by side, on threads of their own: each keeps its own answer.
#[cfg(test)]
thread_local! {
    static POLICY: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub fn policy() -> Policy {
    policy_of(POLICY.get())
}

#[cfg(test)]
pub fn set_policy(policy: Policy) {
    POLICY.set(policy_code(policy));
}

/// The helper could not be started, as opposed to a request it failed.
#[derive(Debug)]
struct NotStarted(io::Error);

impl std::fmt::Display for NotStarted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for NotStarted {}

/// `error` as a failure to start the helper.
pub(crate) fn not_started(error: io::Error) -> io::Error {
    io::Error::new(error.kind(), NotStarted(error))
}

/// Whether the helper could not be started (a refused prompt, one that never connected): the object
/// fails with its own error, and the operation does not try again (ADR-0007 §3).
pub fn is_not_started(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<NotStarted>())
}

struct Running {
    process: HelperProcess,
    client: Client<PipeStream>,
}

/// One request at a time, as FAR's `m_CS`: a copy holds the helper until it ends.
static HELPER: Mutex<Option<Running>> = Mutex::new(None);

/// Has `request` carried out by the helper, starting it if needed; only one at a time (FAR's `m_CS`).
/// While the helper is busy, `progress(0, 0)` is asked whether to go on waiting: false ends as
/// `Interrupted`, the request not sent.
pub fn call(request: &Request, progress: &mut dyn FnMut(u64, u64) -> bool) -> io::Result<()> {
    let helper = loop {
        match HELPER.try_lock() {
            Ok(helper) => break helper,
            Err(TryLockError::Poisoned(poisoned)) => break poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => {
                if !progress(0, 0) {
                    return Err(io::ErrorKind::Interrupted.into());
                }
                std::thread::sleep(BUSY_POLL);
            }
        }
    };
    if !progress(0, 0) {
        return Err(io::ErrorKind::Interrupted.into());
    }
    with_helper(helper, request, progress)
}

/// Has `request` carried out now, or fails if the helper is busy: for work the keys wait for.
fn call_now(request: &Request) -> io::Result<()> {
    let helper = match HELPER.try_lock() {
        Ok(helper) => helper,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            return Err(io::Error::new(
                io::ErrorKind::ResourceBusy,
                "the helper with the administrator's rights is busy; try again when it ends",
            ));
        }
    };
    with_helper(helper, request, &mut |_, _| true)
}

fn with_helper(
    mut helper: MutexGuard<'_, Option<Running>>,
    request: &Request,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> io::Result<()> {
    let running = match helper.take().filter(|running| running.process.is_running()) {
        Some(running) => running,
        None => start().map_err(not_started)?,
    };
    let running = helper.insert(running);
    let result = running.client.call(request, progress);
    if result.as_ref().is_err_and(|error| broke(error.kind())) {
        *helper = None;
    }
    result
}

fn broke(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::InvalidData
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
    )
}

fn start() -> io::Result<Running> {
    let name = new_pipe_name();
    let listener = PipeListener::create(&name, PipeAccess::Administrators)?;
    let arguments = format!("{ELEVATED_ARGUMENT} {name} {}", std::process::id());
    let process = start_helper(&arguments)?;
    let stream =
        listener.accept_while(process.process_id(), CONNECT_WAIT, &|| process.is_running())?;
    Ok(Running {
        process,
        client: Client::new(stream),
    })
}

/// The program ends: the helper is told to end too (FAR's `C_SERVICE_EXIT`).
pub fn end() {
    let helper = HELPER.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(running) = helper {
        // A helper that is gone needs no telling, and the program is ending anyway.
        let _ = running.client.exit();
    }
}

/// F7 through the helper: `parent/name`, the folders missing on the way made first, as `ops::make_dir`
/// makes them; nothing is made if the name is taken.
pub fn make_dir(parent: &Path, name: &str) -> io::Result<PathBuf> {
    make_dir_with(parent, name, &mut call_now)
}

fn make_dir_with(
    parent: &Path,
    name: &str,
    call: &mut dyn FnMut(&Request) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let path = parent.join(name.trim());
    let mut missing = Vec::new();
    let mut folder = files::exact_path(&path);
    while !folder.exists() {
        missing.push(folder.clone());
        if !folder.pop() {
            break;
        }
    }
    if missing.is_empty() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    for folder in missing.into_iter().rev() {
        call(&Request::CreateDirectory(folder))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::elevation::connect_pipe;
    use crate::vfs::local::elevated::{MAX_FRAME, serve};
    use std::cell::RefCell;
    use std::io::{Cursor, Read, Write};
    use std::rc::Rc;

    /// `input` to read, one byte per read if asked, as a pipe may hand them out; what is written is kept.
    struct Wire {
        input: Cursor<Vec<u8>>,
        written: Rc<RefCell<Vec<u8>>>,
        one_byte_at_a_time: bool,
    }

    impl Read for Wire {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = buffer.len().min(if self.one_byte_at_a_time {
                1
            } else {
                usize::MAX
            });
            self.input.read(&mut buffer[..count])
        }
    }

    impl Write for Wire {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.written.borrow_mut().extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn wire(input: Vec<u8>, one_byte_at_a_time: bool) -> (Wire, Rc<RefCell<Vec<u8>>>) {
        let written: Rc<RefCell<Vec<u8>>> = Rc::default();
        let wire = Wire {
            input: Cursor::new(input),
            written: Rc::clone(&written),
            one_byte_at_a_time,
        };
        (wire, written)
    }

    fn frame_of(request: &Request) -> Vec<u8> {
        let (stream, written) = wire(Vec::new(), false);
        // No answer comes: only what was written matters.
        let _ = Client::new(stream).call(request, &mut |_, _| true);
        written.take()
    }

    #[test]
    fn a_frame_that_arrives_byte_by_byte_is_served() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("victim.txt");
        std::fs::write(&file, "x").unwrap();
        let (mut stream, _) = wire(frame_of(&Request::DeleteFile(file.clone())), true);
        serve(&mut stream).unwrap();
        assert!(
            !file.exists(),
            "the frame was whole: the request was carried out"
        );
    }

    // Two paths and a flag make a frame of odd length, so MAX_FRAME + 1 can be asked for exactly.
    #[test]
    fn a_request_over_the_frames_limit_is_refused_before_anything_is_written() {
        const OVERHEAD: usize = 25; // Version, kind, two lengths, `Z:\` and `Z:\b` in UTF-16, the flag.
        let moving = |units: usize| Request::MoveFile {
            from: PathBuf::from(format!(r"Z:\{}", "a".repeat(units))),
            to: PathBuf::from(r"Z:\b"),
            replace: false,
        };
        let fits = frame_of(&moving((MAX_FRAME - 1 - OVERHEAD) / 2));
        assert_eq!(fits.len() - 4, MAX_FRAME - 1, "the arithmetic of this test");

        let (stream, written) = wire(Vec::new(), false);
        let too_long = moving((MAX_FRAME + 1 - OVERHEAD) / 2);
        let error = Client::new(stream)
            .call(&too_long, &mut |_, _| true)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidFilename, "{error:?}");
        assert!(written.borrow().is_empty(), "no part of it goes out");
    }

    // A helper that read the request and went away: over a real pipe, whose close must read as the
    // end of the stream.
    #[test]
    fn a_helper_that_closes_mid_request_is_a_broken_pipe() {
        let pid = std::process::id();
        let name = format!("vc-elevated-closing-pipe-test-{pid}");
        let listener = PipeListener::create(&name, PipeAccess::CurrentUser).unwrap();
        let helper = std::thread::spawn(move || {
            let mut stream = connect_pipe(&name, pid).unwrap();
            let mut header = [0u8; 4];
            stream.read_exact(&mut header).unwrap();
            let mut body = vec![0u8; u32::from_le_bytes(header) as usize];
            stream.read_exact(&mut body).unwrap();
        });
        let stream = listener
            .accept_while(pid, Duration::from_secs(30), &|| true)
            .unwrap();
        let request = Request::DeleteFile(PathBuf::from(r"Z:\a"));
        let error = Client::new(stream)
            .call(&request, &mut |_, _| true)
            .unwrap_err();
        helper.join().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe, "{error:?}");
    }

    #[test]
    fn a_test_never_starts_the_helper() {
        let error = call(
            &Request::CreateDirectory(PathBuf::from(r"C:\never")),
            &mut |_, _| true,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
        assert!(is_not_started(&error), "a start that failed says so");
        assert!(!is_not_started(&io::Error::from_raw_os_error(5)));
    }

    #[test]
    fn a_cancelled_request_is_not_sent() {
        let error = call(
            &Request::CreateDirectory(PathBuf::from(r"C:\never")),
            &mut |_, _| false,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn f7_makes_the_missing_folders_from_the_top_and_keeps_what_is_there() {
        let temp = tempfile::tempdir().unwrap();
        let mut made = Vec::new();
        let path = make_dir_with(temp.path(), r" a\b ", &mut |request| {
            let Request::CreateDirectory(folder) = request else {
                panic!("{request:?}");
            };
            made.push(folder.clone());
            std::fs::create_dir(folder)
        })
        .unwrap();
        assert_eq!(path, temp.path().join(r"a\b"));
        assert_eq!(
            made,
            [
                files::exact_path(&temp.path().join("a")),
                files::exact_path(&temp.path().join(r"a\b"))
            ]
        );
        let again = make_dir_with(temp.path(), r"a\b", &mut |request| {
            panic!("nothing to make: {request:?}")
        })
        .unwrap_err();
        assert_eq!(again.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn the_answer_for_the_session_is_kept() {
        assert_eq!(policy(), Policy::Ask);
        set_policy(Policy::Never);
        assert_eq!(policy(), Policy::Never);
        set_policy(Policy::Ask);
    }
}
