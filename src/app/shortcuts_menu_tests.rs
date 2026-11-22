//! The lists' menu (RCtrl+n) and Commands > Folder shortcuts.

use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Command;
use crate::app::shortcuts::tests::{app, right_ctrl, shortcut, wait_in};
use crate::settings::FolderShortcut;

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn menu(app: &App) -> &ShortcutMenu {
    let Some(Overlay::Shortcuts(menu)) = &app.overlay else {
        panic!("no menu");
    };
    menu
}

fn texts(app: &App) -> Vec<String> {
    menu(app)
        .lines
        .iter()
        .map(|line| line.text.clone())
        .collect()
}

fn service(name: &str) -> FolderShortcut {
    FolderShortcut {
        name: name.to_string(),
        folder: String::new(),
    }
}

#[test]
fn a_lists_lines_have_hotkeys_its_title_and_separators_from_lines_with_no_folder() {
    let (temp, mut app) = app();
    let two = temp.path().join("two");
    // The other folders are only shown: no test goes to a folder outside its own.
    app.shortcuts.lists[1] = vec![
        service("Projects"),
        shortcut(Path::new(r"C:\Work")),
        service("--"),
        FolderShortcut {
            name: "&Two".to_string(),
            folder: two.display().to_string(),
        },
        service("Not a title"),
        FolderShortcut {
            name: "&Tom && Jerry".to_string(),
            folder: r"C:\Tom".to_string(),
        },
    ];
    app.handle_key(right_ctrl('1'));
    assert_eq!(menu(&app).title, "Projects");
    assert_eq!(
        texts(&app),
        [r"&C:\Work", "", "&Two", "T&om && Jerry"],
        "the second T is taken from Tom"
    );
    assert!(menu(&app).lines[1].separator);

    // A hotkey takes its line.
    press(&mut app, KeyCode::Char('t'), KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    wait_in(&mut app, 0, &two);
}

#[test]
fn a_list_of_separators_alone_takes_nothing() {
    let (temp, mut app) = app();
    app.shortcuts.lists[8] = vec![service("--")];
    app.handle_key(right_ctrl('8'));
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    assert_eq!(app.active_panel().path.as_local(), Some(temp.path()));
    app.handle_key(right_ctrl('8'));
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert!(
        matches!(app.overlay, Some(Overlay::Shortcuts(_))),
        "no dialog"
    );
    assert_eq!(app.shortcuts.lists[8], [service("--")]);
}

#[test]
fn ins_adds_the_panels_folder_before_the_cursor_and_del_takes_a_line_out() {
    let (temp, mut app) = app();
    let one = temp.path().join("one");
    app.shortcuts.lists[2] = vec![shortcut(&one), shortcut(Path::new(r"C:\x"))];
    app.handle_key(right_ctrl('2'));
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Insert, KeyModifiers::NONE);
    assert_eq!(app.shortcuts.lists[2][1], shortcut(temp.path()));
    assert_eq!(menu(&app).selected, 1, "on the new line");
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(
        app.shortcuts.lists[2],
        [shortcut(&one), shortcut(Path::new(r"C:\x"))]
    );
}

