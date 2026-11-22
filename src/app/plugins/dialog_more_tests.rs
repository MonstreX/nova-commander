//! More of a plugin's dialog in the program: every kind's value and change both ways, answers taken in
//! the order their events went, a dialog without a handler, a plugin gone before its dialog opens, what
//! was typed before a button told first (and what was typed while closing never handed back), the dialog
//! aside while its plugin's window is up and never over another window, one dialog at a time.

use super::*;

/// One of each kind: [?] A, [x] B, (•) C, ( ) D, a list on Green, an edit line, a password line, a
/// separator, a text and the buttons OK and Go.
fn every_kind() -> Vec<DialogItem> {
    let with = |kind, text: &str, state, change: fn(&mut DialogItem)| {
        let mut item = DialogItem {
            state,
            ..item(kind, text)
        };
        change(&mut item);
        item
    };
    vec![
        with(ItemKind::Check, "&A", 2, |item| item.three_state = true),
        with(ItemKind::Check, "&B", 1, |_| {}),
        with(ItemKind::Radio, "&C", 1, |item| item.group = true),
        with(ItemKind::Radio, "&D", 0, |_| {}),
        with(ItemKind::List, "", 1, |item| {
            item.lines = vec!["Red".to_string(), "Green".to_string()];
        }),
        item(ItemKind::Edit, "e"),
        item(ItemKind::Password, "p"),
        item(ItemKind::Separator, ""),
        item(ItemKind::Text, "t"),
        item(ItemKind::Button, "OK"),
        item(ItemKind::Button, "Go"),
    ]
}

/// Esc in the dialog: the closing event's values and where its answer goes.
fn closing_values(
    app: &mut App,
    heard: &Receiver<DialogMessage>,
) -> (Vec<ItemValue>, Sender<Handled>) {
    key(app, KeyCode::Esc);
    let (kind, values, reply) = event(heard);
    assert_eq!(kind, DialogEventKind::Close(None));
    (values, reply)
}

#[test]
fn every_kinds_value_and_change_go_both_ways() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, every_kind(), true);
    let (_, values, reply) = event(&heard);
    assert_eq!(
        states(&values),
        [
            (2, "&A"),
            (1, "&B"),
            (1, "&C"),
            (0, "&D"),
            (1, "Green"),
            (0, "e"),
            (0, "p"),
            (0, ""),
            (0, "t"),
            (0, "OK"),
            (0, "Go")
        ]
    );
    let changes = vec![
        Change::State(0, 0),
        Change::State(1, 0),
        Change::State(3, 1),
        Change::State(4, -5),
        Change::Text(0, "&Ay".to_string()),
        Change::Text(1, "&Bee".to_string()),
        Change::Text(6, "q".to_string()),
        Change::Text(9, "&Yes".to_string()),
        Change::Enabled(10, false),
        Change::Visible(8, false),
    ];
    answer(&reply, changes, false);
    app.pump_plugins();
    nothing_more(&heard);
    let (values, reply) = closing_values(&mut app, &heard);
    assert_eq!(
        states(&values),
        [
            (0, "&Ay"),
            (0, "&Bee"),
            (0, "&C"),
            (1, "&D"),
            (0, "Red"),
            (0, "e"),
            (0, "q"),
            (0, ""),
            (0, "t"),
            (0, "&Yes"),
            (0, "Go")
        ]
    );
    let changes = vec![
        Change::State(0, 1),
        Change::State(1, 1),
        Change::State(3, 0),
        Change::Enabled(10, true),
        Change::Visible(8, true),
    ];
    answer(&reply, changes, true);
    app.pump_plugins();
    let form = shown_form(&app);
    let go = laid_item(&app, 10);
    let text = laid_item(&app, 8);
    assert!(!form.items[go].disabled && !form.items[text].hidden, "back");
    nothing_more(&heard);
    let (values, _reply) = closing_values(&mut app, &heard);
    let three: Vec<(i32, &str)> = states(&values)[..4].to_vec();
    assert_eq!(three, [(1, "&Ay"), (1, "&Bee"), (0, "&C"), (0, "&D")]);
}

