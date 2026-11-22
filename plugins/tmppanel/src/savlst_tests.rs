//! SavLst's file as FAR's `SaveListFile` writes it: UTF-8 with its BOM, or UTF-16 LE with its BOM, each
//! path and CR LF.

use super::*;

fn paths() -> Vec<PathBuf> {
    vec![PathBuf::from(r"C:\a.txt"), PathBuf::from(r"D:\ж\b")]
}

#[test]
fn utf8_has_its_bom_and_cr_lf() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("list.temp");
    save(&target, &paths(), true).unwrap();
    let bytes = std::fs::read(&target).unwrap();
    assert_eq!(&bytes[..3], [0xEF, 0xBB, 0xBF]);
    assert_eq!(
        String::from_utf8(bytes[3..].to_vec()).unwrap(),
        "C:\\a.txt\r\nD:\\ж\\b\r\n"
    );
}

#[test]
fn utf16_has_its_bom_and_cr_lf_little_endian() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("list.temp");
    save(&target, &paths(), false).unwrap();
    let bytes = std::fs::read(&target).unwrap();
    assert_eq!(&bytes[..2], [0xFF, 0xFE], "FAR's SIGN_UNICODE");
    let units: Vec<u16> = (bytes[2..].chunks(2))
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    assert_eq!(
        String::from_utf16(&units).unwrap(),
        "C:\\a.txt\r\nD:\\ж\\b\r\n"
    );
}

#[test]
fn a_target_that_cannot_be_written_is_an_error() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("no such folder").join("list.temp");
    assert!(save(&target, &paths(), true).is_err());
}

/// FAR writes the name's own `wchar_t`s: an unpaired surrogate stays as it is.
#[cfg(windows)]
#[test]
fn utf16_keeps_a_names_own_units() {
    use std::os::windows::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("list.temp");
    let odd = PathBuf::from(std::ffi::OsString::from_wide(&[0x61, 0xD800]));
    save(&target, &[odd], false).unwrap();
    let bytes = std::fs::read(&target).unwrap();
    assert_eq!(bytes, [0xFF, 0xFE, 0x61, 0, 0x00, 0xD8, b'\r', 0, b'\n', 0]);
}
