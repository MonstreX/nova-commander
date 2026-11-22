//! The windows the probe asks for with `windows` in its folder (ADR-0016 5b-1), through the host: what
//! reaches the program and what the probe hears back (it tells the answer in a message and its log).

#![cfg(windows)]

use super::tests::{load, next_event, probe_with_markers};
use super::windows::{
    Change, DialogEventKind, DialogMessage, Handled, InputRequest, ItemValue, MenuLine, MenuRequest,
};
use super::*;
use std::time::Duration;

/// The probe with its window commands; each test runs one of them.
fn host() -> (tempfile::TempDir, PluginHost) {
    let folder = probe_with_markers(&["windows"]);
    let host = load(folder.path(), &|_| true);
    (folder, host)
}

/// The next event as a request the plugin waits on, and the reply.
fn next_request(host: &PluginHost) -> (Request, Sender<Answer>) {
    match next_event(host) {
        Event::Ask { request, reply, .. } => (request, reply),
        other => panic!("a request: {other:?}"),
    }
}

/// The probe's "Answer" message: what it heard, OK sent.
fn told(host: &PluginHost) -> String {
    let (request, reply) = next_request(host);
    let Request::Message(message) = request else {
        panic!("the answer: {request:?}");
    };
    assert_eq!(message.title, "Answer");
    reply.send(Answer::Button(0)).unwrap();
    message.lines.join("\n")
}

/// The probe's log once its calls so far have ended (a failing command after them).
fn log_after(host: &PluginHost, folder: &Path) -> String {
    host.run_command(0, 1, 0, 0);
    let Event::Failed { .. } = next_event(host) else {
        panic!("the failure");
    };
    std::fs::read_to_string(folder.join("Logs").join("probe.log")).unwrap_or_default()
}

#[test]
fn the_window_commands_are_there_only_with_the_marker() {
    let (_folder, host) = host();
    assert_eq!(
        host.plugins[0].commands,
        [
            "Say hello",
            "Fail",
            "Panic",
            "Ask",
            "Choose",
            "Type",
            "Work",
            "Log",
            "Dialog",
            "Error",
            "Options"
        ]
    );
}

#[test]
fn a_message_with_buttons_returns_the_one_pressed() {
    let (folder, host) = host();
    host.run_command(0, 3, 0, 0);
    let (request, reply) = next_request(&host);
    let Request::Message(message) = request else {
        panic!("a message");
    };
    assert_eq!(message.title, "Question");
    assert_eq!(message.lines, ["Go on?", "Really"]);
    assert_eq!(message.buttons, ["&Yes", "&No", "&Cancel"]);
    assert!(message.warning && !message.left_align);
    reply.send(Answer::Button(2)).unwrap();
    assert_eq!(told(&host), "Some(2)");
    host.run_command(0, 3, 0, 0);
    let (_, reply) = next_request(&host);
    reply.send(Answer::Esc).unwrap();
    assert_eq!(told(&host), "None");
    let log = log_after(&host, folder.path());
    assert!(
        log.contains(" Some(2)\n") && log.contains(" None\n"),
        "{log}"
    );
}

#[test]
fn a_menu_returns_the_line_and_the_break_key() {
    let (_folder, host) = host();
    host.run_command(0, 4, 0, 0);
    let (request, reply) = next_request(&host);
    let Request::Menu(menu) = request else {
        panic!("a menu");
    };
    let line = |text: &str, separator, checked, disabled| MenuLine {
        text: text.to_string(),
        separator,
        checked,
        disabled,
        ..MenuLine::default()
    };
    assert_eq!(
        menu,
        MenuRequest {
            title: "Probe menu".to_string(),
            bottom: "F4 Shift+Del".to_string(),
            lines: vec![
                line("First", false, false, false),
                line("", true, false, false),
                line("Grey", false, false, true),
                line("Third", false, true, false),
            ],
            selected: 3,
            break_keys: vec!["F4".to_string(), "Shift+Del".to_string()],
            auto_hotkeys: true,
            ..MenuRequest::default()
        }
    );
    reply
        .send(Answer::Chosen {
            item: Some(0),
            key: Some(1),
        })
        .unwrap();
    assert_eq!(told(&host), "Some(Chosen { item: Some(0), key: Some(1) })");
    for (answer, heard) in [
        (
            Answer::Chosen {
                item: Some(3),
                key: None,
            },
            "Some(Chosen { item: Some(3), key: None })",
        ),
        (Answer::Esc, "None"),
    ] {
        host.run_command(0, 4, 0, 0);
        let (_, reply) = next_request(&host);
        reply.send(answer).unwrap();
        assert_eq!(told(&host), heard);
    }
}

#[test]
fn an_input_line_returns_the_text() {
    let (_folder, host) = host();
    host.run_command(0, 5, 0, 0);
    let (request, reply) = next_request(&host);
    assert_eq!(
        request,
        Request::Input(InputRequest {
            title: "Probe".to_string(),
            prompt: "Type something:".to_string(),
            text: "start".to_string(),
            ..InputRequest::default()
        })
    );
    reply.send(Answer::Text("typed".to_string())).unwrap();
    assert_eq!(told(&host), "Some(\"typed\")");
    host.run_command(0, 5, 0, 0);
    let (_, reply) = next_request(&host);
    drop(reply);
    assert_eq!(told(&host), "None", "the window gone without an answer");
}

