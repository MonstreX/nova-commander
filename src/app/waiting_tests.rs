//! The waiting for work: its window, Esc, the keys held, its end done to the app.

use super::*;
use crate::app::Overlay;
use crate::app::fileops::tests::app_in;
use crate::app::held::ESC_STOPS;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn keys_wait_while_the_work_runs_and_its_end_is_done_to_the_app() {
    quick_as_the_program();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let (go, wait) = mpsc::channel::<()>();
    app.wait_for(vec![WAITING_FOR_DISK.to_string()], move || {
        let _ = wait.recv();
        Box::new(|app: &mut App| app.show_hidden = !app.show_hidden)
    });
    let hidden = app.show_hidden;
    app.handle_key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    assert!(
        app.held_window()
            .is_some_and(|window| window.lines[0] == WAITING_FOR_DISK)
    );
    assert!(app.pump_waiting(), "its window drawn");
    assert!(!app.pump_waiting(), "still running");
    go.send(()).unwrap();
    finish_waiting(&mut app);
    assert_eq!(app.show_hidden, !hidden);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Confirm { .. }))),
        "F10, held meanwhile, asks to quit after the work"
    );
}

#[test]
fn esc_stops_the_waiting_and_the_app_stays_as_it_was() {
    quick_as_the_program();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let (go, wait) = mpsc::channel::<()>();
    app.wait_for(Vec::new(), move || {
        let _ = wait.recv();
        Box::new(|app: &mut App| app.show_hidden = !app.show_hidden)
    });
    assert_eq!(
        held_text(&app),
        Some(vec![WORKING.to_string(), ESC_STOPS.to_string()]),
        "a window over the screen past the quick time"
    );
    let hidden = app.show_hidden;
    app.handle_key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.is_waiting(), "no longer waited for");
    assert_eq!(app.held_window(), None);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { lines, .. })) if lines[0] == STOPPED[0]),
        "the notice, not F10's question: the held keys went"
    );
    go.send(()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(!app.pump_waiting());
    assert_eq!(
        app.show_hidden, hidden,
        "its late end is not done to the app"
    );
}

#[test]
fn the_window_esc_stopped_the_work_over_comes_back_after_the_notice() {
    quick_as_the_program();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.show_dialog(Dialog::message("Behind", vec!["the menu".to_string()]));
    let (_go, wait) = mpsc::channel::<()>();
    app.wait_for(Vec::new(), move || {
        let _ = wait.recv();
        Box::new(|_: &mut App| {})
    });
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { title, .. })) if title == "Stopped")
    );
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { title, .. })) if title == "Behind"),
        "back as it was"
    );
}

#[test]
fn its_own_window_says_that_esc_stops_it() {
    quick_as_the_program();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let (go, wait) = mpsc::channel::<()>();
    app.wait_for(vec![WAITING_FOR_DISK.to_string()], move || {
        let _ = wait.recv();
        Box::new(|_: &mut App| {})
    });
    assert_eq!(
        held_text(&app),
        Some(vec![WAITING_FOR_DISK.to_string(), ESC_STOPS.to_string()]),
        "its lines, then Esc"
    );
    go.send(()).unwrap();
    finish_waiting(&mut app);
}

/// Owner 2026-11-21: the window shows the work goes on - a busy bar and its time, or FAR's bar
/// with the percent once the work says how far it is (the editor's save).
#[test]
fn the_window_shows_the_work_going_on_and_how_far_it_is() {
    quick_as_the_program();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let (go, wait) = mpsc::channel::<()>();
    app.wait_for(vec!["Saving".to_string()], move || {
        let _ = wait.recv();
        Box::new(|_: &mut App| {})
    });
    let window = app.held_window().unwrap();
    assert_eq!(window.lines, ["Saving"]);
    assert!(matches!(window.bar, Some(WaitBar::Busy(_))), "{window:?}");
    assert_eq!(window.tail, [ESC_STOPS]);
    let done = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(25));
    app.show_progress(std::sync::Arc::clone(&done), 100);
    assert_eq!(app.held_window().unwrap().bar, Some(WaitBar::Percent(25)));
    assert!(app.pump_waiting(), "the new percent drawn");
    done.store(50, std::sync::atomic::Ordering::Relaxed);
    assert!(app.pump_waiting(), "and the next");
    assert_eq!(app.held_window().unwrap().bar, Some(WaitBar::Percent(50)));
    go.send(()).unwrap();
    finish_waiting(&mut app);
}

