//! A plugin's dialog in the program, the plugin's thread played by the test: what its handler hears as
//! the user acts, its answers applied, closing as it lets, the dialog gone or aside.

use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::plugin_host::Event;
use crate::plugin_host::windows::{Answer, DialogItem, MessageRequest, Request};

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    (temp, app)
}

fn item(kind: ItemKind, text: &str) -> DialogItem {
    DialogItem {
        kind,
        text: text.to_string(),
        ..DialogItem::default()
    }
}

/// Name, its line, a check box, OK (default), Fill (does not close), Cancel.
fn items() -> Vec<DialogItem> {
    let mut ok = item(ItemKind::Button, "OK");
    ok.default = true;
    let mut fill = item(ItemKind::Button, "&Fill");
    fill.no_close = true;
    vec![
        item(ItemKind::Text, "&Name:"),
        item(ItemKind::Edit, "start"),
        item(ItemKind::Check, "&Upper"),
        ok,
        fill,
        item(ItemKind::Button, "Cancel"),
    ]
}

/// The dialog of `items` asked by "probe", with a handler or not: what the plugin's thread hears.
fn open(app: &mut App, items: Vec<DialogItem>, handled: bool) -> Receiver<DialogMessage> {
    let (talk, heard) = channel();
    app.plugins.host.send_event(Event::Dialog {
        plugin: "probe".to_string(),
        request: DialogRequest {
            title: "Dialog".to_string(),
            items,
            handled,
            ..DialogRequest::default()
        },
        talk,
    });
    assert!(app.pump_plugins());
    heard
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    app.pump_plugins();
}

fn key(app: &mut App, code: KeyCode) {
    press(app, code, KeyModifiers::NONE);
}

/// The next event the handler hears, its values and where its answer goes; the dialog drawn (FAR's
/// `DN_DRAWDIALOG`, told as it comes in front: `input_tests` check it) answered on the way, as a plugin
/// that does nothing with it.
fn event(heard: &Receiver<DialogMessage>) -> (DialogEventKind, Vec<ItemValue>, Sender<Handled>) {
    loop {
        match heard.recv_timeout(Duration::from_secs(1)) {
            Ok(DialogMessage::Event {
                kind: DialogEventKind::Draw { .. },
                reply,
                ..
            }) => answer(&reply, Vec::new(), false),
            Ok(DialogMessage::Event {
                kind,
                values,
                reply,
            }) => return (kind, values, reply),
            other => panic!("an event: {other:?}"),
        }
    }
}

/// The next event answered with no changes (the plugin's thread is alive while it answers).
fn skip(heard: &Receiver<DialogMessage>) {
    let (_, _, reply) = event(heard);
    answer(&reply, Vec::new(), false);
}

/// Nothing more heard but the dialog drawn (answered).
fn nothing_more(heard: &Receiver<DialogMessage>) {
    loop {
        match heard.try_recv() {
            Ok(DialogMessage::Event {
                kind: DialogEventKind::Draw { .. },
                reply,
                ..
            }) => answer(&reply, Vec::new(), false),
            next => {
                assert!(next.is_err(), "nothing more: {next:?}");
                return;
            }
        }
    }
}

/// The focus heard leaving item `from` and coming to `to` (FAR's `DN_KILLFOCUS`, `DN_GOTFOCUS`).
fn focus_moved(heard: &Receiver<DialogMessage>, from: usize, to: usize) {
    for wanted in [DialogEventKind::Unfocus(from), DialogEventKind::Focus(to)] {
        let (kind, _, reply) = event(heard);
        assert_eq!(kind, wanted);
        answer(&reply, Vec::new(), false);
    }
}

/// Each value's state and text.
fn states(values: &[ItemValue]) -> Vec<(i32, &str)> {
    (values.iter())
        .map(|value| (value.state, value.text.as_str()))
        .collect()
}

fn shown_form(app: &App) -> &Form {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Plugin(PluginForm::Dialog),
        })) => form,
        _ => panic!("the plugin's dialog"),
    }
}

