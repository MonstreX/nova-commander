//! A panel key waiting for its plugin: the keys after it held and done after the answer in order, the key
//! passed back done first and only where it was pressed, the wait let go after [`QUICK`](crate::app::held::QUICK) or when a
//! window comes over the panel (FAR's `ProcessPanelInput` is synchronous).

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::*;
use crate::app::Overlay;
use crate::app::dialog::Dialog;
use crate::app::fileops::tests::app_in;
use crate::app::held::{ESC_STOPS, QUICK};
use crate::vfs::Entry;
use std::time::Duration;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// The left panel as a plugin's ("probe") with three entries, the cursor on the first.
fn plugin_panel() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let panel = &mut app.panels[0];
    panel.path = VfsPath::in_session("probe", "one", "/");
    panel.entries = ["a", "b", "c"]
        .map(|name| Entry {
            name: name.to_string(),
            ..Entry::default()
        })
        .to_vec();
    panel.cursor = 0;
    app.active = Side::Left;
    (temp, app)
}

/// `key` sent to the plugin as the press `serial`, `age` ago.
fn waiting(app: &mut App, key: KeyEvent, serial: u64, age: Duration) {
    let panel = &app.panels[0];
    app.plugins.key_wait = Some(KeyWait {
        serial,
        plugin: "probe".to_string(),
        side: Side::Left,
        key,
        path: panel.path.clone(),
        cursor: panel.current().map(|entry| entry.name.clone()),
        held: Held::new(Instant::now() - age, None),
        told: None,
        step: 0,
        kind: WaitKind::Key,
        editor: None,
    });
}

fn passed() -> KeyAnswer {
    KeyAnswer {
        pass: true,
        ..KeyAnswer::default()
    }
}

#[test]
fn the_keys_after_wait_and_the_passed_key_comes_first() {
    let (_temp, mut app) = plugin_panel();
    waiting(&mut app, key(KeyCode::Insert), 1, Duration::ZERO);
    app.dispatch_key(key(KeyCode::Down));
    assert_eq!(app.panels[0].cursor, 0, "Down held");
    app.plugin_key_done("probe", 0, 1, passed());
    let panel = &app.panels[0];
    assert!(
        panel.selected.contains("a"),
        "Insert done on its own entry first"
    );
    assert!(!panel.selected.contains("b"));
    // Insert marks and steps down (FAR), then the held Down.
    assert_eq!(panel.cursor, 2, "then Down");
    assert!(app.plugins.key_wait.is_none());
}

#[test]
fn a_passed_key_is_let_be_when_the_cursor_moved_and_a_stale_answer_holds_nothing() {
    let (_temp, mut app) = plugin_panel();
    waiting(&mut app, key(KeyCode::Insert), 1, Duration::ZERO);
    app.panels[0].cursor = 2;
    app.plugin_key_done("probe", 0, 1, passed());
    assert!(app.panels[0].selected.is_empty(), "not on another entry");
    // An answer for another press neither ends the wait nor is done.
    waiting(&mut app, key(KeyCode::Insert), 2, Duration::ZERO);
    app.plugin_key_done("probe", 0, 1, passed());
    assert!(app.panels[0].selected.is_empty());
    assert!(app.plugins.key_wait.is_some(), "still waits for its own");
}

#[test]
fn esc_stops_a_slow_plugins_wait_and_its_late_answer_is_let_be() {
    let (_temp, mut app) = plugin_panel();
    // Not slow yet: Esc waits with the other keys.
    waiting(&mut app, key(KeyCode::Insert), 1, Duration::ZERO);
    app.dispatch_key(key(KeyCode::Esc));
    assert!(
        app.plugins
            .key_wait
            .as_ref()
            .is_some_and(|wait| wait.held.keys.len() == 1)
    );
    // Slow: Esc stops the wait, the key and those held dropped (D14, as D3).
    waiting(
        &mut app,
        key(KeyCode::Insert),
        2,
        QUICK + Duration::from_millis(50),
    );
    app.dispatch_key(key(KeyCode::Down));
    app.dispatch_key(key(KeyCode::Esc));
    assert!(app.plugins.key_wait.is_none());
    assert_eq!(app.panels[0].cursor, 0, "the held Down dropped");
    app.plugin_key_done("probe", 0, 2, passed());
    assert!(
        app.panels[0].selected.is_empty(),
        "its late answer passes nothing"
    );
    // Changed (ADR-0019 §3): the stop ends with a notice; once it is closed the keys are the panel's
    // again.
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Message { .. }))
    ));
    app.dispatch_key(key(KeyCode::Enter));
    app.dispatch_key(key(KeyCode::Down));
    assert_eq!(app.panels[0].cursor, 1);
}

