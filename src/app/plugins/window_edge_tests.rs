//! A plugin's windows at their edges: a window without buttons gone another way, a failure after one, an
//! input line kept while the program looks for news, the busy window's own box, a screen of a few rows,
//! the mouse over a grey line.

use super::*;
use crate::app::console_input::{Mouse, MouseAction};
use crate::app::form::Control;
use crate::app::{Cells, MenuArea};
use crate::platform::input::MouseButtons;

#[test]
fn a_window_without_buttons_gone_another_way_is_not_closed_again() {
    let (_temp, mut app) = app();
    busy(&app, "probe", "Working...");
    app.pump_plugins();
    let own = Dialog::message("Own", vec![]);
    app.show_dialog(own.clone());
    assert!(!app.pump_plugins());
    assert!(app.plugins.busy.is_none(), "not up any more");
    done(&app, "probe");
    assert!(!app.pump_plugins(), "its end changes nothing");
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(dialog)) if *dialog == own),
        "the window up stays"
    );
}

#[test]
fn a_failure_after_a_window_without_buttons_stays_past_the_calls_end() {
    let (_temp, mut app) = app();
    // As a plugin's thread sends them for a call that showed one and failed.
    busy(&app, "probe", "Working...");
    app.plugins.host.send_event(Event::Failed {
        plugin: "probe".to_string(),
        error: "boom".to_string(),
        serial: None,
    });
    done(&app, "probe");
    assert!(app.pump_plugins());
    let failure = Dialog::error("probe", vec!["boom".to_string()]);
    let failure_up =
        |app: &App| matches!(&app.overlay, Some(Overlay::Dialog(dialog)) if *dialog == failure);
    assert!(failure_up(&app), "the failure shown");
    assert!(!app.pump_plugins());
    assert!(failure_up(&app), "the end of the call leaves the failure");
}

#[test]
fn an_input_line_up_is_not_answered_by_the_next_look_for_news() {
    let (_temp, mut app) = app();
    let answer = ask(
        &app,
        "probe",
        Request::Input(InputRequest {
            title: "T".to_string(),
            prompt: "P".to_string(),
            text: "start".to_string(),
            ..InputRequest::default()
        }),
    );
    app.pump_plugins();
    assert!(!app.pump_plugins());
    assert_eq!(answer.try_recv(), Err(TryRecvError::Empty), "still waits");
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: FormAction::Plugin(PluginForm::Input { .. }),
            ..
        }))
    ));
}

#[test]
fn a_window_without_buttons_closes_under_its_lines() {
    let (_temp, mut app) = app();
    busy(&app, "probe", "Working...");
    app.pump_plugins();
    let (shown, _) = form(&app);
    assert_eq!(
        shown.height, 5,
        "the box, a margin, the line, a margin, the box"
    );
    assert!(
        !(shown.items.iter()).any(|item| matches!(item.control, Control::Separator)),
        "no separator before buttons that are not there"
    );
    let answer = ask(&app, "probe", Request::Message(message(&["Hi"], &["OK"])));
    app.pump_plugins();
    let (shown, _) = form(&app);
    assert_eq!(shown.height, 7, "with its buttons");
    press(&mut app, KeyCode::Enter);
    assert_eq!(answer.try_recv(), Ok(Answer::Button(0)));
}

#[test]
fn a_screen_of_a_few_rows_keeps_one_line_of_a_message() {
    let (_temp, mut app) = app();
    app.height = 5;
    let answer = ask(
        &app,
        "probe",
        Request::Message(message(&["one", "two"], &["OK"])),
    );
    app.pump_plugins();
    let (shown, _) = form(&app);
    assert_eq!(labels(&shown).len(), 1);
    assert_eq!(labels(&shown)[0].1, "one");
    press(&mut app, KeyCode::Enter);
    assert_eq!(answer.try_recv(), Ok(Answer::Button(0)));
}

const NONE: MouseButtons = MouseButtons {
    left: false,
    right: false,
    middle: false,
};
const LEFT: MouseButtons = MouseButtons { left: true, ..NONE };

fn mouse(row: u16, buttons: MouseButtons, action: MouseAction) -> Mouse {
    Mouse {
        column: 20,
        row,
        buttons,
        pressed: buttons,
        action,
        modifiers: KeyModifiers::NONE,
    }
}

fn selected(app: &App) -> usize {
    match &app.overlay {
        Some(Overlay::PluginMenu(menu)) => menu.selected,
        _ => panic!("the plugin's menu"),
    }
}

#[test]
fn the_mouse_passes_over_a_grey_line_and_chooses_another() {
    let (_temp, mut app) = app();
    let mut request = menu_request();
    request.lines.insert(
        1,
        MenuLine {
            disabled: true,
            ..line("&Grey")
        },
    );
    let answer = ask(&app, "probe", Request::Menu(request));
    app.pump_plugins();
    // The menu's lines from row 6.
    let frame = Cells {
        x: 10,
        y: 5,
        width: 30,
        height: 5,
    };
    app.mouse.menu = Some(MenuArea {
        frame,
        outer: frame,
        first: 0,
    });
    app.handle_mouse(mouse(7, NONE, MouseAction::Moved));
    assert_eq!(selected(&app), 0, "not the grey line");
    app.handle_mouse(mouse(8, NONE, MouseAction::Moved));
    assert_eq!(selected(&app), 2);
    app.handle_mouse(mouse(8, LEFT, MouseAction::Button));
    app.handle_mouse(mouse(8, NONE, MouseAction::Button));
    assert_eq!(
        answer.try_recv(),
        Ok(Answer::Chosen {
            item: Some(2),
            key: None
        })
    );
}