fn answer(reply: &Sender<Handled>, changes: Vec<Change>, keep_open: bool) {
    reply
        .send(Handled {
            changes,
            keep_open,
            taken: false,
        })
        .unwrap();
}

#[test]
fn the_handler_hears_init_with_the_values_and_its_changes_show() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    let (kind, values, reply) = event(&heard);
    assert_eq!(kind, DialogEventKind::Init);
    let texts: Vec<&str> = values.iter().map(|value| value.text.as_str()).collect();
    assert_eq!(
        texts,
        ["&Name:", "start", "&Upper", "OK", "&Fill", "Cancel"]
    );
    let changes = vec![
        Change::Text(1, "set".to_string()),
        Change::State(2, 1),
        Change::Enabled(4, false),
        Change::Text(0, "&Who:".to_string()),
        Change::Focus(2),
        Change::Text(99, "no such item".to_string()),
    ];
    answer(&reply, changes, false);
    assert!(app.pump_plugins(), "applied");
    let form = shown_form(&app);
    assert_eq!(form.edit_text(1), "set");
    assert!(form.is_checked(2));
    assert!(
        form.items[5].disabled,
        "Fill, the form's item after the separator"
    );
    assert!(matches!(&form.items[0].control, Control::Text(text) if text == "&Who:"));
    assert_eq!(form.focus, 2);
    app.pump_plugins();
    // The focus it moved is told moved (FAR's `DM_SETFOCUS`), its other changes not told back.
    focus_moved(&heard, 1, 2);
    nothing_more(&heard);
}

#[test]
fn a_change_by_the_user_is_heard_once_with_the_new_values() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    let (_, _, reply) = event(&heard);
    answer(&reply, Vec::new(), false);
    key(&mut app, KeyCode::Char('x'));
    let (kind, values, reply) = event(&heard);
    assert_eq!(kind, DialogEventKind::Changed(1));
    assert_eq!(values[1].text, "startx");
    answer(&reply, Vec::new(), false);
    key(&mut app, KeyCode::Tab);
    focus_moved(&heard, 1, 2);
    nothing_more(&heard);
    key(&mut app, KeyCode::Char(' '));
    let (kind, values, _) = event(&heard);
    assert_eq!(kind, DialogEventKind::Changed(2));
    assert_eq!(values[2].state, 1);
}

#[test]
fn a_button_that_does_not_close_is_heard_and_the_dialog_stays() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    press(&mut app, KeyCode::Char('f'), KeyModifiers::ALT);
    focus_moved(&heard, 1, 4);
    let (kind, _, reply) = event(&heard);
    assert_eq!(kind, DialogEventKind::Button(4));
    answer(&reply, Vec::new(), false);
    app.pump_plugins();
    shown_form(&app);
}

#[test]
fn closing_waits_for_the_handler_which_may_keep_the_dialog_open() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    key(&mut app, KeyCode::Enter);
    let (kind, _, reply) = event(&heard);
    assert_eq!(kind, DialogEventKind::Close(Some(3)), "OK, the default");
    shown_form(&app);
    key(&mut app, KeyCode::Enter);
    nothing_more(&heard);
    answer(&reply, vec![Change::Text(1, String::new())], true);
    app.pump_plugins();
    assert_eq!(
        shown_form(&app).edit_text(1),
        "",
        "kept open, its changes shown"
    );
    key(&mut app, KeyCode::Esc);
    let (kind, _, reply) = event(&heard);
    assert_eq!(
        kind,
        DialogEventKind::Close(None),
        "Esc, heard again once kept"
    );
    answer(&reply, Vec::new(), false);
    app.pump_plugins();
    assert!(app.overlay.is_none());
    match heard.recv_timeout(Duration::from_secs(1)) {
        Ok(DialogMessage::Closed { button, values }) => {
            assert_eq!(button, None);
            assert_eq!(values[1].text, "");
        }
        other => panic!("closed: {other:?}"),
    }
    assert!(app.plugins.dialog.is_none());
}

