//! The SDK's dialog against a host faked in this process: what it hands the host, how the handler hears
//! the host's events and asks for changes, how the end is read.

use std::sync::Mutex;

use super::*;
use std::ffi::c_void;

use crate::Host;
use crate::api::*;

/// The fake host: the events it sends the handler, the values it shows with them, how the dialog ends,
/// and what it heard.
#[derive(Default)]
struct Fake {
    events: Vec<(u32, i32)>,
    values: Vec<(i32, &'static str)>,
    button: i32,
    heard: Mutex<Vec<String>>,
    /// The handler's answer to each event.
    answers: Mutex<Vec<i32>>,
    /// The dialog's size and the room for values each call handed.
    calls: Mutex<Vec<(usize, usize)>>,
    /// The events sent with their values at null.
    null_values: bool,
}

/// # Safety
/// `context` is a live `Fake`.
unsafe fn fake<'a>(context: *mut c_void) -> &'a Fake {
    // SAFETY: as the caller promises.
    unsafe { &*context.cast::<Fake>() }
}

fn note(context: *mut c_void, line: String) {
    // SAFETY: the tests' context is a live `Fake`.
    unsafe { fake(context) }.heard.lock().unwrap().push(line);
}

unsafe extern "C" fn dialog(
    context: *mut c_void,
    dialog: *const NcDialog,
    values: *mut NcDialogValue,
    count: usize,
) -> i32 {
    // SAFETY: the tests' context is a live `Fake`; the plugin's dialog, its items and lines live through
    // the call; `values` has room for `count`.
    unsafe {
        let fake = fake(context);
        let dialog = &*dialog;
        fake.calls.lock().unwrap().push((dialog.size, count));
        note(
            context,
            format!(
                "dialog {} flags {} handler {}",
                dialog.title.as_str(),
                dialog.flags,
                dialog.handler.is_some()
            ),
        );
        if dialog.width > 0 || dialog.x >= 0 {
            let (x, y, width, height) = (dialog.x, dialog.y, dialog.width, dialog.height);
            note(context, format!("placed {x} {y} {width} {height}"));
        }
        for item in std::slice::from_raw_parts(dialog.items, dialog.count) {
            let lines: Vec<&str> = std::slice::from_raw_parts(item.lines, item.line_count)
                .iter()
                .map(|line| line.as_str())
                .collect();
            let line = format!(
                "item {} {} [{}] {} {} {:?}",
                item.kind,
                item.flags,
                item.text.as_str(),
                item.state,
                item.width,
                lines
            );
            note(context, line);
            let more = (item.history.as_str(), item.mask.as_str(), item.height);
            let place = (item.x1, item.y1, item.x2, item.y2);
            if more != ("", "", 0) || place != (0, 0, 0, 0) {
                note(context, format!("more {more:?} {place:?}"));
            }
        }
        let shown: Vec<NcDialogValue> = (fake.values.iter())
            .map(|(state, text)| NcDialogValue {
                state: *state,
                text: NcStr::new(text),
            })
            .collect();
        if let Some(handler) = dialog.handler {
            for &(kind, item) in &fake.events {
                let event = NcDialogEvent {
                    kind,
                    item,
                    values: if fake.null_values {
                        std::ptr::null()
                    } else {
                        shown.as_ptr()
                    },
                    count: shown.len(),
                    key: NcStr::new("Ctrl+Enter"),
                    x: 3,
                    y: 4,
                };
                let answer = handler(dialog.handler_state, context, &raw const event);
                fake.answers.lock().unwrap().push(answer);
            }
        }
        for (index, value) in shown.iter().enumerate().take(count) {
            values.add(index).write(*value);
        }
        fake.button
    }
}

unsafe extern "C" fn dialog_change(
    context: *mut c_void,
    handle: *mut c_void,
    change: *const NcDialogChange,
) -> i32 {
    // SAFETY: the plugin's change lives through the call.
    let change = unsafe { &*change };
    assert_eq!(handle, context, "the handle the host gave");
    // SAFETY: the plugin's text and lines live through the call.
    let (text, lines) = unsafe {
        let lines = match change.line_count {
            0 => &[][..],
            count => std::slice::from_raw_parts(change.lines, count),
        };
        let lines: Vec<&str> = lines.iter().map(|line| line.as_str()).collect();
        (change.text.as_str(), lines)
    };
    let lines = if lines.is_empty() {
        String::new()
    } else {
        format!(" {lines:?}")
    };
    note(
        context,
        format!(
            "change {} {} {} [{text}]{lines}",
            change.item, change.what, change.state
        ),
    );
    if change.item < 10 { 0 } else { -1 }
}

unsafe extern "C" fn log(context: *mut c_void, text: NcStr) {
    // SAFETY: the plugin's text lives through the call.
    note(context, format!("log [{}]", unsafe { text.as_str() }));
}

/// `act` on a host faked by `fake`: what it returned, what the host heard and the handler's answers.
fn with_host<T>(fake: Fake, act: impl FnOnce(Host) -> T) -> (T, Vec<String>, Vec<i32>) {
    let table = NcHost {
        dialog,
        dialog_change,
        log,
        context: (&raw const fake).cast_mut().cast(),
        ..crate::tests::host(api::API_VERSION)
    };
    let result = act(Host {
        table: &raw const table,
    });
    let answers = fake.answers.into_inner().unwrap();
    (result, fake.heard.into_inner().unwrap(), answers)
}

fn probe_dialog() -> Dialog {
    Dialog::new(
        "Title",
        vec![
            DialogItem::text("&Name:").hidden(true),
            DialogItem::edit("start").width(20).focus().same_row(),
            DialogItem::password("pw").disabled(true),
            DialogItem::check("&On", true),
            DialogItem::check3("&Three", None),
            DialogItem::radio("&R", false).group(),
            DialogItem::list(&["Red", "Green"], 1),
            DialogItem::separator(),
            DialogItem::button("OK").default_button(),
            DialogItem::button("&Fill").no_close(),
        ],
    )
}

#[test]
fn a_dialog_hands_its_items_as_built_and_no_handler_without_one() {
    let fake = Fake {
        button: 8,
        ..Fake::default()
    };
    let (closed, heard, answers) = with_host(fake, |host| host.dialog(&probe_dialog().warning()));
    assert_eq!(closed.button, Some(8));
    assert!(answers.is_empty());
    assert_eq!(
        heard,
        [
            "dialog Title flags 1 handler false",
            "item 0 2 [&Name:] 0 0 []",
            "item 1 132 [start] 0 20 []",
            "item 2 1 [pw] 0 0 []",
            "item 3 0 [&On] 1 0 []",
            "item 3 64 [&Three] 2 0 []",
            "item 4 8 [&R] 0 0 []",
            "item 5 0 [] 1 0 [\"Red\", \"Green\"]",
            "item 6 0 [] 0 0 []",
            "item 7 16 [OK] 0 0 []",
            "item 7 32 [&Fill] 0 0 []",
        ],
        "hidden 2, same row 4, focus 128, disabled 1, three states 64, group 8, default 16, no close 32"
    );
    assert_eq!(
        DialogItem::check3("x", Some(true)).state,
        1,
        "[x] on, [ ] off, [?] 2"
    );
    assert_eq!(DialogItem::check3("x", Some(false)).state, 0);
}

#[test]
fn the_end_is_the_button_and_every_items_last_value() {
    let fake = Fake {
        values: vec![(0, "a"), (1, "b")],
        button: -1,
        ..Fake::default()
    };
    let (closed, _, _) = with_host(fake, |host| host.dialog(&probe_dialog()));
    assert_eq!(closed.button, None, "Esc");
    assert_eq!(closed.values.len(), 10, "one for each item");
    assert_eq!(
        closed.values[..2],
        [
            Value {
                state: 0,
                text: "a".to_string()
            },
            Value {
                state: 1,
                text: "b".to_string()
            }
        ]
    );
    assert!(closed.values[1].checked() && !closed.values[0].checked());
    assert_eq!(
        closed.values[9],
        Value::default(),
        "what the host did not write"
    );
}

#[test]
fn the_handler_hears_each_event_and_keeps_a_closing_dialog_open_with_false() {
    let fake = Fake {
        events: vec![
            (api::EVENT_INIT, -1),
            (api::EVENT_CHANGED, 2),
            (api::EVENT_BUTTON, 9),
            (api::EVENT_CLOSE, 8),
            (api::EVENT_CLOSE, -1),
            (99, 0),
        ],
        values: vec![(0, "x")],
        ..Fake::default()
    };
    let mut heard_events = Vec::new();
    let (_, heard, answers) = with_host(fake, |host| {
        host.dialog_with(&probe_dialog(), |event, controls| {
            heard_events.push((event, controls.value(0).text));
            false
        })
    });
    assert_eq!(heard[0], "dialog Title flags 0 handler true");
    let events: Vec<DialogEvent> = heard_events
        .iter()
        .map(|(event, _)| event.clone())
        .collect();
    assert_eq!(
        events,
        [
            DialogEvent::Init,
            DialogEvent::Changed(2),
            DialogEvent::Button(9),
            DialogEvent::Close(Some(8)),
            DialogEvent::Close(None),
        ],
        "an event not known is not heard"
    );
    assert!(
        heard_events.iter().all(|(_, text)| text == "x"),
        "the values sent"
    );
    assert_eq!(
        answers,
        [0, 0, 0, 1, 1, 0],
        "false keeps only a closing dialog"
    );
}

#[test]
fn true_lets_a_dialog_close() {
    let fake = Fake {
        events: vec![(api::EVENT_CLOSE, 8)],
        ..Fake::default()
    };
    let (_, _, answers) = with_host(fake, |host| host.dialog_with(&probe_dialog(), |_, _| true));
    assert_eq!(answers, [0]);
}

#[test]
fn the_handlers_changes_go_to_the_host() {
    let fake = Fake {
        events: vec![(api::EVENT_INIT, -1)],
        values: vec![(1, "v")],
        ..Fake::default()
    };
    let mut taken = Vec::new();
    let (_, heard, _) = with_host(fake, |host| {
        host.dialog_with(&probe_dialog(), |_, controls| {
            assert_eq!(controls.value(0).state, 1);
            assert_eq!(controls.value(5), Value::default(), "no such value");
            taken = vec![
                controls.set_text(1, "new"),
                controls.set_state(6, 0),
                controls.set_checked(3, false),
                controls.set_checked(3, true),
                controls.enable(2, true),
                controls.enable(2, false),
                controls.show(0, true),
                controls.show(0, false),
                controls.focus(1),
                controls.set_text(10, "no such item"),
            ];
            true
        })
    });
    assert_eq!(
        taken,
        [true, true, true, true, true, true, true, true, true, false]
    );
    let changes: Vec<&str> = (heard.iter())
        .filter(|line| line.starts_with("change"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        changes,
        [
            "change 1 0 0 [new]",
            "change 6 1 0 []",
            "change 3 1 0 []",
            "change 3 1 1 []",
            "change 2 2 0 []",
            "change 2 3 0 []",
            "change 0 4 0 []",
            "change 0 5 0 []",
            "change 1 6 0 []",
            "change 10 0 0 [no such item]",
        ]
    );
}

#[test]
fn a_panicking_handler_changes_nothing_lets_the_dialog_close_and_logs_why() {
    let fake = Fake {
        events: vec![(api::EVENT_CLOSE, 8), (api::EVENT_INIT, -1)],
        ..Fake::default()
    };
    let (_, heard, answers) = with_host(fake, |host| {
        host.dialog_with(&probe_dialog(), |event, _| {
            assert!(event != DialogEvent::Close(Some(8)), "the handler panics");
            false
        })
    });
    assert_eq!(
        answers,
        [0, 0],
        "closing lets it close; the next event is heard"
    );
    assert!(
        // The test program's panic hook is its own (the SDK's keeps the text in a plugin's library).
        heard
            .iter()
            .any(|line| line.starts_with("log [The plugin failed: ")),
        "{heard:?}"
    );
}

#[test]
fn a_dialog_that_wrote_no_value_ends_with_none() {
    let fake = Fake {
        button: -1,
        ..Fake::default()
    };
    let (closed, _, _) = with_host(fake, |host| host.dialog(&probe_dialog()));
    assert_eq!(closed, Closed::default(), "gone: no button, no values");
}

#[test]
fn the_dialog_tells_its_size_and_room_for_a_value_of_each_item() {
    let faked = Fake {
        values: vec![(1, "v"); 10],
        button: 0,
        ..Fake::default()
    };
    let ((closed, calls), _, _) = with_host(faked, |host| {
        let closed = host.dialog(&probe_dialog());
        // SAFETY: the fake's context lives through `with_host`.
        let heard = unsafe { fake(host.table().context) };
        (closed, heard.calls.lock().unwrap().clone())
    });
    assert_eq!(
        calls,
        [(size_of::<NcDialog>(), 10)],
        "this build's size; room for all ten items"
    );
    assert_eq!(closed.values.len(), 10);
    assert!(
        (closed.values.iter()).all(|value| value.text == "v" && value.checked()),
        "every value the host wrote"
    );
}

#[test]
fn only_state_one_is_checked_and_a_choice_past_any_index_is_the_first() {
    let value = |state| Value {
        state,
        text: String::new(),
    };
    assert!(value(1).checked());
    assert!(!value(0).checked() && !value(2).checked(), "[?] is not on");
    assert_eq!(DialogItem::list(&["a"], usize::MAX).state, 0);
    assert_eq!(DialogItem::list(&["a", "b"], 1).state, 1);
}

#[test]
fn values_at_null_are_none_to_the_handler() {
    let fake = Fake {
        events: vec![(api::EVENT_INIT, -1)],
        values: vec![(1, "x"), (1, "y")],
        null_values: true,
        ..Fake::default()
    };
    let mut seen = Vec::new();
    let _ = with_host(fake, |host| {
        host.dialog_with(&probe_dialog(), |_, controls| {
            seen.push(controls.value(0));
            true
        })
    });
    assert_eq!(seen, [Value::default()]);
}

#[test]
fn keys_clicks_focus_and_drawing_are_heard_and_false_takes_a_key_or_a_click() {
    let fake = Fake {
        events: vec![
            (api::EVENT_KEY, 1),
            (api::EVENT_MOUSE, -1),
            (api::EVENT_FOCUS, 2),
            (api::EVENT_UNFOCUS, 1),
            (api::EVENT_FOCUS, -1),
            (api::EVENT_DRAW, -1),
        ],
        ..Fake::default()
    };
    let mut heard_events = Vec::new();
    let (_, heard, answers) = with_host(fake, |host| {
        let dialog = probe_dialog().hear_keys();
        host.dialog_with(&dialog, |event, _| {
            heard_events.push(event);
            false
        })
    });
    assert_eq!(heard[0], "dialog Title flags 2 handler true");
    let key = "Ctrl+Enter".to_string();
    assert_eq!(
        heard_events,
        [
            DialogEvent::Key {
                item: Some(1),
                key: key.clone()
            },
            DialogEvent::Mouse {
                item: None,
                key,
                x: 3,
                y: 4
            },
            DialogEvent::Focus(2),
            DialogEvent::Unfocus(1),
            DialogEvent::Draw {
                columns: 3,
                rows: 4
            },
        ],
        "a focus without its item is not heard"
    );
    assert_eq!(
        answers,
        [1, 1, 0, 0, 0, 0],
        "false takes the key and the click"
    );
}

#[test]
fn a_fixed_line_a_list_box_a_history_and_a_placed_dialog_are_handed_as_built() {
    let dialog = Dialog::new(
        "Placed",
        vec![
            DialogItem::fixed("99:99", "12:30").at(5, 2, 0, 0),
            DialogItem::list_box("Box", &["one", "two"], 1).height(4),
            DialogItem::edit("")
                .history("Names", true)
                .edit_path()
                .read_only(),
        ],
    )
    .at(2, 1)
    .size(40, 12);
    let (_, heard, _) = with_host(Fake::default(), |host| host.dialog(&dialog));
    assert_eq!(
        heard,
        [
            "dialog Placed flags 0 handler false",
            "placed 2 1 40 12",
            "item 8 0 [12:30] 0 0 []",
            "more (\"\", \"99:99\", 0) (5, 2, 0, 0)",
            "item 9 0 [Box] 1 0 [\"one\", \"two\"]",
            "more (\"\", \"\", 4) (0, 0, 0, 0)",
            "item 1 1792 [] 0 0 []",
            "more (\"Names\", \"\", 0) (0, 0, 0, 0)",
        ],
        "use the last history 256, a path 512, read only 1024"
    );
}

#[test]
fn a_lists_lines_are_set_through_the_host() {
    let fake = Fake {
        events: vec![(api::EVENT_INIT, -1)],
        ..Fake::default()
    };
    let (_, heard, _) = with_host(fake, |host| {
        host.dialog_with(&probe_dialog(), |_, controls| {
            assert!(controls.set_lines(6, &["Blue", "Cyan"]));
            true
        })
    });
    assert_eq!(
        heard.last().unwrap(),
        "change 6 7 0 [] [\"Blue\", \"Cyan\"]"
    );
}
