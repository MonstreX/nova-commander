//! Ctrl+Tab and Ctrl+Shift+Tab through the screens, the Screens list's hotkeys and its lines kept up
//! while it is open, a paused task's clock; every task works in the test's own folder.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::dialog::{Dialog, ListAction};
use super::super::find::results::FindResults;
use super::super::transfer_dialog::Operation;
use super::*;
use crate::app::fileops::tests::{answers, app_in, finish_transfer, select_all_files};
use crate::jobs::find::Search;

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn ctrl_tab(app: &mut App) {
    press(app, KeyCode::Tab, KeyModifiers::CONTROL);
}

fn ctrl_shift_tab(app: &mut App) {
    press(
        app,
        KeyCode::BackTab,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
}

/// A search that never ends, found nothing, behind the panels; `mask` tells it apart.
fn search_behind(app: &mut App, mask: &str) {
    let mut results = FindResults::new(Search::stalled(Vec::new(), false), "Find".into(), None);
    results.mask = mask.to_string();
    app.send_find_to_background(Box::new(results));
}

/// The mask of the search in front, if one is.
fn search_in_front(app: &App) -> Option<&str> {
    match &app.overlay {
        Some(Overlay::FindResults(results)) => Some(&results.mask),
        _ => None,
    }
}

/// A copy of `src/a.txt` into `empty`, paused before it starts, its progress in front.
fn paused_task() -> (tempfile::TempDir, App, TaskId) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    let target = temp.path().join("empty");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(source.join("a.txt"), "new").unwrap();
    let mut app = app_in(&source);
    select_all_files(&mut app);
    let sources = app.active_panel().targets();
    app.start_transfer(
        &sources,
        &answers(&target.display().to_string()),
        Operation::Copy,
    );
    let id = app.tasks.iter().next().unwrap().id;
    app.tasks.set_paused(id, true);
    app.tick();
    (temp, app, id)
}

fn shown(app: &App) -> Option<TaskId> {
    app.shown_task.as_ref().map(|shown| shown.id)
}

fn end(mut app: App) {
    app.overlay = None;
    app.tasks.cancel_all();
    finish_transfer(&mut app);
}

#[test]
fn ctrl_tab_goes_through_the_screens_and_ctrl_shift_tab_back() {
    let (_temp, mut app, id) = paused_task();
    search_behind(&mut app, "*.one");
    assert_eq!(shown(&app), Some(id), "the progress in front");

    ctrl_tab(&mut app);
    assert_eq!(shown(&app), None, "the task went behind");
    assert_eq!(search_in_front(&app), Some("*.one"));
    ctrl_tab(&mut app);
    assert!(app.overlay.is_none() && shown(&app).is_none(), "the panels");
    assert_eq!(app.background_searches().len(), 1);
    ctrl_tab(&mut app);
    assert_eq!(shown(&app), Some(id));
    ctrl_shift_tab(&mut app);
    assert!(shown(&app).is_none() && app.overlay.is_none());
    ctrl_shift_tab(&mut app);
    assert_eq!(search_in_front(&app), Some("*.one"), "round to the last");
    end(app);
}

#[test]
fn a_search_brought_to_the_front_goes_back_to_its_place() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    search_behind(&mut app, "*.a");
    search_behind(&mut app, "*.b");
    ctrl_tab(&mut app);
    assert_eq!(search_in_front(&app), Some("*.a"));
    ctrl_tab(&mut app);
    assert_eq!(search_in_front(&app), Some("*.b"));
    assert_eq!(app.background_searches()[0].mask, "*.a", "in its place");
    ctrl_shift_tab(&mut app);
    assert_eq!(search_in_front(&app), Some("*.a"));
    let masks: Vec<&str> = (app.background_searches().iter())
        .map(|results| results.mask.as_str())
        .collect();
    assert_eq!(masks, ["*.b"]);
    end(app);
}

#[test]
fn a_dialog_in_front_keeps_ctrl_tab() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    search_behind(&mut app, "*.a");
    app.show_error("Error", &std::io::Error::other("x"));
    ctrl_tab(&mut app);
    assert!(matches!(app.overlay, Some(Overlay::Dialog(_))));
    assert_eq!(app.background_searches().len(), 1, "nothing switched");
    end(app);
}