#[test]
fn without_a_handler_a_button_or_esc_closes_at_once() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), false);
    key(&mut app, KeyCode::Char('x'));
    nothing_more(&heard);
    key(&mut app, KeyCode::Enter);
    assert!(app.overlay.is_none());
    match heard.try_recv() {
        Ok(DialogMessage::Closed { button, values }) => {
            assert_eq!(button, Some(3));
            assert_eq!(values[1].text, "startx");
        }
        other => panic!("closed: {other:?}"),
    }
    let heard = open(&mut app, items(), false);
    key(&mut app, KeyCode::Esc);
    assert!(matches!(
        heard.try_recv(),
        Ok(DialogMessage::Closed { button: None, .. })
    ));
}

#[test]
fn a_dialog_gone_another_way_is_esc_for_the_plugin() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    app.overlay = None;
    app.pump_plugins();
    assert!(app.plugins.dialog.is_none());
    assert_eq!(
        heard.recv_timeout(Duration::from_secs(1)).err(),
        Some(RecvTimeoutError::Disconnected)
    );
}

#[test]
fn the_handlers_thread_gone_takes_the_dialog_away() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    let (_, _, reply) = event(&heard);
    drop(reply);
    assert!(app.pump_plugins());
    assert!(app.overlay.is_none() && app.plugins.dialog.is_none());
}

#[test]
fn the_handlers_message_comes_over_the_dialog_which_comes_back_after_it() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    let message = MessageRequest {
        title: "Probe".to_string(),
        lines: vec!["The name is empty".to_string()],
        buttons: vec!["OK".to_string()],
        ..MessageRequest::default()
    };
    let (reply, answered) = channel();
    app.plugins.host.send_event(Event::Ask {
        plugin: "probe".to_string(),
        request: Request::Message(message),
        reply,
    });
    assert!(app.pump_plugins());
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: FormAction::Plugin(PluginForm::Message { .. }),
            ..
        }))
    ));
    assert!(
        app.plugins.dialog.as_ref().unwrap().hidden.is_some(),
        "aside"
    );
    key(&mut app, KeyCode::Enter);
    assert_eq!(answered.try_recv(), Ok(Answer::Button(0)));
    shown_form(&app);
    assert!(app.plugins.dialog.as_ref().unwrap().hidden.is_none());
    nothing_more(&heard);
}

#[test]
fn another_plugins_window_waits_for_the_dialog() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    let (reply, answered) = channel();
    app.plugins.host.send_event(Event::Ask {
        plugin: "other".to_string(),
        request: Request::Message(MessageRequest::default()),
        reply,
    });
    app.pump_plugins();
    shown_form(&app);
    assert!(answered.try_recv().is_err(), "waiting");
}

#[test]
fn a_second_dialog_of_its_plugin_comes_over_the_first() {
    // FAR's nested modal dialogs (E14): the first waits aside under it.
    let (_temp, mut app) = app();
    let first = open(&mut app, items(), true);
    skip(&first);
    let (talk, second) = channel();
    app.plugins.host.send_event(Event::Dialog {
        plugin: "probe".to_string(),
        request: DialogRequest {
            title: "Second".to_string(),
            handled: true,
            ..DialogRequest::default()
        },
        talk,
    });
    app.pump_plugins();
    assert!(matches!(
        second.recv_timeout(Duration::from_secs(1)),
        Ok(DialogMessage::Event {
            kind: DialogEventKind::Init,
            ..
        })
    ));
    assert_eq!(shown_form(&app).title, "Second");
    assert_eq!(app.plugins.outer_dialogs.len(), 1);
}