#[test]
fn a_slow_wait_shows_its_window_once_until_a_window_comes() {
    let (_temp, mut app) = plugin_panel();
    waiting(&mut app, key(KeyCode::F(7)), 1, Duration::ZERO);
    assert_eq!(app.plugin_wait_lines(), None, "not slow yet");
    assert!(!app.plugin_wait_due());
    waiting(
        &mut app,
        key(KeyCode::F(7)),
        2,
        QUICK + Duration::from_millis(50),
    );
    let lines = app.plugin_wait_lines().unwrap();
    assert_eq!(
        lines,
        ["probe", PLUGIN_WORKING, ESC_STOPS],
        "no such plugin loaded: its name"
    );
    assert!(app.plugin_wait_due(), "drawn again for it");
    assert!(!app.plugin_wait_due(), "once");
    app.show_dialog(Dialog::error("Busy", vec!["x".to_string()]));
    assert_eq!(app.plugin_wait_lines(), None, "a window over the panel");
}

#[test]
fn disk_work_takes_the_held_keys_before_the_new_one() {
    let (_temp, mut app) = plugin_panel();
    waiting(&mut app, key(KeyCode::F(5)), 1, Duration::ZERO);
    app.dispatch_key(key(KeyCode::Insert));
    crate::app::waiting::quick_as_the_program();
    app.wait_for(Vec::new(), || {
        std::thread::sleep(Duration::from_millis(500));
        Box::new(|_: &mut App| {})
    });
    assert!(app.is_waiting());
    app.dispatch_key(key(KeyCode::Down));
    assert!(
        app.plugins.key_wait.is_none(),
        "the work holds the keys now"
    );
    crate::app::waiting::finish_waiting(&mut app);
    let panel = &app.panels[0];
    assert!(
        panel.selected.contains("a"),
        "Insert, held first, done first"
    );
    assert_eq!(panel.cursor, 2, "then Down");
}

#[test]
fn a_failed_key_call_lets_the_keys_after_it_go_on() {
    let (_temp, mut app) = plugin_panel();
    waiting(&mut app, key(KeyCode::F(5)), 1, Duration::ZERO);
    app.dispatch_key(key(KeyCode::Down));
    app.plugin_key_failed("other", None);
    assert!(app.plugins.key_wait.is_some(), "another plugin's failure");
    app.plugin_key_failed("probe", None);
    assert!(app.plugins.key_wait.is_none());
    assert_eq!(app.panels[0].cursor, 1, "the held Down done");
}

#[test]
fn a_window_over_the_panel_takes_the_keys_at_once() {
    let (_temp, mut app) = plugin_panel();
    waiting(&mut app, key(KeyCode::F(8)), 1, Duration::ZERO);
    // The plugin's call shows a window (as TmpPanel's switch menu): the keys are its own.
    app.overlay = Some(Overlay::Dialog(Dialog::message(
        "Probe",
        vec!["x".to_string()],
    )));
    app.dispatch_key(key(KeyCode::Esc));
    assert!(app.overlay.is_none(), "Esc went to the window");
    assert!(app.plugins.key_wait.is_none());
}

#[test]
fn a_passed_key_is_let_be_when_the_other_panel_became_active() {
    let (_temp, mut app) = plugin_panel();
    waiting(&mut app, key(KeyCode::Insert), 1, Duration::ZERO);
    app.active = Side::Right;
    app.plugin_key_done("probe", 0, 1, passed());
    assert!(app.panels[0].selected.is_empty() && app.panels[1].selected.is_empty());
}

/// The answer `go` for the press `serial`.
fn going(go: KeyGo) -> KeyAnswer {
    KeyAnswer {
        go: Some(go),
        ..KeyAnswer::default()
    }
}

#[test]
fn answers_of_calls_that_opened_windows_are_applied_in_full_each_by_its_serial() {
    let (_temp, mut app) = plugin_panel();
    // Two calls, each of which opened a window: both let go of their keys, both still answered.
    for serial in [1, 2] {
        waiting(&mut app, key(KeyCode::F(8)), serial, Duration::ZERO);
        app.overlay = Some(Overlay::Dialog(Dialog::message("Probe", vec![])));
        assert!(app.expire_plugin_key());
        app.overlay = None;
    }
    assert_eq!(
        app.plugins.key_late.len(),
        2,
        "one slot would lose the first"
    );
    app.plugin_key_done("probe", 0, 1, going(KeyGo::CommandLine("one".to_string())));
    assert_eq!(app.command_line.text(), "one", "applied after its window");
    app.plugin_key_done("probe", 0, 2, going(KeyGo::CommandLine("two".to_string())));
    assert_eq!(app.command_line.text(), "two");
    assert!(app.plugins.key_late.is_empty());
}

