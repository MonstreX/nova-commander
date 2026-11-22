//! The windows (ADR-0010 step 3): viewers and editors behind the window in front, the Screens list
//! and Ctrl+Tab going to them, closing bringing back the window in front before, modal windows,
//! quitting asking about unsaved changes (FAR's `ExitAll`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Overlay;
use crate::app::dialog::{ConfirmAction, Dialog};

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn ctrl_tab(app: &mut App) {
    press(app, KeyCode::Tab, KeyModifiers::CONTROL);
}

/// Ticks until the editor in front has read its file and no work waits.
fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.tick();
        let ready = match &mut app.screen {
            Screen::Editor(editor) => {
                editor.text.room.set((40, 5));
                editor.text.place().is_some()
            }
            _ => true,
        };
        if ready && !app.is_waiting() {
            return;
        }
        assert!(Instant::now() < deadline, "never done");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// An app in a folder with the files `a.txt` and `b.txt`.
fn two_files() -> (tempfile::TempDir, App, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let (a, b) = (temp.path().join("a.txt"), temp.path().join("b.txt"));
    std::fs::write(&a, "first\r\n").unwrap();
    std::fs::write(&b, "second\r\n").unwrap();
    let app = crate::app::fileops::tests::app_in(temp.path());
    (temp, app, a, b)
}

/// What is in front: "Panels", "View <name>" or "Edit <name>".
fn front(app: &App) -> String {
    let name = |path: &PathBuf| path.file_name().unwrap().to_string_lossy().into_owned();
    match &app.screen {
        Screen::Panels => "Panels".to_string(),
        Screen::Viewer(viewer) => format!("View {}", name(&viewer.path)),
        Screen::Editor(editor) => format!("Edit {}", name(&editor.path)),
    }
}

/// The Screens list's lines and its cursor.
fn screens_list(app: &App) -> (Vec<String>, usize) {
    let Some(Overlay::Dialog(Dialog::List { items, cursor, .. })) = &app.overlay else {
        panic!("no list");
    };
    (
        items
            .iter()
            .map(|item| item.trim_end().to_string())
            .collect(),
        *cursor,
    )
}

#[test]
fn f12_in_the_viewer_lists_the_windows_and_enter_on_the_panels_puts_it_behind() {
    let (_temp, mut app, a, b) = two_files();
    app.view_file(&a, false);
    app.edit_file(&b, false);
    settle(&mut app);
    press(&mut app, KeyCode::F(12), KeyModifiers::NONE);
    let (lines, cursor) = screens_list(&app);
    assert_eq!(lines[0], "0  Panels");
    assert!(lines[1].starts_with("1  View"), "{lines:?}");
    assert!(
        lines[1].ends_with("a.txt") && !lines[1].contains('*'),
        "not modified, the path's middle cut: {lines:?} ({})",
        a.display()
    );
    assert!(lines[2].starts_with("2  Edit"), "{lines:?}");
    assert_eq!(cursor, 2, "on the window in front");
    press(&mut app, KeyCode::Char('0'), KeyModifiers::NONE);
    assert_eq!(front(&app), "Panels");
    assert!(app.overlay.is_none());
    press(&mut app, KeyCode::F(12), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('1'), KeyModifiers::NONE);
    assert_eq!(front(&app), "View a.txt");
}

