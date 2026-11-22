//! More of a plugin's dialog on its thread: every kind and flag read one by one, the caps on what is
//! read, the room for the values the plugin hands, each event's own changes, handles that are not the
//! running handler's.

use nc_plugin_api::{
    CHANGE_FOCUS, CHANGE_LINES, DIALOG_KEYS, EVENT_CLOSE, EVENT_DRAW, EVENT_FOCUS, EVENT_KEY,
    EVENT_MOUSE, EVENT_UNFOCUS, ITEM_EDIT_PATH, ITEM_FIXED, ITEM_LIST_BOX, ITEM_USE_LAST_HISTORY,
    NcDialogChange, NcDialogEvent, NcDialogItem,
};

use super::*;

/// The kinds in the ABI's order, each with one flag in the ABI's order, and what the program reads.
const KINDS: [(u32, u32, ItemKind); 8] = [
    (ITEM_TEXT, ITEM_DISABLED, ItemKind::Text),
    (ITEM_EDIT, ITEM_HIDDEN, ItemKind::Edit),
    (ITEM_PASSWORD, ITEM_SAME_ROW, ItemKind::Password),
    (ITEM_CHECK, ITEM_GROUP, ItemKind::Check),
    (ITEM_RADIO, ITEM_DEFAULT, ItemKind::Radio),
    (ITEM_LIST, ITEM_NO_CLOSE, ItemKind::List),
    (ITEM_SEPARATOR, ITEM_THREE_STATE, ItemKind::Separator),
    (ITEM_BUTTON, ITEM_FOCUS, ItemKind::Button),
];

fn plain_item(kind: u32, flags: u32, text: &'static str) -> NcDialogItem {
    NcDialogItem {
        kind,
        flags,
        text: NcStr::new(text),
        state: 0,
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
    }
}

fn dialog_of(items: &[NcDialogItem], flags: u32) -> NcDialog {
    NcDialog {
        size: size_of::<NcDialog>(),
        flags,
        title: NcStr::new("T"),
        items: items.as_ptr(),
        count: items.len(),
        handler: None,
        handler_state: std::ptr::null_mut(),
        x: -1,
        y: -1,
        width: 0,
        height: 0,
    }
}

/// An item's flags as the program read them, in the ABI's order.
fn flags_read(item: &DialogItem) -> [bool; 8] {
    [
        item.disabled,
        item.hidden,
        item.same_row,
        item.group,
        item.default,
        item.no_close,
        item.three_state,
        item.focus,
    ]
}

/// The program taking the dialog, `check` given its request, then closing it by `button`.
fn closing_program(
    events: Receiver<Event>,
    button: Option<usize>,
    values: Vec<ItemValue>,
    check: impl FnOnce(DialogRequest) + Send + 'static,
) -> JoinHandle<()> {
    program(events, move |request, talk| {
        check(request);
        let _ = talk.send(DialogMessage::Closed { button, values });
    })
}

#[test]
fn every_kind_and_every_flag_is_read_as_the_abi_numbers_it() {
    let (context, events) = host_context(true, &no_logs());
    let mut items: Vec<NcDialogItem> = (KINDS.iter())
        .map(|&(kind, flags, _)| plain_item(kind, flags, "x"))
        .collect();
    items[3].state = 1;
    items[3].width = 300;
    let shown = closing_program(events, Some(7), Vec::new(), |request| {
        assert!(!request.warning, "no warning flag");
        let kinds: Vec<ItemKind> = request.items.iter().map(|item| item.kind).collect();
        let expected: Vec<ItemKind> = KINDS.iter().map(|(_, _, kind)| *kind).collect();
        assert_eq!(kinds, expected);
        for (at, item) in request.items.iter().enumerate() {
            let mut only = [false; 8];
            only[at] = true;
            assert_eq!(flags_read(item), only, "item {at}: its own flag only");
        }
        assert_eq!((request.items[3].state, request.items[3].width), (1, 300));
        assert_eq!(request.items[0].width, 0);
    });
    assert_eq!(call(&context, &dialog_of(&items, 0), &mut []), 7);
    shown.join().unwrap();
}

