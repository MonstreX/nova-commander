//! A plugin's dialog whose handler hears the keys and clicks first, the dialog drawn told, the dialog
//! placed where the plugin asks: the plugin's thread played by the test.

use super::*;
use crate::app::console_input::{Mouse, MouseAction};
use crate::app::{Cells, ControlKind, DialogArea, DialogControl};
use crate::platform::input::MouseButtons;

/// The dialog of [`items`] asked as `request` says (its items, title and "probe" set here).
fn open_as(app: &mut App, request: DialogRequest) -> Receiver<DialogMessage> {
    let (talk, heard) = channel();
    app.plugins.host.send_event(Event::Dialog {
        plugin: "probe".to_string(),
        request: DialogRequest {
            title: "Dialog".to_string(),
            items: items(),
            handled: true,
            ..request
        },
        talk,
    });
    assert!(app.pump_plugins());
    heard
}

/// The next event as it comes, the dialog drawn too.
fn raw_event(
    heard: &Receiver<DialogMessage>,
) -> (DialogEventKind, Vec<ItemValue>, Sender<Handled>) {
    match heard.recv_timeout(Duration::from_secs(1)) {
        Ok(DialogMessage::Event {
            kind,
            values,
            reply,
        }) => (kind, values, reply),
        other => panic!("an event: {other:?}"),
    }
}

fn answer_input(reply: &Sender<Handled>, taken: bool) {
    let handled = Handled {
        taken,
        ..Handled::default()
    };
    reply.send(handled).unwrap();
}

#[test]
fn drawing_is_told_as_the_dialog_comes_and_as_the_screen_changes_once_each() {
    let (_temp, mut app) = app();
    let heard = open_as(&mut app, DialogRequest::default());
    let (kind, _, reply) = raw_event(&heard);
    assert_eq!(kind, DialogEventKind::Init);
    answer(&reply, Vec::new(), false);
    app.pump_plugins();
    let (kind, _, reply) = raw_event(&heard);
    let shown = DialogEventKind::Draw {
        columns: 100,
        rows: 30,
    };
    assert_eq!(kind, shown);
    answer(&reply, Vec::new(), false);
    app.pump_plugins();
    assert!(heard.try_recv().is_err(), "once for the same screen");
    app.resize(120, 40);
    app.pump_plugins();
    let (kind, _, _) = raw_event(&heard);
    let resized = DialogEventKind::Draw {
        columns: 120,
        rows: 40,
    };
    assert_eq!(kind, resized);
}

#[test]
fn a_key_goes_to_the_handler_first_and_the_dialog_does_it_only_when_let_go() {
    let (_temp, mut app) = app();
    let heard = open_as(
        &mut app,
        DialogRequest {
            keys: true,
            ..DialogRequest::default()
        },
    );
    skip(&heard);
    key(&mut app, KeyCode::Char('x'));
    let (kind, values, first) = event(&heard);
    let x = DialogEventKind::Key {
        item: Some(1),
        key: "x".to_string(),
    };
    assert_eq!(kind, x);
    assert_eq!(values[1].text, "start", "not done yet");
    // Pressed while the handler has not answered: held for after it.
    press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    assert_eq!(shown_form(&app).edit_text(1), "start");
    answer_input(&first, false);
    app.pump_plugins();
    assert_eq!(shown_form(&app).edit_text(1), "startx", "let go: done");
    let (kind, values, reply) = event(&heard);
    assert_eq!(kind, DialogEventKind::Changed(1), "its change, per key");
    assert_eq!(values[1].text, "startx");
    answer(&reply, Vec::new(), false);
    let (kind, _, second) = event(&heard);
    let y = DialogEventKind::Key {
        item: Some(1),
        key: "y".to_string(),
    };
    assert_eq!(kind, y, "the key held comes after");
    answer_input(&second, true);
    app.pump_plugins();
    assert_eq!(shown_form(&app).edit_text(1), "startx", "taken: not done");
    nothing_more(&heard);
}

#[test]
fn esc_once_the_handler_keeps_a_key_takes_the_dialog_away() {
    let (_temp, mut app) = app();
    let heard = open_as(
        &mut app,
        DialogRequest {
            keys: true,
            ..DialogRequest::default()
        },
    );
    skip(&heard);
    key(&mut app, KeyCode::Char('x'));
    let (_, _, _kept) = event(&heard);
    key(&mut app, KeyCode::Esc);
    assert!(app.plugins.dialog.is_some(), "held at first");
    std::thread::sleep(crate::app::held::QUICK);
    key(&mut app, KeyCode::Esc);
    assert!(app.plugins.dialog.is_none(), "taken away");
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { title, .. })) if title == "Stopped")
    );
    // The events sent before it went (its drawing) are there to read; then the line is gone.
    let next = loop {
        match heard.recv_timeout(Duration::from_secs(1)) {
            Ok(DialogMessage::Event {
                kind: DialogEventKind::Draw { .. },
                ..
            }) => {}
            next => break next,
        }
    };
    assert!(
        matches!(next, Err(RecvTimeoutError::Disconnected)),
        "Esc for the plugin: {next:?}"
    );
}

