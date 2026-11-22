use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::console_input::{Mouse, MouseAction};
use crate::app::fileops::tests::app_in;
use crate::app::{App, Cells, Dialog, DialogArea, Overlay};
use crate::platform::input::MouseButtons;

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.show_dialog(Dialog::message("Note", vec!["moved".to_string()]));
    (temp, app)
}

fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn record(
    app: &mut App,
    (column, row): (u16, u16),
    (left, right): (bool, bool),
    action: MouseAction,
) {
    let buttons = MouseButtons {
        left,
        right,
        middle: false,
    };
    app.handle_mouse(Mouse {
        column,
        row,
        buttons,
        pressed: buttons,
        action,
        modifiers: KeyModifiers::NONE,
    });
}

/// The dialog's box as the renderer reports it, without items.
fn drawn(app: &mut App) {
    app.mouse.dialog = Some(DialogArea {
        frame: Cells {
            x: 20,
            y: 8,
            width: 40,
            height: 8,
        },
        controls: Vec::new(),
    });
}

#[test]
fn ctrl_f5_moves_the_dialog_by_the_keys_esc_puts_it_back_and_enter_leaves_it() {
    let (_temp, mut app) = app();
    key(&mut app, KeyCode::F(5), KeyModifiers::CONTROL);
    assert_eq!(app.dialog_place.keys, Some((0, 0)));
    key(&mut app, KeyCode::Left, KeyModifiers::NONE);
    key(&mut app, KeyCode::Right, KeyModifiers::CONTROL);
    key(&mut app, KeyCode::Up, KeyModifiers::CONTROL);
    key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(app.dialog_place.offset, (9, -5));
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.dialog_place.offset, (0, 0), "Esc: back where it was");
    assert!(app.overlay.is_some(), "and the dialog stays");
    key(&mut app, KeyCode::F(5), KeyModifiers::CONTROL);
    key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    key(&mut app, KeyCode::End, KeyModifiers::NONE);
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.dialog_place.keys.is_none() && app.overlay.is_some());
    assert_eq!(app.dialog_place.offset.1, 1);
    assert!(
        app.dialog_place.offset.0 > 1000,
        "to the edge: the drawing stops it there"
    );
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.overlay.is_none(), "now Enter is the dialog's");
    app.show_dialog(Dialog::message("Next", vec![String::new()]));
    assert_eq!(
        app.dialog_place.offset,
        (0, 0),
        "the next one in the middle"
    );
}

#[test]
fn the_left_button_drags_the_dialog_by_a_place_with_no_item_and_the_right_one_puts_it_back() {
    let (_temp, mut app) = app();
    drawn(&mut app);
    record(&mut app, (25, 9), (true, false), MouseAction::Button);
    record(&mut app, (30, 7), (true, false), MouseAction::Moved);
    assert_eq!(app.dialog_place.offset, (5, -2));
    record(&mut app, (31, 7), (false, false), MouseAction::Button);
    assert_eq!(app.dialog_place.offset, (5, -2), "let go: it stays");
    assert!(matches!(app.overlay, Some(Overlay::Dialog(_))));
    drawn(&mut app);
    record(&mut app, (25, 9), (true, false), MouseAction::Button);
    record(&mut app, (40, 12), (true, false), MouseAction::Moved);
    record(&mut app, (40, 12), (true, true), MouseAction::Button);
    assert_eq!(app.dialog_place.offset, (5, -2), "the right button: back");
}