#[test]
fn items_and_list_lines_past_the_caps_are_not_read() {
    let (context, events) = host_context(true, &no_logs());
    let lines = vec![NcStr::new("l"); 100_001];
    let mut items = vec![plain_item(ITEM_TEXT, 0, "t"); 1001];
    items[0] = NcDialogItem {
        lines: lines.as_ptr(),
        line_count: lines.len(),
        ..plain_item(ITEM_LIST, 0, "")
    };
    let shown = closing_program(events, None, Vec::new(), |request| {
        assert_eq!(request.items.len(), 1000, "the most items read");
        assert_eq!(request.items[0].lines.len(), 100_000, "the most lines read");
    });
    assert_eq!(call(&context, &dialog_of(&items, 0), &mut []), -1);
    shown.join().unwrap();
}

#[test]
fn values_go_only_to_the_room_handed_and_none_to_null() {
    let (context, events) = host_context(true, &no_logs());
    let items = [plain_item(ITEM_EDIT, 0, "a"), plain_item(ITEM_EDIT, 0, "b")];
    let three = values_of(&["x", "y", "z"]);
    let shown = closing_program(events, Some(1), three.clone(), |_| {});
    let mut room = empty_values(3);
    let dialog = dialog_of(&items, 0);
    // SAFETY: the context and the dialog live through the call; room for 2 values at the start of 3.
    let button =
        unsafe { super::super::dialog(raw(&context), &raw const dialog, room.as_mut_ptr(), 2) };
    assert_eq!(button, 1);
    shown.join().unwrap();
    assert_eq!(
        (room[0].state, room[1].state, room[2].state),
        (0, 1, -7),
        "the third past the room left as it was"
    );
    let (context, events) = host_context(true, &no_logs());
    let shown = closing_program(events, Some(0), three, |_| {});
    // SAFETY: as above; no room at all, said by null.
    let button =
        unsafe { super::super::dialog(raw(&context), &raw const dialog, std::ptr::null_mut(), 5) };
    assert_eq!(button, 0, "null room: the button all the same");
    shown.join().unwrap();
    assert_eq!(context.dialog_texts.borrow().as_slice(), ["x", "y", "z"]);
}

#[test]
fn each_event_carries_only_the_changes_asked_in_its_own_call() {
    let (context, events) = host_context(true, &no_logs());
    let dialog = RawDialog::new();
    let mut recorder = recorder(&context);
    recorder.changes = vec![NcDialogChange {
        item: 1,
        what: CHANGE_FOCUS,
        state: 0,
        text: NcStr::EMPTY,
        lines: std::ptr::null(),
        line_count: 0,
    }];
    let shown = program(events, |_, talk| {
        for kind in [DialogEventKind::Init, DialogEventKind::Changed(1)] {
            assert_eq!(
                ask(talk, kind.clone(), &["a"]).changes,
                [Change::Focus(1)],
                "{kind:?}"
            );
        }
        let _ = talk.send(DialogMessage::Closed {
            button: None,
            values: Vec::new(),
        });
    });
    let table = dialog.table(Some((&mut recorder, recording)));
    assert_eq!(call(&context, &table, &mut []), -1);
    shown.join().unwrap();
}

/// A handler asking a change through a handle that is not the one it was given: a call of the host's
/// own kind, alive, but not the running one's.
unsafe extern "C" fn foreign(
    state: *mut c_void,
    _handle: *mut c_void,
    _event: *const NcDialogEvent,
) -> i32 {
    // SAFETY: the tests hand a live `Recorder`.
    let recorder = unsafe { &mut *state.cast::<Recorder>() };
    let other = DialogCall {
        count: 10,
        changes: RefCell::new(Vec::new()),
    };
    let change = NcDialogChange {
        item: 0,
        what: CHANGE_FOCUS,
        state: 0,
        text: NcStr::EMPTY,
        lines: std::ptr::null(),
        line_count: 0,
    };
    let handle = std::ptr::from_ref(&other).cast_mut().cast();
    // SAFETY: the context lives; the handle is checked before it is read.
    let taken = unsafe { dialog_change(recorder.context, handle, &raw const change) };
    recorder.taken.push(taken);
    recorder
        .taken
        .push(i32::try_from(other.changes.borrow().len()).unwrap_or(-2));
    0
}