#[test]
fn a_click_goes_to_the_handler_with_its_place_and_item() {
    let (_temp, mut app) = app();
    let heard = open_as(
        &mut app,
        DialogRequest {
            keys: true,
            ..DialogRequest::default()
        },
    );
    skip(&heard);
    let cells = |x, y, width| Cells {
        x,
        y,
        width,
        height: 1,
    };
    app.mouse.dialog = Some(DialogArea {
        frame: Cells {
            height: 8,
            ..cells(20, 8, 60)
        },
        controls: vec![DialogControl {
            cells: cells(25, 10, 40),
            index: laid_line(&app),
            kind: ControlKind::Edit { first: 0 },
        }],
    });
    let right = MouseButtons {
        left: false,
        right: true,
        middle: false,
    };
    let changed = app.handle_mouse(Mouse {
        column: 27,
        row: 10,
        buttons: right,
        pressed: right,
        action: MouseAction::Button,
        modifiers: KeyModifiers::CONTROL,
    });
    assert!(changed);
    let (kind, _, reply) = event(&heard);
    let click = DialogEventKind::Mouse {
        item: Some(1),
        key: "Ctrl+MsRClick".to_string(),
        x: 7,
        y: 2,
    };
    assert_eq!(kind, click);
    answer_input(&reply, true);
    app.pump_plugins();
    shown_form(&app);
}

/// The form's item showing the plugin's edit line (item 1).
fn laid_line(app: &App) -> usize {
    (app.plugins.dialog.as_ref())
        .and_then(|session| session.items.get(1).copied())
        .unwrap()
}

#[test]
fn a_dialog_stands_where_its_plugin_puts_it() {
    let (_temp, mut app) = app();
    let _heard = open_as(
        &mut app,
        DialogRequest {
            x: Some(2),
            y: Some(1),
            ..DialogRequest::default()
        },
    );
    shown_form(&app);
    // The drawing turns it into the shift, in the room it draws the dialog in (`ui::dialog_place`).
    assert_eq!(app.dialog_place.at, Some((Some(2), Some(1))));
    let (_temp, mut app) = self::app();
    let _heard = open_as(&mut app, DialogRequest::default());
    assert_eq!(app.dialog_place.at, None, "centred");
}

#[test]
fn a_button_let_go_before_the_handler_answers_its_press_is_pressed_once_after_it() {
    let (_temp, mut app) = app();
    let heard = open_as(
        &mut app,
        DialogRequest {
            keys: true,
            ..DialogRequest::default()
        },
    );
    skip(&heard);
    let ok = (app.plugins.dialog.as_ref())
        .and_then(|session| session.items.get(3).copied())
        .unwrap();
    let area = DialogArea {
        frame: Cells {
            x: 20,
            y: 8,
            width: 60,
            height: 8,
        },
        controls: vec![DialogControl {
            cells: Cells {
                x: 40,
                y: 13,
                width: 6,
                height: 1,
            },
            index: ok,
            kind: ControlKind::Button,
        }],
    };
    let record = |left: bool| {
        let buttons = MouseButtons {
            left,
            right: false,
            middle: false,
        };
        Mouse {
            column: 41,
            row: 13,
            buttons,
            pressed: buttons,
            action: MouseAction::Button,
            modifiers: KeyModifiers::NONE,
        }
    };
    app.mouse.dialog = Some(area.clone());
    app.handle_mouse(record(true));
    let (kind, _, reply) = event(&heard);
    assert!(
        matches!(kind, DialogEventKind::Mouse { item: Some(3), .. }),
        "{kind:?}"
    );
    // Let go before the answer: held after the press.
    app.handle_mouse(record(false));
    nothing_more(&heard);
    answer_input(&reply, false);
    app.mouse.dialog = Some(area);
    app.pump_plugins();
    focus_moved(&heard, 1, 3);
    let (kind, _, _) = event(&heard);
    assert_eq!(
        kind,
        DialogEventKind::Close(Some(3)),
        "pressed once, after its press"
    );
    nothing_more(&heard);
}

#[test]
fn esc_in_a_nested_dialog_kept_by_its_handler_gives_the_first_back_after_the_notice() {
    let (_temp, mut app) = app();
    let first = open(&mut app, items(), true);
    skip(&first);
    let (talk, second) = channel();
    app.plugins.host.send_event(Event::Dialog {
        plugin: "probe".to_string(),
        request: DialogRequest {
            title: "Second".to_string(),
            items: vec![item(ItemKind::Edit, ""), item(ItemKind::Button, "OK")],
            handled: true,
            keys: true,
            ..DialogRequest::default()
        },
        talk,
    });
    app.pump_plugins();
    skip(&second);
    assert_eq!(shown_form(&app).title, "Second");
    key(&mut app, KeyCode::Char('x'));
    let (_, _, _kept) = event(&second);
    std::thread::sleep(crate::app::held::QUICK);
    key(&mut app, KeyCode::Esc);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Message { title, .. })) if title == "Stopped")
    );
    key(&mut app, KeyCode::Enter);
    app.pump_plugins();
    assert_eq!(shown_form(&app).title, "Dialog", "the first back, not lost");
}
