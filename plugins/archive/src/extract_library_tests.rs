//! Extracting from an archive 7-Zip's library reads (S6.3b, arclite's `ArchiveExtractor`), through
//! 7-Zip's installed library: `testdata/a.7z` (`docs\readme.txt` "hello", `top.txt` "top", their time
//! 2024-01-02 03:04:05 local), `testdata/data_locked.7z` (the same, its data encrypted by "secret").

use nc_plugin_sdk::sevenzip::{Library, Opening};

use super::*;
use crate::library_open::{Request, open};

struct NoAsk;
impl Opening for NoAsk {}

/// `bytes` written into `folder` as `a.7z` and opened as the panel opens it.
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

const SEVEN: &[u8] = include_bytes!("../testdata/a.7z");
const DATA_LOCKED: &[u8] = include_bytes!("../testdata/data_locked.7z");

/// The folder and its file, the file of the root: their data whole, their times the archive's; the
/// progress told by the bytes written of all.
#[test]
fn the_files_are_extracted_through_the_library() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), SEVEN);
    let out = temp.path().join("out");
    let chosen = nodes(&opened, &["docs", "top.txt"]);
    let mut asks = Scripted::default();
    let log = extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(
        std::fs::read(out.join("docs").join("readme.txt")).unwrap(),
        b"hello"
    );
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"top");
    let top = (opened.items.iter())
        .find(|item| item.path == ["top.txt"])
        .unwrap();
    let written = std::fs::metadata(out.join("top.txt"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(Some(written), top.modified);
    assert_eq!(asks.progress.last(), Some(&(8, 8)));
    assert!(asks.asked.is_empty(), "{:?}", asks.asked);
}

/// A file there: asked about; Overwrite writes it anew, Skip keeps it (and is noted as skipped).
#[test]
fn a_file_there_is_asked_about() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), SEVEN);
    let out = temp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("top.txt"), "kept").unwrap();
    let chosen = nodes(&opened, &["top.txt"]);
    let mut asks = Scripted::default();
    asks.overwrites.push_back(Some((Overwrite::Skip, false)));
    let (log, skipped) = extract_noting(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert!(log.is_empty());
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"kept");
    assert_eq!(skipped.len(), 1);
    assert_eq!(asks.asked, ["overwrite top.txt 3 4"]);
    asks.overwrites
        .push_back(Some((Overwrite::Overwrite, false)));
    extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"top");
}

/// Cancel at the question stops the extraction; nothing of the file made, the one there as it was.
#[test]
fn cancel_at_the_question_stops() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), SEVEN);
    let out = temp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("top.txt"), "kept").unwrap();
    let chosen = nodes(&opened, &["docs", "top.txt"]);
    let mut asks = Scripted::default();
    asks.overwrites.push_back(None);
    assert_eq!(
        extract(&mut opened, &chosen, &options(&out), &mut asks),
        Err(Abort)
    );
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"kept");
}

/// Encrypted data: the password asked when the library asks it, once for the archive; the files whole.
#[test]
fn the_password_is_asked_once() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), DATA_LOCKED);
    let out = temp.path().join("out");
    let chosen = nodes(&opened, &["docs", "top.txt"]);
    let mut asks = Scripted::default();
    asks.passwords.push_back(Some("secret".to_string()));
    let log = extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(asks.asked, ["password"]);
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"top");
    assert_eq!(opened.password.as_deref(), Some("secret"));
}

/// A wrong password: the failure in arclite's words for an encrypted archive, Ignore lets it be, the
/// password forgotten (the next failure no longer an encrypted archive's). The solid block's other item,
/// passed over, is told too, by its path from the root (arclite's `GetStream` names every item).
#[test]
fn a_wrong_password_is_told_and_forgotten() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), DATA_LOCKED);
    let out = temp.path().join("out");
    let chosen = nodes(&opened, &["top.txt"]);
    let mut asks = Scripted::default();
    asks.passwords.push_back(Some("nope".to_string()));
    asks.choices.push_back(Choice::Ignore);
    asks.choices.push_back(Choice::Ignore);
    let log = extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert_eq!(log.len(), 2, "{log:?}");
    let readme = out.join("docs").join("readme.txt").display().to_string();
    assert_eq!(log[0].messages[1], readme);
    assert_eq!(log[1].messages[0], "Data error");
    assert_eq!(
        log[1].messages[1],
        out.join("top.txt").display().to_string()
    );
    assert!(
        log[0].messages[0].starts_with("Wrong password or"),
        "{log:?}"
    );
    assert_eq!(log[0].messages[2], opened.path.display().to_string());
    assert!(asks.asked[1].ends_with("retry=false"), "{:?}", asks.asked);
    assert_eq!(opened.password, None);
    assert!(
        !out.join("top.txt").exists(),
        "an item of a block that failed comes to be tested (7-Zip's): no file made"
    );
}

