//! Acceptance tests of the elevated helper's requests (ADR-0007 §1, §3, §4; 3d step 3). Each test
//! carries its Given / When / Then. `Client` and `serve` talk over an in-memory duplex stream, and
//! malformed input is raw bytes fed to `serve`. Nothing here starts a process, and the Recycle Bin is
//! only ever asked to take a drive root, which it must refuse.

#![cfg(windows)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::io::{Cursor, ErrorKind};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::*;
use crate::platform::elevation::{PipeAccess, PipeListener, connect_pipe, encode_path};
use crate::platform::files::{exact_path, file_identity, link_target, make_link, set_read_only};

#[path = "elevated_tests_refused.rs"]
mod refused;
#[path = "elevated_tests_setting.rs"]
mod setting;

/// How long a test waits for the other side before it fails instead of hanging.
const WAIT: Duration = Duration::from_secs(30);

/// One end of an in-memory duplex stream; the other end's drop reads as the end of the stream.
struct Duplex {
    incoming: Receiver<Vec<u8>>,
    outgoing: Sender<Vec<u8>>,
    pending: VecDeque<u8>,
}

fn duplex() -> (Duplex, Duplex) {
    let (to_far, from_near) = mpsc::channel();
    let (to_near, from_far) = mpsc::channel();
    let end = |incoming, outgoing| Duplex {
        incoming,
        outgoing,
        pending: VecDeque::new(),
    };
    (end(from_far, to_far), end(from_near, to_near))
}

impl Read for Duplex {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.pending.is_empty() {
            match self.incoming.recv_timeout(WAIT) {
                Ok(bytes) => self.pending.extend(bytes),
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
                Err(RecvTimeoutError::Timeout) => return Err(ErrorKind::TimedOut.into()),
            }
        }
        let count = buffer.len().min(self.pending.len());
        for (slot, byte) in buffer.iter_mut().zip(self.pending.drain(..count)) {
            *slot = byte;
        }
        Ok(count)
    }
}

impl Write for Duplex {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.outgoing
            .send(buffer.to_vec())
            .map_err(|_| io::Error::from(ErrorKind::BrokenPipe))?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// `serve` on one thread and a `Client` for the test, over a `Duplex`.
struct Helper {
    client: Client<Duplex>,
    server: JoinHandle<io::Result<()>>,
}

impl Helper {
    fn start() -> Self {
        let (near, mut far) = duplex();
        let server = thread::spawn(move || serve(&mut far));
        Self {
            client: Client::new(near),
            server,
        }
    }

    fn call(&mut self, request: Request) -> io::Result<()> {
        self.client.call(&request, &mut |_, _| true)
    }

    /// Proves the helper was still serving: it ends only now, and cleanly.
    fn exit(self) {
        self.client.exit().unwrap();
        self.server.join().unwrap().unwrap();
    }
}

/// Bytes to read, and whatever is written kept for the test. Reading past the input is the end of
/// the stream.
struct Script {
    input: Cursor<Vec<u8>>,
    written: Rc<RefCell<Vec<u8>>>,
}

impl Read for Script {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.input.read(buffer)
    }
}

impl Write for Script {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.written.borrow_mut().extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn serve_bytes(input: Vec<u8>) -> io::Result<()> {
    serve(&mut Script {
        input: Cursor::new(input),
        written: Rc::default(),
    })
}

fn recording_client() -> (Client<Script>, Rc<RefCell<Vec<u8>>>) {
    let written: Rc<RefCell<Vec<u8>>> = Rc::default();
    let script = Script {
        input: Cursor::new(Vec::new()),
        written: Rc::clone(&written),
    };
    (Client::new(script), written)
}

/// The frame `Client` writes for `request`. The stream gives no answer, so the call fails; only what
/// it wrote matters.
fn captured(request: &Request) -> Vec<u8> {
    let (mut client, written) = recording_client();
    let result = client.call(request, &mut |_, _| true);
    checked_frame(written.take(), &result)
}

fn captured_exit() -> Vec<u8> {
    let (client, written) = recording_client();
    let result = client.exit();
    checked_frame(written.take(), &result)
}

fn checked_frame(frame: Vec<u8>, result: &io::Result<()>) -> Vec<u8> {
    assert!(frame.len() >= 6, "no frame written: {result:?}");
    let length = u32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    assert_eq!(length, frame.len() - 4, "a u32 LE length, that many bytes");
    assert!(length <= MAX_FRAME, "{length}");
    assert_eq!(frame[4], VERSION, "the first byte is the version");
    frame
}

fn framed(body: &[u8]) -> Vec<u8> {
    let length = u32::try_from(body.len()).unwrap();
    [&length.to_le_bytes()[..], body].concat()
}

/// An absolute path of the same length as `path`: what `Client` is given for a frame whose path is
/// then swapped for `path`. `Client` never sees what it would not send.
fn stand_in_for(path: &Path) -> PathBuf {
    let units = path.as_os_str().encode_wide().count();
    PathBuf::from(format!(r"Z:\{}", "p".repeat(units - 3)))
}

/// The frame of `build(stand-in)` with the stand-in's bytes replaced by `path`'s.
fn patched(build: &dyn Fn(PathBuf) -> Request, path: &Path) -> Vec<u8> {
    let stand_in = stand_in_for(path);
    let mut frame = captured(&build(stand_in.clone()));
    let (old, new) = (encode_path(&stand_in), encode_path(path));
    assert!(!old.is_empty() && old.len() == new.len(), "encode_path");
    let at = frame
        .windows(old.len())
        .position(|window| window == old.as_slice())
        .expect("paths travel in encode_path's form");
    frame[at..at + new.len()].copy_from_slice(&new);
    frame
}

/// What a folder holds, flat: names, kinds, sizes and read-only flags.
fn snapshot(dir: &Path) -> Vec<(String, bool, u64, bool)> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                metadata.is_dir(),
                metadata.len(),
                metadata.permissions().readonly(),
            )
        })
        .collect();
    entries.sort();
    entries
}

