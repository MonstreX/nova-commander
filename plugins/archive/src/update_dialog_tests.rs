use nc_plugin_sdk::{ItemKind, Value};

use super::*;

fn options() -> UpdateOptions {
    UpdateOptions {
        level: 7,
        method: "PPMD".to_string(),
        advanced: String::new(),
        encrypt: false,
        show_password: false,
        password: String::new(),
        overwrite: Overwrite::Skip,
        move_files: false,
        ignore_errors: true,
        open_shared: true,
        kind: crate::update_options::Kind::default(),
    }
}

/// The values a dialog closed with, by its items, with `changed` over its own.
fn closed(dialog: &Dialog, button: usize, changed: &[(usize, i32, &str)]) -> Closed {
    let mut values: Vec<Value> = (dialog.items.iter())
        .map(|item| Value {
            state: item.state,
            text: item.text.clone(),
        })
        .collect();
    for (item, state, text) in changed {
        values[*item] = Value {
            state: *state,
            text: (*text).to_string(),
        };
    }
    Closed {
        button: Some(button),
        values,
    }
}

/// arclite's items in its order and words for an archive that is there.
#[test]
fn the_dialog_is_arclites() {
    let dialog = dialog(&options());
    assert_eq!(dialog.title, TITLE);
    let text = |item: usize| dialog.items[item].text.as_str();
    assert_eq!(text(0), "Compression &level:");
    assert_eq!(dialog.items[LEVEL].kind, ItemKind::List);
    assert_eq!(dialog.items[LEVEL].state, 4, "Maximum");
    assert_eq!(
        dialog.items[METHOD].state, 2,
        "PPMD: arclite's LZMA, LZMA2, PPMD, Deflate, Deflate64"
    );
    assert_eq!(text(SOLID), "S&olid");
    assert!(dialog.items[SOLID].disabled, "7z's only");
    assert_eq!(dialog.items[ADVANCED].history, "arclite.advanced");
    assert_eq!(text(ENCRYPT), "&Encrypt archive");
}

/// The encryption's items as they start, the answers, the moves and the buttons.
#[test]
fn the_dialogs_lower_items_are_arclites() {
    let dialog = dialog(&options());
    let text = |item: usize| dialog.items[item].text.as_str();
    assert!(dialog.items[ENCRYPT_HEADER].disabled);
    assert!(dialog.items[PASSWORD].disabled, "not encrypted");
    assert!(dialog.items[PASSWORD_VISIBLE].hidden, "hidden password");
    assert!(!dialog.items[PASSWORD].hidden);
    assert_eq!(text(FIRST_ANSWER + 2), "Ski&p");
    assert_eq!(dialog.items[FIRST_ANSWER + 2].state, 1);
    assert_eq!(text(MOVE_FILES), "&Delete files after archiving");
    assert_eq!(dialog.items[IGNORE_ERRORS].state, 1);
    assert_eq!(text(OPEN_SHARED), "Archive files open for w&riting");
    assert!(
        dialog.items[OPEN_SHARED + 1].disabled,
        "no filter for plugins"
    );
    assert_eq!(
        [text(OK), text(CANCEL), text(SAVE)],
        ["OK", "Cancel", "Save settings"]
    );
    assert!(dialog.items[SAVE].no_close);
    let place = dialog.items[PASSWORD_VISIBLE].place;
    assert_eq!(
        place.1, dialog.items[PASSWORD].place.1,
        "over the hidden line"
    );
}

/// Store: no method; encrypted and shown: the visible line only.
#[test]
fn the_items_follow_the_level_and_encryption() {
    let stored = UpdateOptions {
        level: 0,
        encrypt: true,
        show_password: true,
        ..options()
    };
    let dialog = dialog(&stored);
    assert!(dialog.items[METHOD].disabled && dialog.items[METHOD_LABEL].disabled);
    assert!(!dialog.items[PASSWORD_VISIBLE].disabled && !dialog.items[PASSWORD_VISIBLE].hidden);
    assert!(dialog.items[PASSWORD].hidden && dialog.items[PASSWORD_VERIFY].hidden);
}

