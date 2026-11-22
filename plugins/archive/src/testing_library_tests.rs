//! Through 7-Zip's installed library: `testdata/a.7z`, the same with a byte of its packed data changed,
//! `testdata/data_locked.7z` (its data encrypted by "secret").

use std::path::Path;

use nc_plugin_sdk::sevenzip::{Library, Opening};

use super::*;
use crate::library_open::{Request, open};

struct NoAsk;
impl Opening for NoAsk {}

fn library_opened(folder: &Path, bytes: &[u8]) -> Opened {
    let path = folder.join("a.7z");
    std::fs::write(&path, bytes).unwrap();
    let library = Library::find(folder);
    let formats = library.formats();
    let enabled: Vec<usize> = (0..formats.len()).collect();
    let request = Request {
        library: &library,
        formats: &formats,
        enabled: &enabled,
        every: false,
    };
    let chain = open(&request, &path, &mut NoAsk).unwrap().remove(0);
    Opened::from_library(&path, chain, ("7z".to_string(), "a"), None).unwrap()
}

#[derive(Default)]
struct Asks {
    passwords: Vec<Option<String>>,
    asked: usize,
    shown: Vec<(String, u64, u64)>,
    stop: bool,
    /// Esc pressed at this look only (counted from 1).
    stop_at: Option<usize>,
    looks: usize,
}

impl TestAsks for Asks {
    fn password(&mut self) -> Option<String> {
        self.asked += 1;
        self.passwords.pop().flatten()
    }

    fn testing(&mut self, file: &str, done: u64, total: u64) {
        self.shown.push((file.to_string(), done, total));
    }

    fn stopped(&mut self) -> bool {
        self.looks += 1;
        self.stop || self.stop_at == Some(self.looks)
    }
}

/// The archive's files, by their paths.
fn files(opened: &Opened) -> Vec<(usize, String)> {
    (opened.items.iter().enumerate())
        .filter(|(_, item)| !item.is_dir)
        .map(|(index, item)| (index, item.path.join("\\")))
        .collect()
}

/// A good archive passes; the window told each file and the library's progress.
#[test]
fn a_good_archive_passes() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), include_bytes!("../testdata/a.7z"));
    let files = files(&opened);
    let mut asks = Asks::default();
    test_files(&mut opened, &files, &mut asks).unwrap();
    assert!(asks.shown.iter().any(|(file, _, _)| file == "top.txt"));
    assert!(asks.shown.iter().any(|(_, _, total)| *total > 0));
}

/// A changed byte: the test fails in the tester's words, the file's path and the archive's under them.
#[test]
fn a_damaged_archive_fails() {
    let temp = tempfile::tempdir().unwrap();
    let mut bytes = include_bytes!("../testdata/a.7z").to_vec();
    // The first file's CRC fails (as `7z t` finds it).
    bytes[36] ^= 0x55;
    let mut opened = library_opened(temp.path(), &bytes);
    let files = files(&opened);
    let tested = test_files(&mut opened, &files, &mut Asks::default());
    let Err(Untested::Failed(fault)) = tested else {
        panic!("the damage not found: {tested:?} {files:?}");
    };
    assert_eq!(fault.messages[0], "CRC check failed");
    assert_eq!(fault.messages[1], r"docs\readme.txt");
    assert_eq!(fault.messages[2], opened.path.display().to_string());
}

/// Encrypted data: the password asked once; the right one passes, a wrong one fails as an encrypted
/// archive's, none stops.
#[test]
fn encrypted_data_asks_the_password() {
    let temp = tempfile::tempdir().unwrap();
    let locked = include_bytes!("../testdata/data_locked.7z");
    let mut opened = library_opened(temp.path(), locked);
    let files = files(&opened);
    let mut asks = Asks {
        passwords: vec![Some("secret".to_string())],
        ..Asks::default()
    };
    test_files(&mut opened, &files, &mut asks).unwrap();
    assert_eq!(asks.asked, 1);
    let mut opened = library_opened(temp.path(), locked);
    let mut wrong = Asks {
        passwords: vec![Some("nope".to_string())],
        ..Asks::default()
    };
    let Err(Untested::Failed(fault)) = test_files(&mut opened, &files, &mut wrong) else {
        panic!("a wrong password passed");
    };
    assert!(
        fault.messages[0].starts_with("Wrong password or"),
        "{fault:?}"
    );
    let mut opened = library_opened(temp.path(), locked);
    let stopped = test_files(&mut opened, &files, &mut Asks::default());
    assert_eq!(stopped, Err(Untested::Stopped));
}

