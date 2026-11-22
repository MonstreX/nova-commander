//! An update's options and rules through 7-Zip's installed library ([`crate::test_archives`]).

use super::*;
use crate::test_archives::{make, opened, zip};

/// arclite's `load_update_props`: a zip's method Deflate (its handler names none), encrypted when an item
/// is; a 7z's its own method and solid; the kept level, F4's put-back overwriting.
#[test]
fn the_archives_method_and_encryption() {
    let temp = tempfile::tempdir().unwrap();
    let plain = temp.path().join("plain.zip");
    zip(&plain, &[("a.txt", Some(b"a")), ("b.txt", Some(b"b"))]);
    let settings = UpdateSettings::default();
    let options = options_for(&opened(&plain), &settings, false);
    assert_eq!(options.method, "Deflate");
    assert!(!options.encrypt);
    assert_eq!(options.kind.format, "zip");
    assert_eq!(options.level, settings.level, "the kept level");
    let secret = temp.path().join("secret.zip");
    make(
        &secret,
        "zip",
        &[("a.txt", Some(b"a"))],
        (false, Some("pw")),
    );
    assert!(options_for(&opened(&secret), &settings, false).encrypt);
    let seven = temp.path().join("a.7z");
    make(
        &seven,
        "7z",
        &[("a.txt", Some(b"aaaa")), ("b.txt", Some(b"bbbb"))],
        (false, None),
    );
    let options = options_for(&opened(&seven), &settings, true);
    assert_eq!(options.kind.format, "7z");
    assert_eq!(options.method, "LZMA2", "7-Zip's default for a 7z");
    assert!(options.kind.solid);
    assert_eq!(
        options.overwrite,
        crate::extract_options::Overwrite::Overwrite
    );
}

/// An archive of the file's own level of a written format is changed (a zip with one name twice too:
/// 7-Zip keeps every item); one inside another (a split's) is not.
#[test]
fn what_is_changeable() {
    let temp = tempfile::tempdir().unwrap();
    let twice = temp.path().join("twice.zip");
    zip(&twice, &[("a.txt", Some(b"one")), ("a.txt", Some(b"two"))]);
    let opened_twice = opened(&twice);
    assert_eq!(opened_twice.items.len(), 2);
    assert!(changeable(&opened_twice).is_ok());
    let tar = temp.path().join("a.tar");
    make(
        &tar,
        "tar",
        &[("big.txt", Some(&[7u8; 3000]))],
        (true, None),
    );
    let bytes = std::fs::read(&tar).unwrap();
    std::fs::write(temp.path().join("v.tar.001"), &bytes[..2048]).unwrap();
    std::fs::write(temp.path().join("v.tar.002"), &bytes[2048..]).unwrap();
    let split = opened(&temp.path().join("v.tar.001"));
    assert_eq!(split.format, "Split→tar");
    let refused = changeable(&split).unwrap_err();
    assert_eq!(refused.message, NOT_WRITTEN_YET);
}

/// A gzip takes only its one file again, by its name.
#[test]
fn a_single_file_archive_takes_its_own_file_only() {
    let temp = tempfile::tempdir().unwrap();
    let gz = temp.path().join("a.txt.gz");
    make(&gz, "gzip", &[("a.txt", Some(b"text"))], (false, None));
    let archive = opened(&gz);
    let request = |sources: &[PathBuf]| -> Result<(), Error> {
        single_file(
            &archive,
            &Request {
                path: "/",
                sources,
                folder: None,
                moving: false,
                mode: GetMode::default(),
            },
        )
    };
    let own = [temp.path().join("a.txt")];
    assert!(request(&own).is_ok());
    let other = [temp.path().join("b.txt")];
    assert_eq!(request(&other).unwrap_err().message, SINGLE_FILE);
    let two = [temp.path().join("a.txt"), temp.path().join("b.txt")];
    assert!(request(&two).is_err());
    assert!(is_single_file_format("xz") && !is_single_file_format("tar"));
}

/// arclite's rule: the sources go only when moved, nothing failed and none was skipped.
#[test]
fn the_sources_go_only_when_all_went_in() {
    let fault = Fault::default();
    assert!(delete_after(true, &[], false));
    assert!(!delete_after(false, &[], false), "copied");
    assert!(!delete_after(true, &[fault], false), "a failure");
    assert!(!delete_after(true, &[], true), "one skipped");
}

/// The way an update stops: the user's stop as Cancelled, a failure with its lines.
#[test]
fn stops_and_failures_read_so() {
    assert_eq!(stopped().kind, ErrorKind::Cancelled);
    let fault = Fault {
        system: Some("Access is denied.".to_string()),
        objects: Vec::new(),
        messages: vec![r"C:\a.zip".to_string()],
        warnings: Vec::new(),
    };
    let error = failed(&fault);
    assert_eq!(error.kind, ErrorKind::Other);
    assert_eq!(error.message, "Access is denied.\nC:\\a.zip");
}
