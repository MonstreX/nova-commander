//! The tree's quick search and the keys that give its folder away (FAR's `TreeList::FastFind`,
//! `ProcessKey`).

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::super::tests::{app, current, on_tree, press};
use crate::app::clipboard::Memory;
use crate::app::{App, Overlay};

fn search_text(app: &App) -> Option<&str> {
    match &app.overlay {
        Some(Overlay::QuickSearch(search)) => Some(&search.text),
        _ => None,
    }
}

fn other_folder(app: &App) -> std::path::PathBuf {
    let other = app.tree_side().unwrap().other().index();
    app.panels[other].path.as_local().unwrap().to_path_buf()
}

#[test]
fn alt_letter_finds_a_folder_by_its_own_name_and_keeps_only_what_matches() {
    let (_temp, mut app) = app();
    on_tree(&mut app);
    press(&mut app, KeyCode::Char('d'), KeyModifiers::ALT);
    assert_eq!(current(&app), "d");
    assert_eq!(search_text(&app), Some("d"));
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(search_text(&app), Some("d"), "dx matches nothing");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(search_text(&app), None);
    press(&mut app, KeyCode::Char('c'), KeyModifiers::ALT);
    assert_eq!(
        current(&app),
        "a/c",
        "from the cursor on, then from the top"
    );
}

#[test]
fn ctrl_enter_in_the_search_goes_to_the_next_and_ctrl_shift_enter_back() {
    let (temp, mut app) = app();
    for folder in ["a/b/bb", "bx"] {
        std::fs::create_dir_all(temp.path().join(folder)).unwrap();
    }
    on_tree(&mut app);
    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    super::super::tests::settle(&mut app);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
    assert_eq!(current(&app), "a/b");
    press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
    assert_eq!(current(&app), "a/b/bb");
    press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
    assert_eq!(current(&app), "bx");
    press(
        &mut app,
        KeyCode::Enter,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(current(&app), "a/b/bb");
}

#[test]
fn the_search_closed_with_auto_change_folder_takes_the_other_panel_there() {
    let (temp, mut app) = app();
    app.prefs.tree.auto_change_folder = true;
    on_tree(&mut app);
    press(&mut app, KeyCode::Char('d'), KeyModifiers::ALT);
    assert_eq!(
        other_folder(&app),
        temp.path().join("a"),
        "not while searching"
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(other_folder(&app), temp.path().join("d"));
}

#[test]
fn ctrl_enter_and_ctrl_f_put_the_folder_s_whole_path_in_the_command_line() {
    let (temp, mut app) = app();
    on_tree(&mut app);
    let a = temp.path().join("a").display().to_string();
    press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
    let quoted = super::super::super::cmdline_insert::quote_for_shell(&a);
    assert_eq!(app.command_line.text(), format!("{quoted} {quoted} "));
}

#[test]
fn ctrl_alt_ins_puts_the_folder_s_path_on_the_clipboard() {
    let (temp, mut app) = app();
    let memory = Memory::default();
    app.clipboard = Box::new(memory.clone());
    on_tree(&mut app);
    press(
        &mut app,
        KeyCode::Insert,
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    let a = temp.path().join("a").display().to_string();
    assert_eq!(memory.contents().text, Some(a));
}

#[test]
fn ctrl_backslash_takes_the_cursor_and_the_other_panel_to_the_root() {
    let (temp, mut app) = app();
    on_tree(&mut app);
    press(&mut app, KeyCode::Char('\\'), KeyModifiers::CONTROL);
    assert_eq!(current(&app), "");
    assert_eq!(other_folder(&app), temp.path());
}

#[test]
fn ctrl_shift_digit_keeps_the_tree_s_folder_as_a_shortcut() {
    let (temp, mut app) = app();
    on_tree(&mut app);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(
        &mut app,
        KeyCode::Char('3'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    let kept: Vec<&str> = (app.shortcuts.lists[3].iter())
        .map(|shortcut| shortcut.folder.as_str())
        .collect();
    assert_eq!(kept, [temp.path().join("d").display().to_string()]);
}

#[test]
fn alt_shift_digit_keeps_the_tree_s_folder_and_starts_no_search() {
    let (temp, mut app) = app();
    on_tree(&mut app);
    app.handle_key(crate::app::shortcuts::tests::alt_shift(0x35, '%'));
    assert!(search_text(&app).is_none(), "no quick search");
    let kept: Vec<&str> = (app.shortcuts.lists[5].iter())
        .map(|shortcut| shortcut.folder.as_str())
        .collect();
    assert_eq!(kept, [temp.path().join("a").display().to_string()]);
}

#[test]
fn enter_closing_the_search_takes_the_other_panel_to_the_folder_found() {
    let (temp, mut app) = app();
    on_tree(&mut app);
    app.command_line.insert_str("typed");
    press(&mut app, KeyCode::Char('d'), KeyModifiers::ALT);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(other_folder(&app), temp.path().join("d"));
    assert_eq!(app.command_line.text(), "typed", "not run");
}

#[test]
fn a_key_that_takes_the_keys_elsewhere_still_lets_auto_change_folder_go() {
    let (temp, mut app) = app();
    app.prefs.tree.auto_change_folder = true;
    on_tree(&mut app);
    press(&mut app, KeyCode::Char('d'), KeyModifiers::ALT);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert!(!app.tree_is_active());
    assert_eq!(other_folder(&app), temp.path().join("d"));
}

#[test]
fn backspace_and_ctrl_y_empty_the_search_on_the_tree_too() {
    let (_temp, mut app) = app();
    on_tree(&mut app);
    press(&mut app, KeyCode::Char('d'), KeyModifiers::ALT);
    assert_eq!(search_text(&app), Some("d"));
    press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(search_text(&app), Some(""), "FAR's `Name + '*'` takes it");
    assert_eq!(current(&app), "d", "the cursor stays on what still matches");
    press(&mut app, KeyCode::Char('d'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert_eq!(search_text(&app), Some(""));
}
