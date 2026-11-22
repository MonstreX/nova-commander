use super::*;

/// The include masks as a test's program matches them: `*.zip` only; the exclude ones `b.*`.
fn matches(masks: &str, name: &str) -> Option<bool> {
    Some(match masks {
        "b.*" => name.starts_with("b."),
        _ => name.ends_with(".zip"),
    })
}

/// arclite's `analyse_open`: Enter by the masks, Ctrl+PgDn on any file unless the masks apply to it; the
/// exclude masks when active; nothing with "Handle archive commands" off.
#[test]
fn a_file_is_tried_as_arclite_tries_it() {
    let mut options = Options::default();
    assert!(takes(&options, "a.zip", false, matches));
    assert!(!takes(&options, "a.docx", false, matches));
    assert!(takes(&options, "a.docx", true, matches), "Ctrl+PgDn: any");
    options.pgdn_masks = true;
    assert!(!takes(&options, "a.docx", true, matches));
    options.use_exclude_masks = true;
    options.exclude_masks = "b.*".to_string();
    assert!(!takes(&options, "b.zip", false, matches));
    options.use_include_masks = false;
    assert!(
        takes(&options, "a.docx", false, matches),
        "no include masks: any"
    );
    let broken = |_: &str, _: &str| None;
    options.use_include_masks = true;
    assert!(
        !takes(&options, "a.zip", false, broken),
        "masks with errors"
    );
    options.handle_commands = false;
    assert!(!takes(&options, "a.zip", true, matches));
}

/// arclite's open command: its parameters, a quoted path, the environment's names.
#[test]
fn the_open_command_is_read_as_arclites() {
    let plain = parse("a.zip").unwrap();
    assert_eq!(
        plain,
        OpenCommand {
            detect: false,
            password: None,
            path: "a.zip".to_string()
        }
    );
    let full = parse(r#"-d -t:ZIP -p:"se cret" "C:\my files\a.zip""#).unwrap();
    assert!(full.detect);
    assert_eq!(full.password.as_deref(), Some("se cret"));
    assert_eq!(full.path, r"C:\my files\a.zip");
    // SAFETY-free: the test's own variable.
    let home = std::env::var("SystemRoot").unwrap_or_default();
    let expanded = parse(r"%SystemRoot%\a.zip").unwrap();
    assert_eq!(expanded.path, format!(r"{home}\a.zip"));
    assert_eq!(
        parse("%NO_SUCH_VC_NAME%.zip").unwrap().path,
        "%NO_SUCH_VC_NAME%.zip"
    );
}

/// What arclite refuses: no path, a parameter last, one it does not know, another format, an empty
/// password; another command is not this parser's (`command_line`).
#[test]
fn a_bad_command_is_refused() {
    for text in [
        "",
        "-d",
        "-q a.zip",
        "-t:rar a.zip",
        "-p: a.zip",
        "x -d a.zip",
    ] {
        assert!(parse(text).is_err(), "{text:?}");
    }
    assert!(
        parse("x a.zip")
            .unwrap_err()
            .contains("Not an open command")
    );
    assert!(parse("-q a.zip").unwrap_err().contains(USAGE));
}

#[test]
fn a_relative_path_is_in_the_folder() {
    let folder = Path::new(r"C:\work");
    assert_eq!(
        full_path("a.zip", Some(folder)),
        PathBuf::from(r"C:\work\a.zip")
    );
    assert_eq!(
        full_path(r"D:\b.zip", Some(folder)),
        PathBuf::from(r"D:\b.zip")
    );
    let alone = full_path("a.zip", None);
    assert!(
        alone.is_absolute() && alone.ends_with("a.zip"),
        "the process's folder: {alone:?}"
    );
    // FAR's `get_absolute_path` (`GetFullPathName`): `..` and `.` folded, `/` turned to `\`.
    assert_eq!(
        full_path(r"..\up\.\b.zip", Some(Path::new(r"C:\work\sub"))),
        PathBuf::from(r"C:\work\up\b.zip")
    );
    assert_eq!(
        full_path("E:/w/a.zip", Some(folder)),
        PathBuf::from(r"E:\w\a.zip")
    );
}

/// arclite's `unquote`: only the quotes around the whole text go.
#[test]
fn only_the_outer_quotes_go() {
    assert_eq!(unquote(r#""C:\a b\x.zip""#), r"C:\a b\x.zip");
    assert_eq!(unquote(r#"pa"ss"#), r#"pa"ss"#);
    assert_eq!(unquote(r#""half"#), r#""half"#);
    assert_eq!(unquote(r#""""#), "");
}