#[test]
fn ctrl_tab_goes_through_the_windows_in_the_order_they_were_opened() {
    let (_temp, mut app, a, b) = two_files();
    app.view_file(&a, false);
    app.edit_file(&b, false);
    settle(&mut app);
    ctrl_tab(&mut app);
    assert_eq!(front(&app), "Panels", "after the last, the first");
    ctrl_tab(&mut app);
    assert_eq!(front(&app), "View a.txt");
    press(
        &mut app,
        KeyCode::BackTab,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(front(&app), "Panels");
}

#[test]
fn closing_a_window_brings_back_the_one_in_front_before_it() {
    let (_temp, mut app, a, b) = two_files();
    app.view_file(&a, false);
    ctrl_tab(&mut app);
    ctrl_tab(&mut app);
    assert_eq!(front(&app), "View a.txt");
    app.edit_file(&b, false);
    settle(&mut app);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(front(&app), "View a.txt", "not the panels");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(front(&app), "Panels");
    assert!(app.window_lines().is_empty());
}

#[test]
fn a_viewer_opened_from_find_files_results_is_modal() {
    let (_temp, mut app, a, _) = two_files();
    app.view_file(&a, false);
    assert!(app.front_can_lose_focus());
    let search = crate::jobs::find::Search::stalled(Vec::new(), false);
    let results = crate::app::find::results::FindResults::new(search, "Find".to_string(), None);
    app.find.results = Some(Box::new(results));
    assert!(!app.front_can_lose_focus());
    press(&mut app, KeyCode::F(12), KeyModifiers::NONE);
    assert!(app.overlay.is_none(), "F12 does nothing");
    ctrl_tab(&mut app);
    assert_eq!(front(&app), "View a.txt", "Ctrl+Tab does nothing");
}

#[test]
fn quitting_asks_about_an_editor_behind_and_cancel_keeps_the_program() {
    let (_temp, mut app, a, b) = two_files();
    app.edit_file(&a, false);
    settle(&mut app);
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    settle(&mut app);
    app.view_file(&b, false);
    ctrl_tab(&mut app);
    assert_eq!(front(&app), "Panels");
    app.finish_quit();
    assert_eq!(
        front(&app),
        "Edit a.txt",
        "the editor with changes in front"
    );
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Confirm {
            action: ConfirmAction::CloseEditor,
            ..
        }))
    ));
    press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
    settle(&mut app);
    assert!(!app.quit, "Cancel: the program stays");
    assert_eq!(front(&app), "Edit a.txt");
    press(&mut app, KeyCode::F(12), KeyModifiers::NONE);
    let (lines, cursor) = screens_list(&app);
    assert!(
        lines[1].starts_with("1  Edit") && lines[1].contains(" * "),
        "{lines:?}"
    );
    assert!(
        lines[2].starts_with("2  View") && !lines[2].contains('*'),
        "{lines:?}"
    );
    assert_eq!(cursor, 1);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.finish_quit();
    press(&mut app, KeyCode::Char('n'), KeyModifiers::NONE);
    settle(&mut app);
    assert!(app.quit, "No: the changes dropped, the rest closed");
    assert!(app.window_lines().is_empty());
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "first\r\n");
}

/// The buttons of the question in front.
fn question(app: &App) -> Vec<&'static str> {
    let Some(Overlay::Dialog(Dialog::Confirm { buttons, lines, .. })) = &app.overlay else {
        panic!("no question");
    };
    assert_eq!(lines[1], "already loaded. How to open this file?");
    buttons.clone()
}

