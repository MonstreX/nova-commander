//! The mouse on the tree (FAR's `TreeList::ProcessMouse`). The panels of the tests from row 1 (row 0 is
//! the screen's top line), 28 rows high: the tree's frame on row 1, the root on row 2, then `a`, `a/b`,
//! `a/c`, `d`; the separator on row 26.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::super::tests::{app, ctrl_t, current, on_tree, press, settle};
use crate::app::console_input::{Mouse, MouseAction};
use crate::app::mouse_drag_tests::asked;
use crate::app::{App, ListArea, PanelArea, Side};
use crate::platform::input::MouseButtons;

const LEFT: MouseButtons = MouseButtons {
    left: true,
    right: false,
    middle: false,
};
const RIGHT: MouseButtons = MouseButtons {
    left: false,
    right: true,
    middle: false,
};
const NONE: MouseButtons = MouseButtons {
    left: false,
    right: false,
    middle: false,
};

/// Both panels drawn side by side, the tree's list 24 rows.
fn draw(app: &mut App) {
    for (index, x) in [0u16, 50].into_iter().enumerate() {
        app.mouse.areas[index] = Some(PanelArea {
            x,
            y: 1,
            width: 50,
            height: 28,
            list: Some(ListArea {
                top: 3,
                rows: 23,
                stripe_lefts: vec![x + 1, x + 25],
            }),
        });
    }
    app.tree_panel.as_mut().unwrap().set_rows(24);
}

fn tree_side(app: &App) -> Side {
    app.tree_side().unwrap()
}