/// `input` ends `serve` with `InvalidData` and `dir` is as it was.
fn assert_malformed(input: Vec<u8>, dir: &Path, why: &str) {
    let before = snapshot(dir);
    let error = serve_bytes(input).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData, "{why}: {error:?}");
    assert_eq!(snapshot(dir), before, "{why}: nothing is carried out");
}

fn assert_os_error(error: &io::Error, kind: ErrorKind) {
    assert!(error.raw_os_error().is_some(), "an OS error: {error:?}");
    assert_eq!(error.kind(), kind, "{error:?}");
}

fn victim() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("victim.txt");
    fs::write(&file, "keep").unwrap();
    (temp, file)
}

// Given a folder, when CreateDirectory names a free place in it, then that one folder is made.
#[test]
fn create_directory_makes_one_folder() {
    let temp = tempfile::tempdir().unwrap();
    let made = temp.path().join("new");
    let mut helper = Helper::start();
    helper.call(Request::CreateDirectory(made.clone())).unwrap();
    assert!(made.is_dir());
    helper.exit();
}

// Given a taken name (folder or file), then CreateDirectory fails with the OS error, keeping it.
#[test]
fn create_directory_fails_when_the_name_is_taken() {
    let (temp, file) = victim();
    let folder = temp.path().join("folder");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("inside.txt"), "x").unwrap();
    let mut helper = Helper::start();
    for taken in [&folder, &file] {
        let error = helper
            .call(Request::CreateDirectory(taken.clone()))
            .unwrap_err();
        assert_os_error(&error, ErrorKind::AlreadyExists);
    }
    assert!(folder.join("inside.txt").is_file());
    assert_eq!(fs::read_to_string(&file).unwrap(), "keep");
    helper.exit();
}

// Given a missing parent, then CreateDirectory fails and the parent is not made.
#[test]
fn create_directory_does_not_make_a_missing_parent() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("missing");
    let mut helper = Helper::start();
    let error = helper
        .call(Request::CreateDirectory(parent.join("child")))
        .unwrap_err();
    assert_os_error(&error, ErrorKind::NotFound);
    assert!(!parent.exists());
    helper.exit();
}

// Given the verbatim form of a name Win32 would shorten, then it is that name which is made.
#[test]
fn create_directory_takes_a_verbatim_name_exactly() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("d")).unwrap();
    let dotted = exact_path(&temp.path().join("d."));
    let mut helper = Helper::start();
    helper
        .call(Request::CreateDirectory(dotted.clone()))
        .unwrap();
    assert!(dotted.is_dir());
    assert_eq!(snapshot(temp.path()).len(), 2, "next to its sibling");
    helper.exit();
}

// Given an empty folder and a full one, then only the empty one goes.
#[test]
fn remove_directory_takes_an_empty_folder_and_refuses_a_full_one() {
    let temp = tempfile::tempdir().unwrap();
    let (empty, full) = (temp.path().join("empty"), temp.path().join("full"));
    fs::create_dir(&empty).unwrap();
    fs::create_dir(&full).unwrap();
    fs::write(full.join("inside.txt"), "x").unwrap();
    let mut helper = Helper::start();
    helper
        .call(Request::RemoveDirectory(empty.clone()))
        .unwrap();
    assert!(!empty.exists());
    let error = helper
        .call(Request::RemoveDirectory(full.clone()))
        .unwrap_err();
    assert_os_error(&error, ErrorKind::DirectoryNotEmpty);
    assert!(full.join("inside.txt").is_file());
    helper.exit();
}

