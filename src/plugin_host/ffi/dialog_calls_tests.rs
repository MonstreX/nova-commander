//! A plugin's dialog on its thread without a library: what is read, what reaches the program, what the
//! handler hears and may change, what is handed back, what is never shown.

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use nc_plugin_api::{CHANGE_TEXT, NcDialogChange, NcDialogItem};

use super::*;
use crate::plugin_host::log::PluginLog;

fn host_context(started: bool, logs: &Path) -> (HostContext, Receiver<Event>) {
    let (sender, events) = channel();
    let log = PluginLog::new(logs, "probe");
    let context = HostContext::new("probe", sender, Path::new(r"C:\plugins\probe"), log);
    context
        .started
        .store(started, std::sync::atomic::Ordering::Release);
    (context, events)
}

fn no_logs() -> std::path::PathBuf {
    std::env::current_exe().unwrap().join("Logs")
}

fn raw(context: &HostContext) -> *mut c_void {
    std::ptr::from_ref(context).cast_mut().cast()
}

/// An event as the handler heard it: its kind, its item and the values (state, text).
type Heard = (u32, i32, Vec<(i32, String)>);

/// What the test's handler heard and does: the changes it asks for, what it answers on closing.
struct Recorder {
    context: *mut c_void,
    heard: Vec<Heard>,
    changes: Vec<NcDialogChange>,
    taken: Vec<i32>,
    answer: i32,
    handle: *mut c_void,
}

unsafe extern "C" fn recording(
    state: *mut c_void,
    handle: *mut c_void,
    event: *const NcDialogEvent,
) -> i32 {
    // SAFETY: the tests hand a live `Recorder`; the event and its values live through the call.
    unsafe {
        let recorder = &mut *state.cast::<Recorder>();
        let event = &*event;
        let values = std::slice::from_raw_parts(event.values, event.count);
        let values = (values.iter())
            .map(|value| (value.state, value.text.as_str().to_string()))
            .collect();
        recorder.heard.push((event.kind, event.item, values));
        for change in &recorder.changes {
            recorder
                .taken
                .push(dialog_change(recorder.context, handle, change));
        }
        recorder.handle = handle;
        recorder.answer
    }
}

fn recorder(context: &HostContext) -> Recorder {
    Recorder {
        context: raw(context),
        heard: Vec::new(),
        changes: Vec::new(),
        taken: Vec::new(),
        answer: 0,
        handle: std::ptr::null_mut(),
    }
}

/// A dialog as a plugin builds it: its items' texts, lines and the table.
struct RawDialog {
    texts: Vec<&'static str>,
    lines: Vec<NcStr>,
    items: Vec<NcDialogItem>,
}

impl RawDialog {
    /// A label, an edit line, a three-state check box, a list of two lines, a button.
    fn new() -> Self {
        let texts = vec!["&Name:", "start", "&Upper", "", "OK"];
        let lines = vec![NcStr::new("Red"), NcStr::new("Green")];
        let item = |kind, flags, at: usize, state| NcDialogItem {
            kind,
            flags,
            text: NcStr::new(texts[at]),
            state,
            width: 0,
            lines: std::ptr::null(),
            line_count: 0,
            history: NcStr::EMPTY,
            mask: NcStr::EMPTY,
            height: 0,
            x1: 0,
            y1: 0,
            x2: 0,
            y2: 0,
        };
        let mut items = vec![
            item(ITEM_TEXT, ITEM_HIDDEN, 0, 0),
            item(ITEM_EDIT, ITEM_FOCUS | ITEM_SAME_ROW, 1, 0),
            item(ITEM_CHECK, ITEM_THREE_STATE | ITEM_DISABLED, 2, 2),
            item(ITEM_LIST, 0, 3, 1),
            item(ITEM_BUTTON, ITEM_DEFAULT | ITEM_NO_CLOSE, 4, 0),
        ];
        items[1].width = 70_000;
        items[3].lines = lines.as_ptr();
        items[3].line_count = lines.len();
        Self {
            texts,
            lines,
            items,
        }
    }

