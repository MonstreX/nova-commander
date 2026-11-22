//! The mouse in dialogs (`mouse_dialogs`).

use ratatui::crossterm::event::KeyModifiers;

use super::console_input::{Mouse, MouseAction};
use super::fileops::tests::app_in;
use super::form::{Control, Form, Item, ListLine};
use super::{
    App, Cells, Command, ControlKind, Dialog, DialogArea, DialogControl, FormAction, MenuArea,
    Overlay, Side,
};
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
/// The dialog's box in the tests.
const FRAME: Cells = Cells {
    x: 20,
    y: 8,
    width: 60,
    height: 10,
};

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

fn control(x: u16, y: u16, width: u16, index: usize, kind: ControlKind) -> DialogControl {
    DialogControl {
        cells: Cells {
            x,
            y,
            width,
            height: 1,
        },
        index,
        kind,
    }
}

/// F10's question with its buttons as if drawn: Yes at 40, No at 50 on row 15.
fn quit_question() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.execute(Command::Quit);
    app.mouse.dialog = Some(DialogArea {
        frame: FRAME,
        controls: vec![
            control(40, 15, 7, 0, ControlKind::Button),
            control(50, 15, 6, 1, ControlKind::Button),
        ],
    });
    (temp, app)
}

fn focused(app: &App) -> usize {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Confirm { focused, .. })) => *focused,
        _ => panic!("no question"),
    }
}

#[test]
fn a_button_takes_the_focus_and_is_pressed_when_let_go_on_it() {
    let (_temp, mut app) = quit_question();
    press(&mut app, 52, 15, LEFT);
    assert_eq!(focused(&app), 1, "No has the focus");
    // FAR presses it up to four columns past its text.
    assert!(release(&mut app, 58, 15));
    assert!(app.overlay.is_none(), "No: the question went");
    assert!(!app.should_quit());
}

#[test]
fn a_button_let_go_elsewhere_only_takes_the_focus() {
    let (_temp, mut app) = quit_question();
    press(&mut app, 52, 15, LEFT);
    assert!(!release(&mut app, 52, 16));
    assert!(app.overlay.is_some());
    assert_eq!(focused(&app), 1);
}

#[test]
fn outside_the_dialog_either_button_is_esc_and_never_answers_the_question() {
    let (_temp, mut app) = quit_question();
    press(&mut app, 5, 2, LEFT);
    assert!(app.overlay.is_none());
    assert!(!app.should_quit());
    let (_temp, mut app) = quit_question();
    press(&mut app, 5, 2, RIGHT);
    assert!(app.overlay.is_none());
    assert!(!app.should_quit(), "the owner's change: not FAR's Enter");
}

#[test]
fn a_click_in_an_edit_line_puts_the_cursor_there_and_a_double_click_selects_the_word() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    // An input line of its own (F7's is a form since R7 review D4).
    app.show_dialog(Dialog::Input {
        title: "Apply command".to_string(),
        prompt: String::new(),
        input: crate::app::line_input::LineInput::default(),
        action: crate::app::dialog::InputAction::ApplyCommand,
    });
    for ch in "new folder".chars() {
        app.handle_key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char(ch),
            KeyModifiers::NONE,
        ));
    }
    app.mouse.dialog = Some(DialogArea {
        frame: FRAME,
        controls: vec![control(22, 12, 50, 0, ControlKind::Edit { first: 0 })],
    });
    press(&mut app, 24, 12, LEFT);
    release(&mut app, 24, 12);
    app.handle_mouse(mouse(24, 12, LEFT, MouseAction::DoubleClick));
    let Some(Overlay::Dialog(Dialog::Input { input, .. })) = &app.overlay else {
        panic!("no input dialog");
    };
    assert_eq!(input.cursor(), 2);
    assert_eq!(input.selection(), Some(0..3));
}

/// A form with a check box (item 0), a drop-down list (item 1) and a button (item 2), as if drawn.
fn form() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let check = Control::Check {
        label: "&Box".to_string(),
        checked: false,
    };
    let list = Control::DropDown {
        lines: vec![
            ListLine::Choice("one".to_string()),
            ListLine::Separator,
            ListLine::Choice("three".to_string()),
        ],
        chosen: 0,
        open: None,
        caption: None,
        bottom_title: None,
    };
    let button = Control::Button {
        label: "OK".to_string(),
        default: true,
    };
    let items = vec![
        Item::new(5, 2, check),
        Item::new(5, 3, list),
        Item::new(5, 5, button),
    ];
    app.show_dialog(Dialog::Form {
        form: Form::new("Test", 40, 8, items),
        action: FormAction::DriveColumns {
            side: Side::Left,
            selected: 0,
        },
    });
    app.mouse.dialog = Some(DialogArea {
        frame: FRAME,
        controls: vec![
            control(25, 10, 7, 0, ControlKind::Toggle),
            control(25, 11, 10, 1, ControlKind::DropDown),
            control(40, 13, 6, 2, ControlKind::Button),
        ],
    });
    (temp, app)
}

