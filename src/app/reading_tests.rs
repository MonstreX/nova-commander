//! Folders read in the background, as the application handles them (ADR-0013 §5).

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::dialog::ConfirmAction;
use super::{App, Dialog, Overlay, persist};
use crate::vfs::VfsPath;
use crate::vfs::testing::{endless, gate, open};

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let app = App::new(temp.path(), 100, 40, &crate::settings::Settings::default());
    (temp, app)
}

/// Runs the event loop's periodic work until `done` holds, at most 10 s.
fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) && Instant::now() < deadline {
        app.tick();
        std::thread::yield_now();
    }
}

#[test]
fn esc_while_a_folder_is_read_asks_and_yes_shows_what_was_read() {
    let (_temp, mut app) = app();
    let slow = endless();
    app.active_panel_mut().set_read_wait(Duration::ZERO);
    app.active_panel_mut().change_dir(&slow, None).unwrap();

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Confirm {
            action: ConfirmAction::StopReading,
            ..
        }))
    ));
    assert!(!app.panels_hidden(), "Esc did not hide the panels");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.overlay.is_none());
    assert_eq!(app.active_panel().reading(), None);
    assert_eq!(app.active_panel().path, slow);
    assert_eq!(app.active_panel().entries[0].name, "first");
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(
        app.panels_hidden(),
        "nothing is read now: Esc is the command line's again"
    );
}

#[test]
fn with_its_question_off_esc_stops_the_reading_at_once() {
    let (_temp, mut app) = app();
    app.prefs.confirm_interrupt = false;
    let slow = endless();
    app.active_panel_mut().set_read_wait(Duration::ZERO);
    app.active_panel_mut().change_dir(&slow, None).unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.overlay.is_none());
    assert_eq!(app.active_panel().reading(), None);
}

#[test]
fn a_late_failure_waits_for_the_dialog_the_user_opened() {
    let (temp, mut app) = app();
    app.active_panel_mut().set_read_wait(Duration::ZERO);
    app.active_panel_mut()
        .change_dir(&gate("late-failure"), None)
        .unwrap();
    app.show_dialog(Dialog::message("Mine", vec![String::new()]));
    open("late-failure");

    tick_until(&mut app, |app| app.active_panel().reading().is_none());
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { title, .. })) if title == "Mine"),
        "the user's dialog stays"
    );
    app.overlay = None;
    app.tick();

    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Message { title, error: true, .. })) if title == "Change directory"
    ));
    assert_eq!(app.active_panel().path, temp.path(), "the panel stayed");
}

#[test]
fn a_provider_s_panel_has_no_os_folder() {
    let (_temp, mut app) = app();
    app.active_panel_mut().path = endless();
    assert_eq!(
        app.active_folder(),
        crate::platform::process::system_folder(),
        "commands run in the system folder until providers exist"
    );
    // Nothing is listed yet, so nothing is under the cursor.
    assert!(app.active_panel().targets().is_empty());
    assert_eq!(app.active_panel().current_path(), None);
    let start = std::env::temp_dir();
    assert_eq!(
        persist::panel_paths(
            ["sftp://host/home"],
            &start,
            persist::START_LOOK,
            std::path::Path::is_dir
        )[0],
        VfsPath::local(&start),
        "a provider's folder is not reopened on start yet"
    );
}

#[test]
fn a_folder_changed_on_disk_waits_while_a_dialog_is_up_and_the_panels_are_hidden() {
    use crate::vfs::testing::put;
    let (_temp, mut app) = app();
    let dir = put("changing://app/", &["a"]);
    app.active_panel_mut().change_dir(&dir, None).unwrap();
    put("changing://app/", &["a", "b"]);
    app.show_dialog(Dialog::message("Mine", vec![String::new()]));
    app.tick();
    assert_eq!(app.active_panel().entries.len(), 1, "behind a dialog");

    app.overlay = None;
    app.active_panel_mut().visible = false;
    app.tick();
    assert_eq!(app.active_panel().entries.len(), 1, "a hidden panel");

    app.active_panel_mut().visible = true;
    app.main_view = super::MainView::CommandOutput;
    app.tick();
    assert_eq!(app.active_panel().entries.len(), 1, "a command runs");

    app.main_view = super::MainView::Panels;
    assert!(app.tick());
    // Read in the background: taken by a later tick.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.active_panel().entries.len() != 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "the refresh did not end"
        );
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