#[test]
fn a_panel_opened_for_a_press_nobody_waits_for_is_closed_as_never_shown() {
    let (_temp, mut app) = plugin_panel();
    // Esc stopped the wait: the plugin's late answer opened a panel no panel will show.
    app.plugin_key_done("probe", 0, 7, going(KeyGo::Open("local9".to_string())));
    assert_eq!(
        app.plugins.opening,
        [(VfsPath::in_session("probe", "local9", "/"), Side::Left)],
        "the next tick closes it (FAR's ClosePanel)"
    );
}

#[test]
fn a_file_passed_on_is_let_be_when_the_panel_moved_meanwhile() {
    let (_temp, mut app) = plugin_panel();
    waiting(&mut app, key(KeyCode::Enter), 3, Duration::ZERO);
    if let Some(wait) = &mut app.plugins.key_wait {
        wait.kind = WaitKind::File {
            path: std::path::PathBuf::from(r"C:\nowhere\list.temp"),
            name: "a".to_string(),
            alternative: false,
            next: Vec::new(),
            temp: None,
        };
    }
    app.panels[0].cursor = 2;
    app.plugin_key_done("probe", 0, 3, passed());
    assert!(
        app.overlay.is_none(),
        "nothing opened: the cursor is elsewhere"
    );
    assert!(app.plugins.key_wait.is_none());
}

#[test]
fn found_files_a_plugin_refuses_go_to_the_next_and_then_nowhere() {
    let (_temp, mut app) = plugin_panel();
    waiting(&mut app, key(KeyCode::Enter), 4, Duration::ZERO);
    if let Some(wait) = &mut app.plugins.key_wait {
        wait.kind = WaitKind::FindList {
            found: vec![Entry {
                name: r"C:\a.txt".to_string(),
                real_path: Some(std::path::PathBuf::from(r"C:\a.txt")),
                ..Entry::default()
            }],
            next: Vec::new(),
        };
        wait.held.keys.push(key(KeyCode::Down));
    }
    let before = app.panels[0].path.clone();
    app.plugin_key_done("probe", 0, 4, passed());
    assert!(app.plugins.key_wait.is_none(), "no plugin left to ask");
    assert_eq!(app.panels[0].path, before, "no panel opened");
    assert_eq!(app.panels[0].cursor, 1, "the key held after it went on");
}

#[test]
fn a_command_waited_for_lets_its_keys_go_on_and_one_esc_let_go_has_its_panel_closed() {
    // ADR-0019 (review R13): F11's commands and the drive menu's items are waited for as keys are, each
    // call by its number.
    let (_temp, mut app) = plugin_panel();
    waiting(&mut app, key(KeyCode::Enter), 5, Duration::ZERO);
    if let Some(wait) = &mut app.plugins.key_wait {
        wait.kind = WaitKind::Run;
        wait.held.keys.push(key(KeyCode::Down));
    }
    app.plugin_run_ran("probe", 4);
    assert!(app.plugins.key_wait.is_some(), "another call's end");
    app.plugin_run_ran("probe", 5);
    assert!(app.plugins.key_wait.is_none(), "its end");
    assert_eq!(app.panels[0].cursor, 1, "the key held after it went on");

    waiting(
        &mut app,
        key(KeyCode::Enter),
        6,
        QUICK + Duration::from_millis(50),
    );
    if let Some(wait) = &mut app.plugins.key_wait {
        wait.kind = WaitKind::Run;
    }
    assert!(app.plugin_wait_lines().is_some(), "its window, slow");
    app.dispatch_key(key(KeyCode::Esc));
    assert!(app.plugins.key_wait.is_none(), "Esc stopped the waiting");
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { lines, .. })) if lines[0] == PLUGIN_STOPPED[0]),
        "a notice"
    );
    app.overlay = None;
    let late = VfsPath::in_session("probe", "late", "/");
    let before = app.panels[0].path.clone();
    app.plugin_run_panel("probe", 0, &late, 6);
    assert_eq!(app.panels[0].path, before, "its late panel not opened");
    assert!(
        app.plugins.opening.iter().any(|(path, _)| *path == late),
        "but closed as never shown"
    );
}
