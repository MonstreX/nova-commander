use std::collections::VecDeque;
use std::path::Path;

use super::*;
use crate::open_results::OpenResults;
use crate::test_archives::{Entry, make, opened};

/// The answers the test gives and what it was told.
#[derive(Default)]
struct Scripted {
    passwords: VecDeque<Option<String>>,
    asked: Vec<String>,
    files: Vec<String>,
    progress: Vec<(u64, u64)>,
    /// Esc after this many looks.
    stop_after: Option<usize>,
}

impl TestAsks for Scripted {
    fn password(&mut self) -> Option<String> {
        self.asked.push("password".to_string());
        self.passwords.pop_front().flatten()
    }

    fn testing(&mut self, file: &str, done: u64, total: u64) {
        // The library's progress before its first item names none.
        if !file.is_empty() && self.files.last().is_none_or(|last| last != file) {
            self.files.push(file.to_string());
        }
        self.progress.push((done, total));
    }

    fn stopped(&mut self) -> bool {
        match self.stop_after {
            Some(0) => true,
            Some(left) => {
                self.stop_after = Some(left - 1);
                false
            }
            None => false,
        }
    }
}

/// A zip of `files` (name, data; a name ending in `/` a folder), stored through 7-Zip's library; every
/// file encrypted by `pw` when one is named `secret.txt`.
fn zip_of(folder: &Path, files: &[(&str, &[u8])]) -> std::path::PathBuf {
    let path = folder.join("t.zip");
    let entries: Vec<Entry<'_>> = (files.iter())
        .map(|(name, data)| match name.strip_suffix('/') {
            Some(folder) => (folder, None),
            None => (*name, Some(*data)),
        })
        .collect();
    let secret = files.iter().any(|(name, _)| *name == "secret.txt");
    make(&path, "zip", &entries, (true, secret.then_some("pw")));
    path
}

fn open(path: &Path) -> Opened {
    opened(path)
}

/// The root's entries.
fn root(opened: &Opened) -> Vec<usize> {
    opened.tree.nodes[0].children.clone()
}

/// Every file under the entries read through in the archive's order, their paths from the folder
/// tested, the bytes counted against the files' whole size; nothing asked.
#[test]
fn every_file_under_the_entries_is_read_through_in_the_archives_order() {
    let temp = tempfile::tempdir().unwrap();
    let files: [(&str, &[u8]); 4] = [
        ("top.txt", b"top"),
        ("docs/", b""),
        ("docs/sub/b.txt", b"beta"),
        ("docs/a.txt", b"alpha"),
    ];
    let mut opened = open(&zip_of(temp.path(), &files));
    let mut asks = Scripted::default();
    let entries = root(&opened);
    assert_eq!(test(&mut opened, &entries, &mut asks), Ok(()));
    assert_eq!(asks.files, ["top.txt", r"docs\sub\b.txt", r"docs\a.txt"]);
    assert!(asks.asked.is_empty());
    assert_eq!(asks.progress.last(), Some(&(12, 12)));
    // A folder's files from that folder: "docs" alone.
    let docs = opened.tree.find("/docs").unwrap();
    let mut asks = Scripted::default();
    assert_eq!(test(&mut opened, &[docs], &mut asks), Ok(()));
    assert_eq!(asks.files, [r"docs\sub\b.txt", r"docs\a.txt"]);
    assert_eq!(asks.progress.last(), Some(&(9, 9)));
}

/// A CRC failure ends the test in arclite's words: the file's path from the folder tested, the
/// archive's; the files after it are not read.
#[test]
fn a_crc_failure_ends_the_test_in_arclites_words() {
    let temp = tempfile::tempdir().unwrap();
    let files: [(&str, &[u8]); 2] = [("data.txt", b"hello world"), ("next.txt", b"next")];
    let path = zip_of(temp.path(), &files);
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes.windows(11).position(|w| w == b"hello world").unwrap();
    bytes[at + 4] = b'X';
    std::fs::write(&path, bytes).unwrap();
    let mut opened = open(&path);
    let mut asks = Scripted::default();
    let entries = root(&opened);
    let fault = Fault::message(vec![
        "CRC check failed".to_string(),
        "data.txt".to_string(),
        path.display().to_string(),
    ]);
    assert_eq!(
        test(&mut opened, &entries, &mut asks),
        Err(Untested::Failed(fault))
    );
    assert_eq!(asks.files, ["data.txt"], "next.txt not read");
}

