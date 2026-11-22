//! F4 on a folder shortcut.

use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Overlay;
use crate::app::dialog::Dialog;
use crate::app::shortcuts::tests::{app, right_ctrl, shortcut};
use crate::app::waiting::finish_waiting;

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// Writes the dialog's title and path and presses OK.
fn answer(app: &mut App, name: &str, folder: &str) {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    assert_eq!(form.title, "Folder shortcuts");
    form.set_edit_text(NAME, name);
    form.set_edit_text(PATH, folder);
    let ok = form.items.len() - 2;
    form.focus_item(ok);
    press(app, KeyCode::Enter);
    finish_waiting(app);
    app.tick();
}

fn question(app: &App) -> Vec<String> {
    let Some(Overlay::Dialog(Dialog::Confirm { title, lines, .. })) = &app.overlay else {
        panic!("no question");
    };
    assert_eq!(title, "Error");
    lines.clone()
}

/// The list of RCtrl+1 with two lines, F4 on the first.
fn editing(folder: &Path) -> (tempfile::TempDir, App) {
    let (temp, mut app) = app();
    app.shortcuts.lists[1] = vec![shortcut(folder), shortcut(Path::new(r"C:\b"))];
    app.handle_key(right_ctrl('1'));
    press(&mut app, KeyCode::F(4));
    (temp, app)
}

#[test]
fn f4_shows_the_title_and_path_and_ok_keeps_them() {
    let (temp, mut app) = editing(Path::new(r"C:\a"));
    let two = temp.path().join("two");
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no dialog");
    };
    assert_eq!(form.edit_text(PATH), r"C:\a");
    answer(&mut app, "&Two", &two.display().to_string());
    assert_eq!(
        app.shortcuts.lists[1][0],
        FolderShortcut {
            name: "&Two".to_string(),
            folder: two.display().to_string(),
        }
    );
    assert!(
        matches!(&app.overlay, Some(Overlay::Shortcuts(menu)) if menu.lines[0].text == "&Two"),
        "the list comes back"
    );
}

#[test]
fn no_title_and_no_path_is_a_separator() {
    let (_temp, mut app) = editing(Path::new(r"C:\a"));
    answer(&mut app, "", "");
    assert_eq!(app.shortcuts.lists[1][0].name, "--");
    assert!(app.shortcuts.lists[1][0].folder.is_empty());
}

#[test]
fn a_path_that_is_not_there_is_asked_about() {
    let (temp, mut app) = editing(Path::new(r"C:\a"));
    let missing = temp.path().join("missing").display().to_string();
    answer(&mut app, "", &missing);
    let lines = question(&app);
    assert_eq!(
        lines[1..],
        [missing.clone(), "Save this shortcut?".to_string()]
    );
    // FAR's Yes and No have no hotkeys.
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Enter);
    app.tick();
    assert_eq!(app.shortcuts.lists[1][0], shortcut(Path::new(r"C:\a")));
    assert!(matches!(app.overlay, Some(Overlay::Shortcuts(_))));

    press(&mut app, KeyCode::F(4));
    answer(&mut app, "", &missing);
    press(&mut app, KeyCode::Enter);
    app.tick();
    assert_eq!(app.shortcuts.lists[1][0].folder, missing);
}

#[test]
fn in_a_list_of_the_ten_a_path_is_not_looked_for() {
    let (temp, mut app) = app();
    app.shortcuts.lists[2] = vec![shortcut(Path::new(r"C:\a"))];
    app.execute(crate::app::Command::FolderShortcuts);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::F(4));
    let missing = temp.path().join("missing").display().to_string();
    answer(&mut app, "", &missing);
    assert_eq!(app.shortcuts.lists[2][0].folder, missing);
    assert!(matches!(&app.overlay, Some(Overlay::Shortcuts(menu)) if menu.list.is_none()));
}
