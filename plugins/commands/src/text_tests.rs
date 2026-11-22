use super::*;

#[test]
fn variables_that_are_set_expand_the_others_stay() {
    let path = std::env::var("PATH").unwrap();
    assert_eq!(expand("a%PATH%b"), format!("a{path}b"));
    assert_eq!(expand("%NO_SUCH_VARIABLE_VC%x"), "%NO_SUCH_VARIABLE_VC%x");
    assert_eq!(expand("50% off"), "50% off");
    assert_eq!(expand("%%"), "%%");
}

#[test]
fn quotes_go_and_the_name_starts_after_the_last_separator() {
    assert_eq!(unquote(r#""C:\a b"\c"#), r"C:\a b\c");
    assert_eq!(name_start(r"C:\a\b.txt"), 5);
    assert_eq!(name_start("C:b.txt"), 2);
    assert_eq!(name_start("b.txt"), 0);
    assert_eq!(name_start(r"C:\a\"), 5);
}

#[test]
fn a_name_is_made_whole_against_the_folder() {
    let folder = Path::new(r"C:\work\sub");
    assert_eq!(full_path("x.txt", folder), Path::new(r"C:\work\sub\x.txt"));
    assert_eq!(full_path(r"..\y", folder), Path::new(r"C:\work\y"));
    assert_eq!(full_path(r"D:\z", folder), Path::new(r"D:\z"));
    assert_eq!(full_path(r"\root", folder), Path::new(r"C:\root"));
}

#[test]
fn text_is_read_by_its_mark_utf8_and_utf16() {
    assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'h', b'i'], false), "hi");
    assert_eq!(decode(&[0xFF, 0xFE, b'h', 0, b'i', 0], false), "hi");
    assert_eq!(decode(&[0xFE, 0xFF, 0, b'h', 0, b'i'], false), "hi");
    assert_eq!(decode("привет".as_bytes(), false), "привет");
    assert_eq!(
        decode(&[b'o', 0, b'k', 0], false),
        "ok",
        "UTF-16 without a mark"
    );
    assert_eq!(
        decode(b"up to\0here", false),
        "up to",
        "the first NUL ends it"
    );
    assert_eq!(decode(b"plain", false), "plain");
}

#[test]
fn the_first_line_ends_at_its_break() {
    assert_eq!(first_line("one\r\ntwo"), "one");
    assert_eq!(first_line("one\ntwo"), "one");
    assert_eq!(first_line("only"), "only");
}

#[test]
fn a_file_is_read_up_to_the_most_asked() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("t.txt");
    std::fs::write(&file, "abcdef").unwrap();
    assert_eq!(read_text(&file, 3, false).as_deref(), Some("abc"));
    assert_eq!(read_text(&temp.path().join("none"), 3, true), None);
}
