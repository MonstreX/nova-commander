//! Acceptance tests of the helper's named pipe and path form (ADR-0007 §4; 3d step 3): a real pipe
//! between two threads of the test, open to the current user only. Nothing here starts a process.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::panic;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Instant;

use super::*;

/// How long a test waits before it fails instead of hanging.
const WAIT: Duration = Duration::from_secs(30);

static NEXT_NAME: AtomicUsize = AtomicUsize::new(0);

/// Unique among the tests (and not made by `new_pipe_name`, which has its own test).
fn unique_name() -> String {
    let number = NEXT_NAME.fetch_add(1, Ordering::SeqCst);
    format!("vc-elevation-test-{}-{number}", process::id())
}

/// Runs `work` on its own thread; a pipe call that hangs fails the test instead of hanging it.
fn within<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, finished) = mpsc::channel();
    let worker = thread::spawn(move || {
        // The receiver is gone only when the test has already failed by the timeout.
        let _ = done.send(work());
    });
    match finished.recv_timeout(WAIT) {
        Ok(value) => value,
        Err(RecvTimeoutError::Timeout) => panic!("a pipe call hung"),
        Err(RecvTimeoutError::Disconnected) => match worker.join() {
            Ok(()) => panic!("the worker ended without a result"),
            Err(failure) => panic::resume_unwind(failure),
        },
    }
}

fn listen(name: &str) -> PipeListener {
    PipeListener::create(name, PipeAccess::CurrentUser).unwrap()
}

// Given a listener and a client of this process, then bytes flow both ways.
#[test]
fn bytes_flow_both_ways() {
    let (ping, pong) = within(|| {
        let (name, pid) = (unique_name(), process::id());
        let listener = listen(&name);
        let client = thread::spawn(move || {
            let mut stream = connect_pipe(&name, pid).unwrap();
            stream.write_all(b"ping").unwrap();
            stream.flush().unwrap();
            let mut pong = [0u8; 4];
            stream.read_exact(&mut pong).unwrap();
            pong
        });
        let mut server = listener.accept(pid, WAIT).unwrap();
        let mut ping = [0u8; 4];
        server.read_exact(&mut ping).unwrap();
        server.write_all(b"pong").unwrap();
        server.flush().unwrap();
        (ping, client.join().unwrap())
    });
    assert_eq!((&ping, &pong), (b"ping", b"pong"));
}

// `serve` reads a length and then the rest: a write can be read in pieces.
#[test]
fn a_write_can_be_read_in_pieces() {
    let (head, tail) = within(|| {
        let (name, pid) = (unique_name(), process::id());
        let listener = listen(&name);
        let client = thread::spawn(move || {
            let mut stream = connect_pipe(&name, pid).unwrap();
            stream.write_all(b"12345678").unwrap();
            stream.flush().unwrap();
            stream
        });
        let mut server = listener.accept(pid, WAIT).unwrap();
        let (mut head, mut tail) = ([0u8; 3], [0u8; 5]);
        server.read_exact(&mut head).unwrap();
        server.read_exact(&mut tail).unwrap();
        let _client_end = client.join().unwrap();
        (head, tail)
    });
    assert_eq!((&head, &tail), (b"123", b"45678"));
}

// More than a pipe buffer holds goes through whole.
#[test]
fn a_large_write_arrives_whole() {
    const SIZE: usize = 200_000;
    let received = within(|| {
        let (name, pid) = (unique_name(), process::id());
        let listener = listen(&name);
        let client = thread::spawn(move || {
            let mut stream = connect_pipe(&name, pid).unwrap();
            stream.write_all(&vec![9u8; SIZE]).unwrap();
            stream.flush().unwrap();
            stream
        });
        let mut server = listener.accept(pid, WAIT).unwrap();
        let mut received = vec![0u8; SIZE];
        server.read_exact(&mut received).unwrap();
        let _client_end = client.join().unwrap();
        received
    });
    assert!(received == vec![9u8; SIZE], "the bytes as written");
}

// When the other end closes, then reading ends cleanly after what was written: `serve` returns Ok
// when the program is gone only if the pipe says so as the end of the stream.
#[test]
fn the_other_ends_closing_is_the_end_of_the_stream() {
    let received = within(|| {
        let (name, pid) = (unique_name(), process::id());
        let listener = listen(&name);
        let client = thread::spawn(move || {
            let mut stream = connect_pipe(&name, pid).unwrap();
            stream.write_all(b"last").unwrap();
            stream.flush().unwrap();
        });
        let mut server = listener.accept(pid, WAIT).unwrap();
        client.join().unwrap();
        let mut received = Vec::new();
        server.read_to_end(&mut received).unwrap();
        received
    });
    assert_eq!(received, b"last");
}

