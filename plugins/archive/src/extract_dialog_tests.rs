use nc_plugin_sdk::{GetMode, ItemKind, Value};

use super::*;

fn options() -> ExtractOptions {
    let settings = ExtractSettings {
        overwrite: Overwrite::Rename,
        ..ExtractSettings::default()
    };
    ExtractOptions::starting(PathBuf::from(r"C:\out"), GetMode::default(), &settings)
}

/// The dialog's values as its items start, `changes` put over them.
fn closed(button: usize, changes: &[(usize, i32, &str)]) -> Closed {
    let dialog = dialog(&options());
    let mut values: Vec<Value> = (dialog.items.iter())
        .map(|item| Value {
            state: item.state,
            text: item.text.clone(),
        })
        .collect();
    for &(item, state, text) in changes {
        values[item] = Value {
            state,
            text: text.to_string(),
        };
    }
    Closed {
        button: Some(button),
        values,
    }
}

/// arclite's items in their order: the folder with its closing `\` and history, the kept answer's
/// radio on, "Delete files" and "Go to" disabled where not offered, the buttons in one row.
#[test]
fn the_dialog_is_arclites() {
    let dialog = dialog(&options());
    assert_eq!(dialog.title, "Extract files");
    let line = &dialog.items[DST_DIR];
    assert_eq!(line.text, r"C:\out\");
    assert_eq!(line.history, "arclite.extract_dir");
    assert_eq!(dialog.items[FIRST_ANSWER + 3].text, "&Rename");
    assert_eq!(dialog.items[FIRST_ANSWER + 3].state, 1, "the kept answer");
    assert!(dialog.items[FIRST_ANSWER].group);
    assert!(dialog.items[MOVE_FILES].disabled && dialog.items[OPEN_DIR].disabled);
    assert!(!dialog.items[DELETE_ARCHIVE].disabled);
    assert_eq!(dialog.items[SEPARATE_DIR].state, 2, "[?] by default");
    assert_eq!(dialog.items[OK].text, "OK");
    assert_eq!(dialog.items[SAVE].text, "Sa&ve settings");
    assert!(dialog.items[SAVE].no_close);
    let row = dialog.items[OK].place.1;
    assert!(dialog.items[CANCEL].place.1 == row && dialog.items[SAVE].place.1 == row);
    assert_eq!(dialog.size, Some((70, 18)));
}

/// OK reads the answers: the folder unquoted and expanded once changed, the radio on, the check boxes;
/// Cancel and Esc none.
#[test]
fn the_answers_are_read_as_arclite_reads_them() {
    let same = answers(&options(), &closed(OK, &[])).unwrap();
    assert_eq!(same.dst_dir, PathBuf::from(r"C:\out\"));
    assert_eq!(same.overwrite, Overwrite::Rename);
    let changed = closed(
        OK,
        &[
            (DST_DIR, 0, r#" "D:\to" "#),
            (FIRST_ANSWER + 3, 0, "&Rename"),
            (FIRST_ANSWER + 2, 1, "&Skip"),
            (IGNORE_ERRORS, 1, "&Ignore errors"),
            (SEPARATE_DIR, 1, ""),
            (DELETE_ARCHIVE, 1, ""),
            (PASSWORD, 0, "secret"),
            (MOVE_FILES, 1, ""),
        ],
    );
    let read = answers(&options(), &changed).unwrap();
    assert_eq!(read.dst_dir, PathBuf::from(r"D:\to"));
    assert_eq!(read.overwrite, Overwrite::Skip);
    assert!(read.ignore_errors && read.delete_archive);
    assert_eq!(read.separate_dir, Tri::True);
    assert_eq!(read.password, "secret");
    assert_eq!(read.move_files, Tri::Undef, "not offered: as it was");
    assert_eq!(answers(&options(), &closed(CANCEL, &[])), None);
    let esc = Closed {
        button: None,
        values: Vec::new(),
    };
    assert_eq!(answers(&options(), &esc), None);
}

/// Every index the dialog is read by is the item arclite has there, in its words and kinds (a check
/// box's third state as `3`); the radios one group from the first; OK the default button.
#[test]
fn each_index_is_its_item() {
    use ItemKind::{Button, Check, Edit, Password, Radio, Separator, Text};
    let dialog = dialog(&options());
    let items: Vec<(usize, ItemKind, &str, bool)> = (dialog.items.iter().enumerate())
        .map(|(index, item)| (index, item.kind, item.text.as_str(), item.three_state))
        .collect();
    let expected = [
        (0, Text, "&Extract files to:", false),
        (DST_DIR, Edit, r"C:\out\", false),
        (2, Separator, "", false),
        (IGNORE_ERRORS, Check, "&Ignore errors", false),
        (4, Text, "When destination file exists:", false),
        (FIRST_ANSWER, Radio, "As&k", false),
        (6, Radio, "&Overwrite", false),
        (7, Radio, "&Skip", false),
        (8, Radio, "&Rename", false),
        (9, Radio, "&Append", false),
        (
            MOVE_FILES,
            Check,
            "&Delete files from archive after extraction",
            true,
        ),
        (
            DELETE_ARCHIVE,
            Check,
            "De&lete archive after extraction",
            false,
        ),
        (
            SEPARATE_DIR,
            Check,
            "Ex&tract archive into separate directory",
            true,
        ),
        (
            OPEN_DIR,
            Check,
            "&Go to target directory after extraction",
            true,
        ),
        (14, Text, "Pass&word:", false),
        (PASSWORD, Password, "", false),
        (16, Separator, "", false),
        (OK, Button, "OK", false),
        (CANCEL, Button, "Cancel", false),
        (SAVE, Button, "Sa&ve settings", false),
    ];
    assert_eq!(items, expected);
    let groups: Vec<usize> = (0..20).filter(|&index| dialog.items[index].group).collect();
    assert_eq!(groups, [FIRST_ANSWER]);
    let defaults: Vec<usize> = (0..20)
        .filter(|&index| dialog.items[index].default)
        .collect();
    assert_eq!(defaults, [OK]);
    assert!(dialog.items[DST_DIR].edit_path && !dialog.items[OK].no_close);
}

/// The answers in two rows (Rename starts the second), each two columns after the one before; the
/// password line 20 wide after its label.
#[test]
fn the_answers_and_the_password_are_placed_as_arclite_places_them() {
    let dialog = dialog(&options());
    let place = |index: usize| dialog.items[index].place;
    let (ask, overwrite, skip) = (
        place(FIRST_ANSWER),
        place(FIRST_ANSWER + 1),
        place(FIRST_ANSWER + 2),
    );
    let (rename, append) = (place(FIRST_ANSWER + 3), place(FIRST_ANSWER + 4));
    assert_eq!(ask, (7, 7, 13, 7), "As&k: 3 and 4, after two");
    assert_eq!(overwrite, (16, 7, 28, 7));
    assert_eq!(skip, (31, 7, 38, 7));
    assert_eq!(rename, (7, 8, 16, 8), "the next row");
    assert_eq!(append.0, 19);
    assert_eq!(place(PASSWORD - 1), (5, 13, 13, 13));
    assert_eq!(place(PASSWORD), (14, 13, 33, 13));
    assert_eq!(
        place(DST_DIR),
        (5, 3, 63, 3),
        "60 wide, its history's arrow"
    );
}

/// "Overwrite in the archive's case" shows as Overwrite; what is offered enabled with its state; the
/// archive's deletion disabled when there is no file to delete; an empty folder shown empty.
#[test]
fn what_is_offered_shows_as_it_stands() {
    let offered = ExtractOptions {
        dst_dir: PathBuf::new(),
        overwrite: Overwrite::OverwriteCase,
        move_files: Tri::True,
        open_dir: Tri::False,
        disable_delete_archive: true,
        delete_archive: true,
        ignore_errors: true,
        separate_dir: Tri::False,
        password: "pw".to_string(),
    };
    let dialog = dialog(&offered);
    let items = &dialog.items;
    assert_eq!(items[DST_DIR].text, "");
    let on: Vec<usize> = (0..5)
        .filter(|index| items[FIRST_ANSWER + index].state == 1)
        .collect();
    assert_eq!(on, [1], "Overwrite alone");
    assert!(!items[MOVE_FILES].disabled && !items[OPEN_DIR].disabled);
    assert_eq!((items[MOVE_FILES].state, items[OPEN_DIR].state), (1, 0));
    assert!(items[DELETE_ARCHIVE].disabled);
    assert_eq!(items[DELETE_ARCHIVE].state, 1);
    assert_eq!(items[IGNORE_ERRORS].state, 1);
    assert_eq!(items[SEPARATE_DIR].state, 0);
    assert_eq!(items[PASSWORD].text, "pw");
    let asking = dialog_of(Overwrite::Ask);
    assert_eq!(asking.items[FIRST_ANSWER].state, 1);
    assert_eq!(asking.items[FIRST_ANSWER + 1].state, 0);
    let appending = dialog_of(Overwrite::Append);
    assert_eq!(appending.items[FIRST_ANSWER + 4].state, 1);
    assert_eq!(appending.items[FIRST_ANSWER].state, 0);
}

fn dialog_of(overwrite: Overwrite) -> Dialog {
    dialog(&ExtractOptions {
        overwrite,
        ..options()
    })
}

/// The folder's closing `\` added once: kept after `\` or `/`, none for an empty one.
#[test]
fn the_folder_gets_its_closing_slash_once() {
    assert_eq!(with_slash(r"C:\a"), r"C:\a\");
    assert_eq!(with_slash(r"C:\a\"), r"C:\a\");
    assert_eq!(with_slash("C:/a/"), "C:/a/");
    assert_eq!(with_slash(""), "");
}

/// What is offered is read from its box (move off, going there on); with no radio on the answer is Ask;
/// "Delete archive" and "Ignore errors" off when their boxes are; an item not there reads empty.
#[test]
fn offered_boxes_are_read_and_missing_ones_are_off() {
    let offered = ExtractOptions {
        move_files: Tri::True,
        open_dir: Tri::False,
        disable_delete_archive: true,
        ..options()
    };
    let mut values = closed(OK, &[(FIRST_ANSWER + 3, 0, "&Rename")]).values;
    values[MOVE_FILES].state = 0;
    values[OPEN_DIR].state = 1;
    values[DELETE_ARCHIVE].state = 0;
    values[IGNORE_ERRORS].state = 0;
    values[SEPARATE_DIR].state = 2;
    let read = answers(
        &offered,
        &Closed {
            button: Some(OK),
            values,
        },
    )
    .unwrap();
    assert_eq!(read.overwrite, Overwrite::Ask, "no radio on");
    assert_eq!((read.move_files, read.open_dir), (Tri::False, Tri::True));
    assert_eq!(read.separate_dir, Tri::Undef);
    assert!(!read.delete_archive && !read.ignore_errors);
    assert!(read.disable_delete_archive, "as it was");
    let nothing = Closed {
        button: Some(OK),
        values: Vec::new(),
    };
    let empty = answers(&options(), &nothing).unwrap();
    assert_eq!(empty.dst_dir, PathBuf::new());
    assert_eq!(empty.overwrite, Overwrite::Ask);
    assert_eq!(empty.separate_dir, Tri::False, "state 0: [ ]");
    assert!(empty.password.is_empty() && !empty.ignore_errors);
    let saved = answers(&options(), &closed(SAVE, &[]));
    assert!(saved.is_some(), "only Cancel and Esc give none");
}

/// The folder as offered is taken as typed (its closing `\` or not), `%NAME%` and all; one changed has
/// its `%NAME%` expanded.
#[cfg(windows)]
#[test]
fn a_changed_folder_has_its_variables_expanded() {
    let root = std::env::var("SystemRoot").unwrap();
    let offered = ExtractOptions {
        dst_dir: PathBuf::from(r"%SystemRoot%\x"),
        ..options()
    };
    let read_with = |typed: &str| {
        let mut values = closed(OK, &[]).values;
        values[DST_DIR].text = typed.to_string();
        let closed = Closed {
            button: Some(OK),
            values,
        };
        answers(&offered, &closed).unwrap().dst_dir
    };
    assert_eq!(
        read_with(r"%SystemRoot%\x\"),
        PathBuf::from(r"%SystemRoot%\x\")
    );
    assert_eq!(
        read_with(r"%SystemRoot%\x"),
        PathBuf::from(r"%SystemRoot%\x")
    );
    assert_eq!(
        read_with(r"%SystemRoot%\y"),
        PathBuf::from(format!(r"{root}\y"))
    );
}