/// An encrypted file asks the archive's password once; Cancel stops the test; a wrong one fails as
/// arclite says it for an encrypted file.
#[test]
fn an_encrypted_file_asks_the_password_once() {
    let temp = tempfile::tempdir().unwrap();
    let files: [(&str, &[u8]); 2] = [("secret.txt", b"hidden"), ("plain.txt", b"plain")];
    let path = zip_of(temp.path(), &files);
    let mut opened = open(&path);
    let entries = root(&opened);
    let mut asks = Scripted::default();
    asks.passwords.push_back(None);
    assert_eq!(
        test(&mut opened, &entries, &mut asks),
        Err(Untested::Stopped)
    );
    let mut asks = Scripted::default();
    asks.passwords.push_back(Some("pw".to_string()));
    assert_eq!(test(&mut opened, &entries, &mut asks), Ok(()));
    assert_eq!(asks.asked, ["password"]);
    assert_eq!(opened.password.as_deref(), Some("pw"));
    // Given already: not asked again.
    let mut asks = Scripted::default();
    assert_eq!(test(&mut opened, &entries, &mut asks), Ok(()));
    assert!(asks.asked.is_empty());
    // A wrong one: the first file fails in the tester's words.
    let mut wrong = open(&path);
    wrong.password = Some("bad".to_string());
    let mut asks = Scripted::default();
    match test(&mut wrong, &entries, &mut asks) {
        Err(Untested::Failed(fault)) => {
            assert!(!fault.messages[0].is_empty(), "{fault:?}");
            assert_eq!(fault.messages[1], "secret.txt");
        }
        other => panic!("{other:?}"),
    }
}

/// Esc stops the test before the next file, or inside one.
#[test]
fn esc_stops_the_test() {
    let temp = tempfile::tempdir().unwrap();
    let files: [(&str, &[u8]); 2] = [("a.txt", b"a"), ("b.txt", b"b")];
    let mut opened = open(&zip_of(temp.path(), &files));
    let entries = root(&opened);
    for after in [0, 1, 2] {
        let mut asks = Scripted {
            stop_after: Some(after),
            ..Scripted::default()
        };
        assert_eq!(
            test(&mut opened, &entries, &mut asks),
            Err(Untested::Stopped),
            "{after}"
        );
    }
}

/// Every call of `testing` as it came.
#[derive(Default)]
struct Told {
    calls: Vec<(String, u64, u64)>,
}

impl TestAsks for Told {
    fn password(&mut self) -> Option<String> {
        None
    }

    fn testing(&mut self, file: &str, done: u64, total: u64) {
        self.calls.push((file.to_string(), done, total));
    }

    fn stopped(&mut self) -> bool {
        false
    }
}

fn told(file: &str, done: u64, total: u64) -> (String, u64, u64) {
    (file.to_string(), done, total)
}

