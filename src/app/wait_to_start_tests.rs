//! Looks at the disks an operation makes before it starts, held as a disk that does not answer: a window
//! over the screen, Esc stops them, the notice says the operation never started, and nothing did
//! (ADR-0019 §3: back to before; review of stage 19-3).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Overlay;
use crate::app::dialog::Dialog;
use crate::app::fileops::tests::app_in;
use crate::app::held::ESC_STOPS;
use crate::jobs::transfer::{Existing, Keep, Request};
use crate::vfs::VfsPath;

fn esc(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
}

/// The notice Esc left, its first line.
fn notice(app: &App) -> Option<String> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Message { lines, .. })) => lines.first().cloned(),
        _ => None,
    }
}

#[test]
fn esc_on_f8s_looks_at_the_disks_asks_nothing_and_deletes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("kept.txt"), "x").unwrap();
    let mut app = app_in(temp.path());
    app.active_panel_mut().end();
    let release = hold_next_work();
    app.ask_delete(false, false);
    assert!(app.is_waiting(), "the disks asked off the screen's thread");
    assert_eq!(
        super::held_text(&app),
        Some(vec![
            "Looking at the disks\u{2026}".to_string(),
            ESC_STOPS.to_string()
        ]),
        "what the work is, in place of \"Working...\""
    );
    esc(&mut app);
    assert_eq!(
        notice(&app).as_deref(),
        Some(NOT_STARTED[0]),
        "it never started"
    );
    drop(release);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(!app.pump_waiting(), "its late end is not done");
    assert!(app.tasks.is_empty(), "no task");
    assert!(temp.path().join("kept.txt").exists());
}

#[test]
fn esc_before_a_copy_starts_leaves_no_task() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "x").unwrap();
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let mut app = app_in(temp.path());
    let request = Request {
        sources: vec![VfsPath::local(temp.path().join("a.txt"))],
        target: VfsPath::local(&target),
        move_: false,
        existing: Existing::Ask,
        ask_read_only: true,
        keep: Keep::default(),
        link: None,
        retrying: Vec::new(),
        filter: None,
    };
    let release = hold_next_work();
    app.retry_skipped(request);
    assert!(
        app.is_waiting(),
        "its volumes asked off the screen's thread"
    );
    esc(&mut app);
    assert_eq!(notice(&app).as_deref(), Some(NOT_STARTED[0]));
    drop(release);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(!app.pump_waiting());
    assert!(app.tasks.is_empty() && app.shown_task.is_none(), "no task");
    assert!(!target.join("a.txt").exists());
}

#[test]
fn a_start_held_and_let_end_starts_the_task() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("gone.txt"), "x").unwrap();
    let mut app = app_in(temp.path());
    let request = crate::jobs::delete::Request {
        items: vec![crate::jobs::delete::Item {
            path: VfsPath::local(temp.path().join("gone.txt")),
            entry: crate::vfs::Entry::default(),
        }],
        mode: crate::jobs::delete::Mode::Remove,
        ask_folders: false,
        ask_read_only: false,
        count_totals: false,
    };
    let release = hold_next_work();
    app.start_delete(request, None);
    assert!(app.tasks.is_empty(), "not started yet");
    release.send(()).unwrap();
    finish_waiting(&mut app);
    assert!(app.shown_task.is_some(), "started once its disks answered");
}