fn column(side: Side) -> u16 {
    if side == Side::Left { 5 } else { 55 }
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

fn on_tree_row(app: &mut App, row: u16, buttons: MouseButtons, action: MouseAction) {
    let column = column(tree_side(app));
    app.handle_mouse(mouse(column, row, buttons, action));
}

/// The folder of the panel beside the tree.
fn other_folder(app: &App) -> PathBuf {
    let other = tree_side(app).other().index();
    app.panels[other].path.as_local().unwrap().to_path_buf()
}

/// The tree passive, drawn.
fn shown_tree() -> (tempfile::TempDir, App) {
    let (temp, mut app) = app();
    ctrl_t(&mut app);
    settle(&mut app);
    draw(&mut app);
    (temp, app)
}

#[test]
fn a_click_takes_the_keys_and_the_cursor_there_the_other_panel_staying() {
    let (temp, mut app) = shown_tree();
    assert!(!app.tree_is_active());
    on_tree_row(&mut app, 6, LEFT, MouseAction::Button);
    assert!(app.tree_is_active());
    assert_eq!(current(&app), "d");
    assert_eq!(other_folder(&app), temp.path().join("a"));
}

#[test]
fn a_double_click_or_the_right_button_takes_the_other_panel_there() {
    let (temp, mut app) = shown_tree();
    on_tree_row(&mut app, 6, LEFT, MouseAction::DoubleClick);
    assert_eq!(other_folder(&app), temp.path().join("d"));
    on_tree_row(&mut app, 4, RIGHT, MouseAction::Button);
    assert_eq!(current(&app), "a/b");
    assert_eq!(other_folder(&app), temp.path().join("a").join("b"));
}

#[test]
fn with_auto_change_folder_a_click_elsewhere_is_enter() {
    let (temp, mut app) = shown_tree();
    app.prefs.tree.auto_change_folder = true;
    on_tree_row(&mut app, 5, LEFT, MouseAction::Button);
    assert_eq!(other_folder(&app), temp.path().join("a").join("c"));
}

#[test]
fn the_wheel_scrolls_the_tree_with_the_keys() {
    let (_temp, mut app) = app();
    on_tree(&mut app);
    draw(&mut app);
    assert_eq!(current(&app), "a");
    let mut down = mouse(5, 5, NONE, MouseAction::Wheel(-1));
    down.modifiers = KeyModifiers::ALT;
    assert!(app.handle_mouse(down));
    assert_eq!(current(&app), "a/b", "one line with Alt");
}

#[test]
fn a_button_held_over_the_frame_or_the_separator_moves_up_or_down() {
    let (_temp, mut app) = app();
    on_tree(&mut app);
    draw(&mut app);
    on_tree_row(&mut app, 1, LEFT, MouseAction::Button);
    assert_eq!(current(&app), "", "up from a");
    on_tree_row(&mut app, 1, NONE, MouseAction::Button);
    on_tree_row(&mut app, 27, LEFT, MouseAction::Button);
    assert_eq!(current(&app), "a");
    on_tree_row(&mut app, 27, NONE, MouseAction::Button);
    assert!(app.mouse.hold.is_none());
}

#[test]
fn a_folder_dragged_to_the_other_panel_asks_to_copy_it() {
    let (temp, mut app) = shown_tree();
    let tree = column(tree_side(&app));
    let other = column(tree_side(&app).other());
    on_tree_row(&mut app, 6, LEFT, MouseAction::Button);
    app.handle_mouse(mouse(other, 10, LEFT, MouseAction::Moved));
    assert!(app.mouse.drag_text.is_some(), "the note follows the mouse");
    app.handle_mouse(mouse(other, 10, NONE, MouseAction::Button));
    let d = temp.path().join("d").display().to_string();
    assert_eq!(asked(&app).1, [d]);
    assert_eq!(column(tree_side(&app)), tree, "dropped on the other panel");
}

#[test]
fn a_drag_that_went_over_other_lines_first_takes_the_folder_it_began_on() {
    let (temp, mut app) = shown_tree();
    let tree = column(tree_side(&app));
    let other = column(tree_side(&app).other());
    on_tree_row(&mut app, 6, LEFT, MouseAction::Button);
    // Within 15 columns the cursor follows the mouse, to `a`.
    app.handle_mouse(mouse(tree + 2, 3, LEFT, MouseAction::Moved));
    assert_eq!(current(&app), "a");
    app.handle_mouse(mouse(other, 10, LEFT, MouseAction::Moved));
    assert_eq!(current(&app), "d", "back on the folder dragged");
    let note = app.mouse.drag_text.as_ref().unwrap().text.clone();
    assert_eq!(note, "Copy d");
    app.handle_mouse(mouse(other, 10, NONE, MouseAction::Button));
    let d = temp.path().join("d").display().to_string();
    assert_eq!(asked(&app).1, [d]);
}

#[test]
fn with_auto_change_folder_a_press_still_starts_a_drag() {
    let (_temp, mut app) = shown_tree();
    app.prefs.tree.auto_change_folder = true;
    on_tree_row(&mut app, 6, LEFT, MouseAction::Button);
    assert!(app.mouse.drag.is_some());
}

#[test]
fn the_wheel_over_a_passive_tree_scrolls_the_active_files() {
    let (_temp, mut app) = shown_tree();
    assert!(!app.tree_is_active());
    let before = app.active_panel().cursor;
    let mut down = mouse(column(tree_side(&app)), 5, NONE, MouseAction::Wheel(-1));
    down.modifiers = KeyModifiers::ALT;
    app.handle_mouse(down);
    assert_eq!(app.active_panel().cursor, before + 1);
    assert_eq!(current(&app), "a", "the tree's cursor stays");
}

#[test]
fn a_button_brought_onto_the_separator_moves_the_cursor_on() {
    let (_temp, mut app) = app();
    on_tree(&mut app);
    draw(&mut app);
    on_tree_row(&mut app, 3, LEFT, MouseAction::Button);
    assert_eq!(current(&app), "a");
    on_tree_row(&mut app, 27, LEFT, MouseAction::Moved);
    assert_eq!(current(&app), "a/b");
    assert!(app.mouse.hold.is_some());
}

#[test]
fn the_root_is_not_dragged() {
    let (_temp, mut app) = shown_tree();
    on_tree_row(&mut app, 2, LEFT, MouseAction::Button);
    assert_eq!(current(&app), "");
    assert!(app.mouse.drag.is_none());
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
}