#[test]
fn a_window_without_buttons_is_shown_until_its_call_ends() {
    let (folder, host) = host();
    host.run_command(0, 6, 0, 0);
    let Event::Busy {
        plugin, message, ..
    } = next_event(&host)
    else {
        panic!("a window without buttons");
    };
    assert_eq!(plugin, "probe");
    assert_eq!(message.lines, ["Working..."]);
    assert!(message.buttons.is_empty());
    let Event::Done { plugin } = next_event(&host) else {
        panic!("its end");
    };
    assert_eq!(plugin, "probe");
    let log = log_after(&host, folder.path());
    assert!(log.ends_with(" worked\n"), "{log}");
}

#[test]
fn a_call_without_such_a_window_ends_unreported() {
    let (_folder, host) = host();
    host.run_command(0, 1, 0, 0);
    host.run_command(0, 1, 0, 0);
    for _ in 0..2 {
        let event = next_event(&host);
        assert!(matches!(event, Event::Failed { .. }), "{event:?}");
    }
    host.run_command(0, 3, 0, 0);
    let (_, reply) = next_request(&host);
    reply.send(Answer::Esc).unwrap();
    told(&host);
    host.run_command(0, 1, 0, 0);
    let event = next_event(&host);
    assert!(
        matches!(event, Event::Failed { .. }),
        "no end reported for a call with windows that waited: {event:?}"
    );
}

#[test]
fn the_log_has_a_line_for_each_line_written() {
    let (folder, host) = host();
    host.run_command(0, 7, 0, 0);
    let log = log_after(&host, folder.path());
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(lines[0].ends_with(" first line") && lines[1].ends_with(" second line"));
}

#[test]
fn the_end_of_a_window_without_buttons_is_told_once() {
    let (_folder, host) = host();
    host.run_command(0, 6, 0, 0);
    let Event::Busy { .. } = next_event(&host) else {
        panic!("a window without buttons");
    };
    let Event::Done { .. } = next_event(&host) else {
        panic!("its end");
    };
    // The calls after it showed none: their ends are not told.
    for _ in 0..2 {
        host.run_command(0, 1, 0, 0);
        let event = next_event(&host);
        assert!(matches!(event, Event::Failed { .. }), "{event:?}");
    }
}

/// An event sent into the probe's dialog and its handler's answer.
fn dialog_event(
    talk: &Sender<DialogMessage>,
    kind: DialogEventKind,
    values: Vec<ItemValue>,
) -> std::sync::mpsc::Receiver<Handled> {
    let (reply, answer) = std::sync::mpsc::channel();
    talk.send(DialogMessage::Event {
        kind,
        values,
        reply,
    })
    .unwrap();
    answer
}

/// The probe dialog's values with the name `name` and "Upper case" `upper`.
fn probe_values(name: &str, upper: i32) -> Vec<ItemValue> {
    let mut values = vec![ItemValue::default(); 11];
    values[1].text = name.to_string();
    values[2].state = upper;
    values[4].text = "secret".to_string();
    values[5].state = 1;
    values[7].text = "Green".to_string();
    values
}

#[test]
fn the_probes_dialog_hears_its_events_and_tells_its_end() {
    let (folder, host) = host();
    host.run_command(0, 8, 0, 0);
    let Event::Dialog { request, talk, .. } = next_event(&host) else {
        panic!("a dialog");
    };
    assert_eq!(request.title, "Probe dialog");
    assert!(request.handled);
    assert_eq!(request.items.len(), 11);
    assert!(request.items[4].hidden && request.items[9].no_close && request.items[8].default);
    let wait = |answer: std::sync::mpsc::Receiver<Handled>| {
        answer.recv_timeout(Duration::from_secs(10)).unwrap()
    };
    let init = dialog_event(&talk, DialogEventKind::Init, probe_values("start", 0));
    assert_eq!(wait(init), Handled::default());
    let upper = dialog_event(&talk, DialogEventKind::Changed(2), probe_values("abc", 1));
    assert_eq!(wait(upper).changes, [Change::Text(1, "ABC".to_string())]);
    let fill = dialog_event(&talk, DialogEventKind::Button(9), probe_values("abc", 1));
    assert_eq!(
        wait(fill).changes,
        [
            Change::Text(1, "filled".to_string()),
            Change::Enabled(7, false)
        ]
    );
    // OK with no name: the handler's message, then the dialog kept.
    let close = dialog_event(&talk, DialogEventKind::Close(Some(8)), probe_values("", 1));
    let (request, reply) = next_request(&host);
    let Request::Message(message) = request else {
        panic!("the handler's message");
    };
    assert_eq!(message.lines, ["The name is empty"]);
    reply.send(Answer::Button(0)).unwrap();
    let kept = wait(close);
    assert!(kept.keep_open);
    assert_eq!(kept.changes, [Change::Focus(1)]);
    talk.send(DialogMessage::Closed {
        button: Some(8),
        values: probe_values("abc", 1),
    })
    .unwrap();
    assert_eq!(
        told(&host),
        "Some(8) name=abc upper=1 code=secret radio=1 list=Green"
    );
    let log = log_after(&host, folder.path());
    let events: Vec<&str> = log
        .lines()
        .filter_map(|line| line.split_once(' ').map(|(_, rest)| rest))
        .collect();
    assert!(events.iter().any(|line| line.ends_with("Init")), "{log}");
    assert!(
        events.iter().any(|line| line.ends_with("Close(Some(8))")),
        "{log}"
    );
}

#[test]
fn the_probes_dialog_gone_is_esc_for_it() {
    let (_folder, host) = host();
    host.run_command(0, 8, 0, 0);
    let Event::Dialog { talk, .. } = next_event(&host) else {
        panic!("a dialog");
    };
    drop(talk);
    assert_eq!(told(&host), "None name= upper=0 code= radio=0 list=");
}
