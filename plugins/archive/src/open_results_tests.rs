//! arclite's words for the open flags; an archive with data after its end, through the library.

use super::*;
use crate::test_archives::{make, opened};

/// Each flag in arclite's words, in its order; bits it does not know as one number.
#[test]
fn flags_are_arclites_words() {
    assert!(flag_texts(0).is_empty());
    assert_eq!(flag_texts(2), ["Headers Error"]);
    assert_eq!(
        flag_texts(1 | 64 | 1024),
        [
            "Is not archive",
            "There are some data after the end of the payload data",
            "CRC check failed"
        ]
    );
    assert_eq!(
        flag_texts(32 | 0x1_0000),
        ["Unexpected end of data", "Unknown error: 65536"]
    );
}

/// A 7z with bytes after it: the warning of data after its end; the same archive whole: nothing; inside
/// another (no file size of its own): no size compared.
#[test]
fn data_after_the_end_is_a_warning() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.7z");
    make(&path, "7z", &[("a.txt", Some(b"alpha"))], (false, None));
    let whole = opened(&path);
    let (chain, _) = whole.library().unwrap();
    assert_eq!(
        read(chain.archive(), Some(whole.file.size)),
        OpenResults::default()
    );
    drop(whole);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(b"trailing bytes");
    std::fs::write(&path, &bytes).unwrap();
    let tailed = opened(&path);
    let (chain, _) = tailed.library().unwrap();
    let results = read(chain.archive(), Some(tailed.file.size));
    assert!(results.errors.is_empty(), "{results:?}");
    assert!(
        results
            .warnings
            .iter()
            .any(|warning| warning.starts_with("There are some data")),
        "{results:?}"
    );
    assert!(read(chain.archive(), None).warnings.len() <= results.warnings.len());
}

/// Every flag in arclite's order, each once.
#[test]
fn every_flag_has_its_words() {
    assert_eq!(
        flag_texts(0x7FF),
        [
            "Is not archive",
            "Headers Error",
            "EncryptedHeadersError",
            "Unavailable data",
            "UnconfirmedStart",
            "Unexpected end of data",
            "There are some data after the end of the payload data",
            "Unsupported compression method",
            "UnsupportedFeature",
            "Data error",
            "CRC check failed",
        ]
    );
    assert_eq!(flag_texts(0x800), ["Unknown error: 2048"]);
}

/// An archive longer than its file: the error of an unexpected end, no warning; the file's own size:
/// nothing; no size compared for one inside another.
#[test]
fn an_archive_longer_than_its_file_is_an_error() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.7z");
    make(&path, "7z", &[("a.txt", Some(b"alpha"))], (false, None));
    let whole = opened(&path);
    let (chain, _) = whole.library().unwrap();
    let size = whole.file.size;
    let shorter = read(chain.archive(), Some(size - 1));
    assert_eq!(shorter.errors, ["Unexpected end of data"]);
    assert!(shorter.warnings.is_empty(), "{shorter:?}");
    let longer = read(chain.archive(), Some(size + 1));
    assert!(longer.errors.is_empty(), "{longer:?}");
    assert_eq!(
        longer.warnings,
        ["There are some data after the end of the payload data"]
    );
    assert_eq!(read(chain.archive(), None), OpenResults::default());
}
