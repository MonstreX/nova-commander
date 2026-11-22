//! Searches in the background: Background in the results dialog, their lines in the status row and the
//! Screens list, Enter bringing the results back, Del, the end of a search and F10; every search stays in
//! the test's own folder.

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::super::tests::{app, find, form, press, results, until};
use super::super::{MASK, TEXT};
use super::*;
use crate::app::dialog::{ConfirmAction, Dialog, ListAction};
use crate::jobs::find::{Found, Search};
use crate::vfs::{Entry, VfsPath};

/// A search that goes on and never ends, one file found, in front.
fn going_on(app: &mut App) {
    let found = Found {
        folder: VfsPath::local("C:\\x"),
        entry: Entry {
            name: "one.txt".to_string(),
            ..Entry::default()
        },
    };
    let search = Search::stalled(vec![found], false);
    let mut results = FindResults::new(search, "Find file: *.txt".into(), Some("hi".into()));
    results.mask = "*.txt".to_string();
    results.pump();
    app.overlay = Some(Overlay::FindResults(Box::new(results)));
}

fn screens(app: &App) -> &[String] {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::List {
            items,
            action: ListAction::Screens(_),
            ..
        })) => items,
        _ => panic!("no Screens list"),
    }
}

#[test]
fn background_sends_the_search_behind_the_panels_and_f12_brings_it_back() {
    let (_temp, mut app) = app();
    going_on(&mut app);
    press(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
    assert!(app.overlay.is_none(), "the panels are back");
    assert_eq!(app.finds_running(), 1);
    assert!(app.status_row_shown());
    assert!(app.tick(), "its block moves");
    press(&mut app, KeyCode::F(12), KeyModifiers::NONE);
    let line = &screens(&app)[1];
    assert!(line.starts_with("1  Find 1 "), "{line:?}");
    assert!(line.contains("*.txt \"hi\""), "{line:?}");
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(
        app.background_searches().len(),
        1,
        "Del leaves one that goes on"
    );
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.background_searches().is_empty());
    assert_eq!(results(&app).lines.len(), 2, "what it found is all there");
    assert!(!results(&app).finalized);
    assert!(!app.status_row_shown());
}

#[test]
fn the_key_of_a_command_to_the_background_sends_a_search_as_well() {
    let (_temp, mut app) = app();
    going_on(&mut app);
    press(
        &mut app,
        KeyCode::BackTab,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert!(app.overlay.is_none());
    assert_eq!(app.background_searches().len(), 1);
}

#[test]
fn a_search_over_in_the_background_says_so_and_stays_until_it_is_seen() {
    let (_temp, mut app) = app();
    for _ in 0..2 {
        press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
        form(&mut app).set_edit_text(MASK, "*.txt");
        form(&mut app).set_edit_text(TEXT, "hello");
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        // No tick yet: the search has not been seen to end.
        press(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
    }
    app.take_task_ended();
    until(&mut app, |app| app.finds_running() == 0);
    assert!(app.take_task_ended(), "the taskbar button flashes");
    assert_eq!(search_label(&app.background_searches()[0]), "Find 1 done");
    assert!(app.status_row_shown(), "it stays until it is seen");
    press(&mut app, KeyCode::F(12), KeyModifiers::NONE);
    assert!(screens(&app)[2].contains("Find 1 done"));
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(
        app.background_searches().len(),
        1,
        "Del takes one that is over"
    );
    // The cursor stays on the line, now the other search's.
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(results(&app).finalized);
    assert!(
        app.background_searches().is_empty(),
        "seen, it leaves the list"
    );
    assert!(!app.status_row_shown());
}

#[test]
fn a_search_that_is_over_in_front_stays_there() {
    let (_temp, mut app) = app();
    find(&mut app, "*.txt", "hello");
    press(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
    assert!(results(&app).finalized, "the dialog stays");
    assert!(app.background_searches().is_empty());
}

#[test]
fn f10_says_the_searches_in_the_background_will_stop() {
    let (_temp, mut app) = app();
    app.prefs.confirm_exit = false;
    going_on(&mut app);
    press(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
    press(&mut app, KeyCode::F(10), KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Confirm {
        lines,
        action: ConfirmAction::Quit,
        ..
    })) = &app.overlay
    else {
        panic!("no question, though a search goes on");
    };
    assert_eq!(lines[1], "1 search(es) in the background will be stopped.");
}