/// Each file told at its start, in the archive's order; the progress the library's, its last the total
/// of the files' whole size.
#[test]
fn the_progress_counts_each_files_bytes_from_its_start() {
    let temp = tempfile::tempdir().unwrap();
    let files: [(&str, &[u8]); 4] = [
        ("top.txt", b"top"),
        ("docs/", b""),
        ("docs/b.txt", b"beta"),
        ("docs/a.txt", b"alpha"),
    ];
    let mut opened = open(&zip_of(temp.path(), &files));
    let entries = root(&opened);
    let mut asks = Told::default();
    assert_eq!(test(&mut opened, &entries, &mut asks), Ok(()));
    let mut files: Vec<&str> = asks
        .calls
        .iter()
        .map(|(file, _, _)| file.as_str())
        .filter(|file| !file.is_empty())
        .collect();
    files.dedup();
    assert_eq!(files, ["top.txt", r"docs\b.txt", r"docs\a.txt"]);
    assert_eq!(
        asks.calls.last().map(|(_, done, total)| (*done, *total)),
        Some((12, 12))
    );
    // An empty file: told too.
    let files: [(&str, &[u8]); 2] = [("empty.txt", b""), ("one.txt", b"1")];
    let mut opened = open(&zip_of(temp.path(), &files));
    let entries = root(&opened);
    let mut asks = Told::default();
    assert_eq!(test(&mut opened, &entries, &mut asks), Ok(()));
    assert!(
        asks.calls.contains(&told("empty.txt", 0, 1)),
        "{:?}",
        asks.calls
    );
    assert_eq!(asks.calls.last().map(|(_, done, _)| *done), Some(1));
}

/// An item reached twice (a folder handed twice, a file handed with its folder) is tested once, by the
/// path it was reached first, and counted once in the total.
#[test]
fn an_item_reached_twice_is_tested_once() {
    let temp = tempfile::tempdir().unwrap();
    let files: [(&str, &[u8]); 3] = [
        ("docs/", b""),
        ("docs/a.txt", b"alpha"),
        ("docs/b.txt", b"beta"),
    ];
    let mut opened = open(&zip_of(temp.path(), &files));
    let docs = opened.tree.find("/docs").unwrap();
    let a = opened.tree.find("/docs/a.txt").unwrap();
    let mut asks = Told::default();
    assert_eq!(test(&mut opened, &[docs, a, docs], &mut asks), Ok(()));
    let mut files: Vec<&str> = asks
        .calls
        .iter()
        .map(|(file, _, _)| file.as_str())
        .filter(|file| !file.is_empty())
        .collect();
    files.dedup();
    assert_eq!(files, [r"docs\a.txt", r"docs\b.txt"], "each once");
    assert_eq!(
        asks.calls.last().map(|(_, done, total)| (*done, *total)),
        Some((9, 9))
    );
    // The file first, its folder after: the file's own path.
    let mut asks = Told::default();
    assert_eq!(test(&mut opened, &[a, docs], &mut asks), Ok(()));
    assert!(
        asks.calls.iter().any(|(file, _, _)| file == "a.txt"),
        "{:?}",
        asks.calls
    );
}

/// Nothing handed: nothing told, nothing asked.
#[test]
fn no_entries_test_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let files: [(&str, &[u8]); 1] = [("a.txt", b"a")];
    let mut opened = open(&zip_of(temp.path(), &files));
    let mut asks = Told::default();
    assert_eq!(test(&mut opened, &[], &mut asks), Ok(()));
    assert!(asks.calls.is_empty());
}

/// A 7z with bytes after its end: its files read well, yet the open warning fails the test (arclite's
/// `SetResults`); a CRC failure after it: the warning with the failure's lines.
#[test]
fn an_open_warning_fails_the_test() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("t.7z");
    make(&path, "7z", &[("a.txt", Some(b"alpha"))], (false, None));
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(b"trailing bytes");
    std::fs::write(&path, &bytes).unwrap();
    let mut opened = open(&path);
    let mut asks = Scripted::default();
    let entries = root(&opened);
    let Err(Untested::Failed(fault)) = test(&mut opened, &entries, &mut asks) else {
        panic!("the warning fails the test");
    };
    assert_eq!(asks.files, ["a.txt"], "the files read all the same");
    assert!(fault.messages.is_empty(), "{fault:?}");
    assert!(
        (fault.warnings.iter()).any(|warning| warning.starts_with("There are some data")),
        "{fault:?}"
    );
    let crc = Fault::message(vec!["CRC check failed".to_string()]);
    let warned = OpenResults {
        errors: vec!["Headers Error".to_string()],
        warnings: vec!["UnconfirmedStart".to_string()],
    };
    let merged = with_open_results(Err(Untested::Failed(crc)), warned.clone());
    let Err(Untested::Failed(merged)) = merged else {
        panic!("still failed");
    };
    assert_eq!(merged.messages, ["Headers Error", "CRC check failed"]);
    assert_eq!(merged.warnings, ["UnconfirmedStart"]);
    assert_eq!(
        with_open_results(Err(Untested::Stopped), warned),
        Err(Untested::Stopped)
    );
    assert_eq!(with_open_results(Ok(()), OpenResults::default()), Ok(()));
}

