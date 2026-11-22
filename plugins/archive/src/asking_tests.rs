use std::time::{Duration, UNIX_EPOCH};

use nc_plugin_sdk::{Closed, Value};

use super::*;

fn info(size: u64, seconds: u64, is_dir: bool) -> FileInfo {
    FileInfo {
        is_dir,
        size,
        modified: Some(UNIX_EPOCH + Duration::from_secs(seconds)),
    }
}

fn labels_of(dialog: &nc_plugin_sdk::Dialog) -> Vec<String> {
    dialog.items.iter().map(|item| item.text.clone()).collect()
}

/// arclite's overwrite dialog: the path, the question, the sizes and times in columns, the newer one
/// marked, a folder without its size, "Remember choice", the five answers in one row, the warning.
#[test]
fn the_overwrite_dialog_is_arclites() {
    let path = Path::new(r"C:\out\a&b.txt");
    let dialog = overwrite_dialog(path, &info(10, 2_000, false), &info(1234, 1_000, false));
    assert!(dialog.warning);
    assert_eq!(dialog.title, "Overwrite file");
    let texts = labels_of(&dialog);
    assert_eq!(texts[0], r"C:\out\a&&b.txt", "its & shown");
    assert_eq!(texts[1], "Destination file already exists. Overwrite?");
    let source = texts.iter().position(|text| text == "Source:").unwrap();
    assert_eq!(texts[source + 1], "10");
    assert_eq!(texts[source + 3], "(newer)", "the source is newer");
    let size = &dialog.items[source + 1];
    assert_eq!(size.place.0, 5 + 13, "after the longer label and a space");
    let time = &dialog.items[source + 2];
    assert_eq!(time.place.0, 5 + 13 + 5, "after the wider size and a space");
    let tail: Vec<&str> = texts.iter().rev().take(5).map(String::as_str).collect();
    assert_eq!(
        tail,
        ["&Cancel", "A&ppend", "R&ename", "&Skip", "&Overwrite"]
    );
    let folder = overwrite_dialog(path, &info(0, 1, true), &info(0, 5, true));
    let labels = labels_of(&folder);
    let source = labels.iter().position(|text| text == "Source:").unwrap();
    assert_ne!(labels[source + 1], "0", "a folder's size not shown");
    assert!(
        labels.contains(&"(newer)".to_string()),
        "the destination is"
    );
}

/// The button pressed is its answer; "Remember choice" read from its check box; Cancel and Esc none.
#[test]
fn the_overwrite_answer_is_read() {
    let dialog = overwrite_dialog(Path::new("a"), &info(1, 1, false), &info(1, 1, false));
    let remember = (dialog.items.iter())
        .position(|item| item.kind == ItemKind::Check)
        .unwrap();
    let count = dialog.items.len();
    let closed = |button: Option<usize>, checked: bool| {
        let mut values = vec![Value::default(); count];
        values[remember].state = i32::from(checked);
        Closed { button, values }
    };
    assert_eq!(
        overwrite_answer(&dialog, &closed(Some(count - 3), true)),
        Some((Overwrite::Rename, true))
    );
    assert_eq!(
        overwrite_answer(&dialog, &closed(Some(count - 5), false)),
        Some((Overwrite::Overwrite, false))
    );
    assert_eq!(
        overwrite_answer(&dialog, &closed(Some(count - 1), false)),
        None
    );
    assert_eq!(overwrite_answer(&dialog, &closed(None, false)), None);
    assert_eq!(
        overwrite_answer(&dialog, &closed(Some(count - 4), false)),
        Some((Overwrite::Skip, false))
    );
    assert_eq!(
        overwrite_answer(&dialog, &closed(Some(count - 2), true)),
        Some((Overwrite::Append, true))
    );
    assert_eq!(
        overwrite_answer(&dialog, &closed(Some(count - 6), true)),
        None,
        "not a button"
    );
    assert_eq!(overwrite_answer(&dialog, &closed(Some(0), true)), None);
    assert_eq!(overwrite_answer(&dialog, &closed(Some(count), true)), None);
    let short = Closed {
        button: Some(count - 5),
        values: Vec::new(),
    };
    assert_eq!(
        overwrite_answer(&dialog, &short),
        Some((Overwrite::Overwrite, false)),
        "no values: not remembered"
    );
}

/// Overwrite is the default button and the only one; "Remember choice" starts off; the same times or
/// a time not known mark none newer; a time not known is shown empty.
#[test]
fn the_overwrite_dialogs_defaults_and_its_newer_mark() {
    let dialog = overwrite_dialog(Path::new("a"), &info(5, 7, false), &info(5, 7, false));
    let defaults: Vec<&str> = (dialog.items.iter())
        .filter(|item| item.default)
        .map(|item| item.text.as_str())
        .collect();
    assert_eq!(defaults, ["&Overwrite"]);
    let remember = (dialog.items.iter())
        .find(|item| item.kind == ItemKind::Check)
        .unwrap();
    assert_eq!(
        (remember.text.as_str(), remember.state),
        ("&Remember choice", 0)
    );
    assert!(
        !labels_of(&dialog).contains(&"(newer)".to_string()),
        "the same"
    );
    let unknown = FileInfo {
        is_dir: false,
        size: 3,
        modified: None,
    };
    let one_unknown = overwrite_dialog(Path::new("a"), &unknown, &info(5, 7, false));
    assert!(!labels_of(&one_unknown).contains(&"(newer)".to_string()));
    let texts = labels_of(&one_unknown);
    let source = texts.iter().position(|text| text == "Source:").unwrap();
    assert_eq!(
        (texts[source + 1].as_str(), texts[source + 2].as_str()),
        ("3", "")
    );
    assert_eq!(texts[source + 3], "Destination:", "no newer mark after it");
    assert_eq!(time_text(None), "");
}

