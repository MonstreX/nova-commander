//! List files as FAR's TmpPanel reads them: their encodings, lines, `%VAR%`, `|label|`, relative names,
//! devices and where a list named on the command line is found.

use super::*;

/// The OEM page as Latin-1 (enough to tell it was asked).
fn oem(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

fn utf16le(text: &str, bom: bool) -> Vec<u8> {
    let mut bytes = if bom { vec![0xFF, 0xFE] } else { Vec::new() };
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}

#[test]
fn lines_split_at_cr_or_lf_and_the_empty_ones_drop() {
    let read = lines(b"a.txt\r\n\r\n  b.txt \n\"c d\"\rlast", &oem);
    assert_eq!(
        read,
        ["a.txt", "  b.txt ", "\"c d\"", "last"],
        "nothing trimmed"
    );
    assert!(lines(b"", &oem).is_empty());
    assert!(lines(b"\r\n\n", &oem).is_empty());
}

#[test]
fn the_byte_order_marks_say_the_encoding() {
    assert_eq!(lines(&utf16le("пап\r\nb", true), &oem), ["пап", "b"]);
    let mut big = vec![0xFE, 0xFF];
    big.extend("п\nb".encode_utf16().flat_map(u16::to_be_bytes));
    assert_eq!(lines(&big, &oem), ["п", "b"]);
    let mut utf8 = vec![0xEF, 0xBB, 0xBF];
    utf8.extend("файл.txt\n".as_bytes());
    assert_eq!(lines(&utf8, &oem), ["файл.txt"]);
}

#[test]
fn without_a_mark_utf8_utf16_or_the_oem_page() {
    assert_eq!(
        lines("файл\n".as_bytes(), &oem),
        ["файл"],
        "UTF-8 by its bytes"
    );
    assert_eq!(
        lines(&utf16le("ab\r\ncd", false), &oem),
        ["ab", "cd"],
        "UTF-16 by its zeros"
    );
    // 0xAF is no UTF-8: the OEM page reads it.
    assert_eq!(lines(&[0x61, 0xAF, b'\n', 0x62], &oem), ["a\u{af}", "b"]);
    assert_eq!(
        lines(b"plain\n", &oem),
        ["plain"],
        "ASCII: the OEM page, the same"
    );
}

#[test]
fn utf8_is_told_as_far_tells_it() {
    assert!(is_text_utf8("ф".as_bytes()));
    assert!(
        !is_text_utf8(b"ascii only"),
        "FAR's IsTextUTF8 wants more than ASCII"
    );
    assert!(!is_text_utf8(&[0xC0, 0x41]), "a continuation missing");
    assert!(!is_text_utf8(&[0x80]), "a lone continuation");
    assert!(
        is_text_utf8(&[0x61, 0xD1]),
        "a character cut at the very end let be"
    );
}

#[test]
fn variables_are_expanded_and_others_left() {
    let lookup = |name: &str| (name == "LISTS").then(|| r"C:\lists".to_string());
    let expand = |text: &str| expand_with(text, &lookup);
    assert_eq!(expand(r"%LISTS%\a.temp"), r"C:\lists\a.temp");
    assert_eq!(expand(r"%NOT_SET%\%LISTS%"), r"%NOT_SET%\C:\lists");
    assert_eq!(expand("50% off"), "50% off");
    assert_eq!(expand("%%"), "%%");
    assert_eq!(expand("%LISTS"), "%LISTS", "no closing %");
}

#[test]
fn a_label_is_between_the_first_two_bars() {
    assert_eq!(parse_param("|Docs|  C:\\docs"), Some(("Docs", "C:\\docs")));
    assert_eq!(parse_param("|-|"), Some(("-", "")));
    assert_eq!(parse_param("C:\\docs"), None);
    assert_eq!(parse_param("|no end"), None);
}

#[test]
fn devices_are_named_as_far_names_them() {
    for device in [
        r"\\.\C:",
        r"\\.\PhysicalDrive0",
        r"\\.\cdrom12",
        r"\\.\physicaldrive3",
    ] {
        assert!(is_device(device), "{device}");
    }
    for other in [
        r"\\.\C:\x",
        r"\\.\PhysicalDrive",
        r"\\.\cdromA",
        r"C:",
        r"\\?\C:",
    ] {
        assert!(!is_device(other), "{other}");
    }
}

#[test]
fn a_lines_item_is_its_whole_path_from_the_folder() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    let folder = Some(temp.path());
    let a = temp.path().join("a.txt");
    assert_eq!(item("a.txt", folder, false), Some(a.clone()));
    assert_eq!(
        item(r"sub\..\a.txt", folder, false),
        Some(a.clone()),
        ".. gone"
    );
    assert_eq!(
        item("|label| a.txt", folder, false),
        Some(a.clone()),
        "the label dropped"
    );
    assert_eq!(item(&a.display().to_string(), None, false), Some(a.clone()));
    assert_eq!(item("gone.txt", folder, false), None);
    assert_eq!(
        item("|x| not a path", folder, true),
        Some(PathBuf::from("not a path")),
        "any elements: as written"
    );
    assert_eq!(
        item(r"\\.\C:", folder, false),
        Some(PathBuf::from(r"\\.\C:"))
    );
}

#[test]
fn a_list_is_found_in_the_folder_then_the_plugins_then_the_programs() {
    let temp = tempfile::tempdir().unwrap();
    let program = temp.path();
    let plugin = program.join("plugins").join("tmppanel");
    std::fs::create_dir_all(&plugin).unwrap();
    let work = program.join("work");
    std::fs::create_dir(&work).unwrap();
    std::fs::write(work.join("here.temp"), "x").unwrap();
    std::fs::write(plugin.join("own.temp"), "x").unwrap();
    std::fs::write(program.join("home.temp"), "x").unwrap();
    let found = |name: &str| find(name, Some(&work), &plugin);
    assert_eq!(found("here.temp"), Some(work.join("here.temp")));
    assert_eq!(found("own.temp"), Some(plugin.join("own.temp")));
    assert_eq!(found("home.temp"), Some(program.join("home.temp")));
    assert_eq!(found("nowhere.temp"), None);
}