#[test]
fn a_handle_not_of_the_running_handler_is_refused() {
    let (context, events) = host_context(true, &no_logs());
    let dialog = RawDialog::new();
    let mut recorder = recorder(&context);
    let shown = program(events, |_, talk| {
        assert!(ask(talk, DialogEventKind::Init, &["a"]).changes.is_empty());
        let _ = talk.send(DialogMessage::Closed {
            button: None,
            values: Vec::new(),
        });
    });
    let table = dialog.table(Some((&mut recorder, foreign)));
    assert_eq!(call(&context, &table, &mut []), -1);
    shown.join().unwrap();
    assert_eq!(recorder.taken, [-1, 0], "refused and nothing written to it");
}

#[test]
fn a_fixed_line_a_list_box_a_history_and_the_plugins_places_are_read() {
    let (context, events) = host_context(true, &no_logs());
    let lines = [NcStr::new("one"), NcStr::new("two")];
    let items = [
        NcDialogItem {
            mask: NcStr::new("99:99"),
            ..plain_item(ITEM_FIXED, 0, "12:30")
        },
        NcDialogItem {
            lines: lines.as_ptr(),
            line_count: lines.len(),
            height: 5,
            x1: 5,
            y1: 2,
            x2: 30,
            y2: 8,
            ..plain_item(ITEM_LIST_BOX, ITEM_NO_CLOSE, "Box")
        },
        NcDialogItem {
            history: NcStr::new("Names"),
            ..plain_item(
                ITEM_EDIT,
                ITEM_USE_LAST_HISTORY | ITEM_EDIT_PATH | nc_plugin_api::ITEM_READ_ONLY,
                "",
            )
        },
        plain_item(ITEM_EDIT, 0, ""),
        NcDialogItem {
            x1: 4,
            y1: 9,
            x2: 40,
            y2: 9,
            ..plain_item(nc_plugin_api::ITEM_BOX, 0, "Mode")
        },
    ];
    let dialog = NcDialog {
        x: 4,
        y: -1,
        width: 60,
        height: 12,
        ..dialog_of(&items, DIALOG_KEYS)
    };
    let shown = closing_program(events, None, Vec::new(), |request| {
        assert!(request.keys, "the handler hears the keys");
        assert_eq!(
            (request.x, request.y, request.size),
            (Some(4), None, Some((60, 12)))
        );
        let kinds: Vec<ItemKind> = request.items.iter().map(|item| item.kind).collect();
        let wanted = [
            ItemKind::Fixed,
            ItemKind::ListBox,
            ItemKind::Edit,
            ItemKind::Edit,
            ItemKind::Box,
        ];
        assert_eq!(kinds, wanted);
        let [fixed, list, line, plain, boxed] = &request.items[..] else {
            panic!("five items");
        };
        assert_eq!((boxed.text.as_str(), boxed.place), ("Mode", (4, 9, 40, 9)));
        assert_eq!(
            (fixed.mask.as_str(), fixed.text.as_str()),
            ("99:99", "12:30")
        );
        assert_eq!(
            (list.lines.len(), list.height, list.place),
            (2, 5, (5, 2, 30, 8))
        );
        assert!(list.no_close);
        assert_eq!(line.history.as_deref(), Some("Names"));
        assert!(line.use_last_history && line.edit_path && line.read_only);
        assert!(plain.history.is_none() && !plain.use_last_history && !plain.edit_path);
        assert!(!plain.read_only);
    });
    assert_eq!(call(&context, &dialog, &mut []), -1);
    shown.join().unwrap();
}

/// What the test's handler heard of a key, a click or the drawing: the event's kind, item, key and
/// place; it takes "F4" and every click, and keeps a closing dialog open.
#[derive(Default)]
struct Keyed {
    heard: Vec<(u32, i32, String, i32, i32)>,
}

