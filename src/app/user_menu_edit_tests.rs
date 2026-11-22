use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::waiting::finish_waiting;
use crate::settings::user_menu::MENU_FILE;

fn app(local: &str) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join(MENU_FILE), local).unwrap();
    let mut app = app_in(temp.path());
    app.set_main_menu_file(temp.path().join("main.ini"));
    (temp, app)
}

fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    finish_waiting(app);
    // What came back after the work is shown on the next frame.
    app.tick();
}

fn press(app: &mut App, code: KeyCode) {
    key(app, code, KeyModifiers::NONE);
}

fn menu(app: &App) -> &UserMenu {
    match &app.overlay {
        Some(Overlay::UserMenu(menu)) => menu,
        _ => panic!("no user menu"),
    }
}

fn form_mut(app: &mut App) -> &mut Form {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("no dialog"),
    }
}

fn saved(temp: &tempfile::TempDir) -> String {
    std::fs::read_to_string(temp.path().join(MENU_FILE)).unwrap_or_default()
}

/// The item's dialog filled in and OK pressed.
fn fill_and_ok(app: &mut App, hotkey: &str, label: &str, commands: &[&str]) {
    let form = form_mut(app);
    form.set_edit_text(HOTKEY, hotkey);
    form.set_edit_text(LABEL, label);
    for (line, command) in commands.iter().enumerate() {
        form.set_edit_text(FIRST_COMMAND + line, command);
    }
    let ok = form.items.len() - 2;
    form.focus_item(ok);
    press(app, KeyCode::Enter);
}

#[test]
fn ins_asks_command_or_menu_and_inserts_before_the_cursor() {
    let (temp, mut app) = app("B:   Bravo\n     echo b\n");
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Insert);
    let Some(Overlay::Dialog(Dialog::Confirm { buttons, .. })) = &app.overlay else {
        panic!("the question");
    };
    assert_eq!(buttons, &["Insert command", "Insert menu"]);
    press(&mut app, KeyCode::Enter);
    assert_eq!(form_mut(&mut app).title, "Edit user menu");
    fill_and_ok(&mut app, "A", "Alpha", &["echo a", "", "echo c"]);
    let shown: Vec<&str> = menu(&app)
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(shown, ["&A   Alpha", "&B   Bravo"]);
    assert_eq!(
        saved(&temp),
        "A:   Alpha\r\n     echo a\r\n     \r\n     echo c\r\nB:   Bravo\r\n     echo b\r\n"
    );
}

#[test]
fn a_submenu_has_its_label_only_and_opens_empty() {
    let (temp, mut app) = app("B:   Bravo\n");
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Insert);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Enter);
    assert_eq!(form_mut(&mut app).title, "Edit submenu label");
    fill_and_ok(&mut app, "T", "Tools", &[]);
    assert!(menu(&app).lines[0].submenu);
    assert!(
        saved(&temp).starts_with("T:   Tools\r\n{\r\n}\r\n"),
        "{}",
        saved(&temp)
    );
}

#[test]
fn f4_changes_the_item_and_a_wrong_one_comes_back_with_why() {
    let (temp, mut app) = app("A:   Alpha\n     echo a\n");
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::F(4));
    assert_eq!(form_mut(&mut app).edit_text(FIRST_COMMAND), "echo a");
    fill_and_ok(&mut app, "AB", "Alpha", &[]);
    let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
        panic!("no error");
    };
    assert_eq!(lines, &["Invalid format for Hot Key"]);
    press(&mut app, KeyCode::Enter);
    fill_and_ok(&mut app, "", "", &[]);
    press(&mut app, KeyCode::Enter);
    fill_and_ok(&mut app, "F12", "Twelve", &[]);
    assert_eq!(menu(&app).lines[0].text, "F12 Twelve");
    assert!(
        saved(&temp).starts_with("F12: Twelve\r\n"),
        "{}",
        saved(&temp)
    );
}

