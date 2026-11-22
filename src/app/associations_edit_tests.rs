use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::{Command, Overlay};

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let app = app_in(temp.path());
    (temp, app)
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    app.tick();
}

fn lines(app: &App) -> Vec<String> {
    match &app.overlay {
        Some(Overlay::EditList(list)) => list.lines.clone(),
        _ => panic!("no list"),
    }
}

fn form_mut(app: &mut App) -> &mut Form {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("no dialog"),
    }
}

/// Ins, the dialog filled in, OK.
fn add(app: &mut App, masks: &str, description: &str, execute: &str) {
    press(app, KeyCode::Insert, KeyModifiers::NONE);
    let form = form_mut(app);
    assert_eq!(form.title, "Edit file associations");
    form.set_edit_text(MASKS, masks);
    form.set_edit_text(DESCRIPTION, description);
    form.set_edit_text(FIRST_COMMAND + 1, execute);
    let ok = form.items.len() - 2;
    form.focus_item(ok);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

#[test]
fn ins_adds_an_association_with_all_its_boxes_on_before_the_cursor() {
    let (_temp, mut app) = app();
    app.execute(Command::FileAssociations);
    assert!(lines(&app).is_empty());
    add(&mut app, "*.md", "Markdown", "code !.!");
    add(&mut app, "*.txt", "Text", "notepad !.!");
    assert_eq!(
        lines(&app),
        ["Text     \u{2502} *.txt", "Markdown \u{2502} *.md"]
    );
    assert!(app.config.associations[0].view.on && app.config.associations[0].alt_edit.on);
    assert_eq!(app.config.associations[1].execute.command, "code !.!");
}

#[test]
fn a_mask_written_wrong_is_said_and_the_dialog_comes_back() {
    let (_temp, mut app) = app();
    app.execute(Command::FileAssociations);
    add(&mut app, " ", "Nothing", "x");
    let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
        panic!("no warning");
    };
    assert_eq!(lines, &["File-mask string contains errors"]);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(form_mut(&mut app).edit_text(DESCRIPTION), "Nothing");
    assert!(app.config.associations.is_empty());
}

#[test]
fn f4_changes_del_asks_and_ctrl_arrows_move() {
    let (_temp, mut app) = app();
    app.execute(Command::FileAssociations);
    add(&mut app, "*.b", "B", "b");
    add(&mut app, "*.a", "A", "a");
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert_eq!(form_mut(&mut app).edit_text(MASKS), "*.a");
    form_mut(&mut app).set_edit_text(DESCRIPTION, "Alpha");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(lines(&app)[0], "Alpha \u{2502} *.a");
    press(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert_eq!(lines(&app), ["B     \u{2502} *.b", "Alpha \u{2502} *.a"]);
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Confirm { lines: asked, .. })) = &app.overlay else {
        panic!("no question");
    };
    assert_eq!(asked, &["Do you wish to delete association for", "\"*.a\""]);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(lines(&app), ["B \u{2502} *.b"]);
}

#[test]
fn the_associations_outlive_the_program() {
    let (temp, mut app) = app();
    app.execute(Command::FileAssociations);
    add(&mut app, "*.log", "Logs", "less !.!");
    let settings = app.to_settings();
    let text = serde_json::to_string(&settings).unwrap();
    let back: crate::settings::Settings = serde_json::from_str(&text).unwrap();
    let restored = App::new(temp.path(), 100, 40, &back);
    assert_eq!(restored.config.associations, app.config.associations);
}

#[test]
fn del_then_cancel_and_a_bad_mask_then_cancel_leave_the_list_as_it_was() {
    let (_temp, mut app) = app();
    app.execute(Command::FileAssociations);
    add(&mut app, "*.a", "A", "a");
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(lines(&app), ["A \u{2502} *.a"]);
    add(&mut app, "", "Bad", "b");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(
        lines(&app),
        ["A \u{2502} *.a"],
        "the dialog gone, the list back"
    );
    assert_eq!(app.config.associations.len(), 1);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
}

#[test]
fn keys_with_modifiers_do_not_edit_the_list() {
    let (_temp, mut app) = app();
    app.execute(Command::FileAssociations);
    add(&mut app, "*.a", "A", "a");
    press(&mut app, KeyCode::Delete, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Insert, KeyModifiers::CONTROL);
    assert_eq!(lines(&app), ["A \u{2502} *.a"], "no question, no dialog");
}