/// A CRC failure answered by Ignore keeps the file with what was written (arclite keeps it); the other
/// file of the block whole.
#[test]
fn a_crc_failure_keeps_the_file_written() {
    let temp = tempfile::tempdir().unwrap();
    let mut bytes = SEVEN.to_vec();
    // `top.txt`'s CRC fails (its data decoded all the same).
    bytes[41] ^= 0x55;
    let mut opened = library_opened(temp.path(), &bytes);
    let out = temp.path().join("out");
    let chosen = nodes(&opened, &["docs", "top.txt"]);
    let mut asks = Scripted::default();
    asks.choices.push_back(Choice::Ignore);
    let log = extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!(log[0].messages[0], "CRC check failed");
    let top = out.join("top.txt");
    assert_eq!(log[0].messages[1], top.display().to_string());
    assert_eq!(std::fs::metadata(&top).unwrap().len(), 3);
    assert_eq!(
        std::fs::read(out.join("docs").join("readme.txt")).unwrap(),
        b"hello"
    );
}

/// One file of a solid block: the progress is the library's, of the whole block it decodes (not of the
/// file's size); the file passed over is not written.
#[test]
fn the_total_is_the_librarys() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), SEVEN);
    let out = temp.path().join("out");
    let chosen = nodes(&opened, &["top.txt"]);
    let mut asks = Scripted::default();
    let log = extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"top");
    assert_eq!(asks.progress.last(), Some(&(8, 8)), "{:?}", asks.progress);
    assert!(!out.join("docs").exists());
}

/// A file there renamed: kept, the archive's beside it under the next free number; added to: the
/// archive's data after its own.
#[test]
fn a_file_there_is_renamed_or_added_to() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), SEVEN);
    let out = temp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("top.txt"), "kept").unwrap();
    let chosen = nodes(&opened, &["top.txt"]);
    let mut asks = Scripted::default();
    asks.overwrites.push_back(Some((Overwrite::Rename, false)));
    let log = extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"kept");
    assert_eq!(std::fs::read(out.join("top.txt.0")).unwrap(), b"top");
    asks.overwrites.push_back(Some((Overwrite::Append, false)));
    let log = extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"kepttop");
}

/// A read-only file there is overwritten or added to: its attributes cleared first.
#[test]
fn a_read_only_file_there_is_overwritten_or_added_to() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), SEVEN);
    let out = temp.path().join("out");
    let top = out.join("top.txt");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(&top, "kept").unwrap();
    let chosen = nodes(&opened, &["top.txt"]);
    for (overwrite, expected) in [
        (Overwrite::Overwrite, &b"top"[..]),
        (Overwrite::Append, b"toptop"),
    ] {
        nc_plugin_sdk::system::set_attributes(&top, 0x1).unwrap();
        let given = ExtractOptions {
            overwrite,
            ..options(&out)
        };
        let log = extract(&mut opened, &chosen, &given, &mut Scripted::default()).unwrap();
        assert!(log.is_empty(), "{overwrite:?} {log:?}");
        assert_eq!(std::fs::read(&top).unwrap(), expected, "{overwrite:?}");
    }
    let _ = nc_plugin_sdk::system::set_attributes(&top, 0);
}

/// A password the archive was given is not asked; an empty one is (arclite asks while it is empty).
#[test]
fn a_known_password_is_not_asked_an_empty_one_is() {
    let temp = tempfile::tempdir().unwrap();
    let out = temp.path().join("out");
    for (known, asked) in [("secret", 0), ("", 1)] {
        let mut opened = library_opened(temp.path(), DATA_LOCKED);
        opened.password = Some(known.to_string());
        let chosen = nodes(&opened, &["top.txt"]);
        let mut asks = Scripted::default();
        asks.passwords.push_back(Some("secret".to_string()));
        let given = ExtractOptions {
            overwrite: Overwrite::Overwrite,
            ..options(&out)
        };
        let log = extract(&mut opened, &chosen, &given, &mut asks).unwrap();
        assert!(log.is_empty(), "{known:?} {log:?}");
        assert_eq!(asks.asked.len(), asked, "{known:?} {:?}", asks.asked);
        assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"top");
        assert_eq!(opened.password.as_deref(), Some("secret"));
    }
}

/// `testdata/anti.7z` (an update's difference archive of 7-Zip, `-up0q3`): `gone.txt` an anti item. It
/// deletes the file it stands for and makes none.
#[test]
fn an_anti_item_deletes_its_file() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), include_bytes!("../testdata/anti.7z"));
    let out = temp.path().join("out");
    let chosen = nodes(&opened, &["gone.txt"]);
    let given = ExtractOptions {
        overwrite: Overwrite::Overwrite,
        ..options(&out)
    };
    let log = extract(&mut opened, &chosen, &given, &mut Scripted::default()).unwrap();
    assert!(log.is_empty(), "{log:?}");
    assert!(!out.join("gone.txt").exists(), "none made");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("gone.txt"), "old").unwrap();
    let log = extract(&mut opened, &chosen, &given, &mut Scripted::default()).unwrap();
    assert!(log.is_empty(), "{log:?}");
    assert!(!out.join("gone.txt").exists(), "deleted");
}

