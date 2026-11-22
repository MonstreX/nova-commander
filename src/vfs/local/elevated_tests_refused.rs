//! Acceptance tests of what the elevated helper refuses and what ends it (ADR-0007 §3-4; 3d step
//! 3): drive roots, exit, and malformed input as raw bytes fed to `serve`. Every malformed input
//! must leave the temp folder as it was.

use std::ffi::OsString;

use super::*;
use crate::platform::volumes::volume_roots;

type Build = Box<dyn Fn(PathBuf) -> Request>;

/// A drive letter that is not mounted: there the OS has nothing to fail on but "not found", so any
/// other answer is the refusal itself (as `ops`'s own test of roots).
fn unmounted_root() -> PathBuf {
    let mounted: Vec<char> = volume_roots()
        .iter()
        .filter_map(|root| root.letter)
        .collect();
    let letter = ('D'..='Z')
        .find(|letter| !mounted.contains(letter))
        .expect("a free drive letter");
    PathBuf::from(format!("{letter}:\\"))
}

// Given a drive root in any spelling, then every way of removing it answers with a failure, the
// helper goes on serving and the root is there.
#[test]
fn a_drive_root_is_never_removed() {
    let temp = tempfile::tempdir().unwrap();
    // The unmounted root first: a helper that does not refuse stops the test there (`NotFound`),
    // before a real drive is ever passed to the Recycle Bin.
    let mut roots = vec![unmounted_root()];
    roots.extend([r"C:\", "C:/", r"\\?\C:\"].iter().map(PathBuf::from));
    let mut helper = Helper::start();
    for root in &roots {
        for request in [
            Request::DeleteFile(root.clone()),
            Request::RemoveDirectory(root.clone()),
            Request::MoveToTrash(root.clone()),
        ] {
            let error = helper.call(request.clone()).unwrap_err();
            assert_ne!(error.kind(), ErrorKind::Unsupported, "{request:?}");
            assert_ne!(error.kind(), ErrorKind::NotFound, "refused: {request:?}");
        }
    }
    assert!(Path::new(r"C:\").is_dir());
    let after = temp.path().join("after");
    helper
        .call(Request::CreateDirectory(after.clone()))
        .unwrap();
    assert!(after.is_dir(), "the helper goes on");
    helper.exit();
}

// `Client::exit` ends `serve` with Ok, and what came before it was carried out.
#[test]
fn exit_ends_the_helper_cleanly() {
    let temp = tempfile::tempdir().unwrap();
    let made = temp.path().join("made");
    let mut helper = Helper::start();
    helper.call(Request::CreateDirectory(made.clone())).unwrap();
    helper.exit();
    assert!(made.is_dir());
}

// A stream that simply ends is the program gone: `serve` returns Ok.
#[test]
fn a_stream_that_ends_returns_ok() {
    let (near, mut far) = duplex();
    let server = thread::spawn(move || serve(&mut far));
    drop(near);
    server.join().unwrap().unwrap();
    serve_bytes(Vec::new()).unwrap();
}

// Exit ends the loop: a request behind it is never read.
#[test]
fn nothing_is_carried_out_after_exit() {
    let (temp, file) = victim();
    let input = [
        captured_exit(),
        captured(&Request::DeleteFile(file.clone())),
    ]
    .concat();
    let before = snapshot(temp.path());
    serve_bytes(input).unwrap();
    assert_eq!(snapshot(temp.path()), before);
}

// The control for every test below: a well-formed frame fed as raw bytes does delete the victim.
#[test]
fn a_well_formed_frame_is_carried_out() {
    let (_temp, file) = victim();
    serve_bytes(captured(&Request::DeleteFile(file.clone()))).unwrap();
    assert!(!file.exists());
}

fn requests_of_every_kind() -> Vec<Request> {
    let (a, b) = (PathBuf::from(r"Z:\a"), PathBuf::from(r"Z:\b"));
    vec![
        Request::CreateDirectory(a.clone()),
        Request::RemoveDirectory(a.clone()),
        Request::DeleteFile(a.clone()),
        Request::MoveToTrash(a.clone()),
        move_request(&a, &b, false),
        copy_request(&a, &b),
        Request::SetReadOnly {
            path: a.clone(),
            read_only: true,
        },
        Request::MakeLink {
            kind: NewLink::Hard,
            target: a.clone(),
            link: b.clone(),
        },
        Request::CopyLink {
            source: a.clone(),
            destination: b,
        },
        Request::Set {
            path: a,
            setting: Setting::Owner("someone".to_string()),
        },
    ]
}

// A frame's length is limited: a longer one is refused though its start is a valid request.
#[test]
fn a_frame_longer_than_max_frame_is_malformed() {
    let (temp, file) = victim();
    let mut body = captured(&Request::DeleteFile(file))[4..].to_vec();
    body.resize(MAX_FRAME + 1, 0);
    assert_malformed(framed(&body), temp.path(), "one byte too long");
    let announced = [&u32::MAX.to_le_bytes()[..], &body].concat();
    assert_malformed(announced, temp.path(), "a length of u32::MAX");
}

// A frame of exactly MAX_FRAME is not too long: the request is answered and the helper goes on.
#[test]
fn a_frame_of_exactly_max_frame_is_served() {
    let temp = tempfile::tempdir().unwrap();
    let probe = stand_in_for(Path::new(r"Z:\probe"));
    let overhead =
        captured(&Request::DeleteFile(probe.clone())).len() - 4 - encode_path(&probe).len();
    let folder_units = temp.path().as_os_str().encode_wide().count();
    let name = "a".repeat((MAX_FRAME - overhead) / 2 - folder_units - 1);
    let frame = captured(&Request::DeleteFile(temp.path().join(name)));
    assert!(frame.len() - 4 + 1 >= MAX_FRAME, "{}", frame.len());
    serve_bytes(frame).unwrap();
    assert!(snapshot(temp.path()).is_empty());
}

// Truncated: the stream ends inside a frame.
#[test]
fn a_truncated_frame_is_malformed() {
    let (temp, file) = victim();
    let frame = captured(&Request::DeleteFile(file));
    assert_malformed(frame[..frame.len() - 1].to_vec(), temp.path(), "body cut");
    assert_malformed(frame[..2].to_vec(), temp.path(), "length cut");
    assert_malformed(framed(&[]), temp.path(), "no version");
    assert_malformed(framed(&[VERSION]), temp.path(), "no kind");
}

#[test]
fn another_version_is_malformed() {
    let (temp, file) = victim();
    let body = captured(&Request::DeleteFile(file))[4..].to_vec();
    for version in [0, VERSION + 1, u8::MAX] {
        let mut changed = body.clone();
        changed[0] = version;
        assert_malformed(framed(&changed), temp.path(), &format!("version {version}"));
    }
}

#[test]
fn an_unknown_kind_is_malformed() {
    let (temp, file) = victim();
    let body = captured(&Request::DeleteFile(file))[4..].to_vec();
    let mut known: Vec<u8> = requests_of_every_kind()
        .iter()
        .map(|request| captured(request)[5])
        .collect();
    known.push(captured_exit()[5]);
    for kind in (0..=u8::MAX).filter(|kind| !known.contains(kind)) {
        let mut changed = body.clone();
        changed[1] = kind;
        assert_malformed(framed(&changed), temp.path(), &format!("kind {kind}"));
    }
}

/// A temp folder inside the current directory, so that relative spellings can name the file in it
/// and a helper that took them would really reach it.
struct RelativePlace {
    dir: tempfile::TempDir,
    name: String,
    spellings: Vec<String>,
}

fn relative_place() -> RelativePlace {
    let cwd = std::env::current_dir().unwrap();
    let dir = tempfile::Builder::new()
        .prefix(".elevated-tests-")
        .tempdir_in(&cwd)
        .unwrap();
    fs::write(dir.path().join("a.txt"), "keep").unwrap();
    fs::create_dir(dir.path().join("empty")).unwrap();
    let name = dir.path().file_name().unwrap().to_str().unwrap().to_owned();
    let cwd_text = cwd.to_str().unwrap();
    assert_eq!(&cwd_text[1..2], ":", "the tests run on a drive letter");
    let (drive, rest) = cwd_text.split_at(2);
    let leaf = cwd.file_name().unwrap().to_str().unwrap();
    let spellings = vec![
        format!(r"{name}\a.txt"),
        format!(r".\{name}\a.txt"),
        format!(r"..\{leaf}\{name}\a.txt"),
        format!(r"{drive}{name}\a.txt"),
        format!(r"{rest}\{name}\a.txt"),
    ];
    RelativePlace {
        dir,
        name,
        spellings,
    }
}

// Every way of writing a relative path that would reach the file ends the helper; the file stays.
#[test]
fn a_relative_path_is_malformed() {
    let place = relative_place();
    for spelling in &place.spellings {
        assert!(Path::new(spelling).exists(), "{spelling} names the file");
        let frame = patched(&Request::DeleteFile, Path::new(spelling));
        assert_malformed(frame, place.dir.path(), spelling);
    }
}

// Each path of each request is checked: a relative one in any place ends the helper. (The Recycle
// Bin is left out: a helper that took the path would put the file there.)
#[test]
fn a_relative_path_is_malformed_in_every_request() {
    let place = relative_place();
    let name = &place.name;
    let file = place.dir.path().join("a.txt");
    let new = place.dir.path().join("new.txt");
    let (relative_file, relative_new) = (format!(r"{name}\a.txt"), format!(r"{name}\new.txt"));
    let relative_empty = format!(r"{name}\empty");
    let (f, n) = (file.clone(), new.clone());
    let cases: Vec<(&str, Build)> = vec![
        (&relative_new, Box::new(Request::CreateDirectory)),
        (&relative_empty, Box::new(Request::RemoveDirectory)),
        (&relative_file, {
            let n = n.clone();
            Box::new(move |s| move_request(&s, &n, false))
        }),
        (&relative_new, {
            let f = f.clone();
            Box::new(move |s| move_request(&f, &s, false))
        }),
        (&relative_file, {
            let n = n.clone();
            Box::new(move |s| copy_request(&s, &n))
        }),
        (&relative_new, {
            let f = f.clone();
            Box::new(move |s| copy_request(&f, &s))
        }),
        (
            &relative_file,
            Box::new(|path| Request::SetReadOnly {
                path,
                read_only: true,
            }),
        ),
        (&relative_file, {
            let n = n.clone();
            Box::new(move |target| Request::MakeLink {
                kind: NewLink::Hard,
                target,
                link: n.clone(),
            })
        }),
        (&relative_new, {
            let f = f.clone();
            Box::new(move |link| Request::MakeLink {
                kind: NewLink::Hard,
                target: f.clone(),
                link,
            })
        }),
    ];
    for (index, (path, build)) in cases.iter().enumerate() {
        let frame = patched(build.as_ref(), Path::new(path));
        assert_malformed(frame, place.dir.path(), &format!("case {index}: {path}"));
    }
}

// Path bytes that are no path: an odd count and none at all.
#[test]
fn undecodable_path_bytes_are_malformed() {
    let (temp, file) = victim();
    let frame = captured(&Request::DeleteFile(file.clone()));
    let body = &frame[4..];
    assert_malformed(framed(&body[..body.len() - 1]), temp.path(), "odd bytes");
    let encoded = encode_path(&file);
    assert!(!encoded.is_empty(), "encode_path");
    let without = &body[..body.len() - encoded.len()];
    assert_malformed(framed(without), temp.path(), "no path");
}

// A NUL inside a path would cut the name for the OS call: `victim.txt<NUL>x` must not reach
// `victim.txt`. Whether it ends the helper or is answered with a failure is an open question.
#[test]
fn a_path_with_a_nul_never_reaches_the_shorter_name() {
    let (temp, file) = victim();
    let (cut_file, cut_new) = (with_nul(&file), with_nul(&temp.path().join("new.txt")));
    let new = temp.path().join("new.txt");
    let (f, n) = (file.clone(), new.clone());
    let cases: Vec<(PathBuf, Build)> = vec![
        (cut_file.clone(), Box::new(Request::DeleteFile)),
        (cut_file.clone(), {
            let n = n.clone();
            Box::new(move |s| move_request(&s, &n, false))
        }),
        (
            cut_new.clone(),
            Box::new(move |s| move_request(&f, &s, false)),
        ),
        (cut_file.clone(), Box::new(move |s| copy_request(&s, &n))),
        (
            cut_file,
            Box::new(|path| Request::SetReadOnly {
                path,
                read_only: true,
            }),
        ),
    ];
    for (index, (path, build)) in cases.iter().enumerate() {
        let before = snapshot(temp.path());
        let result = serve_bytes(patched(build.as_ref(), path));
        let accepted = result.is_ok()
            || result
                .as_ref()
                .is_err_and(|e| e.kind() == ErrorKind::InvalidData);
        assert!(accepted, "case {index}: {result:?}");
        assert_eq!(snapshot(temp.path()), before, "case {index}");
        assert_eq!(fs::read_to_string(&file).unwrap(), "keep", "case {index}");
    }
}

fn with_nul(path: &Path) -> PathBuf {
    let mut name = OsString::from(path);
    name.push("\0x");
    PathBuf::from(name)
}

// Bytes left over after a request end the helper before it is carried out.
#[test]
fn leftover_bytes_are_malformed() {
    let (temp, file) = victim();
    let body = captured(&Request::DeleteFile(file))[4..].to_vec();
    for extra in [&[0x41][..], &[0x41, 0x00][..], &[0, 0][..]] {
        let frame = framed(&[&body[..], extra].concat());
        assert_malformed(frame, temp.path(), &format!("left over {extra:?}"));
    }
    let exit = captured_exit()[4..].to_vec();
    let frame = framed(&[&exit[..], &[0x41][..]].concat());
    assert_malformed(frame, temp.path(), "left over after exit");
}

// Requests before a malformed frame were carried out; none after it is.
#[test]
fn a_malformed_frame_ends_the_helper_at_that_frame() {
    let (temp, file) = victim();
    let first = temp.path().join("first.txt");
    fs::write(&first, "x").unwrap();
    let good = captured(&Request::DeleteFile(first.clone()));
    let later = captured(&Request::DeleteFile(file.clone()));
    let bad = framed(&[VERSION + 1, 0]);
    let error = serve_bytes([good, bad, later].concat()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData, "{error:?}");
    assert!(!first.exists(), "what came before was carried out");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "keep",
        "what came after was not"
    );
}

// Given a path that names no file or folder, or climbs out with `..`, then it is no path for the helper,
// in whatever form it comes; and the client does not send it, so it does not cost the helper.
#[test]
fn only_files_and_folders_are_paths_for_the_helper() {
    for path in [
        r"\\.\PhysicalDrive0",
        r"\\?\PhysicalDrive0",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1\Windows",
        r"\\?\CON",
        r"\\?\pipe\x",
        r"C:\x\..",
        r"C:\x\..\y",
        r"\\?\C:\.",
        r"\\?\Volume{12345678-1234-1234-1234-123456789012}\.",
    ] {
        assert!(!is_plain_absolute(Path::new(path)), "{path}");
        let (mut client, _) = recording_client();
        let error = client
            .call(&Request::DeleteFile(PathBuf::from(path)), &mut |_, _| true)
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{path}");
    }
    for path in [
        r"C:\x",
        r"\\server\share\x",
        r"\\?\C:\x",
        r"\\?\UNC\server\share\x",
        r"\\?\Volume{12345678-1234-1234-1234-123456789012}\x",
    ] {
        assert!(is_plain_absolute(Path::new(path)), "{path}");
    }
}

// Allow-list edges: the kind of prefix decides, in any spelling; the client sends none of the refused.
#[test]
fn the_allow_list_goes_by_the_kind_of_prefix_in_any_spelling() {
    const GUID: &str = "{12345678-1234-1234-1234-123456789012}";
    for path in [
        format!(r"\\?\volume{GUID}\x"),
        format!(r"\\?\VOLUME{GUID}\x"),
        format!(r"\\?\Volume{GUID}"),
        r"C:/x".to_owned(),
    ] {
        assert!(is_plain_absolute(Path::new(&path)), "{path}");
    }
    for path in [
        format!(r"\\.\Volume{GUID}\x"),
        r"\\.\C:\x".to_owned(),
        r"\\?\Volume\x".to_owned(),
        r"\\?\VolumeShadowCopy1\x".to_owned(),
        r"\\?\C:\x\..\y".to_owned(),
        r"\\?\UNC\server\share\..\x".to_owned(),
        r"\\server\share\x\..\y".to_owned(),
        r"C:/x/../y".to_owned(),
        r"C:\..".to_owned(),
        String::new(),
    ] {
        assert!(!is_plain_absolute(Path::new(&path)), "{path}");
        let (mut client, written) = recording_client();
        let request = move_request(Path::new(r"Z:\a"), Path::new(&path), false);
        let error = client.call(&request, &mut |_, _| true).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{path}");
        assert!(written.borrow().is_empty(), "nothing is sent: {path}");
    }
}

// Spellings only `Path` knows are roots, on a letter nothing is mounted at: the refusal, not the OS.
#[test]
fn a_drive_root_is_refused_in_the_spellings_that_look_like_a_name() {
    let root = unmounted_root();
    let letter = &root.to_str().unwrap()[..1];
    let mut helper = Helper::start();
    for spelling in [format!(r"\\?\{letter}:"), format!(r"{letter}:\.")] {
        for request in [
            Request::DeleteFile(PathBuf::from(&spelling)),
            Request::RemoveDirectory(PathBuf::from(&spelling)),
            Request::MoveToTrash(PathBuf::from(&spelling)),
        ] {
            let error = helper.call(request.clone()).unwrap_err();
            let seen = (error.kind(), error.raw_os_error());
            assert_eq!(seen, (ErrorKind::InvalidInput, None), "{request:?}");
        }
    }
    helper.exit();
}

/// The frame of an answer of `kind`.
fn reply_frame(kind: u8, fields: &[u8]) -> Vec<u8> {
    framed(&[&[VERSION, kind][..], fields].concat())
}

/// `Client::call` with `input` as the helper's answer: the result, what progress saw, what was sent back.
fn answered(input: Vec<u8>, go_on: bool) -> (io::Result<()>, Vec<(u64, u64)>, Vec<u8>) {
    let written: Rc<RefCell<Vec<u8>>> = Rc::default();
    let script = Script {
        input: Cursor::new(input),
        written: Rc::clone(&written),
    };
    let request = Request::DeleteFile(PathBuf::from(r"Z:\a"));
    let mut seen = Vec::new();
    let result = Client::new(script).call(&request, &mut |done, total| {
        seen.push((done, total));
        go_on
    });
    let after_the_request = written.borrow()[captured(&request).len()..].to_vec();
    (result, seen, after_the_request)
}

// Whatever is malformed in an answer ends the call as InvalidData, before progress hears of it.
#[test]
fn a_malformed_answer_is_invalid_data_and_reports_no_progress() {
    let too_long = u32::try_from(MAX_FRAME + 1).unwrap().to_le_bytes().to_vec();
    let cases = [
        ("another version", framed(&[VERSION + 1, DONE])),
        ("version 0", framed(&[0, DONE])),
        ("an unknown kind", reply_frame(0x7F, &[])),
        ("past the last kind", reply_frame(GO_ON + 1, &[])),
        ("a request's kind", reply_frame(DELETE_FILE, &[])),
        ("the client's own kind", reply_frame(GO_ON, &[1])),
        ("done with more", reply_frame(DONE, &[0])),
        ("stopped with more", reply_frame(STOPPED, &[0])),
        ("failed, code cut", reply_frame(FAILED, &[5, 0, 0])),
        ("failed with more", reply_frame(FAILED, &[5, 0, 0, 0, 0])),
        ("progress cut", reply_frame(PROGRESS, &[0; 15])),
        ("progress with more", reply_frame(PROGRESS, &[0; 17])),
        ("a length of 0", framed(&[])),
        ("no kind", framed(&[VERSION])),
        ("over the limit", too_long),
        ("the body cut", reply_frame(DONE, &[])[..5].to_vec()),
        ("the length cut", vec![2, 0]),
    ];
    for (why, input) in cases {
        let (result, seen, _) = answered(input, true);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData, "{why}");
        assert!(seen.is_empty(), "{why}: {seen:?}");
    }
}

// Fields in their order, and the client's answer to progress as it goes on the wire.
#[test]
fn an_answer_is_read_field_by_field() {
    let fields = [3u64.to_le_bytes(), 7u64.to_le_bytes()].concat();
    let progress = reply_frame(PROGRESS, &fields);
    let (result, seen, sent) = answered([progress.clone(), reply_frame(DONE, &[])].concat(), true);
    result.unwrap();
    assert_eq!(seen, [(3, 7)], "done first, then total");
    assert_eq!(sent, reply_frame(GO_ON, &[1]));

    let (result, _, sent) = answered([progress, reply_frame(STOPPED, &[])].concat(), false);
    assert_eq!(result.unwrap_err().kind(), ErrorKind::Interrupted);
    assert_eq!(sent, reply_frame(GO_ON, &[0]), "no is sent as a no");

    let failed = |code: i32| {
        let answer = reply_frame(FAILED, &code.to_le_bytes());
        answered(answer, true).0.unwrap_err()
    };
    assert_eq!(failed(5).raw_os_error(), Some(5), "the helper's OS error");
    let own = failed(0);
    assert_eq!(
        (own.kind(), own.raw_os_error()),
        (ErrorKind::InvalidInput, None)
    );
}

// The stream ends where an answer should start: the helper is gone, in the middle of a copy too.
#[test]
fn a_helper_that_stops_answering_is_a_broken_pipe() {
    let kind = |input| answered(input, true).0.unwrap_err().kind();
    assert_eq!(kind(Vec::new()), ErrorKind::BrokenPipe);
    let after_a_report = reply_frame(PROGRESS, &[0; 16]);
    assert_eq!(kind(after_a_report), ErrorKind::BrokenPipe);
}

// A malformed answer to a copy's progress ends the helper with InvalidData and the copy with it: no
// target stays. Good answers follow the bad one, so a helper that took it would finish the copy.
#[test]
fn a_malformed_answer_to_progress_ends_the_helper_and_the_copy() {
    let (temp, source) = victim();
    fs::write(&source, vec![1u8; 3_000_000]).unwrap();
    let copy = captured(&copy_request(&source, &temp.path().join("copy.bin")));
    let good = reply_frame(GO_ON, &[1]).repeat(64);
    let cases = [
        ("another kind, a flag after it", reply_frame(DONE, &[1])),
        ("no flag", reply_frame(GO_ON, &[])),
        ("not a flag", reply_frame(GO_ON, &[2])),
        ("a flag and more", reply_frame(GO_ON, &[1, 0])),
        ("another version", framed(&[VERSION + 1, GO_ON, 1])),
    ];
    for (why, bad) in cases {
        assert_malformed([copy.clone(), bad, good.clone()].concat(), temp.path(), why);
    }
    assert_malformed(
        copy,
        temp.path(),
        "the stream ends where the answer should be",
    );
}