#[test]
fn ctrl_up_and_down_move_a_line_and_the_cursor_with_it() {
    let (_temp, mut app) = app();
    let (a, b) = (shortcut(Path::new(r"C:\a")), shortcut(Path::new(r"C:\b")));
    app.shortcuts.lists[3] = vec![a.clone(), b.clone()];
    app.handle_key(right_ctrl('3'));
    press(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert_eq!(app.shortcuts.lists[3], [b.clone(), a.clone()]);
    assert_eq!(menu(&app).selected, 1);
    press(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert_eq!(app.shortcuts.lists[3], [b, a], "not past the end");
}

#[test]
fn rctrl_and_the_lists_digit_goes_down_another_digit_shows_that_list() {
    let (temp, mut app) = app();
    let two = temp.path().join("two");
    app.shortcuts.lists[4] = vec![shortcut(Path::new(r"C:\a")), shortcut(Path::new(r"C:\b"))];
    app.shortcuts.lists[5] = vec![shortcut(&two), shortcut(Path::new(r"C:\c"))];
    app.handle_key(right_ctrl('4'));
    app.handle_key(right_ctrl('4'));
    assert_eq!(menu(&app).selected, 1);
    app.handle_key(right_ctrl('5'));
    assert_eq!(menu(&app).title, "Folder shortcuts (5)");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    wait_in(&mut app, 0, &two);
}

#[test]
fn the_ten_show_each_lists_first_folder_and_whether_it_has_more() {
    let (_temp, mut app) = app();
    app.shortcuts.lists[1] = vec![shortcut(Path::new(r"C:\a&b"))];
    app.shortcuts.lists[2] = vec![shortcut(Path::new(r"C:\a")), shortcut(Path::new(r"C:\b"))];
    app.execute(Command::FolderShortcuts);
    let menu = menu(&app);
    assert_eq!(menu.title, "Folder shortcuts");
    assert_eq!(menu.lines.len(), 10);
    assert_eq!(menu.lines[0].text, "RCtrl+&0 \u{2502} <none>");
    assert_eq!(menu.lines[1].text, "RCtrl+&1 \u{2502} C:\\a&&b");
    assert!(!menu.lines[1].submenu);
    assert!(menu.lines[2].submenu);
}

#[test]
fn in_the_ten_ins_del_and_f4_change_a_list_of_one_and_open_a_longer_one() {
    let (temp, mut app) = app();
    app.execute(Command::FolderShortcuts);
    press(&mut app, KeyCode::Insert, KeyModifiers::NONE);
    assert_eq!(app.shortcuts.lists[0], [shortcut(temp.path())]);
    press(&mut app, KeyCode::Insert, KeyModifiers::NONE);
    assert_eq!(menu(&app).list, Some(0), "a list of one is opened for Ins");
    assert!(menu(&app).raw);
    press(&mut app, KeyCode::Insert, KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(menu(&app).list, None, "Esc goes back to the ten");
    assert!(menu(&app).lines[0].submenu);

    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(menu(&app).list, Some(0));
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        menu(&app).list,
        None,
        "Enter in a list of the ten goes back"
    );
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert!(app.shortcuts.lists[0].is_empty());
    assert_eq!(menu(&app).lines[0].text, "RCtrl+&0 \u{2502} <none>");
}

#[test]
fn a_list_opened_from_the_ten_shows_its_lines_with_no_folder_as_they_are() {
    let (_temp, mut app) = app();
    app.shortcuts.lists[7] = vec![
        service("Title"),
        service("--"),
        shortcut(Path::new(r"C:\a")),
    ];
    app.execute(Command::FolderShortcuts);
    press(&mut app, KeyCode::Char('7'), KeyModifiers::NONE);
    assert_eq!(menu(&app).list, Some(7), "Enter on a longer list shows it");
    assert!(!menu(&app).raw);
    assert_eq!(menu(&app).title, "Title");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none(), "the ten are not under it");

    app.execute(Command::FolderShortcuts);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(menu(&app).selected, 7);
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert!(menu(&app).raw);
    assert_eq!(texts(&app), ["&Title", "--", r"&C:\a"]);
}

#[test]
fn enter_in_the_ten_goes_to_a_list_of_one() {
    let (temp, mut app) = app();
    let one = temp.path().join("one");
    app.shortcuts.lists[3] = vec![shortcut(&one)];
    app.execute(Command::FolderShortcuts);
    app.handle_key(right_ctrl('3'));
    assert_eq!(menu(&app).list, None, "the ten take no RCtrl digits");
    press(&mut app, KeyCode::Char('3'), KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    wait_in(&mut app, 0, &one);
}