/// The form's item showing the plugin's item `item`.
fn laid_item(app: &App, item: usize) -> usize {
    app.plugins
        .dialog
        .as_ref()
        .map_or(usize::MAX, |session| session.items[item])
}

#[test]
fn a_three_state_box_set_to_its_third_state_is_told_as_two() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, every_kind(), true);
    let (_, _, reply) = event(&heard);
    answer(
        &reply,
        vec![Change::State(0, 1), Change::State(0, 2)],
        false,
    );
    app.pump_plugins();
    assert_eq!(shown_form(&app).tristate(0), None);
    let (values, _reply) = closing_values(&mut app, &heard);
    assert_eq!(values[0].state, 2);
}

#[test]
fn answers_are_taken_in_the_order_their_events_went() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    key(&mut app, KeyCode::Char('a'));
    let (_, _, first) = event(&heard);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Char(' '));
    focus_moved(&heard, 1, 2);
    let (kind, _, second) = event(&heard);
    assert_eq!(kind, DialogEventKind::Changed(2));
    answer(&second, vec![Change::Text(1, "second".to_string())], false);
    app.pump_plugins();
    assert_eq!(
        shown_form(&app).edit_text(1),
        "starta",
        "the first not answered yet"
    );
    answer(&first, vec![Change::Text(1, "first".to_string())], false);
    app.pump_plugins();
    assert_eq!(shown_form(&app).edit_text(1), "second", "both, in order");
    nothing_more(&heard);
}

#[test]
fn without_a_handler_a_button_that_does_not_close_keeps_the_dialog() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), false);
    press(&mut app, KeyCode::Char('f'), KeyModifiers::ALT);
    shown_form(&app);
    nothing_more(&heard);
    assert!(app.plugins.dialog.is_some());
}

#[test]
fn a_dialog_whose_plugin_is_gone_before_it_opens_is_not_shown() {
    let (_temp, mut app) = app();
    let (talk, heard) = channel();
    drop(heard);
    app.plugins.host.send_event(Event::Dialog {
        plugin: "probe".to_string(),
        request: DialogRequest {
            items: items(),
            handled: true,
            ..DialogRequest::default()
        },
        talk,
    });
    app.pump_plugins();
    assert!(app.overlay.is_none() && app.plugins.dialog.is_none());
}

#[test]
fn what_was_typed_before_a_button_is_told_before_it() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let (kind, values, _) = event(&heard);
    assert_eq!(
        (kind, values[1].text.as_str()),
        (DialogEventKind::Changed(1), "startx")
    );
    let (kind, _, _) = event(&heard);
    assert_eq!(kind, DialogEventKind::Close(Some(3)));
}

#[test]
fn the_dialog_stays_aside_while_its_plugins_window_is_up() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    let (reply, answered) = channel();
    app.plugins.host.send_event(Event::Ask {
        plugin: "probe".to_string(),
        request: Request::Message(MessageRequest {
            buttons: vec!["OK".to_string()],
            ..MessageRequest::default()
        }),
        reply,
    });
    app.pump_plugins();
    for _ in 0..2 {
        assert!(!app.pump_plugins(), "nothing new");
        assert!(matches!(
            &app.overlay,
            Some(Overlay::Dialog(Dialog::Form {
                action: FormAction::Plugin(PluginForm::Message { .. }),
                ..
            }))
        ));
    }
    assert!(answered.try_recv().is_err(), "not answered");
    nothing_more(&heard);
}