// A client whose process id is not the one expected is refused and gets nothing.
#[test]
fn accept_refuses_a_client_that_is_not_the_expected_one() {
    let (kind, heard) = within(|| {
        let (name, pid) = (unique_name(), process::id());
        let listener = listen(&name);
        let client = thread::spawn(move || {
            let mut stream = connect_pipe(&name, pid).ok()?;
            let mut byte = [0u8; 1];
            stream.read(&mut byte).ok()
        });
        let error = listener.accept(pid.wrapping_add(1), WAIT).unwrap_err();
        (error.kind(), client.join().unwrap())
    });
    assert_eq!(kind, io::ErrorKind::PermissionDenied);
    assert!(matches!(heard, None | Some(0)), "{heard:?}");
}

// The helper goes on only with the server it was told about.
#[test]
fn connect_refuses_a_server_that_is_not_the_expected_one() {
    let kind = within(|| {
        let (name, pid) = (unique_name(), process::id());
        let _listener = listen(&name);
        connect_pipe(&name, pid.wrapping_add(1)).unwrap_err().kind()
    });
    assert_eq!(kind, io::ErrorKind::PermissionDenied);
}

// Given nobody connecting, then accept gives up after its timeout.
#[test]
fn accept_times_out_when_nobody_connects() {
    const TIMEOUT: Duration = Duration::from_millis(200);
    let (kind, waited) = within(|| {
        let listener = listen(&unique_name());
        let started = Instant::now();
        let error = listener.accept(process::id(), TIMEOUT).unwrap_err();
        (error.kind(), started.elapsed())
    });
    assert_eq!(kind, io::ErrorKind::TimedOut);
    assert!(waited >= TIMEOUT / 2, "waited {waited:?}");
}

// One instance: a second listener under the name fails, and the first one still works.
#[test]
fn a_second_listener_with_the_same_name_fails() {
    let (second, ping) = within(|| {
        let (name, pid) = (unique_name(), process::id());
        let listener = listen(&name);
        let second = PipeListener::create(&name, PipeAccess::CurrentUser).map(|_| ());
        let client = thread::spawn(move || {
            let mut stream = connect_pipe(&name, pid).unwrap();
            stream.write_all(b"ping").unwrap();
            stream.flush().unwrap();
            stream
        });
        let mut server = listener.accept(pid, WAIT).unwrap();
        let mut ping = [0u8; 4];
        server.read_exact(&mut ping).unwrap();
        let _client_end = client.join().unwrap();
        (second, ping)
    });
    assert!(second.is_err());
    assert_eq!(&ping, b"ping");
}

#[test]
fn a_pipe_name_is_this_process_and_something_new() {
    let prefix = format!("nova-commander-elevation-{}-", process::id());
    let (first, second) = (new_pipe_name(), new_pipe_name());
    for name in [&first, &second] {
        assert!(name.starts_with(&prefix), "{name:?}");
        assert!(name.len() > prefix.len(), "{name:?}");
        assert!(!name.contains('\\'), "{name:?}");
    }
    assert_ne!(first, second);
}

fn assert_round_trip(path: &Path) {
    let encoded = encode_path(path);
    assert_eq!(encoded.len(), path.as_os_str().encode_wide().count() * 2);
    let decoded = decode_path(&encoded).expect("it decodes");
    assert_eq!(decoded.as_os_str(), path.as_os_str(), "nothing is lost");
}

#[test]
fn a_path_round_trips_exactly() {
    for text in [
        r"C:\folder\file.txt",
        r"\\?\C:\long\name ",
        r"\\server\share\Папка\日本語.txt",
        "relative\\a.",
    ] {
        assert_round_trip(Path::new(text));
    }
}

#[test]
fn a_non_unicode_path_round_trips_exactly() {
    let units: Vec<u16> = r"C:\a"
        .encode_utf16()
        .chain([0xD800, u16::from(b'b')])
        .collect();
    let path = PathBuf::from(OsString::from_wide(&units));
    assert_round_trip(&path);
}

#[test]
fn an_odd_byte_count_is_no_path() {
    assert!(decode_path(&encode_path(Path::new(r"C:\a"))).is_some());
    for odd in [&[0x41][..], &[0x41, 0x00, 0x42][..]] {
        assert_eq!(decode_path(odd), None, "{odd:?}");
    }
}

#[test]
fn the_rights_cure_what_fars_elevation_required_names() {
    for code in [5, 1307, 1314] {
        assert!(needs_rights(&io::Error::from_raw_os_error(code)), "{code}");
    }
    assert!(!needs_rights(&io::Error::from_raw_os_error(2)));
    assert!(!needs_rights(&io::Error::other("no code")));
}