unsafe extern "C" fn keyed(state: *mut c_void, _: *mut c_void, event: *const NcDialogEvent) -> i32 {
    // SAFETY: the tests hand a live `Keyed`; the event and its key live through the call.
    unsafe {
        let keyed = &mut *state.cast::<Keyed>();
        let event = &*event;
        let key = event.key.as_str().to_string();
        let takes = key == "F4" || event.kind == EVENT_MOUSE || event.kind == EVENT_CLOSE;
        keyed
            .heard
            .push((event.kind, event.item, key, event.x, event.y));
        i32::from(takes)
    }
}

#[test]
fn keys_clicks_the_focus_and_the_drawing_are_told_and_a_key_taken_is_heard_back() {
    let (context, events) = host_context(true, &no_logs());
    let items = [plain_item(ITEM_EDIT, 0, "a"), plain_item(ITEM_EDIT, 0, "b")];
    let mut state = Keyed::default();
    let dialog = NcDialog {
        handler: Some(keyed),
        handler_state: std::ptr::from_mut(&mut state).cast(),
        ..dialog_of(&items, DIALOG_KEYS)
    };
    let shown = program(events, |_, talk| {
        let key = |item, key: &str| DialogEventKind::Key {
            item,
            key: key.to_string(),
        };
        let taken = |handled: Handled| (handled.taken, handled.keep_open);
        assert_eq!(taken(ask(talk, key(Some(1), "F4"), &[])), (true, false));
        assert_eq!(taken(ask(talk, key(None, "a"), &[])), (false, false));
        let click = DialogEventKind::Mouse {
            item: Some(0),
            key: "MsLClick".to_string(),
            x: 7,
            y: 2,
        };
        assert_eq!(taken(ask(talk, click, &[])), (true, false));
        let draw = DialogEventKind::Draw {
            columns: 80,
            rows: 25,
        };
        assert_eq!(taken(ask(talk, draw, &[])), (false, false));
        let _ = ask(talk, DialogEventKind::Unfocus(0), &[]);
        let _ = ask(talk, DialogEventKind::Focus(1), &[]);
        assert_eq!(
            taken(ask(talk, DialogEventKind::Close(None), &[])),
            (false, true),
            "a closing dialog kept, no input taken"
        );
        let _ = talk.send(DialogMessage::Closed {
            button: None,
            values: Vec::new(),
        });
    });
    assert_eq!(call(&context, &dialog, &mut []), -1);
    shown.join().unwrap();
    let heard = |kind, item, key: &str, x, y| (kind, item, key.to_string(), x, y);
    assert_eq!(
        state.heard,
        [
            heard(EVENT_KEY, 1, "F4", 0, 0),
            heard(EVENT_KEY, -1, "a", 0, 0),
            heard(EVENT_MOUSE, 0, "MsLClick", 7, 2),
            heard(EVENT_DRAW, -1, "", 80, 25),
            heard(EVENT_UNFOCUS, 0, "", 0, 0),
            heard(EVENT_FOCUS, 1, "", 0, 0),
            heard(EVENT_CLOSE, -1, "", 0, 0),
        ]
    );
}

#[test]
fn a_lists_new_lines_are_read_and_lines_at_null_are_refused() {
    let (context, events) = host_context(true, &no_logs());
    let dialog = RawDialog::new();
    let mut recorder = recorder(&context);
    let lines = [NcStr::new("x"), NcStr::new("y")];
    let change = |lines: *const NcStr| NcDialogChange {
        item: 3,
        what: CHANGE_LINES,
        state: 0,
        text: NcStr::EMPTY,
        lines,
        line_count: 2,
    };
    recorder.changes = vec![change(lines.as_ptr()), change(std::ptr::null())];
    let shown = program(events, |_, talk| {
        let handled = ask(talk, DialogEventKind::Init, &["a"]);
        let wanted = Change::Lines(3, vec!["x".to_string(), "y".to_string()]);
        assert_eq!(handled.changes, [wanted]);
        let _ = talk.send(DialogMessage::Closed {
            button: None,
            values: Vec::new(),
        });
    });
    let table = dialog.table(Some((&mut recorder, recording)));
    assert_eq!(call(&context, &table, &mut []), -1);
    shown.join().unwrap();
    assert_eq!(recorder.taken, [0, -1]);
}
