//! Options > Groups of file masks, and `<name>` in the masks.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::{Command, EditList, Overlay, Side};

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    for name in ["a.zip", "b.txt", "c.bak"] {
        std::fs::write(temp.path().join(name), "x").unwrap();
    }
    let mut app = app_in(temp.path());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.panels[0].entries.len() < 4 {
        assert!(Instant::now() < deadline, "the folder is not read");
        app.tick();
    }
    (temp, app)
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn list(app: &App) -> &EditList {
    let Some(Overlay::EditList(list)) = &app.overlay else {
        panic!("no list");
    };
    list
}

fn names(app: &App) -> Vec<&str> {
    app.config
        .mask_groups
        .iter()
        .map(|group| group.name.as_str())
        .collect()
}

/// Writes the group's dialog (or F7's, with one line) and presses OK.
fn answer(app: &mut App, texts: &[&str]) {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    for (at, text) in [1, 3].into_iter().zip(texts) {
        form.set_edit_text(at, text);
    }
    let ok = form.items.len() - 2;
    form.focus_item(ok);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

/// Presses the first button of the question in front.
fn yes(app: &mut App) {
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Confirm { .. }))
    ));
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

#[test]
fn with_none_saved_fars_default_groups_are_there_in_the_order_of_their_names() {
    let (_temp, app) = app();
    assert_eq!(names(&app), ["arc", "exec", "temp"]);
    assert_eq!(app.config.mask_groups[2].masks, "*.bak,*.tmp");
    assert!(
        app.config.mask_groups[0]
            .masks
            .starts_with("*.zip,*.rar,*.[7bgxl]z,")
    );
    let saved = app.to_settings();
    assert_eq!(saved.mask_groups, app.config.mask_groups);
}

#[test]
fn the_list_shows_names_in_ten_columns_and_its_keys() {
    let (_temp, mut app) = app();
    app.config.mask_groups[2].name = "temporary files".to_string();
    app.execute(Command::MaskGroups);
    let list = list(&app);
    assert_eq!(list.title, "Groups of file masks");
    assert_eq!(list.bottom, "Ins, Del, F4, F7, Ctrl+R");
    assert_eq!(
        list.lines[1],
        "exec       \u{2502} *.exe,*.cmd,*.bat,*.com,%PATHEXT%"
    );
    assert_eq!(list.lines[2], "temporary  \u{2502} *.bak,*.tmp");
}

#[test]
fn ins_adds_f4_changes_and_a_new_name_takes_the_place() {
    let (_temp, mut app) = app();
    app.execute(Command::MaskGroups);
    press(&mut app, KeyCode::Insert, KeyModifiers::NONE);
    answer(&mut app, &["code", "*.rs,*.toml"]);
    assert_eq!(names(&app), ["arc", "code", "exec", "temp"]);
    assert_eq!(
        list(&app).selected,
        0,
        "the cursor stays where it was, as FAR's"
    );

    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    answer(&mut app, &["bak", "*.bak"]);
    assert_eq!(names(&app), ["arc", "bak", "code", "exec"]);
    assert_eq!(app.config.mask_groups[1].masks, "*.bak");

    // Enter changes as F4 does (the cursor stayed on the last line, now `exec`); the name of another
    // group takes its place.
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    answer(&mut app, &["code", "*.c"]);
    assert_eq!(names(&app), ["arc", "bak", "code"]);
    assert_eq!(app.config.mask_groups[2].masks, "*.c");
}

#[test]
fn del_asks_and_takes_the_group_out_ctrl_r_brings_the_defaults_back() {
    let (_temp, mut app) = app();
    app.config.mask_groups[1].masks = "*.ps1".to_string();
    app.execute(Command::MaskGroups);
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Confirm { lines, .. })) = &app.overlay else {
        panic!("no question");
    };
    assert_eq!(lines, &["Do you wish to delete", "arc"]);
    yes(&mut app);
    assert_eq!(names(&app), ["exec", "temp"]);
    assert!(matches!(app.overlay, Some(Overlay::EditList(_))));

    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    yes(&mut app);
    assert_eq!(names(&app), ["arc", "exec", "temp"]);
    assert!(
        app.config.mask_groups[1].masks.contains("%PATHEXT%"),
        "a default in place of the changed one"
    );
}

#[test]
fn f7_shows_the_groups_whose_masks_take_the_name_and_esc_all_again() {
    let (_temp, mut app) = app();
    app.execute(Command::MaskGroups);
    press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
    answer(&mut app, &["old.TMP"]);
    let found = list(&app);
    assert_eq!(found.title, "old.TMP");
    assert_eq!(found.bottom, "Total: 1");
    assert_eq!(found.items, [2]);
    // F4 changes the group shown.
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    answer(&mut app, &["temp", "*.tmp"]);
    assert_eq!(app.config.mask_groups[2].masks, "*.tmp");
    assert_eq!(list(&app).items, [2]);

    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    let all = list(&app);
    assert_eq!(all.lines.len(), 3);
    assert_eq!(all.title, "Groups of file masks");

    // A second F7 hides more; F10 and Enter show all again too.
    for key in [KeyCode::F(10), KeyCode::Enter] {
        press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
        answer(&mut app, &["a.zip"]);
        assert_eq!(list(&app).items, [0]);
        press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
        answer(&mut app, &["a.tmp"]);
        assert!(list(&app).items.is_empty(), "no group takes both");
        assert_eq!(list(&app).bottom, "Total: 0");
        press(&mut app, key, KeyModifiers::NONE);
        assert_eq!(list(&app).lines.len(), 3, "{key:?}");
    }
    press(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert_eq!(
        names(&app),
        ["arc", "exec", "temp"],
        "the names keep the order"
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
}

#[test]
fn a_mask_takes_a_group_by_its_name() {
    let (_temp, mut app) = app();
    app.select_group(Side::Left, true, "<arc>,<temp>");
    let mut selected: Vec<&String> = app.panels[0].selected.iter().collect();
    selected.sort();
    assert_eq!(selected, ["a.zip", "c.bak"]);
    assert!(
        app.file_mask("*.*|<arc>")
            .is_some_and(|mask| !mask.matches("x.7z"))
    );
    assert!(app.file_mask("<nothing>").is_none());
    assert!(
        app.file_mask("|<temp>")
            .is_some_and(|mask| mask.matches("b.txt") && !mask.matches("c.bak"))
    );
}

#[test]
fn pathext_is_read_from_the_variables_commands_get() {
    let (_temp, mut app) = app();
    app.environment.set("PATHEXT", ".VCX;.VCY");
    let exec = app.file_mask("<exec>").unwrap();
    assert!(exec.matches("tool.vcy"));
    assert!(exec.matches("run.bat"));
    assert!(!exec.matches("notes.txt"));
}