#[test]
fn leaving_with_changed_commands_asks_to_save_them() {
    let (temp, mut app) = app("A:   Alpha\n     echo a\n");
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::F(4));
    form_mut(&mut app).set_edit_text(FIRST_COMMAND, "echo changed");
    press(&mut app, KeyCode::Esc);
    let Some(Overlay::Dialog(Dialog::Confirm { lines, .. })) = &app.overlay else {
        panic!("no question");
    };
    assert_eq!(lines, &["Save changes?"]);
    key(&mut app, KeyCode::Right, KeyModifiers::NONE);
    key(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        form_mut(&mut app).edit_text(FIRST_COMMAND),
        "echo changed",
        "Cancel: back"
    );
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Enter);
    assert!(saved(&temp).contains("echo changed"), "Yes keeps it");
}

#[test]
fn del_asks_and_takes_the_item_out_the_last_one_its_file_too() {
    let (temp, mut app) = app("A:   Alpha\n");
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Delete);
    let Some(Overlay::Dialog(Dialog::Confirm { lines, .. })) = &app.overlay else {
        panic!("no question");
    };
    assert_eq!(lines, &["Do you wish to delete the menu item", "\"Alpha\""]);
    press(&mut app, KeyCode::Esc);
    assert_eq!(menu(&app).lines.len(), 1, "Cancel keeps it");
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Enter);
    assert!(menu(&app).lines.is_empty());
    assert!(!temp.path().join(MENU_FILE).exists());
}

#[test]
fn ctrl_arrows_move_the_item() {
    let (temp, mut app) = app("A:   Alpha\nB:   Bravo\n");
    press(&mut app, KeyCode::F(2));
    key(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert_eq!(menu(&app).selected, 1);
    assert!(saved(&temp).starts_with("B:   Bravo\r\nA:   Alpha\r\n"));
    key(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert_eq!(menu(&app).selected, 1, "the last stays");
}

#[test]
fn alt_f4_edits_the_file_and_the_menu_is_read_again() {
    let (temp, mut app) = app("A:   Alpha\n");
    press(&mut app, KeyCode::F(2));
    key(&mut app, KeyCode::F(4), KeyModifiers::ALT);
    assert!(matches!(app.screen, crate::app::Screen::Editor(_)));
    std::fs::write(temp.path().join(MENU_FILE), "Z:   Zulu\n").unwrap();
    app.screen = crate::app::Screen::Panels;
    app.tick();
    finish_waiting(&mut app);
    assert_eq!(menu(&app).lines[0].text, "&Z   Zulu");
}

#[test]
fn a_move_that_cannot_be_saved_is_said_and_the_menu_comes_back() {
    let (temp, mut app) = app("A:   Alpha\nB:   Bravo\n");
    press(&mut app, KeyCode::F(2));
    // The test's own file: a folder now stands where the menu would be written.
    std::fs::remove_file(temp.path().join(MENU_FILE)).unwrap();
    std::fs::create_dir(temp.path().join(MENU_FILE)).unwrap();
    key(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    let Some(Overlay::Dialog(Dialog::Message { title, .. })) = &app.overlay else {
        panic!("no error");
    };
    assert_eq!(title, TITLE);
    press(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).selected, 1, "the menu is back");
}

#[test]
fn alt_f4_on_a_menu_not_there_leaves_no_file_when_nothing_is_written() {
    let (temp, mut app) = app("");
    std::fs::remove_file(temp.path().join(MENU_FILE)).unwrap();
    app.execute(crate::app::Command::EditUserMenu);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::F(4), KeyModifiers::ALT);
    assert!(matches!(app.screen, crate::app::Screen::Editor(_)));
    app.screen = crate::app::Screen::Panels;
    app.tick();
    finish_waiting(&mut app);
    assert!(!temp.path().join(MENU_FILE).exists());
    assert!(menu(&app).lines.is_empty());
}

#[test]
fn an_item_of_more_than_ten_commands_left_as_it_was_asks_nothing() {
    let commands: String = (0..12).map(|line| format!("     echo {line}\n")).collect();
    let (_temp, mut app) = app(&format!("A:   Alpha\n{commands}"));
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::F(4));
    press(&mut app, KeyCode::Esc);
    assert!(
        matches!(app.overlay, Some(Overlay::UserMenu(_))),
        "no question"
    );
}
