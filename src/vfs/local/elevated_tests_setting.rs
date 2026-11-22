//! Acceptance tests of the helper's `Set` request (Ctrl+A through the helper, ADR-0007 §1; P4.3): every
//! change travels whole, is carried out as the program would make it itself, and a malformed one ends
//! the helper with nothing done.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;
use crate::platform::files::{Attributes, FileTimes, Flag, file_properties};

fn set(path: &Path, setting: Setting) -> Request {
    Request::Set {
        path: path.to_path_buf(),
        setting,
    }
}

fn round_trip(request: &Request) -> Request {
    let (kind, fields) = request.encode().unwrap();
    Request::decode(kind, &fields).unwrap()
}

// Given every kind of change, then the helper reads back what the program wrote.
#[test]
fn every_change_travels_whole() {
    let path = Path::new(r"Z:\a");
    let before_1970 = UNIX_EPOCH - Duration::new(86_400, 500);
    let times = FileTimes {
        created: Some(before_1970),
        accessed: None,
        modified: Some(UNIX_EPOCH + Duration::new(1_741_996_800, 123_456_700)),
        changed: Some(UNIX_EPOCH),
    };
    for setting in [
        Setting::LinkTarget(r"..\каталог\x".to_string()),
        Setting::Owner(r"DOMAIN\someone".to_string()),
        Setting::Compressed(true),
        Setting::Encrypted(false),
        Setting::Sparse(true),
        Setting::RemoveReparsePoint,
        Setting::Attributes(Attributes::from_windows_bits(0x0000_0023)),
        Setting::Times(times),
    ] {
        let request = set(path, setting);
        assert_eq!(round_trip(&request), request);
    }
}

// Given a link's new target that is a device, then the program refuses it before the helper sees it;
// a relative target, a drive's path, a share and a volume pass.
#[test]
fn a_link_target_is_relative_or_a_plain_full_path() {
    let target = |text: &str| set(Path::new(r"Z:\l"), Setting::LinkTarget(text.to_string()));
    for refused in [r"\\.\PhysicalDrive0", r"\??\C:\x", r"\\?\GLOBALROOT\x"] {
        let error = target(refused).encode().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{refused}");
    }
    for passed in [
        r"..\x",
        "x.txt",
        r"C:\Work",
        r"\\server\share\x",
        r"\\?\Volume{0}\",
        r"\x",
    ] {
        assert_eq!(round_trip(&target(passed)), target(passed), "{passed}");
    }
}

// Given a file, then Set makes its change on it and the helper goes on.
#[test]
fn set_makes_the_change_and_the_helper_goes_on() {
    let (_temp, file) = victim();
    let attributes = file_properties(&file).unwrap().attributes;
    let wanted = SystemTime::UNIX_EPOCH + Duration::from_secs(1_741_996_800);
    let mut helper = Helper::start();
    helper
        .call(set(
            &file,
            Setting::Attributes(attributes.with(Flag::Hidden, true)),
        ))
        .unwrap();
    let times = FileTimes {
        modified: Some(wanted),
        ..FileTimes::default()
    };
    helper.call(set(&file, Setting::Times(times))).unwrap();
    let read = file_properties(&file).unwrap();
    assert!(read.attributes.has(Flag::Hidden));
    assert_eq!(read.times.modified, Some(wanted));
    // A change the system refuses comes back with its error.
    let error = helper
        .call(set(&file, Setting::Owner("no-such-account-6f1c".into())))
        .unwrap_err();
    assert!(error.raw_os_error().is_some(), "{error:?}");
    helper.exit();
}

/// The frame of `Set` on `path` with its change's fields replaced by `fields`.
fn with_change(path: &Path, fields: &[u8]) -> Vec<u8> {
    let mut body = vec![VERSION, SET];
    let encoded = encode_path(path);
    body.extend(u32::try_from(encoded.len()).unwrap().to_le_bytes());
    body.extend(encoded);
    body.extend(fields);
    framed(&body)
}

// Given a change that is not one, then the helper ends and the file is as it was.
#[test]
fn a_malformed_change_ends_the_helper_with_nothing_done() {
    let (temp, file) = victim();
    let time = |side: u8, nanos: u32| {
        let mut fields = vec![side];
        fields.extend(7u64.to_le_bytes());
        fields.extend(nanos.to_le_bytes());
        fields
    };
    let times = |first: Vec<u8>| {
        let blank = time(0, 0);
        [vec![7], first, blank.clone(), blank.clone(), blank].concat()
    };
    let cases: [(&str, Vec<u8>); 7] = [
        ("an unknown change", vec![99]),
        ("no change at all", vec![]),
        ("a flag that is neither", vec![2, 7]),
        (
            "a text that is no UTF-8",
            [vec![1], 2u32.to_le_bytes().to_vec(), vec![0xFF, 0xFE]].concat(),
        ),
        (
            "a text longer than its frame",
            [vec![0], 99u32.to_le_bytes().to_vec(), b"x".to_vec()].concat(),
        ),
        ("nanoseconds past a second", times(time(1, 1_000_000_000))),
        ("a blank time with a value", times(time(0, 5))),
    ];
    for (why, fields) in cases {
        assert_malformed(with_change(&file, &fields), temp.path(), why);
    }
    let text = |text: &str| {
        let length = u32::try_from(text.len()).unwrap().to_le_bytes().to_vec();
        [vec![0], length, text.as_bytes().to_vec()].concat()
    };
    for device in [
        r"\\.\PhysicalDrive0",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1",
        r"\??\C:\x",
        "//./PhysicalDrive0",
        r"C:\a\..\..\x",
    ] {
        assert_malformed(with_change(&file, &text(device)), temp.path(), device);
    }
    let mut extra = times(time(3, 0));
    assert_malformed(with_change(&file, &extra), temp.path(), "a time on no side");
    extra = [vec![5], vec![0]].concat();
    assert_malformed(with_change(&file, &extra), temp.path(), "bytes left over");
}