    fn table(&self, handler: Option<(&mut Recorder, DialogHandler)>) -> NcDialog {
        let (state, handler) = match handler {
            Some((recorder, handler)) => (std::ptr::from_mut(recorder).cast(), Some(handler)),
            None => (std::ptr::null_mut(), None),
        };
        NcDialog {
            size: size_of::<NcDialog>(),
            flags: DIALOG_WARNING,
            title: NcStr::new("Title"),
            items: self.items.as_ptr(),
            count: self.items.len(),
            handler,
            handler_state: state,
            x: -1,
            y: -1,
            width: 0,
            height: 0,
        }
    }
}

fn call(context: &HostContext, dialog: &NcDialog, values: &mut [NcDialogValue]) -> i32 {
    // SAFETY: the context, the dialog and the room for the values live through the call.
    unsafe {
        super::dialog(
            raw(context),
            std::ptr::from_ref(dialog),
            values.as_mut_ptr(),
            values.len(),
        )
    }
}

fn empty_values(count: usize) -> Vec<NcDialogValue> {
    vec![
        NcDialogValue {
            state: -7,
            text: NcStr::EMPTY,
        };
        count
    ]
}

/// The program: the dialog taken, `act` given what it asked and the line to its thread.
fn program(
    events: Receiver<Event>,
    act: impl FnOnce(DialogRequest, &Sender<DialogMessage>) + Send + 'static,
) -> JoinHandle<()> {
    std::thread::spawn(move || match events.recv_timeout(Duration::from_secs(10)) {
        Ok(Event::Dialog { request, talk, .. }) => act(request, &talk),
        other => panic!("a dialog: {other:?}"),
    })
}

fn values_of(texts: &[&str]) -> Vec<ItemValue> {
    (texts.iter().enumerate())
        .map(|(state, text)| ItemValue {
            state: i32::try_from(state).unwrap(),
            text: (*text).to_string(),
        })
        .collect()
}

/// An event sent to the plugin's thread and its handler's answer.
fn ask(talk: &Sender<DialogMessage>, kind: DialogEventKind, texts: &[&str]) -> Handled {
    let (reply, answer) = channel();
    let event = DialogMessage::Event {
        kind,
        values: values_of(texts),
        reply,
    };
    talk.send(event).unwrap();
    answer.recv_timeout(Duration::from_secs(10)).unwrap()
}

#[test]
fn a_dialog_is_read_as_the_plugin_built_it_and_its_last_values_handed_back() {
    let (context, events) = host_context(true, &no_logs());
    context.busy.set(true);
    let dialog = RawDialog::new();
    assert_eq!(dialog.texts.len() + dialog.lines.len(), 7);
    let shown = program(events, |request, talk| {
        assert_eq!(request.title, "Title");
        assert!(request.warning && !request.handled);
        let kinds: Vec<ItemKind> = request.items.iter().map(|item| item.kind).collect();
        let kind = [
            ItemKind::Text,
            ItemKind::Edit,
            ItemKind::Check,
            ItemKind::List,
            ItemKind::Button,
        ];
        assert_eq!(kinds, kind);
        let [label, edit, check, list, button] = request.items.as_slice() else {
            panic!("five items");
        };
        assert!(label.hidden && !label.disabled && label.text == "&Name:");
        assert!(edit.focus && edit.same_row && edit.width == u16::MAX && edit.text == "start");
        assert!(check.three_state && check.disabled && check.state == 2);
        assert_eq!(
            (list.lines.as_slice(), list.state),
            (&["Red".to_string(), "Green".to_string()][..], 1)
        );
        assert!(button.default && button.no_close && !button.group);
        let values = values_of(&["a", "b", "c", "d", "e"]);
        talk.send(DialogMessage::Closed {
            button: Some(4),
            values,
        })
        .unwrap();
    });
    let mut values = empty_values(3);
    assert_eq!(call(&context, &dialog.table(None), &mut values), 4);
    shown.join().unwrap();
    assert!(!context.busy.get(), "a window without buttons gives way");
    let read: Vec<(i32, &str)> = (values.iter())
        // SAFETY: the host's texts, kept in the context until its next answer.
        .map(|value| (value.state, unsafe { value.text.as_str() }))
        .collect();
    assert_eq!(read, [(0, "a"), (1, "b"), (2, "c")], "as many as the room");
    assert_eq!(context.dialog_texts.borrow().len(), 5, "all kept");
}

