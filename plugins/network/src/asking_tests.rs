use super::*;

/// FAR's logon dialog: titled by the share, the last name and password in it, the name's history,
/// "Remember password" off at first and free to check (FAR's `bSelected`).
#[test]
fn the_logon_dialog_starts_with_the_last_name_and_password() {
    let last = LastLogon {
        user: "guest".to_string(),
        password: "secret".to_string(),
    };
    let dialog = logon_dialog(r"\\SERVER\Music", &last);
    assert_eq!(dialog.title, r"\\SERVER\Music");
    assert_eq!(dialog.items[USER].text, "guest");
    assert_eq!(dialog.items[USER].history, "NetworkUser");
    assert_eq!(dialog.items[PASSWORD].text, "secret");
    let remember = &dialog.items[REMEMBER];
    assert_eq!(remember.text, "&Remember password");
    assert!(!remember.disabled && remember.state == 0);
    assert_eq!(dialog.items[LOGON_OK].text, "&Ok");
}

/// FAR's disconnection question: "Reconnect at Logon" as last left, changeable only for a disk restored
/// at logon; "Yes" the default.
#[test]
fn the_disconnect_question_and_its_check_box() {
    let dialog = disconnect_dialog("Z:", r"\\SERVER\Music", true, true);
    assert_eq!(dialog.title, "Disconnect network drive");
    assert_eq!(
        dialog.items[0].text,
        "Do you want to disconnect from the drive Z:?"
    );
    assert_eq!(dialog.items[1].text, r"\\SERVER\Music");
    assert_eq!(dialog.items[RECONNECT].state, 1);
    assert!(!dialog.items[RECONNECT].disabled);
    assert_eq!(dialog.items[YES].text, "&Yes");
    let temporary = disconnect_dialog("Z:", r"\\SERVER\Music", false, false);
    assert!(temporary.items[RECONNECT].disabled);
    assert_eq!(temporary.items[RECONNECT].state, 0);
}

/// FAR's letter menu: the free letters with their hotkeys, its title and bottom the kind, F6 its break
/// key.
#[test]
fn the_letter_menu_tells_the_kind() {
    let menu = letter_menu(&['D', 'Z'], true, 1);
    assert_eq!(menu.title, "Permanent connection to");
    assert_eq!(menu.bottom, "F6 - temporary");
    let texts: Vec<&str> = menu.items.iter().map(|item| item.text.as_str()).collect();
    assert_eq!(texts, ["&D:", "&Z:"]);
    assert_eq!(menu.selected, 1);
    assert_eq!(menu.break_keys, ["F6"]);
    let temporary = letter_menu(&['D'], false, 0);
    assert_eq!(temporary.title, "Temporary connection to");
    assert_eq!(temporary.bottom, "F6 - permanent");
}

/// A folder reads or not; a disk's letter read at its root; the process's folder stays where it is when
/// it is not on the disk.
#[test]
fn the_systems_folders_and_the_process_folder() {
    let temp = tempfile::tempdir().unwrap();
    let net = SystemConnections;
    assert!(net.readable(&temp.path().to_string_lossy()));
    assert!(!net.readable(&temp.path().join("none").to_string_lossy()));
    let here = std::env::current_dir().unwrap();
    let first = here.to_string_lossy().chars().next().unwrap();
    let other = ('A'..='Z')
        .find(|letter| !letter.eq_ignore_ascii_case(&first))
        .unwrap();
    assert!(!net.hands_off(other));
    assert_eq!(std::env::current_dir().unwrap(), here);
}