// Given a junction, when it is removed, then the link goes and what it leads to stays.
#[test]
fn remove_directory_takes_a_junction_and_keeps_its_target() {
    let temp = tempfile::tempdir().unwrap();
    let (target, link) = (temp.path().join("target"), temp.path().join("link"));
    fs::create_dir(&target).unwrap();
    fs::write(target.join("inside.txt"), "x").unwrap();
    make_link(NewLink::Junction, &target, &link).unwrap();
    let mut helper = Helper::start();
    helper.call(Request::RemoveDirectory(link.clone())).unwrap();
    assert!(fs::symlink_metadata(&link).is_err());
    assert!(target.join("inside.txt").is_file());
    helper.exit();
}

// Given a read-only file, then DeleteFile deletes it.
#[test]
fn delete_file_deletes_a_read_only_file() {
    let (_temp, file) = victim();
    set_read_only(&file, true).unwrap();
    let mut helper = Helper::start();
    helper.call(Request::DeleteFile(file.clone())).unwrap();
    assert!(fs::symlink_metadata(&file).is_err());
    helper.exit();
}

// Given the wrong kind of object, then each request fails and the object stays.
#[test]
fn delete_file_is_not_for_folders_nor_remove_directory_for_files() {
    let (temp, file) = victim();
    let folder = temp.path().join("folder");
    fs::create_dir(&folder).unwrap();
    let mut helper = Helper::start();
    let error = helper
        .call(Request::DeleteFile(folder.clone()))
        .unwrap_err();
    assert!(error.raw_os_error().is_some(), "{error:?}");
    let error = helper
        .call(Request::RemoveDirectory(file.clone()))
        .unwrap_err();
    assert!(error.raw_os_error().is_some(), "{error:?}");
    assert!(folder.is_dir() && file.is_file());
    helper.exit();
}

// Given `a ` and its sibling `a`, when `a ` is deleted, then the sibling stays.
#[test]
fn delete_file_takes_the_name_exactly() {
    let temp = tempfile::tempdir().unwrap();
    let exact = |name: &str| exact_path(&temp.path().join(name));
    fs::write(exact("a "), "spaced").unwrap();
    fs::write(exact("a"), "plain").unwrap();
    let mut helper = Helper::start();
    helper
        .call(Request::DeleteFile(temp.path().join("a ")))
        .unwrap();
    assert!(fs::symlink_metadata(exact("a ")).is_err());
    assert_eq!(fs::read_to_string(exact("a")).unwrap(), "plain");
    helper.exit();
}

fn move_request(from: &Path, to: &Path, replace: bool) -> Request {
    Request::MoveFile {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        replace,
    }
}

// Given a free target, then MoveFile moves the file.
#[test]
fn move_file_moves_to_a_free_name() {
    let (temp, file) = victim();
    let target = temp.path().join("moved.txt");
    let mut helper = Helper::start();
    helper.call(move_request(&file, &target, false)).unwrap();
    assert!(!file.exists());
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
    helper.exit();
}

// Given an existing target, then without `replace` both files stay; with it the target is replaced.
#[test]
fn move_file_replaces_only_when_asked() {
    let (temp, file) = victim();
    let target = temp.path().join("old.txt");
    fs::write(&target, "old").unwrap();
    let mut helper = Helper::start();
    let error = helper
        .call(move_request(&file, &target, false))
        .unwrap_err();
    assert_os_error(&error, ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(&file).unwrap(), "keep");
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");

    helper.call(move_request(&file, &target, true)).unwrap();
    assert!(!file.exists());
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
    helper.exit();
}

fn copy_request(from: &Path, to: &Path) -> Request {
    Request::CopyFile {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
    }
}

// Given a free target, then CopyFile copies; given an existing one, it fails and keeps it.
#[test]
fn copy_file_copies_and_never_writes_over_a_file() {
    let (temp, file) = victim();
    let target = temp.path().join("copy.txt");
    let mut helper = Helper::start();
    helper.call(copy_request(&file, &target)).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
    assert!(file.is_file(), "the source stays");

    fs::write(&target, "kept").unwrap();
    let error = helper.call(copy_request(&file, &target)).unwrap_err();
    assert_os_error(&error, ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(&target).unwrap(), "kept");
    helper.exit();
}

// Given a file, then SetReadOnly sets and clears the attribute.
#[test]
fn set_read_only_sets_and_clears_the_attribute() {
    let (_temp, file) = victim();
    let read_only = |path: &Path| fs::metadata(path).unwrap().permissions().readonly();
    let mut helper = Helper::start();
    helper
        .call(Request::SetReadOnly {
            path: file.clone(),
            read_only: true,
        })
        .unwrap();
    assert!(read_only(&file));
    helper
        .call(Request::SetReadOnly {
            path: file.clone(),
            read_only: false,
        })
        .unwrap();
    assert!(!read_only(&file));
    helper.exit();
}