/// Esc stops the test.
#[test]
fn esc_stops_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), include_bytes!("../testdata/a.7z"));
    let files = files(&opened);
    let mut asks = Asks {
        stop: true,
        ..Asks::default()
    };
    assert_eq!(
        test_files(&mut opened, &files, &mut asks),
        Err(Untested::Stopped)
    );
}

/// The tester's words: narrower than an extraction's.
#[test]
fn the_testers_words() {
    assert_eq!(tester_words(Outcome::Ok, false), None);
    assert_eq!(
        tester_words(Outcome::DataError, true),
        Some("Wrong password or data error")
    );
    assert_eq!(
        tester_words(Outcome::CrcError, false),
        Some("CRC check failed")
    );
    assert_eq!(
        tester_words(Outcome::UnsupportedMethod, true),
        Some("Unsupported compression method")
    );
    assert_eq!(
        tester_words(Outcome::WrongPassword, false),
        Some("Unknown extraction error")
    );
    assert_eq!(
        tester_words(Outcome::DataAfterEnd, false),
        Some("Unknown extraction error")
    );
    assert_eq!(tester_words(Outcome::DataError, false), Some("Data error"));
    assert_eq!(
        tester_words(Outcome::CrcError, true),
        Some("Wrong password or CRC check failure")
    );
    assert_eq!(
        tester_words(Outcome::UnsupportedMethod, false),
        Some("Unsupported compression method")
    );
    assert_eq!(
        tester_words(Outcome::HeadersError, true),
        Some("Unknown extraction error")
    );
}

/// The window's last look is the library's whole progress: all it decodes done.
#[test]
fn the_progress_comes_to_its_total() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), include_bytes!("../testdata/a.7z"));
    let files = files(&opened);
    let mut asks = Asks::default();
    test_files(&mut opened, &files, &mut asks).unwrap();
    let (_, done, total) = asks.shown.last().unwrap().clone();
    assert_eq!((done, total), (8, 8), "{:?}", asks.shown);
    assert!(
        asks.shown
            .iter()
            .any(|(file, _, _)| file == r"docs\readme.txt"),
        "{:?}",
        asks.shown
    );
}

/// Esc at any one of the window's looks stops the test.
#[test]
fn esc_at_any_look_stops_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), include_bytes!("../testdata/a.7z"));
    let files = files(&opened);
    let mut counted = Asks::default();
    test_files(&mut opened, &files, &mut counted).unwrap();
    assert!(counted.looks >= 3, "{}", counted.looks);
    for look in 1..=counted.looks {
        let mut asks = Asks {
            stop_at: Some(look),
            ..Asks::default()
        };
        let tested = test_files(&mut opened, &files, &mut asks);
        assert_eq!(tested, Err(Untested::Stopped), "at look {look}");
    }
}

/// A failure is named by the path the caller gave the file (from the folder tested); a file passed
/// over in its solid block by its path from the archive's root.
#[test]
fn a_failure_is_named_by_the_files_path() {
    let temp = tempfile::tempdir().unwrap();
    let mut bytes = include_bytes!("../testdata/a.7z").to_vec();
    // The first file's (`docs\readme.txt`) CRC fails.
    bytes[36] ^= 0x55;
    let mut opened = library_opened(temp.path(), &bytes);
    let index = |path: &str| {
        (opened.items.iter())
            .position(|item| item.path.join("\\") == path)
            .unwrap()
    };
    let (readme, top) = (index(r"docs\readme.txt"), index("top.txt"));
    let named = test_files(
        &mut opened,
        &[(readme, "readme.txt".to_string())],
        &mut Asks::default(),
    );
    let Err(Untested::Failed(fault)) = named else {
        panic!("{named:?}");
    };
    assert_eq!(fault.messages[1], "readme.txt");
    let passed_over = test_files(
        &mut opened,
        &[(top, "top.txt".to_string())],
        &mut Asks::default(),
    );
    let Err(Untested::Failed(fault)) = passed_over else {
        panic!("{passed_over:?}");
    };
    assert_eq!(fault.messages[1], r"docs\readme.txt");
}

/// A password the archive was given is not asked; an empty one is.
#[test]
fn a_known_password_is_not_asked_an_empty_one_is() {
    let temp = tempfile::tempdir().unwrap();
    let locked = include_bytes!("../testdata/data_locked.7z");
    for (known, asked) in [("secret", 0), ("", 1)] {
        let mut opened = library_opened(temp.path(), locked);
        opened.password = Some(known.to_string());
        let files = files(&opened);
        let mut asks = Asks {
            passwords: vec![Some("secret".to_string())],
            ..Asks::default()
        };
        test_files(&mut opened, &files, &mut asks).unwrap();
        assert_eq!(asks.asked, asked, "{known:?}");
        assert_eq!(opened.password.as_deref(), Some("secret"));
    }
}