#[test]
fn what_was_typed_while_closing_is_not_handed_back() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    key(&mut app, KeyCode::Enter);
    let (_, _, reply) = event(&heard);
    key(&mut app, KeyCode::Char('x'));
    answer(&reply, Vec::new(), false);
    app.pump_plugins();
    match heard.recv_timeout(Duration::from_secs(1)) {
        Ok(DialogMessage::Closed { values, .. }) => {
            assert_eq!(values[1].text, "start", "as the handler approved it");
        }
        other => panic!("closed: {other:?}"),
    }
}

#[test]
fn the_dialog_aside_never_takes_the_place_of_another_window() {
    let (_temp, mut app) = app();
    let heard = open(&mut app, items(), true);
    skip(&heard);
    let (reply, answered) = channel();
    app.plugins.host.send_event(Event::Ask {
        plugin: "probe".to_string(),
        request: Request::Message(MessageRequest {
            buttons: vec!["OK".to_string()],
            ..MessageRequest::default()
        }),
        reply,
    });
    app.pump_plugins();
    // The program's own window takes the message's place: the plugin hears Esc, its dialog stays aside.
    app.overlay = Some(Overlay::Dialog(Dialog::error(
        "Error",
        vec!["x".to_string()],
    )));
    app.pump_plugins();
    assert_eq!(answered.try_recv(), Ok(Answer::Esc));
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Dialog(Dialog::Message { error: true, .. }))
    ));
    assert!(app.plugins.dialog.as_ref().unwrap().hidden.is_some());
    app.overlay = None;
    assert!(app.pump_plugins(), "back once that window is gone");
    shown_form(&app);
    nothing_more(&heard);
}

#[test]
fn a_dialog_opened_while_one_is_up_is_esc_and_leaves_the_first() {
    let (_temp, mut app) = app();
    let first = open(&mut app, items(), true);
    skip(&first);
    let (talk, second) = channel();
    let request = DialogRequest {
        title: "Second".to_string(),
        ..DialogRequest::default()
    };
    app.open_plugin_dialog("other".to_string(), request, talk);
    assert_eq!(
        second.recv_timeout(Duration::from_secs(1)).err(),
        Some(RecvTimeoutError::Disconnected)
    );
    assert_eq!(shown_form(&app).title, "Dialog");
    assert_eq!(app.plugins.dialog.as_ref().unwrap().plugin, "probe");
    nothing_more(&first);
}

/// FAR's nested modal dialogs (E14): a dialog its handler asks for comes over the first, which waits
/// aside, and comes back in front once the second is gone.
#[test]
fn a_dialog_its_handler_opens_comes_over_it_and_the_first_comes_back() {
    let (_temp, mut app) = app();
    let first = open(&mut app, items(), true);
    skip(&first);
    // "Fill" heard: the handler, still in that event, asks for a second dialog.
    press(&mut app, KeyCode::Char('f'), KeyModifiers::ALT);
    let (kind, _, fill_reply) = loop {
        let heard = event(&first);
        if matches!(heard.0, DialogEventKind::Button(_)) {
            break heard;
        }
        answer(&heard.2, Vec::new(), false);
    };
    assert_eq!(kind, DialogEventKind::Button(4));
    let second = {
        let (talk, heard) = channel();
        app.plugins.host.send_event(Event::Dialog {
            plugin: "probe".to_string(),
            request: DialogRequest {
                title: "Second".to_string(),
                items: vec![item(ItemKind::Text, "Inner"), item(ItemKind::Button, "OK")],
                handled: true,
                ..DialogRequest::default()
            },
            talk,
        });
        app.pump_plugins();
        heard
    };
    assert_eq!(shown_form(&app).title, "Second", "over the first");
    assert_eq!(app.plugins.outer_dialogs.len(), 1);
    assert!(
        app.plugins.outer_dialogs[0].hidden.is_some(),
        "the first aside"
    );
    skip(&second);
    key(&mut app, KeyCode::Esc);
    let (kind, _, reply) = event(&second);
    assert_eq!(kind, DialogEventKind::Close(None));
    answer(&reply, Vec::new(), false);
    app.pump_plugins();
    assert!(matches!(
        second.recv_timeout(Duration::from_secs(1)),
        Ok(DialogMessage::Closed { button: None, .. })
    ));
    assert_eq!(
        shown_form(&app).title,
        "Dialog",
        "the first back on the same tick"
    );
    assert!(app.plugins.outer_dialogs.is_empty());
    // The first's handler goes on: its answer to "Fill" is applied there.
    answer(
        &fill_reply,
        vec![Change::Text(1, "filled".to_string())],
        false,
    );
    app.pump_plugins();
    assert_eq!(shown_form(&app).edit_text(laid_item(&app, 1)), "filled");
}

