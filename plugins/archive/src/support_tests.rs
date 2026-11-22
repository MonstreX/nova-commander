//! The CRC and an archive read back.

use super::*;

/// The standard check value of CRC-32 and an empty one.
#[test]
fn the_crc_is_zips() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
    assert_eq!(crc32(b"x"), 0x8CDC_1683);
}

/// An archive read back by its names; a file that is none gives none.
#[test]
fn an_archive_is_read_back() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.zip");
    crate::test_archives::zip(
        &path,
        &[("docs/a.txt", Some(b"a")), ("top.txt", Some(b"t"))],
    );
    assert_eq!(names_in(&path), ["docs/a.txt", "top.txt"]);
    let plain = temp.path().join("plain.txt");
    std::fs::write(&plain, "plain").unwrap();
    assert!(names_in(&plain).is_empty());
    assert!(opened(&plain).is_none());
}

/// A zip of stored files reads back as written.
#[test]
fn a_zip_of_stored_files_reads_back() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("made.zip");
    let zip = zip_of(&[("docs/readme.txt", b"hello"), ("top.txt", b"top")]);
    std::fs::write(&path, zip).unwrap();
    assert_eq!(names_in(&path), ["docs/readme.txt", "top.txt"]);
}

/// A field past its width is its largest value, one within it itself.
#[test]
fn a_zips_field_past_its_width_is_its_largest() {
    assert_eq!(u32_of(7), 7);
    assert_eq!(u32_of(usize::MAX), u32::MAX);
    assert_eq!(u16_of(7), 7);
    assert_eq!(u16_of(1 << 16), u16::MAX);
}
