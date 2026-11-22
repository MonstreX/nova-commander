//! The mouse on the top line, the key bar and the command line (`mouse_bars`).

use ratatui::crossterm::event::KeyModifiers;

use super::console_input::{Mouse, MouseAction};
use super::{App, CommandLineArea, KeyBarArea, Overlay, Side};
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
/// The key bar's row and the command line's on the 100 x 30 screen of the tests.
const KEYS_ROW: u16 = 29;
const LINE_ROW: u16 = 28;
/// Where the command line's text begins.
const TEXT_X: u16 = 20;

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    app.mouse.key_bar = Some(KeyBarArea {
        x: 0,
        row: KEYS_ROW,
        width: 100,
        ends: (1..=12).map(|key| key * 8).collect(),
    });
    app.mouse.command_line = Some(CommandLineArea {
        x: TEXT_X,
        row: LINE_ROW,
        width: 80,
        first: 0,
    });
    (temp, app)
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

fn press(app: &mut App, column: u16, row: u16, buttons: MouseButtons) -> bool {
    app.handle_mouse(mouse(column, row, buttons, MouseAction::Button))
}

fn release(app: &mut App, column: u16, row: u16) -> bool {
    app.handle_mouse(mouse(column, row, NONE, MouseAction::Button))
}

#[test]
fn a_key_of_the_bar_is_pressed_when_the_button_is_let_go_over_it() {
    let (_temp, mut app) = app();
    // F7: make folder.
    press(&mut app, 6 * 8 + 2, KEYS_ROW, LEFT);
    assert!(app.overlay.is_none(), "not on the way down");
    assert!(release(&mut app, 6 * 8 + 5, KEYS_ROW));
    assert!(matches!(app.overlay, Some(Overlay::Dialog(_))));
}

#[test]
fn a_key_of_the_bar_let_go_elsewhere_is_not_pressed() {
    let (_temp, mut app) = app();
    press(&mut app, 6 * 8 + 2, KEYS_ROW, LEFT);
    // The moves meanwhile are the key's, over the panels too.
    app.handle_mouse(mouse(10, 10, LEFT, MouseAction::Moved));
    assert!(!release(&mut app, 7 * 8 + 2, KEYS_ROW));
    assert!(app.overlay.is_none());
    assert_eq!(app.active_panel().cursor, 0);
}

#[test]
fn the_right_button_on_the_bar_adds_alt() {
    let (_temp, mut app) = app();
    // The modifiers held with the press are the key's: Ctrl+F1 hides the left panel.
    let mut down = mouse(2, KEYS_ROW, LEFT, MouseAction::Button);
    down.modifiers = KeyModifiers::CONTROL;
    app.handle_mouse(down);
    release(&mut app, 2, KEYS_ROW);
    assert!(!app.panels[0].visible, "Ctrl+F1");
    // The right button on F2 is Alt+F2: the right drive menu.
    press(&mut app, 8 + 2, KEYS_ROW, RIGHT);
    release(&mut app, 8 + 2, KEYS_ROW);
    assert!(matches!(app.overlay, Some(Overlay::DriveMenu(_))));
}

#[test]
fn the_top_lines_corner_is_ctrl_o_and_the_rest_opens_the_menu_under_the_mouse() {
    let (_temp, mut app) = app();
    assert!(press(&mut app, 0, 0, LEFT));
    assert!(app.panels_hidden());
    press(&mut app, 0, 0, LEFT);
    assert!(!app.panels_hidden());
    let positions = super::title_positions();
    press(&mut app, positions[1], 0, LEFT);
    assert!(matches!(app.overlay, Some(Overlay::Menu(state)) if state.menu == 1));
    app.overlay = None;
    press(&mut app, positions[1] - 1, 0, RIGHT);
    assert!(matches!(app.overlay, Some(Overlay::Menu(state)) if state.menu == 0));
    app.overlay = None;
    // A move along the top with a button held is the panel's (it scrolls up there).
    app.handle_mouse(mouse(30, 0, LEFT, MouseAction::Moved));
    assert!(app.overlay.is_none());
    assert_eq!(app.active, Side::Left);
}

#[test]
fn a_click_on_the_command_line_puts_the_cursor_there() {
    let (_temp, mut app) = app();
    app.command_line.set("copy abc.txt def");
    press(&mut app, TEXT_X + 6, LINE_ROW, LEFT);
    release(&mut app, TEXT_X + 6, LINE_ROW);
    assert_eq!(app.command_line.cursor(), 6);
    press(&mut app, TEXT_X + 70, LINE_ROW, RIGHT);
    assert_eq!(app.command_line.cursor(), 16, "past the end: at the end");
}

#[test]
fn a_double_click_selects_the_word_and_a_click_after_it_the_line() {
    let (_temp, mut app) = app();
    app.command_line.set("copy abc.txt def");
    let at = TEXT_X + 6;
    press(&mut app, at, LINE_ROW, LEFT);
    release(&mut app, at, LINE_ROW);
    app.handle_mouse(mouse(at, LINE_ROW, LEFT, MouseAction::DoubleClick));
    assert_eq!(app.command_line.selection(), Some(5..8));
    release(&mut app, at, LINE_ROW);
    press(&mut app, at, LINE_ROW, LEFT);
    assert_eq!(app.command_line.selection(), Some(0..16));
    release(&mut app, at, LINE_ROW);
    press(&mut app, at, LINE_ROW, LEFT);
    assert_eq!(
        app.command_line.selection(),
        None,
        "the next click is a click"
    );
}