/// A path longer than the dialog is cut in its middle to 60 columns.
#[test]
fn a_long_path_is_cut_to_the_dialog() {
    let long = format!(r"C:\{}\end.txt", "x".repeat(100));
    let dialog = overwrite_dialog(Path::new(&long), &info(1, 1, false), &info(1, 1, false));
    let shown = &labels_of(&dialog)[0];
    assert_eq!(shown.chars().count(), 60);
    assert!(shown.starts_with(r"C:\xx") && shown.ends_with(r"x\end.txt") && shown.contains("..."));
    let password = password_dialog(Path::new(&long));
    assert_eq!(labels_of(&password)[0], *shown);
}

/// The password dialog: the archive, "Password:" with its line, OK and Cancel (the indexes `password`
/// reads).
#[test]
fn the_password_dialog_is_arclites() {
    let dialog = password_dialog(Path::new(r"C:\a.zip"));
    assert_eq!(dialog.title, "Enter password");
    assert_eq!(labels_of(&dialog)[..2], [r"C:\a.zip", "Password:"]);
    assert_eq!(dialog.items[2].kind, ItemKind::Password);
    assert_eq!(dialog.items[4].text, "OK");
    assert_eq!(dialog.items[5].text, "Cancel");
}

/// A failure's lines: the system's words first; the log each failure's lines and an empty one.
#[test]
fn failures_are_told_as_arclite_tells_them() {
    let system = Fault {
        system: Some("Access is denied.".to_string()),
        objects: Vec::new(),
        messages: vec![r"C:\out\a.txt".to_string()],
        warnings: Vec::new(),
    };
    let data = Fault::message(vec!["Data error".to_string(), "b".to_string()]);
    assert_eq!(fault_lines(&system), ["Access is denied.", r"C:\out\a.txt"]);
    assert_eq!(fault_lines(&data), ["Data error", "b"], "no system's words");
    assert_eq!(log_text(&[]), "");
    assert_eq!(
        log_text(&[system, data]),
        "Access is denied.\nC:\\out\\a.txt\n\nData error\nb\n\n"
    );
}

/// A failure about an archive (a test's): the archive first, "Errors:", its lines (indented in the
/// log), as arclite's `error_dlg` and `show_error_log` write them; none of its lines: no heading.
#[test]
fn a_failure_about_an_archive_names_it_first() {
    let mut tested = Fault::message(vec![
        "CRC check failed".to_string(),
        "a.txt".to_string(),
        r"C:\t.zip".to_string(),
    ]);
    tested.objects = vec![r"C:\t.zip".to_string()];
    assert_eq!(
        fault_lines(&tested),
        [
            r"C:\t.zip",
            "Errors:",
            "CRC check failed",
            "a.txt",
            r"C:\t.zip"
        ]
    );
    assert_eq!(
        log_text(&[tested]),
        "C:\\t.zip\nErrors:\n  CRC check failed\n  a.txt\n  C:\\t.zip\n\n"
    );
    let bare = Fault {
        system: Some("Access is denied.".to_string()),
        objects: vec![r"C:\t.zip".to_string()],
        messages: Vec::new(),
        warnings: Vec::new(),
    };
    assert_eq!(fault_lines(&bare), ["Access is denied.", r"C:\t.zip"]);
}

/// A test's open warnings: "Errors:" over its errors when there are warnings too, "Warnings:" over them
/// (indented in the log); warnings alone: no "Errors:" heading.
#[test]
fn warnings_are_told_under_their_heading() {
    let mut tested = Fault::message(vec!["Headers Error".to_string()]);
    tested.objects = vec![r"C:\t.7z".to_string()];
    tested.warnings = vec!["There are some data after the end of the payload data".to_string()];
    assert_eq!(
        log_text(&[tested]),
        "C:\\t.7z\nErrors:\n  Headers Error\nWarnings:\n  There are some data after the end of the payload data\n\n"
    );
    let warned = Fault {
        warnings: vec!["UnconfirmedStart".to_string()],
        ..Fault::default()
    };
    assert_eq!(fault_lines(&warned), ["Warnings:", "UnconfirmedStart"]);
    let mut errors_only = Fault::message(vec!["Data error".to_string()]);
    errors_only.warnings.clear();
    assert_eq!(fault_lines(&errors_only), ["Data error"]);
}