// Given a file, then MakeLink makes a hard link to it, and refuses a name that is taken.
#[test]
fn make_link_makes_a_hard_link() {
    let (temp, file) = victim();
    let link = temp.path().join("link.txt");
    let request = Request::MakeLink {
        kind: NewLink::Hard,
        target: file.clone(),
        link: link.clone(),
    };
    let mut helper = Helper::start();
    helper.call(request.clone()).unwrap();
    assert_eq!(file_identity(&link).unwrap(), file_identity(&file).unwrap());

    let error = helper.call(request).unwrap_err();
    assert_os_error(&error, ErrorKind::AlreadyExists);
    helper.exit();
}

// Given a junction (it needs no privilege), then CopyLink makes another link to the same folder.
#[test]
fn copy_link_copies_a_junction_as_a_link() {
    let temp = tempfile::tempdir().unwrap();
    let (target, link) = (temp.path().join("target"), temp.path().join("link"));
    fs::create_dir(&target).unwrap();
    fs::write(target.join("inside.txt"), "x").unwrap();
    make_link(NewLink::Junction, &target, &link).unwrap();
    let copy = temp.path().join("copy");
    let mut helper = Helper::start();
    helper
        .call(Request::CopyLink {
            source: link,
            destination: copy.clone(),
        })
        .unwrap();
    assert!(fs::symlink_metadata(&copy).unwrap().is_symlink());
    assert_eq!(link_target(&copy).unwrap(), target);
    helper.exit();
}

// When a request fails, then the OS error comes back and the helper goes on serving.
#[test]
fn a_failure_comes_back_with_the_os_error_and_the_helper_goes_on() {
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    let (temp, file) = victim();
    let mut helper = Helper::start();
    let error = helper
        .call(Request::DeleteFile(temp.path().join("missing.txt")))
        .unwrap_err();
    assert_eq!(
        error.raw_os_error(),
        Some(ERROR_FILE_NOT_FOUND),
        "{error:?}"
    );

    helper.call(Request::DeleteFile(file.clone())).unwrap();
    assert!(!file.exists());
    helper.exit();
}

// Given a copy, then its progress reaches the client and ends at the whole size.
#[test]
fn copy_progress_reaches_the_client() {
    const SIZE: u64 = 300_000;
    let (temp, source) = victim();
    fs::write(&source, vec![7u8; 300_000]).unwrap();
    let target = temp.path().join("copy.bin");
    let mut reports = Vec::new();
    let mut helper = Helper::start();
    helper
        .client
        .call(&copy_request(&source, &target), &mut |done, total| {
            reports.push((done, total));
            true
        })
        .unwrap();
    assert_eq!(reports.last(), Some(&(SIZE, SIZE)), "{reports:?}");
    assert!(reports.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert_eq!(fs::read(&target).unwrap().len() as u64, SIZE);
    helper.exit();
}

// When `progress` answers false, then the copy ends as Interrupted, leaves no target and the helper
// goes on serving.
#[test]
fn a_stopped_copy_is_interrupted_and_leaves_no_target() {
    const SIZE: usize = 3_000_000;
    let (temp, source) = victim();
    fs::write(&source, vec![1u8; SIZE]).unwrap();
    let target = temp.path().join("copy.bin");
    let mut asked = 0;
    let mut helper = Helper::start();
    let error = helper
        .client
        .call(&copy_request(&source, &target), &mut |_, _| {
            asked += 1;
            false
        })
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Interrupted, "{error:?}");
    assert!(asked >= 1, "the client was asked");
    assert!(fs::symlink_metadata(&target).is_err(), "no partial target");
    assert_eq!(fs::read(&source).unwrap().len(), SIZE, "the source stays");

    let after = temp.path().join("after");
    helper
        .call(Request::CreateDirectory(after.clone()))
        .unwrap();
    assert!(after.is_dir());
    helper.exit();
}

// ADR-0007 §4: the same conversation over a real named pipe, the helper as the client of the pipe.
#[test]
fn client_and_helper_talk_over_a_real_pipe() {
    let temp = tempfile::tempdir().unwrap();
    let made = temp.path().join("made");
    let pid = std::process::id();
    let name = format!("vc-elevated-real-pipe-test-{pid}");
    let listener = PipeListener::create(&name, PipeAccess::CurrentUser).unwrap();
    let server = thread::spawn(move || serve(&mut connect_pipe(&name, pid)?));
    let mut client = Client::new(listener.accept(pid, WAIT).unwrap());
    client
        .call(&Request::CreateDirectory(made.clone()), &mut |_, _| true)
        .unwrap();
    assert!(made.is_dir());
    client.exit().unwrap();
    server.join().unwrap().unwrap();
}