/// OK: the answers as arclite's `read_controls` takes them.
#[test]
fn ok_takes_the_answers() {
    let dialog = dialog(&options());
    let done = closed(
        &dialog,
        OK,
        &[
            (LEVEL, 0, ""),
            (METHOD, 3, ""),
            (ENCRYPT, 1, ""),
            (PASSWORD, 0, "pw"),
            (PASSWORD_VERIFY, 0, "pw"),
            (FIRST_ANSWER + 2, 0, ""),
            (FIRST_ANSWER + 1, 1, ""),
            (MOVE_FILES, 1, ""),
            (ADVANCED, 0, "em=AES128"),
        ],
    );
    let read = answers(&options(), &done).unwrap();
    assert_eq!((read.level, read.method.as_str()), (0, "Deflate"));
    assert!(read.encrypt && read.password == "pw");
    assert_eq!(read.overwrite, Overwrite::Overwrite);
    assert!(read.move_files && read.ignore_errors && read.open_shared);
    assert_eq!(read.advanced, "em=AES128");
    assert!(answers(&options(), &closed(&dialog, CANCEL, &[])).is_none());
}

/// The passwords not the same, or none, are not taken; not encrypted, none is kept.
#[test]
fn the_password_is_checked() {
    let dialog = dialog(&options());
    let wrong = [
        (ENCRYPT, 1, ""),
        (PASSWORD, 0, "a"),
        (PASSWORD_VERIFY, 0, "b"),
    ];
    let value_of = |changed: &[(usize, i32, &str)]| {
        let closed = closed(&dialog, OK, changed);
        move |item: usize| {
            let value = &closed.values[item];
            (value.state, value.text.clone())
        }
    };
    assert_eq!(
        read(&options(), &value_of(&wrong)),
        Err("Passwords don't match")
    );
    let empty = [(ENCRYPT, 1, "")];
    assert_eq!(
        read(&options(), &value_of(&empty)),
        Err("Password is empty")
    );
    let shown = [
        (ENCRYPT, 1, ""),
        (SHOW_PASSWORD, 1, ""),
        (PASSWORD_VISIBLE, 0, "seen"),
    ];
    assert_eq!(
        read(&options(), &value_of(&shown)).unwrap().password,
        "seen"
    );
    let plain = [(PASSWORD, 0, "left"), (PASSWORD_VERIFY, 0, "over")];
    assert_eq!(read(&options(), &value_of(&plain)).unwrap().password, "");
}

/// The items' (enabled, shown) for the level's line, Encrypt and Show password as given.
fn states(level: i32, encrypt: i32, shown: i32) -> std::collections::HashMap<usize, (bool, bool)> {
    let state = move |item: usize| match item {
        LEVEL => level,
        ENCRYPT => encrypt,
        SHOW_PASSWORD => shown,
        _ => 0,
    };
    control_state(&state, "zip").into_iter().collect()
}

/// A 7z's: its method, Solid and the header's encryption with a compressing level and Encrypt; a
/// gzip's: neither a method nor encryption; a level a format does not take put to the one it writes.
#[test]
fn the_items_follow_the_format() {
    let state = |item: usize| match item {
        LEVEL => 3,
        ENCRYPT => 1,
        _ => 0,
    };
    let seven: std::collections::HashMap<_, _> = control_state(&state, "7z").into_iter().collect();
    assert_eq!(seven[&SOLID], (true, true));
    assert_eq!(seven[&ENCRYPT_HEADER], (true, true));
    assert_eq!(seven[&METHOD], (true, true));
    let gzip: std::collections::HashMap<_, _> = control_state(&state, "gzip").into_iter().collect();
    assert_eq!(gzip[&METHOD], (false, true));
    assert_eq!(gzip[&ENCRYPT], (false, true));
    assert_eq!(gzip[&SHOW_PASSWORD], (false, true));
    assert_eq!(crate::update_props::fit_level("tar", 5), 0);
    assert_eq!(crate::update_props::fit_level("bzip2", 0), 5);
    assert_eq!(crate::update_props::fit_level("7z", 9), 9);
    let mut seven_options = options();
    seven_options.kind.format = "7z".to_string();
    seven_options.method = "LZMA2".to_string();
    let shown = dialog(&seven_options);
    assert_eq!(
        shown.items[METHOD].lines[shown.items[METHOD].state as usize],
        "LZMA2"
    );
}

