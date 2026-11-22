//! Alt+F10: FAR's "Find folder". The tree's root is each test's own folder (`TEST_ROOT`): `a/b`, `a/c`,
//! `d`, the panels in `a`.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

use super::super::tests::{app, on_tree, press};
use super::FolderTree;
use crate::app::console_input::{Mouse, MouseAction};
use crate::app::{App, Cells, Overlay};
use crate::platform::input::MouseButtons;

fn found(app: &App) -> &FolderTree {
    match &app.overlay {
        Some(Overlay::FolderTree(found)) => found,
        _ => panic!("no Find folder"),
    }
}

fn found_mut(app: &mut App) -> &mut FolderTree {
    match &mut app.overlay {
        Some(Overlay::FolderTree(found)) => found,
        _ => panic!("no Find folder"),
    }
}

/// Alt+F10, the window's worker done and the cursor on the active panel's folder.
fn alt_f10(app: &mut App) {
    press(app, KeyCode::F(10), KeyModifiers::ALT);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.pump_folder_tree();
        let tree = &found(app).tree;
        if !tree.reading && tree.waiting.is_none() {
            break;
        }
        assert!(Instant::now() < deadline, "the tree was not read");
        std::thread::sleep(Duration::from_millis(5));
    }
    // As drawn on a 100 x 40 screen.
    let found = found_mut(app);
    found.area = Some(Cells {
        x: 4,
        y: 2,
        width: 92,
        height: 34,
    });
    found.tree.set_rows(29);
}

fn current(app: &App) -> String {
    let tree = &found(app).tree;
    let item = &tree.items[tree.cursor];
    let rest = item.path.strip_prefix(&tree.root).unwrap();
    rest.to_string_lossy().replace('\\', "/")
}

fn active_folder(app: &App) -> std::path::PathBuf {
    app.active_panel().path.as_local().unwrap().to_path_buf()
}

