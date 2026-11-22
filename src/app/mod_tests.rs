//! The application's own tests.

use super::*;

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let app = App::new(temp.path(), 100, 40, &crate::settings::Settings::default());
    (temp, app)
}
#[test]
fn height_decrement_leaves_room_for_panels() {
    let (_temp, mut app) = app();
    assert_eq!(
        app.height_decrement, 0,
        "FAR: panels over the whole height by default"
    );
    app.change_height_decrement(100);
    assert_eq!(app.height_decrement, 40 - 2 - MIN_PANEL_ROWS);
    app.resize(100, 20);
    assert_eq!(app.height_decrement, 20 - 2 - MIN_PANEL_ROWS);
    app.change_height_decrement(-100);
    assert_eq!(app.height_decrement, 0);
}

#[test]
fn back_or_esc_on_tasks_that_do_not_stop_keeps_the_program_and_quit_anyway_quits() {
    // Review R12: the question about cancelled tasks that do not stop has a way back.
    let temp = tempfile::tempdir().unwrap();
    let mut app = super::fileops::tests::app_in(temp.path());
    app.work.quit_when_idle = true;
    app.finish_confirm(super::dialog::ConfirmAction::QuitNotStopping, 2);
    assert!(!app.work.quit_when_idle && !app.quit, "Back");
    app.work.quit_when_idle = true;
    app.show_dialog(Dialog::Confirm {
        title: "Quit".to_string(),
        lines: Vec::new(),
        buttons: vec!["Wait", "Quit anyway", "Back"],
        focused: 0,
        danger: true,
        action: super::dialog::ConfirmAction::QuitNotStopping,
    });
    app.handle_key(KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Esc,
        ratatui::crossterm::event::KeyModifiers::NONE,
    ));
    assert!(!app.work.quit_when_idle && !app.quit, "Esc as Back");
    app.finish_confirm(super::dialog::ConfirmAction::QuitNotStopping, 1);
    assert!(app.quit, "Quit anyway");
}