fn the_form(app: &App) -> &Form {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("no form"),
    }
}

#[test]
fn a_click_on_a_check_box_is_space() {
    let (_temp, mut app) = form();
    let drawn = app.mouse.dialog.clone();
    press(&mut app, 30, 10, LEFT);
    assert!(the_form(&app).is_checked(0));
    assert!(
        app.mouse.dialog.is_none(),
        "the next draw tells where it is"
    );
    release(&mut app, 30, 10);
    app.mouse.dialog = drawn;
    press(&mut app, 26, 10, LEFT);
    assert!(!the_form(&app).is_checked(0));
}

#[test]
fn a_drop_down_list_opens_on_a_click_and_its_line_is_chosen_with_the_mouse() {
    let (_temp, mut app) = form();
    press(&mut app, 27, 11, LEFT);
    release(&mut app, 27, 11);
    assert_eq!(the_form(&app).focus, 1);
    assert!(matches!(
        the_form(&app).control(1),
        Some(Control::DropDown { open: Some(list), .. }) if list.cursor == 0
    ));
    // The open list as the renderer reports it: under the field, a frame around three lines.
    let list = Cells {
        x: 25,
        y: 12,
        width: 10,
        height: 5,
    };
    app.mouse.menu = Some(MenuArea {
        frame: list,
        outer: list,
        first: 0,
    });
    app.handle_mouse(mouse(28, 13 + 2, LEFT, MouseAction::Button));
    app.handle_mouse(mouse(28, 13 + 2, NONE, MouseAction::Button));
    assert_eq!(the_form(&app).chosen(1), 2, "the separator is skipped");
    assert!(matches!(
        the_form(&app).control(1),
        Some(Control::DropDown { open: None, .. })
    ));
}

#[test]
fn a_button_let_go_after_another_dialog_came_up_presses_nothing() {
    let (_temp, mut app) = quit_question();
    press(&mut app, 52, 15, LEFT);
    // A question comes up while the button is held: another dialog at the same place.
    app.overlay = None;
    app.execute(Command::Quit);
    app.mouse.dialog = Some(DialogArea {
        frame: FRAME,
        controls: vec![
            control(40, 15, 7, 0, ControlKind::Button),
            control(50, 15, 12, 1, ControlKind::Button),
        ],
    });
    assert!(!release(&mut app, 52, 15));
    assert!(app.overlay.is_some(), "the question is still up");
    assert!(!app.should_quit());
}

#[test]
fn a_key_forgets_where_the_dialog_was() {
    let (_temp, mut app) = quit_question();
    press(&mut app, 52, 15, LEFT);
    app.handle_key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Left,
        KeyModifiers::NONE,
    ));
    assert!(app.mouse.dialog.is_none());
    assert!(app.mouse.button_press.is_none());
    assert!(!release(&mut app, 52, 15));
    assert!(app.overlay.is_some());
}

#[test]
fn the_buttons_of_the_progress_dialog_are_pressed_with_the_mouse() {
    let (_temp, mut app) = form();
    app.overlay = None;
    app.shown_task = Some(super::ShownTask {
        id: crate::jobs::tasks::TaskId::default(),
        focus: 0,
        rest: Vec::new(),
    });
    app.mouse.dialog = Some(DialogArea {
        frame: FRAME,
        controls: vec![
            control(30, 15, 14, 0, ControlKind::Button),
            control(46, 15, 9, 1, ControlKind::Button),
            control(57, 15, 10, 2, ControlKind::Button),
        ],
    });
    press(&mut app, 48, 15, LEFT);
    assert_eq!(app.shown_task.as_ref().unwrap().focus, 1);
    press(&mut app, 32, 15, LEFT);
    assert!(
        app.shown_task.is_some(),
        "still held: FAR waits for the button to be let go"
    );
    release(&mut app, 32, 15);
}
