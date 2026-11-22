use super::*;

#[test]
fn the_key_bar_is_fars() {
    let keys = keys();
    let label = |name: &str| {
        (keys.iter())
            .find(|key| key.key == name)
            .map(|key| key.label.as_str())
    };
    assert_eq!(label("Shift+F1"), Some("Prior-"));
    assert_eq!(label("Shift+F2"), Some("Prior+"));
    for kill in KILL_KEYS {
        assert_eq!(label(kill), Some("Kill"), "{kill}");
    }
    assert_eq!(label("F7"), Some(" "), "shown empty");
    assert_eq!(label("Ctrl+F3"), Some(""), "the program's label");
    assert_eq!(label("Ctrl+F5"), Some(""));
    assert_eq!(label("Ctrl+F11"), Some(""));
    assert_eq!(label("Ctrl+0"), Some(""));
    assert_eq!(label("Ctrl+9"), Some(""));
    assert_eq!(label("F6"), None, "another computer: not yet");
    let mut names: Vec<&str> = keys.iter().map(|key| key.key.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), keys.len(), "each key once");
}

#[test]
fn a_path_for_the_command_line_is_quoted_as_far_quotes_it() {
    assert_eq!(command_line_text("C:\\a\\b.exe"), "C:\\a\\b.exe ");
    assert_eq!(
        command_line_text("C:\\Program Files\\b.exe"),
        "\"C:\\Program Files\\b.exe\" "
    );
    assert_eq!(command_line_text("a&b"), "\"a&b\" ");
    assert_eq!(command_line_text("a^b"), "\"a^b\" ");
    assert_eq!(command_line_text(""), " ");
}

#[test]
fn the_kill_question_names_one_process() {
    assert_eq!(
        kill_question(&["a.exe".to_string()]),
        "Do you wish to kill \"a.exe\"?"
    );
    assert_eq!(
        kill_question(&["a.exe".to_string(), "b.exe".to_string()]),
        "Kill selected processes?"
    );
}

#[test]
fn the_system_text_has_no_number() {
    let denied = io::Error::from_raw_os_error(5);
    let text = system_text(&denied);
    assert!(!text.contains("os error"), "{text}");
    assert!(!text.is_empty());
    assert_eq!(system_text(&io::Error::other("plain")), "plain");
}

fn press(key: &str) -> KeyPress {
    KeyPress {
        key: key.to_string(),
        current: Some("a.exe 1".to_string()),
        selected: vec!["b.exe 2".to_string(), "c.exe 3".to_string()],
        ..KeyPress::default()
    }
}

/// FAR's Shift+F8 deletes the entry under the cursor only, its other deleting keys the selection.
#[test]
fn shift_f8_kills_the_process_under_the_cursor_only() {
    assert_eq!(kill_targets(&press("Shift+F8")), ["a.exe 1"]);
    for key in ["F8", "Del", "Shift+Del", "Alt+Del"] {
        assert_eq!(kill_targets(&press(key)), ["b.exe 2", "c.exe 3"], "{key}");
    }
    let alone = KeyPress {
        selected: Vec::new(),
        ..press("F8")
    };
    assert_eq!(
        kill_targets(&alone),
        ["a.exe 1"],
        "no selection: the current one"
    );
    let parent = KeyPress {
        current: None,
        ..press("Shift+F8")
    };
    assert!(kill_targets(&parent).is_empty(), "on \"..\": none");
}