#[test]
fn only_ctrl_tab_and_ctrl_shift_tab_switch() {
    let key = |code, modifiers| direction(KeyEvent::new(code, modifiers));
    assert_eq!(key(KeyCode::Tab, KeyModifiers::CONTROL), Some(true));
    let ctrl_shift = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    assert_eq!(key(KeyCode::BackTab, ctrl_shift), Some(false));
    assert_eq!(key(KeyCode::Tab, ctrl_shift), Some(false));
    assert_eq!(key(KeyCode::Tab, KeyModifiers::NONE), None);
    assert_eq!(key(KeyCode::F(12), ctrl_shift), None, "the detach key");
    assert_eq!(key(KeyCode::Tab, KeyModifiers::ALT), None);
}

fn screens_list(app: &App) -> (&[String], usize) {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::List {
            items,
            cursor,
            action: ListAction::Screens(_),
            ..
        })) => (items, *cursor),
        _ => panic!("no Screens list"),
    }
}

#[test]
fn a_hotkey_of_the_screens_list_brings_its_line_to_the_front() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    search_behind(&mut app, "*.a");
    search_behind(&mut app, "*.b");
    press(&mut app, KeyCode::F(12), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('z'), KeyModifiers::NONE);
    assert_eq!(
        screens_list(&app).0.len(),
        3,
        "no such line: the list stays"
    );
    press(&mut app, KeyCode::Char('2'), KeyModifiers::NONE);
    assert_eq!(search_in_front(&app), Some("*.b"));
    end(app);
}

#[test]
fn hotkeys_are_digits_then_letters_alt_with_them() {
    let line = |ch, modifiers| {
        super::super::screens::hotkey_line(KeyEvent::new(KeyCode::Char(ch), modifiers))
    };
    assert_eq!(line('0', KeyModifiers::NONE), Some(0));
    assert_eq!(line('9', KeyModifiers::NONE), Some(9));
    assert_eq!(line('a', KeyModifiers::NONE), Some(10));
    assert_eq!(line('Z', KeyModifiers::SHIFT), Some(35));
    assert_eq!(line('b', KeyModifiers::ALT), Some(11));
    assert_eq!(line('b', KeyModifiers::CONTROL), None);
    assert_eq!(line('-', KeyModifiers::NONE), None);
}

#[test]
fn the_open_screens_list_follows_and_keeps_its_cursor() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    search_behind(&mut app, "*.a");
    press(&mut app, KeyCode::F(12), KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert!(!app.refresh_screens_list(), "nothing changed");
    search_behind(&mut app, "*.b");
    assert!(app.refresh_screens_list());
    let (items, cursor) = screens_list(&app);
    assert_eq!(items.len(), 3);
    assert!(items[2].contains("*.b"), "{items:?}");
    assert_eq!(cursor, 1, "still on the first search");
    end(app);
}

#[test]
fn a_paused_tasks_clock_stands() {
    let (_temp, mut app, id) = paused_task();
    let before = app.tasks.get(id).unwrap().elapsed();
    let deadline = Instant::now() + Duration::from_millis(150);
    while Instant::now() < deadline {
        app.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    let after = app.tasks.get(id).unwrap().elapsed();
    assert!(
        after - before < Duration::from_millis(60),
        "{before:?} {after:?}"
    );
    end(app);
}

#[test]
fn a_command_on_the_console_and_the_held_back_key_switch_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    search_behind(&mut app, "*.a");
    app.main_view = MainView::CommandOutput;
    ctrl_tab(&mut app);
    assert!(app.overlay.is_none(), "the command's screen keeps it");
    app.main_view = MainView::Panels;
    let mut held = KeyEvent::new(
        KeyCode::BackTab,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    held.kind = ratatui::crossterm::event::KeyEventKind::Repeat;
    app.handle_key(held);
    assert!(app.overlay.is_none(), "the detach key still held");
    assert_eq!(app.background_searches().len(), 1);
    end(app);
}

#[test]
fn the_clock_stands_while_a_question_is_up() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    let target = temp.path().join("dst");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(source.join("a.txt"), "new").unwrap();
    std::fs::write(target.join("a.txt"), "old").unwrap();
    let mut app = app_in(&source);
    select_all_files(&mut app);
    let sources = app.active_panel().targets();
    let target_text = target.display().to_string();
    app.start_transfer(&sources, &answers(&target_text), Operation::Copy);
    let id = app.tasks.iter().next().unwrap().id;
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.tasks.get(id).unwrap().status() != crate::app::TaskStatus::Waiting {
        assert!(Instant::now() < deadline, "no question");
        app.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    app.tick();
    let before = app.tasks.get(id).unwrap().elapsed();
    let deadline = Instant::now() + Duration::from_millis(150);
    while Instant::now() < deadline {
        app.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    let after = app.tasks.get(id).unwrap().elapsed();
    assert!(
        after - before < Duration::from_millis(60),
        "{before:?} {after:?}"
    );
    end(app);
}
