use super::*;

#[test]
fn paths_and_folders_turn_into_each_other() {
    assert_eq!(folder_of("/"), "");
    assert_eq!(folder_of("/Work/Prod"), "Work/Prod");
    assert_eq!(path_of(""), "/");
    assert_eq!(path_of("Work/Prod"), "/Work/Prod");
}

/// Folders as folders, sessions as files; a folder of none shows NetBox's hint alone.
#[test]
fn a_folders_items_are_entries_and_none_is_the_hint() {
    let items = [
        Item::Folder("Work".to_string()),
        Item::Session("home".to_string()),
    ];
    let entries = entries(&items);
    assert_eq!(entries.len(), 2);
    assert!(entries[0].is_dir && entries[0].name == "Work");
    assert!(!entries[1].is_dir && entries[1].name == "home");
    let hint = super::entries(&[]);
    assert_eq!(hint.len(), 1);
    assert_eq!(hint[0].name, HINT);
    assert!(!hint[0].is_dir);
}

#[test]
fn the_title_names_the_folder_as_netboxs() {
    assert_eq!(title("/"), "NetWay: Stored sessions [/]");
    assert_eq!(title("/Work/Prod"), "NetWay: Stored sessions [/Work/Prod]");
}

#[test]
fn every_mode_shows_the_session_names() {
    let modes = modes();
    assert_eq!(modes.len(), 10);
    for mode in &modes {
        let mode = mode.as_ref().unwrap();
        assert_eq!(mode.columns, "N");
        assert_eq!(mode.titles, [Some("Session name".to_string())]);
    }
}

/// NetBox's labels: Export, NewSes, Duplicate, Rename; the blanked keys empty; F5 and those left to the
/// program.
#[test]
fn the_keys_carry_netboxs_labels() {
    let keys = keys();
    let label = |key: &str| {
        (keys.iter())
            .find(|one| one.key == key)
            .map(|one| one.label.as_str())
    };
    assert_eq!(label("Shift+F4"), Some("NewSes"));
    assert_eq!(label("Shift+F5"), Some("Duplicate"));
    assert_eq!(label("Shift+F6"), Some("Rename"));
    assert_eq!(label("Enter"), Some(""));
    assert_eq!(label("F6"), Some(" "));
    assert_eq!(label("Ctrl+F11"), Some(" "));
    assert_eq!(label("F5"), Some("Export"));
    assert!(is_programs("Shift+F8") && is_programs("F5") && is_programs("F6"));
    assert!(!is_programs("F4") && !is_programs("Shift+F5") && !is_programs("Enter"));
}

/// Ctrl+F: a session's whole name, a folder's path with "/", ".." the folder itself; quoted with a
/// space; nothing on the hint.
#[test]
fn ctrl_f_names_the_session_or_folder() {
    let session = Item::Session("db".to_string());
    assert_eq!(
        command_line_name("Work", Some(&session), false).as_deref(),
        Some("Work/db ")
    );
    let folder = Item::Folder("My servers".to_string());
    assert_eq!(
        command_line_name("", Some(&folder), false).as_deref(),
        Some("\"My servers/\" ")
    );
    assert_eq!(
        command_line_name("Work", None, true).as_deref(),
        Some("Work/ ")
    );
    assert_eq!(command_line_name("", None, true).as_deref(), Some("/ "));
    assert_eq!(command_line_name("", None, false), None, "the hint");
}