/// FAR's `FileEditor::Init` with "Reload edited file": Current brings the editor, New instance opens
/// another, Reload opens it again in place of that one.
#[test]
fn a_file_edited_again_asks_how_to_open_it() {
    let (_temp, mut app, a, _) = two_files();
    app.edit_file(&a, false);
    settle(&mut app);
    ctrl_tab(&mut app);
    assert_eq!(front(&app), "Panels");
    app.edit_file(&a, false);
    assert_eq!(
        question(&app),
        ["&Current", "&New instance", "R&eload", "&Cancel"]
    );
    press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
    assert_eq!(front(&app), "Edit a.txt");
    assert_eq!(app.window_lines().len(), 1, "Current: the same window");
    ctrl_tab(&mut app);
    app.edit_file(&a, false);
    press(&mut app, KeyCode::Char('n'), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(app.window_lines().len(), 2, "New instance: another window");
    ctrl_tab(&mut app);
    assert_eq!(front(&app), "Panels");
    app.edit_file(&a, false);
    press(&mut app, KeyCode::Char('e'), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(front(&app), "Edit a.txt");
    assert_eq!(
        app.window_lines().len(),
        2,
        "Reload: one in place of another"
    );
}

#[test]
fn without_reload_edited_file_the_editor_comes_unasked_unless_it_has_changes() {
    let (_temp, mut app, a, _) = two_files();
    app.prefs.confirm_reedit = false;
    app.edit_file(&a, false);
    settle(&mut app);
    ctrl_tab(&mut app);
    app.edit_file(&a, false);
    assert!(app.overlay.is_none());
    assert_eq!(front(&app), "Edit a.txt");
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    settle(&mut app);
    ctrl_tab(&mut app);
    app.edit_file(&a, false);
    assert_eq!(question(&app).len(), 4, "changes unsaved: asked");
    // From Find file's results the new editor is modal: no Current, no Reload.
    app.overlay = None;
    let search = crate::jobs::find::Search::stalled(Vec::new(), false);
    let results = crate::app::find::results::FindResults::new(search, "Find".to_string(), None);
    app.find.results = Some(Box::new(results));
    app.edit_file(&a, false);
    assert_eq!(question(&app), ["&New instance", "&Cancel"]);
}

/// F6 in the viewer: the editor takes its place once it is open; a question cancelled keeps it
/// (review C6).
#[test]
fn f6_in_the_viewer_closes_it_only_once_the_editor_is_open() {
    let (_temp, mut app, a, b) = two_files();
    app.view_file(&a, false);
    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(front(&app), "Edit a.txt");
    assert_eq!(app.window_lines().len(), 1, "the viewer gone");
    app.view_file(&b, false);
    app.edit_file(&b, false);
    settle(&mut app);
    let back = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    press(&mut app, KeyCode::BackTab, back);
    assert_eq!(front(&app), "View b.txt");
    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    assert_eq!(question(&app).len(), 4, "b.txt already loaded");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(front(&app), "View b.txt", "cancelled: the viewer stays");
    assert_eq!(app.window_lines().len(), 3);
}

/// Owner 2026-11-21: a file opened shows coloured at once - the screen waits for its text and its
/// colours rather than show it plain first; a file of no type is not waited for.
#[test]
fn an_editor_just_opened_is_shown_once_it_is_coloured() {
    let temp = tempfile::tempdir().unwrap();
    let code = temp.path().join("main.rs");
    std::fs::write(&code, "fn main() {}\r\n").unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&code, false);
    assert!(app.holds_paint(), "no text, no colours yet");
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.holds_paint() {
        app.tick();
        if let Screen::Editor(editor) = &mut app.screen {
            editor.text.room.set((40, 5));
        }
        assert!(Instant::now() < deadline, "never shown");
        std::thread::sleep(Duration::from_millis(2));
    }
    let Screen::Editor(editor) = &app.screen else {
        panic!("no editor");
    };
    assert!(
        editor.text.syntax().is_on(),
        "shown with its colours, not before (the hold is long enough for a type read once)"
    );
}

/// The viewer's first row once it is read and laid out 80 by 5.
fn viewer_top(app: &mut App) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.tick();
        if let Screen::Viewer(viewer) = &app.screen {
            viewer.text().room.set((80, 5));
            if let Some(screen) = viewer.text().screen(80, 5)
                && let Some(first) = screen.rows.first().filter(|row| !row.is_empty())
            {
                return first.trim_end().to_string();
            }
        }
        assert!(Instant::now() < deadline, "the viewer never showed");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Owner 2026-11-21 (FAR's `FilePos` / `GetCurPos`): F6 keeps the place - the editor from the viewer's
/// first row, the cursor on it, and the viewer from the editor's first row.
#[test]
fn f6_keeps_the_place_both_ways() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lines.txt");
    let text: String = (1..=40).map(|n| format!("line {n}\r\n")).collect();
    std::fs::write(&path, text).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.view_file(&path, false);
    assert_eq!(viewer_top(&mut app), "line 1");
    for _ in 0..10 {
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(viewer_top(&mut app), "line 11");
    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    settle(&mut app);
    let Screen::Editor(editor) = &app.screen else {
        panic!("no editor");
    };
    assert_eq!(
        editor.text.place().map(|place| place.0),
        Some(11),
        "on the viewer's row"
    );
    press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
    settle(&mut app);
    let Screen::Editor(editor) = &app.screen else {
        panic!("no editor");
    };
    let top = editor.text.position().unwrap().top;
    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    let shown = viewer_top(&mut app);
    let row = text_line_at(&path, top);
    assert_eq!(shown, row, "the viewer from the editor's first row");
    assert_ne!(shown, "line 11", "the editor's page moved on");
}

/// Review C2 (owner: never a mixture): two editors of one file, each saving - the second one's save
/// takes the bytes it did not edit from the file it was read from, not from what the first one saved.
#[test]
fn two_editors_of_a_file_never_save_a_mixture() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("big.txt");
    let original: String = (1..=30_000).map(|n| format!("line {n:05}\r\n")).collect();
    std::fs::write(&path, &original).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    press(&mut app, KeyCode::Char('A'), KeyModifiers::NONE);
    settle(&mut app);
    ctrl_tab(&mut app);
    app.edit_file(&path, false);
    press(&mut app, KeyCode::Char('n'), KeyModifiers::NONE);
    settle(&mut app);
    press(&mut app, KeyCode::Char('B'), KeyModifiers::NONE);
    settle(&mut app);
    let ids: Vec<u64> = app.window_lines().iter().map(|line| line.id).collect();
    assert_eq!(ids.len(), 2, "two editors");
    // The first one saves: the file now has "A" before every byte the second one has not read.
    app.bring_window(ids[0]);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("A{original}")
    );
    app.bring_window(ids[1]);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    // FAR's "changed by an external program": Yes.
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    settle(&mut app);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        saved == format!("B{original}"),
        "a mixture saved: {}",
        &saved[..60]
    );
}