#[test]
fn keys_pressed_meanwhile_come_after_the_work_in_order() {
    quick_as_the_program();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let (go, wait) = mpsc::channel::<()>();
    app.wait_for(Vec::new(), move || {
        let _ = wait.recv();
        Box::new(|app: &mut App| app.command_line.insert_str("a"))
    });
    for letter in ['b', 'c'] {
        app.handle_key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
    }
    assert_eq!(app.command_line.text(), "");
    go.send(()).unwrap();
    finish_waiting(&mut app);
    assert_eq!(app.command_line.text(), "abc");
}

#[test]
fn quick_work_is_done_at_once() {
    quick_as_the_program();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.wait_for(vec![WAITING_FOR_DISK.to_string()], || {
        Box::new(|app: &mut App| app.command_line.insert_str("done"))
    });
    assert!(!app.is_waiting());
    assert!(app.overlay.is_none(), "no message for quick work");
    assert_eq!(app.command_line.text(), "done");
}

#[test]
fn work_asked_to_stop_ends_by_itself_and_its_end_is_done() {
    quick_as_the_program();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let (ask, asked) = mpsc::channel::<()>();
    let cancel: Cancel = Box::new(move || {
        let _ = ask.send(());
    });
    let notice = Stop {
        notice: STOPPED,
        on_stop: None,
        back: None,
    };
    app.wait_with(Vec::new(), notice, Some(cancel), move || {
        let _ = asked.recv();
        Box::new(|app: &mut App| app.command_line.insert_str("stopped at 3"))
    });
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.is_waiting(), "asked, not let go");
    assert_eq!(
        held_text(&app),
        Some(vec![
            WORKING.to_string(),
            crate::app::held::STOPPING.to_string()
        ])
    );
    finish_waiting(&mut app);
    assert_eq!(
        app.command_line.text(),
        "stopped at 3",
        "its own end, the key held before Esc gone"
    );
    assert!(app.overlay.is_none(), "no notice of letting go");
}

#[test]
fn work_that_breaks_says_so_and_lets_the_keys_go() {
    quick_as_the_program();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.wait_for(Vec::new(), || panic!("broken on purpose"));
    finish_waiting(&mut app);
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Message { error: true, .. }))
    ));
}

/// Owner 2026-11-21: work with no window of its own says what it is in place of "Working...", the
/// dialog it was asked from left as it is.
#[test]
fn work_without_a_window_of_its_own_says_what_it_is() {
    quick_as_the_program();
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.show_dialog(Dialog::message("Asked from", vec!["here".to_string()]));
    let (go, wait) = mpsc::channel::<()>();
    app.wait_for(Vec::new(), move || {
        let _ = wait.recv();
        Box::new(|_: &mut App| {})
    });
    app.label_waiting("Reading the drives\u{2026}");
    assert_eq!(
        held_text(&app),
        Some(vec![
            "Reading the drives\u{2026}".to_string(),
            ESC_STOPS.to_string()
        ])
    );
    assert!(app.overlay.is_some(), "the dialog stays under it");
    go.send(()).unwrap();
    finish_waiting(&mut app);
}

/// The save's bar: its percent of the document written, never past 100, a document of nothing whole.
#[test]
fn the_percent_done_stays_within_the_whole() {
    assert_eq!(percent_of(1, 3), 33);
    assert_eq!(percent_of(9, 4), 100, "more than the whole counted");
    assert_eq!(percent_of(0, 0), 100, "nothing to write");
    assert_eq!(percent_of(u64::MAX, u64::MAX), 100);
}