/// Every kind of change, one for no such item and one of no kind known: what the test's handler asks.
fn every_change() -> Vec<NcDialogChange> {
    let change = |item, what, state, text: &'static str| NcDialogChange {
        item,
        what,
        state,
        text: NcStr::new(text),
        lines: std::ptr::null(),
        line_count: 0,
    };
    vec![
        change(1, CHANGE_TEXT, 0, "new"),
        change(2, CHANGE_STATE, 1, ""),
        change(3, CHANGE_DISABLE, 0, ""),
        change(3, CHANGE_ENABLE, 0, ""),
        change(0, CHANGE_SHOW, 0, ""),
        change(0, CHANGE_HIDE, 0, ""),
        change(1, CHANGE_FOCUS, 0, ""),
        change(5, CHANGE_TEXT, 0, "no such item"),
        change(1, 99, 0, ""),
    ]
}

/// The program sending each event to a handler that asks for [`every_change`] and refuses to close,
/// then closing the dialog by Esc.
fn every_event(events: Receiver<Event>) -> JoinHandle<()> {
    program(events, |request, talk| {
        assert!(request.handled);
        let texts = ["a", "b", "c", "d", "e"];
        let init = ask(talk, DialogEventKind::Init, &texts);
        assert_eq!(
            init.changes,
            [
                Change::Text(1, "new".to_string()),
                Change::State(2, 1),
                Change::Enabled(3, false),
                Change::Enabled(3, true),
                Change::Visible(0, true),
                Change::Visible(0, false),
                Change::Focus(1),
            ]
        );
        assert!(!init.keep_open, "only closing is kept open");
        for kind in [DialogEventKind::Changed(2), DialogEventKind::Button(4)] {
            assert!(!ask(talk, kind, &texts).keep_open);
        }
        assert!(ask(talk, DialogEventKind::Close(Some(4)), &texts).keep_open);
        assert!(ask(talk, DialogEventKind::Close(None), &texts).keep_open);
        let values = values_of(&texts);
        talk.send(DialogMessage::Closed {
            button: None,
            values,
        })
        .unwrap();
    })
}

#[test]
fn the_handler_hears_each_event_and_its_changes_go_back_with_the_answer() {
    let (context, events) = host_context(true, &no_logs());
    let dialog = RawDialog::new();
    let mut recorder = recorder(&context);
    recorder.changes = every_change();
    recorder.answer = 1;
    let shown = every_event(events);
    let table = dialog.table(Some((&mut recorder, recording)));
    let mut values = empty_values(5);
    assert_eq!(call(&context, &table, &mut values), -1, "Esc");
    shown.join().unwrap();
    // SAFETY: the host's text, kept in the context.
    let last = unsafe { values[4].text.as_str() };
    assert_eq!(last, "e", "handed back on Esc too");
    let heard: Vec<(u32, i32)> = recorder
        .heard
        .iter()
        .map(|(kind, item, _)| (*kind, *item))
        .collect();
    assert_eq!(
        heard,
        [
            (EVENT_INIT, -1),
            (EVENT_CHANGED, 2),
            (EVENT_BUTTON, 4),
            (EVENT_CLOSE, 4),
            (EVENT_CLOSE, -1)
        ]
    );
    assert_eq!(
        recorder.heard[0].2[3],
        (3, "d".to_string()),
        "the values as sent"
    );
    assert_eq!(recorder.taken[..9], [0, 0, 0, 0, 0, 0, 0, -1, -1]);
    // The handle after its call: never read.
    let late = NcDialogChange {
        item: 0,
        what: CHANGE_FOCUS,
        state: 0,
        text: NcStr::EMPTY,
        lines: std::ptr::null(),
        line_count: 0,
    };
    // SAFETY: the context lives; the handle is stale, which the host checks before reading it.
    let stale = unsafe { dialog_change(raw(&context), recorder.handle, &raw const late) };
    assert_eq!(stale, -1);
    // SAFETY: as above, a null change.
    let null = unsafe { dialog_change(raw(&context), recorder.handle, std::ptr::null()) };
    assert_eq!(null, -1);
    assert!(context.handlers.borrow().is_empty());
}