#[test]
fn values_and_states_of_every_kind() {
    let mut three = item(ItemKind::Check, "&Three");
    three.three_state = true;
    three.state = 2;
    let mut radio_one = item(ItemKind::Radio, "&One");
    radio_one.group = true;
    radio_one.state = 1;
    let mut list = item(ItemKind::List, "");
    list.lines = vec!["Red".to_string(), "Green".to_string()];
    list.state = 1;
    let (_temp, mut app) = app();
    let heard = open(
        &mut app,
        vec![three, radio_one, item(ItemKind::Radio, "&Two"), list],
        true,
    );
    let (_, values, reply) = event(&heard);
    let states: Vec<(i32, &str)> = (values.iter())
        .map(|value| (value.state, value.text.as_str()))
        .collect();
    assert_eq!(
        states,
        [(2, "&Three"), (1, "&One"), (0, "&Two"), (1, "Green")]
    );
    let changes = vec![
        Change::State(0, 1),
        Change::State(2, 1),
        Change::State(3, 9),
        Change::Visible(3, false),
        Change::Text(2, "&Second".to_string()),
    ];
    answer(&reply, changes, false);
    app.pump_plugins();
    let form = shown_form(&app);
    assert_eq!(form.tristate(0), Some(true));
    assert!(
        !form.is_checked(1) && form.is_checked(2),
        "one radio button of the group"
    );
    assert_eq!(form.chosen(3), 1, "past the end: the last line");
    assert!(form.items[3].hidden);
    assert!(matches!(&form.items[2].control, Control::Radio { label, .. } if label == "&Second"));
}

#[test]
fn esc_again_while_the_handler_has_not_answered_takes_the_dialog_away() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    key(&mut app, KeyCode::Enter);
    let (kind, _, _reply) = event(&heard);
    assert_eq!(kind, DialogEventKind::Close(Some(3)));
    key(&mut app, KeyCode::Enter);
    shown_form(&app);
    key(&mut app, KeyCode::Esc);
    assert!(app.overlay.is_none() && app.plugins.dialog.is_none());
    assert_eq!(
        heard.recv_timeout(Duration::from_secs(1)).err(),
        Some(RecvTimeoutError::Disconnected),
        "the plugin hears Esc when it comes back"
    );
}

#[test]
fn the_values_closed_with_are_those_the_handler_approved_with_its_last_changes() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    key(&mut app, KeyCode::Enter);
    let (_, values, reply) = event(&heard);
    assert_eq!(values[1].text, "start");
    // Typed while the handler decides: neither heard nor handed back.
    key(&mut app, KeyCode::Char('x'));
    nothing_more(&heard);
    answer(&reply, vec![Change::Text(1, "trimmed".to_string())], false);
    app.pump_plugins();
    assert!(app.overlay.is_none());
    match heard.recv_timeout(Duration::from_secs(1)) {
        Ok(DialogMessage::Closed { button, values }) => {
            assert_eq!(button, Some(3));
            assert_eq!(values[1].text, "trimmed");
        }
        other => panic!("closed: {other:?}"),
    }
}

#[test]
fn the_dialog_is_back_at_once_when_the_window_over_it_is_answered() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    let (reply, _answered) = channel();
    app.plugins.host.send_event(Event::Ask {
        plugin: "probe".to_string(),
        request: Request::Message(MessageRequest {
            buttons: vec!["OK".to_string()],
            ..MessageRequest::default()
        }),
        reply,
    });
    app.pump_plugins();
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    shown_form(&app);
    app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
    assert_eq!(
        shown_form(&app).edit_text(1),
        "starty",
        "the next key is the dialog's"
    );
}

#[test]
fn a_plugins_thread_no_longer_in_the_dialog_takes_it_away_on_its_next_event() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    drop(heard);
    key(&mut app, KeyCode::Enter);
    assert!(app.overlay.is_none() && app.plugins.dialog.is_none());
}

#[test]
fn what_the_user_did_while_closing_is_told_once_the_dialog_is_kept() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    key(&mut app, KeyCode::Enter);
    let (_, _, reply) = event(&heard);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Char(' '));
    nothing_more(&heard);
    answer(&reply, vec![Change::Text(1, "kept".to_string())], true);
    app.pump_plugins();
    app.pump_plugins();
    let (kind, values, reply) = event(&heard);
    assert_eq!(
        kind,
        DialogEventKind::Changed(2),
        "the box ticked meanwhile"
    );
    assert_eq!((values[1].text.as_str(), values[2].state), ("kept", 1));
    answer(&reply, Vec::new(), false);
    focus_moved(&heard, 1, 2);
    nothing_more(&heard);
}

#[path = "dialog_more_tests.rs"]
mod more;

#[path = "dialog_input_tests.rs"]
mod input_tests;

#[path = "dialog_list_tests.rs"]
mod list_tests;
