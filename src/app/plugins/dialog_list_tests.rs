//! A plugin's dialog's lists: a list box's line as its value, the lines replaced by the handler, the
//! mouse on a list box (its line, a double click closing the dialog with it) and one that does not close.

use super::*;

/// A drop-down list on Green, a list box of three on its second line, OK.
fn lists() -> Vec<DialogItem> {
    let lines = |lines: &[&str]| lines.iter().map(ToString::to_string).collect();
    let mut list = item(ItemKind::List, "");
    list.lines = lines(&["Red", "Green"]);
    list.state = 1;
    let mut boxed = item(ItemKind::ListBox, "Box");
    boxed.lines = lines(&["one", "two", "three"]);
    boxed.state = 1;
    let mut ok = item(ItemKind::Button, "OK");
    ok.default = true;
    vec![list, boxed, ok]
}

#[test]
fn a_list_boxs_line_is_its_value_and_the_handler_replaces_the_lines() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, lists(), true);
    let (_, values, reply) = event(&heard);
    assert_eq!(states(&values)[..2], [(1, "Green"), (1, "two")]);
    let changes = vec![
        Change::Lines(0, vec!["Only".to_string()]),
        Change::Lines(1, vec!["a".to_string(), "b".to_string()]),
        Change::State(1, 0),
    ];
    answer(&reply, changes, false);
    app.pump_plugins();
    let form = shown_form(&app);
    assert_eq!(form.chosen(0), 0, "the choice kept where it still is");
    let Some(Control::ListBox(list)) = form.control(1) else {
        panic!("the list box");
    };
    assert_eq!((list.lines.len(), list.current()), (2, "a"));
    // Its cursor moved by the user is a change heard.
    key(&mut app, KeyCode::Tab);
    focus_moved(&heard, 0, 1);
    key(&mut app, KeyCode::Down);
    let (kind, values, _) = event(&heard);
    assert_eq!(kind, DialogEventKind::Changed(1));
    assert_eq!(states(&values)[1], (1, "b"));
}

#[test]
fn a_click_puts_a_list_boxs_cursor_on_its_line_and_a_double_one_closes_with_it() {
    use crate::app::console_input::{Mouse, MouseAction};
    use crate::app::{Cells, ControlKind, DialogArea, DialogControl};
    use crate::platform::input::MouseButtons;
    let (_temp, mut app) = app();
    let heard = open(&mut app, lists(), false);
    let area = |top| DialogArea {
        frame: Cells {
            x: 20,
            y: 8,
            width: 60,
            height: 9,
        },
        controls: vec![DialogControl {
            cells: Cells {
                x: 26,
                y: 11,
                width: 30,
                height: 3,
            },
            index: 1,
            kind: ControlKind::ListBox { top },
        }],
    };
    let left = MouseButtons {
        left: true,
        right: false,
        middle: false,
    };
    let click = |app: &mut App, row, action| {
        app.mouse.dialog = Some(area(0));
        app.handle_mouse(Mouse {
            column: 30,
            row,
            buttons: left,
            pressed: left,
            action,
            modifiers: KeyModifiers::NONE,
        });
    };
    click(&mut app, 13, MouseAction::Button);
    let Some(Control::ListBox(list)) = shown_form(&app).control(1) else {
        panic!("the list box");
    };
    assert_eq!(list.current(), "three", "the third row shown");
    click(&mut app, 11, MouseAction::DoubleClick);
    app.pump_plugins();
    assert!(app.plugins.dialog.is_none(), "closed by the double click");
    match heard.recv_timeout(Duration::from_secs(1)) {
        Ok(DialogMessage::Closed { button, values }) => {
            assert_eq!(
                button,
                Some(1),
                "FAR's exit code: the list box, not the default OK"
            );
            assert_eq!(states(&values)[1], (0, "one"));
        }
        other => panic!("closed: {other:?}"),
    }
}

#[test]
fn a_list_box_that_does_not_close_takes_enter_and_a_double_click_for_nothing() {
    use crate::app::console_input::{Mouse, MouseAction};
    use crate::app::{Cells, ControlKind, DialogArea, DialogControl};
    use crate::platform::input::MouseButtons;
    let (_temp, mut app) = app();
    let mut items = lists();
    items[1].no_close = true;
    let heard = open(&mut app, items, false);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Enter);
    assert!(
        app.plugins.dialog.is_some(),
        "FAR's DIF_LISTNOCLOSE, a default button or not"
    );
    let left = MouseButtons {
        left: true,
        right: false,
        middle: false,
    };
    app.mouse.dialog = Some(DialogArea {
        frame: Cells {
            x: 20,
            y: 8,
            width: 60,
            height: 9,
        },
        controls: vec![DialogControl {
            cells: Cells {
                x: 26,
                y: 11,
                width: 30,
                height: 3,
            },
            index: 1,
            kind: ControlKind::ListBox { top: 0 },
        }],
    });
    app.handle_mouse(Mouse {
        column: 30,
        row: 12,
        buttons: left,
        pressed: left,
        action: MouseAction::DoubleClick,
        modifiers: KeyModifiers::NONE,
    });
    app.pump_plugins();
    assert!(app.plugins.dialog.is_some(), "nor a double click");
    assert!(heard.try_recv().is_err());
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Enter);
    assert!(
        app.plugins.dialog.is_none(),
        "Enter elsewhere: the default button"
    );
}