/// arclite's `set_control_state`: the method with a compressing level only, the password's items with
/// Encrypt only, the hidden lines or the shown one by Show password; Solid and the header's encryption
/// never; moving always.
#[test]
fn each_items_state_is_arclites() {
    let hidden_lines = [PASSWORD_LABEL, PASSWORD, PASSWORD + 1, PASSWORD_VERIFY];
    let shown_lines = [PASSWORD_VISIBLE_LABEL, PASSWORD_VISIBLE];
    let plain = states(4, 0, 0);
    assert_eq!(plain[&METHOD_LABEL], (true, true));
    assert_eq!(plain[&METHOD], (true, true));
    assert_eq!(plain[&SHOW_PASSWORD], (false, true));
    assert!(hidden_lines.iter().all(|item| plain[item] == (false, true)));
    assert!(shown_lines.iter().all(|item| plain[item] == (false, false)));
    assert_eq!(plain[&SOLID], (false, true));
    assert_eq!(plain[&ENCRYPT_HEADER], (false, true));
    assert_eq!(plain[&MOVE_FILES], (true, true));
    let typed = states(0, 1, 0);
    assert_eq!(typed[&METHOD], (false, true), "Store");
    assert_eq!(typed[&SHOW_PASSWORD], (true, true));
    assert!(hidden_lines.iter().all(|item| typed[item] == (true, true)));
    assert!(shown_lines.iter().all(|item| typed[item] == (true, false)));
    let seen = states(4, 1, 1);
    assert!(hidden_lines.iter().all(|item| seen[item] == (true, false)));
    assert!(shown_lines.iter().all(|item| seen[item] == (true, true)));
}

/// The lines as they start: a level not in the list as its first, the answer for an existing file
/// (Overwrite for one in the archive's case), moving.
#[test]
fn the_dialog_starts_from_the_options() {
    let odd = UpdateOptions {
        level: 2,
        overwrite: Overwrite::OverwriteCase,
        move_files: true,
        ..options()
    };
    let shown = dialog(&odd);
    assert_eq!(shown.items[PASSWORD + 1].text, "Repeat:");
    assert_eq!(shown.items[LEVEL].state, 0, "Store, the first");
    let answers: Vec<i32> = (0..3)
        .map(|index| shown.items[FIRST_ANSWER + index].state)
        .collect();
    assert_eq!(answers, [0, 1, 0], "Overwrite");
    assert_eq!(shown.items[MOVE_FILES].state, 1);
    let asked = dialog(&UpdateOptions {
        overwrite: Overwrite::Ask,
        ..options()
    });
    assert_eq!(asked.items[FIRST_ANSWER].state, 1);
    assert_eq!(asked.items[MOVE_FILES].state, 0);
}

/// Values out of the lists keep the options'; no answer on is Ask; a shown password is not checked
/// against the hidden lines but must not be empty; Esc and a wrong password give no answers.
#[test]
fn odd_values_are_read_as_arclite_does() {
    let dialog = dialog(&options());
    let value_of = |changed: &[(usize, i32, &str)]| {
        let closed = closed(&dialog, OK, changed);
        move |item: usize| {
            let value = &closed.values[item];
            (value.state, value.text.clone())
        }
    };
    let odd = [(LEVEL, 99, ""), (METHOD, 99, ""), (FIRST_ANSWER + 2, 0, "")];
    let read_odd = read(&options(), &value_of(&odd)).unwrap();
    assert_eq!((read_odd.level, read_odd.method.as_str()), (7, "PPMD"));
    assert_eq!(read_odd.overwrite, Overwrite::Ask);
    let shown = [
        (ENCRYPT, 1, ""),
        (SHOW_PASSWORD, 1, ""),
        (PASSWORD, 0, "a"),
        (PASSWORD_VERIFY, 0, "b"),
        (PASSWORD_VISIBLE, 0, "seen"),
    ];
    assert_eq!(
        read(&options(), &value_of(&shown)).map(|read| read.password),
        Ok("seen".to_string())
    );
    let empty_shown = [(ENCRYPT, 1, ""), (SHOW_PASSWORD, 1, ""), (PASSWORD, 0, "a")];
    assert_eq!(
        read(&options(), &value_of(&empty_shown)),
        Err("Password is empty")
    );
    let mut esc = closed(&dialog, OK, &[]);
    esc.button = None;
    assert!(answers(&options(), &esc).is_none(), "Esc");
    let wrong = [
        (ENCRYPT, 1, ""),
        (PASSWORD, 0, "a"),
        (PASSWORD_VERIFY, 0, "b"),
    ];
    assert!(answers(&options(), &closed(&dialog, OK, &wrong)).is_none());
    assert!(answers(&options(), &closed(&dialog, OK, &[])).is_some());
}