#[test]
fn without_a_handler_an_event_changes_nothing() {
    let (context, events) = host_context(true, &no_logs());
    let dialog = RawDialog::new();
    let shown = program(events, |_, talk| {
        let handled = ask(talk, DialogEventKind::Close(Some(4)), &["a"]);
        assert_eq!(handled, Handled::default());
        talk.send(DialogMessage::Closed {
            button: Some(0),
            values: Vec::new(),
        })
        .unwrap();
    });
    assert_eq!(call(&context, &dialog.table(None), &mut []), 0);
    shown.join().unwrap();
}

#[test]
fn a_dialog_gone_without_its_end_is_esc_and_hands_nothing_back() {
    let (context, events) = host_context(true, &no_logs());
    let dialog = RawDialog::new();
    let shown = program(events, |_, _| {});
    let mut values = empty_values(1);
    assert_eq!(call(&context, &dialog.table(None), &mut values), -1);
    shown.join().unwrap();
    assert_eq!(values[0].state, -7, "untouched");
    let (context, events) = host_context(true, &no_logs());
    drop(events);
    assert_eq!(
        call(&context, &dialog.table(None), &mut []),
        -1,
        "nobody listening"
    );
}

#[test]
fn a_dialog_while_starting_is_esc_at_once() {
    let (context, events) = host_context(false, &no_logs());
    // The program, were the dialog sent: it closes it with its first item, so a call made anyway
    // returns 0 instead of waiting.
    let heard = std::thread::spawn(move || match events.recv() {
        Ok(Event::Dialog { talk, .. }) => {
            let _ = talk.send(DialogMessage::Closed {
                button: Some(0),
                values: Vec::new(),
            });
            true
        }
        _ => false,
    });
    let dialog = RawDialog::new();
    assert_eq!(call(&context, &dialog.table(None), &mut []), -1);
    drop(context);
    assert!(!heard.join().unwrap(), "nothing shown");
}

/// The program closing every dialog it is sent with its first item: how many it was sent once the
/// plugin's side is gone.
fn answering_dialogs(events: Receiver<Event>) -> JoinHandle<usize> {
    std::thread::spawn(move || {
        let mut shown = 0;
        while let Ok(event) = events.recv() {
            if let Event::Dialog { talk, .. } = event {
                shown += 1;
                let _ = talk.send(DialogMessage::Closed {
                    button: Some(0),
                    values: Vec::new(),
                });
            }
        }
        shown
    })
}

#[test]
fn a_dialog_the_program_cannot_read_is_not_shown_and_logged() {
    let temp = tempfile::tempdir().unwrap();
    let (context, events) = host_context(true, temp.path());
    let dialog = RawDialog::new();
    let older = NcDialog {
        size: size_of::<NcDialog>() - 1,
        ..dialog.table(None)
    };
    let no_items = NcDialog {
        items: std::ptr::null(),
        ..dialog.table(None)
    };
    let mut unknown = RawDialog::new();
    unknown.items[0].kind = 99;
    let mut no_lines = RawDialog::new();
    no_lines.items[3].lines = std::ptr::null();
    let tables = [older, no_items, unknown.table(None), no_lines.table(None)];
    // The program, were one sent anyway: it closes it with its first item instead of letting the call
    // wait.
    let shown = answering_dialogs(events);
    for table in &tables {
        assert_eq!(call(&context, table, &mut []), -1);
    }
    // SAFETY: null is checked before anything is read.
    let null = unsafe { super::dialog(raw(&context), std::ptr::null(), std::ptr::null_mut(), 0) };
    assert_eq!(null, -1);
    drop(context);
    assert_eq!(shown.join().unwrap(), 0, "nothing shown");
    let log = std::fs::read_to_string(temp.path().join("probe.log")).unwrap();
    assert_eq!(log.matches("A dialog the program cannot read").count(), 5);
    let (context, events) = host_context(true, temp.path());
    let empty = NcDialog {
        items: std::ptr::null(),
        count: 0,
        ..dialog.table(None)
    };
    let shown = program(events, |request, talk| {
        assert!(request.items.is_empty());
        talk.send(DialogMessage::Closed {
            button: None,
            values: Vec::new(),
        })
        .unwrap();
    });
    assert_eq!(
        call(&context, &empty, &mut []),
        -1,
        "no items: shown, nothing read"
    );
    shown.join().unwrap();
}

#[path = "dialog_calls_more_tests.rs"]
mod more;