/// The dialog of `title` its plugin "probe" asks for now (its handler's), shown: what its thread hears.
fn nest(app: &mut App, title: &str) -> Receiver<DialogMessage> {
    let (talk, heard) = channel();
    app.plugins.host.send_event(Event::Dialog {
        plugin: "probe".to_string(),
        request: DialogRequest {
            title: title.to_string(),
            items: vec![item(ItemKind::Button, "OK")],
            handled: true,
            ..DialogRequest::default()
        },
        talk,
    });
    app.pump_plugins();
    heard
}

#[test]
fn nested_dialogs_go_back_in_order_and_another_plugins_window_waits() {
    let (_temp, mut app) = app();
    let first = open(&mut app, items(), true);
    skip(&first);
    let second = nest(&mut app, "Second");
    skip(&second);
    let third = nest(&mut app, "Third");
    skip(&third);
    assert_eq!(app.plugins.outer_dialogs.len(), 2, "two under it");
    // Another plugin's message waits behind them all.
    let (reply, answered) = channel();
    app.plugins.host.send_event(Event::Ask {
        plugin: "other".to_string(),
        request: Request::Message(MessageRequest::default()),
        reply,
    });
    app.pump_plugins();
    for (heard, under) in [(&third, "Second"), (&second, "Dialog")] {
        key(&mut app, KeyCode::Esc);
        let (_, _, reply) = event(heard);
        answer(&reply, Vec::new(), false);
        app.pump_plugins();
        assert_eq!(shown_form(&app).title, under);
        assert!(
            answered.try_recv().is_err(),
            "the other plugin's window waits"
        );
    }
    assert!(app.plugins.outer_dialogs.is_empty());
}

#[test]
fn a_nested_dialog_whose_thread_is_gone_gives_the_first_back() {
    let (_temp, mut app) = app();
    let first = open(&mut app, items(), true);
    skip(&first);
    let (talk, heard) = channel();
    drop(heard);
    app.plugins.host.send_event(Event::Dialog {
        plugin: "probe".to_string(),
        request: DialogRequest {
            handled: true,
            ..DialogRequest::default()
        },
        talk,
    });
    app.pump_plugins();
    assert_eq!(
        shown_form(&app).title,
        "Dialog",
        "its Init not heard: the first back"
    );
    assert!(app.plugins.outer_dialogs.is_empty());
}

#[test]
fn a_button_closing_the_dialog_adds_its_lines_to_their_histories() {
    let (_temp, mut app) = app();
    let mut line = item(ItemKind::Edit, "typed");
    line.history = Some("ProbeNames".to_string());
    let items = || vec![line.clone(), item(ItemKind::Button, "OK")];
    let heard = open(&mut app, items(), false);
    key(&mut app, KeyCode::Esc);
    assert!(
        app.history.lists.dialog("ProbeNames").is_empty(),
        "Esc adds none"
    );
    drop(heard);
    let heard = open(&mut app, items(), false);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Enter);
    let kept: Vec<&str> = (app.history.lists.dialog("ProbeNames").iter())
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(kept, ["typed"], "FAR's CloseDialog with an exit code");
    assert!(matches!(
        heard.recv_timeout(Duration::from_secs(1)),
        Ok(DialogMessage::Closed {
            button: Some(1),
            ..
        })
    ));
}