fn typed(app: &mut App, text: &str) {
    for ch in text.chars() {
        press(app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
}

#[test]
fn alt_f10_shows_the_disk_s_tree_on_the_active_panel_s_folder() {
    let (_temp, mut app) = app();
    alt_f10(&mut app);
    assert_eq!(current(&app), "a");
    assert_eq!(found(&app).search.text(), "");
}

#[test]
fn typing_finds_a_folder_by_its_name_and_keeps_only_what_finds_one() {
    let (_temp, mut app) = app();
    alt_f10(&mut app);
    typed(&mut app, "c");
    assert_eq!(current(&app), "a/c");
    typed(&mut app, "x");
    assert_eq!(found(&app).search.text(), "c", "cx finds nothing");
    press(&mut app, KeyCode::Char('d'), KeyModifiers::ALT);
    assert_eq!(
        found(&app).search.text(),
        "c",
        "Alt+d is d: cd finds nothing either"
    );
    press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('d'), KeyModifiers::ALT);
    assert_eq!(current(&app), "d");
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(found(&app).search.text(), "", "the cursor keys clear it");
    assert_eq!(current(&app), "a/c");
}

#[test]
fn ctrl_enter_finds_the_next_and_ctrl_shift_enter_the_one_before() {
    let (temp, mut app) = app();
    std::fs::create_dir_all(temp.path().join("bb")).unwrap();
    alt_f10(&mut app);
    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    alt_settle(&mut app);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    typed(&mut app, "b");
    assert_eq!(current(&app), "a/b");
    press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
    assert_eq!(current(&app), "bb");
    press(
        &mut app,
        KeyCode::Enter,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(current(&app), "a/b");
}

/// The window's worker done again (after Ctrl+R).
fn alt_settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while found(app).tree.reading {
        assert!(Instant::now() < deadline, "the tree was not read again");
        app.pump_folder_tree();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn enter_takes_the_active_panel_there_and_closes_the_window() {
    let (temp, mut app) = app();
    alt_f10(&mut app);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    assert_eq!(active_folder(&app), temp.path().join("d"));
}

#[test]
fn enter_on_a_folder_gone_takes_it_out_and_the_window_stays() {
    let (temp, mut app) = app();
    alt_f10(&mut app);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    std::fs::remove_dir(temp.path().join("d")).unwrap();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let items = &found(&app).tree.items;
    assert!(items.iter().all(|item| !item.path.ends_with("d")));
    assert_eq!(active_folder(&app), temp.path().join("a"));
}

#[test]
fn esc_and_f10_close_it_and_f5_fills_the_screen() {
    let (temp, mut app) = app();
    alt_f10(&mut app);
    press(&mut app, KeyCode::F(5), KeyModifiers::NONE);
    assert!(found(&app).full_screen);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    assert!(found(&app).tree.reading, "F2 reads the disk again");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    alt_f10(&mut app);
    press(&mut app, KeyCode::F(10), KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    assert_eq!(active_folder(&app), temp.path().join("a"));
}

#[test]
fn from_the_tree_the_folder_found_goes_to_the_panel_beside_it() {
    let (temp, mut app) = app();
    on_tree(&mut app);
    alt_f10(&mut app);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let other = app.tree_side().unwrap().other().index();
    assert_eq!(
        app.panels[other].path.as_local().unwrap(),
        temp.path().join("d")
    );
}

#[test]
fn with_the_tree_turned_off_there_is_no_find_folder() {
    let (_temp, mut app) = app();
    app.prefs.tree.turn_off_completely = true;
    press(&mut app, KeyCode::F(10), KeyModifiers::ALT);
    assert!(app.overlay.is_none());
}

fn mouse(column: u16, row: u16, buttons: MouseButtons, action: MouseAction) -> Mouse {
    Mouse {
        column,
        row,
        buttons,
        pressed: buttons,
        action,
        modifiers: KeyModifiers::NONE,
    }
}

const LEFT: MouseButtons = MouseButtons {
    left: true,
    right: false,
    middle: false,
};
const NONE: MouseButtons = MouseButtons {
    left: false,
    right: false,
    middle: false,
};
const RIGHT: MouseButtons = MouseButtons {
    left: false,
    right: true,
    middle: false,
};

#[test]
fn the_mouse_moves_the_cursor_a_double_click_is_enter_and_a_click_outside_esc() {
    let (temp, mut app) = app();
    alt_f10(&mut app);
    // The root on row 3, then a, a/b, a/c, d.
    app.handle_mouse(mouse(10, 7, LEFT, MouseAction::Button));
    assert_eq!(current(&app), "d");
    app.handle_mouse(mouse(10, 1, LEFT, MouseAction::Button));
    assert!(app.overlay.is_none(), "outside: Esc");
    assert_eq!(active_folder(&app), temp.path().join("a"));
    alt_f10(&mut app);
    app.handle_mouse(mouse(10, 6, LEFT, MouseAction::DoubleClick));
    assert!(app.overlay.is_none());
    assert_eq!(active_folder(&app), temp.path().join("a").join("c"));
    alt_f10(&mut app);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    app.handle_mouse(mouse(1, 1, RIGHT, MouseAction::Button));
    assert_eq!(
        active_folder(&app),
        temp.path(),
        "outside with the right button: Enter"
    );
}

#[test]
fn keys_with_other_modifiers_are_not_the_window_s() {
    let (_temp, mut app) = app();
    alt_f10(&mut app);
    press(&mut app, KeyCode::F(10), KeyModifiers::ALT);
    press(&mut app, KeyCode::Esc, KeyModifiers::SHIFT);
    assert!(
        matches!(app.overlay, Some(Overlay::FolderTree(_))),
        "still up"
    );
    press(&mut app, KeyCode::F(5), KeyModifiers::SHIFT);
    assert!(!found(&app).full_screen, "Shift+F5 is not Zoom");
    press(&mut app, KeyCode::Down, KeyModifiers::SHIFT);
    assert_eq!(current(&app), "a", "Shift+Down does not move");
}

#[test]
fn the_cursor_keys_clear_the_search_and_the_last_text_comes_back_as_far_s() {
    let (_temp, mut app) = app();
    alt_f10(&mut app);
    typed(&mut app, "c");
    assert_eq!(current(&app), "a/c");
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(found(&app).search.text(), "");
    typed(&mut app, "x");
    assert_eq!(found(&app).search.text(), "c", "FAR's m_LastName");
}

#[test]
fn the_wheel_clears_the_search_and_gray_plus_keeps_it() {
    let (_temp, mut app) = app();
    alt_f10(&mut app);
    typed(&mut app, "b");
    assert_eq!(current(&app), "a/b");
    let mut plus = KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE);
    plus.state = KeyEventState::KEYPAD;
    app.handle_key(plus);
    assert_eq!(current(&app), "a/c", "the next of the same depth");
    assert_eq!(found(&app).search.text(), "b");
    let mut wheel = mouse(10, 10, NONE, MouseAction::Wheel(-1));
    wheel.modifiers = KeyModifiers::ALT;
    app.handle_mouse(wheel);
    assert_eq!(current(&app), "d");
    assert_eq!(found(&app).search.text(), "", "the wheel is a cursor key");
    assert!(
        !app.handle_mouse(mouse(10, 10, NONE, MouseAction::Moved)),
        "a move: nothing to draw"
    );
}

#[test]
fn the_key_bar_takes_the_mouse_first() {
    let (temp, mut app) = app();
    alt_f10(&mut app);
    app.mouse.key_bar = Some(crate::app::KeyBarArea {
        x: 0,
        row: 39,
        width: 100,
        ends: (1..=12).map(|key| key * 8).collect(),
    });
    // F10 "Quit", the tenth key: pressed and let go on it.
    app.handle_mouse(mouse(75, 39, LEFT, MouseAction::Button));
    assert!(
        matches!(app.overlay, Some(Overlay::FolderTree(_))),
        "not outside: no Esc"
    );
    app.handle_mouse(mouse(75, 39, NONE, MouseAction::Button));
    assert!(app.overlay.is_none(), "Quit");
    assert_eq!(active_folder(&app), temp.path().join("a"));
}

#[test]
fn zoomed_the_window_covers_the_key_bar_and_its_keys_are_not_pressed() {
    let (_temp, mut app) = app();
    alt_f10(&mut app);
    let found = found_mut(&mut app);
    found.full_screen = true;
    found.area = Some(Cells {
        x: 0,
        y: 0,
        width: 100,
        height: 40,
    });
    app.mouse.key_bar = Some(crate::app::KeyBarArea {
        x: 0,
        row: 39,
        width: 100,
        ends: (1..=12).map(|key| key * 8).collect(),
    });
    app.handle_mouse(mouse(75, 39, LEFT, MouseAction::Button));
    app.handle_mouse(mouse(75, 39, NONE, MouseAction::Button));
    assert!(
        matches!(app.overlay, Some(Overlay::FolderTree(_))),
        "the frame, not F10"
    );
}

#[test]
fn enter_waits_for_a_slow_disk_under_the_tree_and_esc_gives_the_tree_back() {
    // ADR-0019, review R22: the folder's disk asked off the screen's thread, the tree under the wait.
    use crate::app::waiting::{finish_waiting, hold_next_work, shows_stopped};
    let (temp, mut app) = app();
    alt_f10(&mut app);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    assert_eq!(current(&app), "d");
    let release = hold_next_work();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.is_waiting());
    assert!(
        matches!(app.overlay, Some(Overlay::FolderTree(_))),
        "the tree under the wait"
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(shows_stopped(&app));
    drop(release);
    assert!(!app.pump_waiting(), "its late end is not done");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(current(&app), "d", "the tree back as it was");
    // A folder gone: it leaves the tree, which stays.
    std::fs::remove_dir_all(temp.path().join("d")).unwrap();
    let release = hold_next_work();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let _ = release.send(());
    finish_waiting(&mut app);
    assert!(matches!(app.overlay, Some(Overlay::FolderTree(_))));
    assert_ne!(current(&app), "d", "d left the tree");
    // One there: the panel goes to it, the tree closes.
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    let chosen = current(&app);
    let release = hold_next_work();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let _ = release.send(());
    finish_waiting(&mut app);
    assert!(app.overlay.is_none(), "the tree closed");
    assert_eq!(active_folder(&app), temp.path().join(&chosen));
}