/// A failed outcome answered by Cancel stops; the file it was written into goes (arclite's cache drops
/// the open file as the run aborts).
#[test]
fn cancel_at_a_failed_outcome_leaves_no_file() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), DATA_LOCKED);
    let out = temp.path().join("out");
    let chosen = nodes(&opened, &["top.txt"]);
    let mut asks = Scripted::default();
    asks.passwords.push_back(Some("nope".to_string()));
    asks.choices.push_back(Choice::Ignore);
    asks.choices.push_back(Choice::Cancel);
    assert_eq!(
        extract(&mut opened, &chosen, &options(&out), &mut asks),
        Err(Abort)
    );
    assert!(!out.join("top.txt").exists());
}

/// A large file: the progress is the library's (its bytes decoded of all), Esc seen while it decodes;
/// the file stopped in goes.
#[test]
fn the_progress_is_the_librarys_and_esc_stops_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), include_bytes!("../testdata/big.7z"));
    let out = temp.path().join("out");
    let chosen = nodes(&opened, &["big.bin"]);
    let mut asks = Scripted::default();
    extract(&mut opened, &chosen, &options(&out), &mut asks).unwrap();
    let big = std::fs::metadata(out.join("big.bin")).unwrap().len();
    assert_eq!(asks.progress.last(), Some(&(big, big)));
    assert!(asks.progress.len() > 2, "{:?}", asks.progress);
    let again = temp.path().join("again");
    let mut asks = Scripted {
        stop_after: Some(4),
        ..Scripted::default()
    };
    assert_eq!(
        extract(&mut opened, &chosen, &options(&again), &mut asks),
        Err(Abort)
    );
    assert!(!again.join("big.bin").exists());
}

/// Esc at any one look of the extraction stops it (the progress's, an item's, a piece's), and the file
/// it was writing goes.
#[test]
fn esc_at_any_look_stops_and_leaves_no_file() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), include_bytes!("../testdata/big.7z"));
    let chosen = nodes(&opened, &["big.bin"]);
    let mut counted = Scripted::default();
    let out = temp.path().join("counted");
    extract(&mut opened, &chosen, &options(&out), &mut counted).unwrap();
    assert!(counted.looks >= 4, "{}", counted.looks);
    let whole = std::fs::metadata(out.join("big.bin")).unwrap().len();
    for look in 1..=counted.looks {
        let out = temp.path().join(format!("at{look}"));
        let mut asks = Scripted {
            esc_at: Some(look),
            ..Scripted::default()
        };
        let stopped = extract(&mut opened, &chosen, &options(&out), &mut asks);
        assert_eq!(stopped, Err(Abort), "at look {look}");
        // Esc at the last look comes after the file was whole: it is kept then, never half written.
        let left = std::fs::metadata(out.join("big.bin"))
            .map(|file| file.len())
            .ok();
        assert!(
            left.is_none() || (look == counted.looks && left == Some(whole)),
            "at look {look}: {left:?}"
        );
    }
}

/// Cancel at the password stops; Esc during it too.
#[test]
fn no_password_or_esc_stops() {
    let temp = tempfile::tempdir().unwrap();
    let mut opened = library_opened(temp.path(), DATA_LOCKED);
    let out = temp.path().join("out");
    let chosen = nodes(&opened, &["top.txt"]);
    let mut asks = Scripted::default();
    asks.passwords.push_back(None);
    assert_eq!(
        extract(&mut opened, &chosen, &options(&out), &mut asks),
        Err(Abort)
    );
    let mut opened = library_opened(temp.path(), SEVEN);
    let mut asks = Scripted {
        stop: true,
        ..Scripted::default()
    };
    assert_eq!(
        extract(&mut opened, &chosen, &options(&out), &mut asks),
        Err(Abort)
    );
    assert!(!out.join("top.txt").exists());
}

/// arclite's words for each outcome, an encrypted archive's apart; none for a good one.
#[test]
fn outcomes_are_in_arclites_words() {
    use nc_plugin_sdk::sevenzip::Outcome;
    let words = crate::extract::library::outcome_words;
    assert_eq!(words(Outcome::Ok, true), None);
    assert_eq!(words(Outcome::DataAfterEnd, false), None);
    assert_eq!(words(Outcome::DataError, false), Some("Data error"));
    assert_eq!(
        words(Outcome::DataError, true),
        Some("Wrong password or data error")
    );
    assert_eq!(words(Outcome::CrcError, false), Some("CRC check failed"));
    assert_eq!(
        words(Outcome::CrcError, true),
        Some("Wrong password or CRC check failure")
    );
    assert_eq!(
        words(Outcome::UnsupportedMethod, false),
        Some("Unsupported compression method")
    );
    assert_eq!(words(Outcome::Unavailable, false), Some("Unavailable data"));
    assert_eq!(
        words(Outcome::UnexpectedEnd, false),
        Some("Unexpected end of data")
    );
    assert_eq!(words(Outcome::IsNotArc, false), Some("Is not archive"));
    assert_eq!(words(Outcome::HeadersError, false), Some("Headers Error"));
    assert_eq!(words(Outcome::WrongPassword, false), Some("Wrong password"));
    assert_eq!(
        words(Outcome::Other(42), false),
        Some("Unknown extraction error")
    );
}