/// The line of `path` holding byte `at`.
fn text_line_at(path: &std::path::Path, at: u64) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    let start = text[..at as usize].rfind('\n').map_or(0, |at| at + 1);
    text[start..].lines().next().unwrap().to_string()
}

/// Review C7: the key after Ctrl+O only ends the user screen - F12 there opens no list.
#[test]
fn f12_after_ctrl_o_only_ends_the_user_screen() {
    let (_temp, mut app, a, _) = two_files();
    app.view_file(&a, false);
    press(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert!(app.backdrop);
    press(&mut app, KeyCode::F(12), KeyModifiers::NONE);
    assert!(!app.backdrop, "ended");
    assert!(app.overlay.is_none(), "no Screens list");
}

/// Review C9, C10: the user menu's editor is modal for the "already loaded" question; an editor with
/// an action asked ahead does not go behind until it is done.
#[test]
fn the_user_menus_editor_asks_as_a_modal_one_and_a_pending_action_holds_the_editor() {
    let (_temp, mut app, a, _) = two_files();
    app.edit_file(&a, false);
    settle(&mut app);
    if let Screen::Editor(editor) = &mut app.screen {
        editor.pending = Some(crate::app::editing::EditorAction::Save);
    }
    assert!(!app.front_can_lose_focus(), "its save first");
    if let Screen::Editor(editor) = &mut app.screen {
        editor.pending = None;
    }
    ctrl_tab(&mut app);
    app.user_menu.reread = Some(crate::app::user_menu::Look::Main);
    app.edit_file(&a, false);
    assert_eq!(question(&app), ["&New instance", "&Cancel"]);
}

/// Ticks until the editor in front is highlighted.
fn coloured(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        app.tick();
        if let Screen::Editor(editor) = &mut app.screen {
            editor.text.room.set((40, 5));
            if editor.text.syntax().is_on() {
                return;
            }
        }
        assert!(Instant::now() < deadline, "never coloured");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Review B13: "Reload schema library" has every editor choose its type again, not the one in front
/// alone.
#[test]
fn reloading_the_schemes_reaches_every_editor() {
    let temp = tempfile::tempdir().unwrap();
    let (one, two) = (temp.path().join("one.rs"), temp.path().join("two.rs"));
    std::fs::write(&one, "fn one() {}\r\n").unwrap();
    std::fs::write(&two, "fn two() {}\r\n").unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&one, false);
    coloured(&mut app);
    app.edit_file(&two, false);
    coloured(&mut app);
    app.colorer_action(crate::app::colorer_menu::Action::Reload);
    let behind = app.window_lines()[0].id;
    app.bring_window(behind);
    let Screen::Editor(editor) = &app.screen else {
        panic!("no editor");
    };
    assert!(
        !editor.text.syntax().is_on(),
        "the one behind chooses again too"
    );
    coloured(&mut app);
}

#[test]
fn alt_f11_in_the_viewer_opens_the_view_and_edit_history() {
    let (_temp, mut app, a, _) = two_files();
    app.view_file(&a, false);
    press(&mut app, KeyCode::F(11), KeyModifiers::ALT);
    assert!(matches!(app.overlay, Some(Overlay::History(_))));
}

/// The windows' numbers and kinds.
fn windows_of(app: &App) -> Vec<(u64, &'static str)> {
    (app.window_lines().iter())
        .map(|line| (line.id, line.kind))
        .collect()
}

/// Review C11: Reload replaces the editor the question was about, not another window.
#[test]
fn reload_replaces_the_editor_it_asked_about() {
    let (_temp, mut app, a, b) = two_files();
    app.edit_file(&a, false);
    settle(&mut app);
    app.view_file(&b, false);
    ctrl_tab(&mut app);
    assert_eq!(windows_of(&app), [(1, "Edit"), (2, "View")]);
    app.edit_file(&a, false);
    press(&mut app, KeyCode::Char('e'), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(front(&app), "Edit a.txt");
    assert_eq!(windows_of(&app), [(2, "View"), (3, "Edit")]);
}

/// The viewer in front in hex with its first byte typed over as `A`, not saved.
fn bytes_typed(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let shown = |app: &mut App, done: &dyn Fn(&crate::editor::view::Screen) -> bool| loop {
        app.tick();
        let Screen::Viewer(viewer) = &app.screen else {
            panic!("no viewer");
        };
        let text = viewer.text();
        text.room.set((80, 5));
        if text.screen(80, 5).is_some_and(|screen| done(&screen)) {
            return;
        }
        assert!(Instant::now() < deadline, "not shown");
        std::thread::sleep(Duration::from_millis(2));
    };
    shown(app, &|screen| !screen.rows.is_empty());
    press(app, KeyCode::F(4), KeyModifiers::NONE);
    shown(app, &|screen| screen.rows[0].starts_with("0000000000: "));
    for code in [KeyCode::F(2), KeyCode::Char('4'), KeyCode::Char('1')] {
        press(app, code, KeyModifiers::NONE);
    }
    shown(app, &|screen| screen.rows[0].contains("41 65"));
}

/// Review C11: quitting with an editor's changes and a viewer's bytes, Yes to each saves it and goes
/// on to the next, then the program ends.
#[test]
fn quitting_saves_on_yes_and_goes_on_to_the_viewers_bytes() {
    let (_temp, mut app, a, b) = two_files();
    app.view_file(&b, false);
    bytes_typed(&mut app);
    app.edit_file(&a, false);
    settle(&mut app);
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    settle(&mut app);
    ctrl_tab(&mut app);
    assert_eq!(front(&app), "Panels");
    app.finish_quit();
    assert_eq!(front(&app), "Edit a.txt", "the one in front last first");
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "xfirst\r\n");
    assert!(!app.quit, "the viewer's bytes asked about first");
    assert_eq!(front(&app), "View b.txt");
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Confirm {
            action: ConfirmAction::ViewerBytes,
            ..
        }))
    ));
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "Aecond\r\n");
    assert!(app.quit, "all saved: the program ends");
    assert!(app.window_lines().is_empty());
}

#[path = "windows_tests_quit.rs"]
mod quit;