/// An open error alone fails a test that read well, its words the failure's; a failure with nothing of
/// the opening is told as it is, its file and system's words kept.
#[test]
fn an_open_error_alone_fails_the_test() {
    let errors = OpenResults {
        errors: vec!["Headers Error".to_string()],
        warnings: Vec::new(),
    };
    let Err(Untested::Failed(fault)) = with_open_results(Ok(()), errors) else {
        panic!("the error fails the test");
    };
    assert_eq!(fault.messages, ["Headers Error"]);
    assert!(fault.warnings.is_empty());
    let failed = Fault {
        system: Some("Data error".to_string()),
        objects: vec!["a.txt".to_string()],
        messages: vec!["CRC check failed".to_string()],
        warnings: Vec::new(),
    };
    assert_eq!(
        with_open_results(
            Err(Untested::Failed(failed.clone())),
            OpenResults::default()
        ),
        Err(Untested::Failed(failed))
    );
}

/// A 7z after a stub (an SFX's program): its size counted from its place, so a whole one tests well.
#[test]
fn a_7z_after_a_stub_tests_well() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("t.7z");
    make(&path, "7z", &[("a.txt", Some(b"alpha"))], (false, None));
    let mut bytes = vec![0x4Du8, 0x5A];
    bytes.resize(4096, 0);
    bytes.extend_from_slice(&std::fs::read(&path).unwrap());
    let stubbed = temp.path().join("t.exe");
    std::fs::write(&stubbed, &bytes).unwrap();
    let mut opened = open(&stubbed);
    let (chain, _) = opened.library().unwrap();
    assert_eq!(chain.parts[0].offset, 4096, "opened after the stub");
    let entries = root(&opened);
    let mut asks = Scripted::default();
    assert_eq!(test(&mut opened, &entries, &mut asks), Ok(()));
    assert_eq!(asks.files, ["a.txt"]);
}

/// The speed is the bytes read by the time taken: 512 bytes in two seconds, 256 a second.
#[test]
fn the_windows_speed_is_the_bytes_by_the_time() {
    let (_, lines) = text("x", 512, 2048, Duration::from_secs(2));
    let sizes = format!(
        "{:>7} / {} @ {:>9}",
        data_size(512, &SIZES),
        data_size(2048, &SIZES),
        data_size(256, &SPEEDS)
    );
    assert_eq!(lines[1], sizes);
    let (_, lines) = text("x", 512, 2048, Duration::from_millis(500));
    assert!(
        lines[1].ends_with(&format!("{:>9}", data_size(1024, &SPEEDS))),
        "{}",
        lines[1]
    );
    let (_, lines) = text("x", 512, 2048, Duration::ZERO);
    assert!(
        lines[1].ends_with(&format!("{:>9}", data_size(0, &SPEEDS))),
        "no time yet: no speed; {}",
        lines[1]
    );
}

/// The window as arclite's tester writes it.
#[test]
fn the_window_is_arclites_testers() {
    let (title, lines) = text(r"docs\a.txt", 512, 2048, Duration::from_secs(1));
    assert_eq!(title, "{25%} Testing archive...");
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], r"docs\a.txt");
    assert!(lines[1].contains(" / "), "{}", lines[1]);
    assert!(lines[1].contains('@'), "{}", lines[1]);
    assert_eq!(lines[2].chars().count(), WIDTH);
    let (none, _) = text("x", 0, 0, Duration::ZERO);
    assert_eq!(none, "{0%} Testing archive...");
}
