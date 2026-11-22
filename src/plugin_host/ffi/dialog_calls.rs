//! A plugin's dialog (ADR-0016 §11) on the plugin's thread: the dialog read and sent to the program, the
//! thread waiting in it while its handler is called with each event the program sends, the changes the
//! handler asks for gathered for the answer, the items' last values handed back when it closes.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::mpsc::channel;

use nc_plugin_api::{
    CHANGE_DISABLE, CHANGE_ENABLE, CHANGE_FOCUS, CHANGE_HIDE, CHANGE_LINES, CHANGE_SHOW,
    CHANGE_STATE, CHANGE_TEXT, DIALOG_KEYS, DIALOG_WARNING, DialogHandler, EVENT_BUTTON,
    EVENT_CHANGED, EVENT_CLOSE, EVENT_DRAW, EVENT_FOCUS, EVENT_INIT, EVENT_KEY, EVENT_MOUSE,
    EVENT_UNFOCUS, ITEM_BUTTON, ITEM_CHECK, ITEM_DEFAULT, ITEM_DISABLED, ITEM_EDIT, ITEM_EDIT_PATH,
    ITEM_FIXED, ITEM_FOCUS, ITEM_GROUP, ITEM_HIDDEN, ITEM_LIST, ITEM_LIST_BOX, ITEM_NO_CLOSE,
    ITEM_PASSWORD, ITEM_RADIO, ITEM_SAME_ROW, ITEM_SEPARATOR, ITEM_TEXT, ITEM_THREE_STATE,
    ITEM_USE_LAST_HISTORY, NcDialog, NcDialogChange, NcDialogEvent, NcDialogItem, NcDialogValue,
    NcStr,
};

use super::calls::context;
use super::{HostContext, text};
use crate::plugin_host::Event;
use crate::plugin_host::windows::{
    Change, DialogEventKind, DialogItem, DialogMessage, DialogRequest, Handled, ItemKind, ItemValue,
};

/// The most items of a dialog and lines of a list read.
const MOST_ITEMS: usize = 1000;
const MOST_LIST_LINES: usize = 100_000;

/// A dialog waiting on the plugin's thread: its items' number and the changes its handler asked for in
/// the call running.
struct DialogCall {
    count: usize,
    changes: RefCell<Vec<Change>>,
}

/// The plugin's handler and its state.
type Handler = (DialogHandler, *mut c_void);

/// `NcHost::dialog`.
pub unsafe extern "C" fn dialog(
    context: *mut c_void,
    dialog: *const NcDialog,
    values: *mut NcDialogValue,
    count: usize,
) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        // Another thread of the plugin's (review R45).
        return -1;
    };
    // SAFETY: the plugin hands its dialog, alive through the call.
    let Some((request, handler)) = (unsafe { dialog_request(dialog) }) else {
        context
            .log
            .write("A dialog the program cannot read: not shown");
        return -1;
    };
    if !context.listens() {
        return -1;
    }
    // A window asked for takes the place of one without buttons.
    context.busy.set(false);
    let call = DialogCall {
        count: request.items.len(),
        changes: RefCell::new(Vec::new()),
    };
    let (talk, heard) = channel();
    let event = Event::Dialog {
        plugin: context.plugin.clone(),
        request,
        talk,
    };
    if context.events.send(event).is_err() {
        return -1;
    }
    let closed = loop {
        match heard.recv() {
            Ok(DialogMessage::Event {
                kind,
                values,
                reply,
            }) => {
                let _ = reply.send(handle(context, &call, handler, kind, &values));
            }
            Ok(DialogMessage::Closed { button, values }) => break (button, values),
            // The program gone, or the dialog without its end: Esc, nothing handed back.
            Err(_) => return -1,
        }
    };
    let (button, last) = closed;
    // SAFETY: the plugin hands room for `count` values at `values`, or null.
    unsafe { hand_back(context, &last, values, count) };
    button
        .and_then(|button| i32::try_from(button).ok())
        .unwrap_or(-1)
}

/// The items' last values written for the plugin, their texts kept in the context.
///
/// # Safety
/// `values` is null or room for `count` values.
unsafe fn hand_back(
    context: &HostContext,
    last: &[ItemValue],
    values: *mut NcDialogValue,
    count: usize,
) {
    let mut texts = context.dialog_texts.borrow_mut();
    *texts = last.iter().map(|value| value.text.clone()).collect();
    if values.is_null() {
        return;
    }
    for (index, (value, text)) in last.iter().zip(texts.iter()).enumerate().take(count) {
        // SAFETY: `index` is below `count`, room the plugin handed; the text stays in the context until
        // the next dialog replaces it.
        unsafe {
            values.add(index).write(NcDialogValue {
                state: value.state,
                text: NcStr::new(text),
            });
        }
    }
}

/// The handler called with `kind` and the values now: its changes, and whether a closing dialog stays.
fn handle(
    context: &HostContext,
    call: &DialogCall,
    handler: Option<Handler>,
    kind: DialogEventKind,
    values: &[ItemValue],
) -> Handled {
    let Some((handler, state)) = handler else {
        return Handled::default();
    };
    let raw: Vec<NcDialogValue> = (values.iter())
        .map(|value| NcDialogValue {
            state: value.state,
            text: NcStr::new(&value.text),
        })
        .collect();
    let told = told(&kind);
    let event = NcDialogEvent {
        kind: told.code,
        item: told.item,
        values: raw.as_ptr(),
        count: raw.len(),
        key: NcStr::new(told.key),
        x: told.x,
        y: told.y,
    };
    let handle = std::ptr::from_ref(call);
    context.handlers.borrow_mut().push(handle as usize);
    // SAFETY: the plugin's own handler and state; the handle and the event live through the call, made
    // on the plugin's thread.
    let answer = unsafe { handler(state, handle.cast_mut().cast(), &raw const event) };
    context.handlers.borrow_mut().pop();
    let input = matches!(
        kind,
        DialogEventKind::Key { .. } | DialogEventKind::Mouse { .. }
    );
    Handled {
        changes: call.changes.take(),
        keep_open: matches!(kind, DialogEventKind::Close(_)) && answer == 1,
        taken: input && answer == 1,
    }
}

/// An event as the ABI tells it.
struct Told<'a> {
    code: u32,
    item: i32,
    key: &'a str,
    x: i32,
    y: i32,
}

fn told(kind: &DialogEventKind) -> Told<'_> {
    let index = |item: usize| i32::try_from(item).unwrap_or(-1);
    let some = |item: Option<usize>| item.map_or(-1, index);
    let plain = |code: u32, item: i32| Told {
        code,
        item,
        key: "",
        x: 0,
        y: 0,
    };
    match kind {
        DialogEventKind::Init => plain(EVENT_INIT, -1),
        DialogEventKind::Changed(item) => plain(EVENT_CHANGED, index(*item)),
        DialogEventKind::Button(item) => plain(EVENT_BUTTON, index(*item)),
        DialogEventKind::Close(button) => plain(EVENT_CLOSE, some(*button)),
        DialogEventKind::Focus(item) => plain(EVENT_FOCUS, index(*item)),
        DialogEventKind::Unfocus(item) => plain(EVENT_UNFOCUS, index(*item)),
        DialogEventKind::Key { item, key } => Told {
            key,
            ..plain(EVENT_KEY, some(*item))
        },
        DialogEventKind::Mouse { item, key, x, y } => Told {
            key,
            x: *x,
            y: *y,
            ..plain(EVENT_MOUSE, some(*item))
        },
        DialogEventKind::Draw { columns, rows } => Told {
            x: i32::from(*columns),
            y: i32::from(*rows),
            ..plain(EVENT_DRAW, -1)
        },
    }
}

/// `NcHost::dialog_change`.
pub unsafe extern "C" fn dialog_change(
    context: *mut c_void,
    handle: *mut c_void,
    change: *const NcDialogChange,
) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        // Another thread of the plugin's (review R45).
        return -1;
    };
    // Only a handle whose handler runs now is ever read.
    if change.is_null() || !context.handlers.borrow().contains(&(handle as usize)) {
        return -1;
    }
    // SAFETY: the handle is the address of a `DialogCall` alive while its handler runs (checked above).
    let call = unsafe { &*handle.cast::<DialogCall>() };
    // SAFETY: the plugin hands its change, alive through the call.
    let change = unsafe { &*change };
    if change.item >= call.count {
        return -1;
    }
    // SAFETY: the plugin's lines, alive through the call.
    let Some(taken) = (unsafe { read_change(change) }) else {
        return -1;
    };
    call.changes.borrow_mut().push(taken);
    0
}

/// The change asked for; `None` for one not known or lines at null.
///
/// # Safety
/// `line_count` lines at `lines` (any pointer when 0).
unsafe fn read_change(change: &NcDialogChange) -> Option<Change> {
    let item = change.item;
    Some(match change.what {
        CHANGE_LINES => {
            // SAFETY: as the caller promises.
            Change::Lines(item, unsafe { lines(change.lines, change.line_count) }?)
        }
        CHANGE_TEXT => Change::Text(item, text(change.text)),
        CHANGE_STATE => Change::State(item, change.state),
        CHANGE_ENABLE => Change::Enabled(item, true),
        CHANGE_DISABLE => Change::Enabled(item, false),
        CHANGE_SHOW => Change::Visible(item, true),
        CHANGE_HIDE => Change::Visible(item, false),
        CHANGE_FOCUS => Change::Focus(item),
        _ => return None,
    })
}

/// The plugin's dialog read, with its handler; `None` for a null one, a smaller (older) one, items or
/// lines at null, or an item of a kind not known.
///
/// # Safety
/// `dialog` is null or the plugin's dialog: `count` items at `items`, each list's `line_count` lines at
/// `lines`.
unsafe fn dialog_request(dialog: *const NcDialog) -> Option<(DialogRequest, Option<Handler>)> {
    if dialog.is_null() {
        return None;
    }
    // SAFETY: the size comes first in any build's dialog.
    if unsafe { (*dialog).size } < size_of::<NcDialog>() {
        return None;
    }
    // SAFETY: a dialog at least this build's size.
    let dialog = unsafe { &*dialog };
    let count = dialog.count.min(MOST_ITEMS);
    if count > 0 && dialog.items.is_null() {
        return None;
    }
    // SAFETY: `count` items at `items` (not null when there are any), alive through the call.
    let items = unsafe { super::calls::slice(dialog.items, count) };
    // SAFETY: each item's lines as the caller promises.
    let items: Option<Vec<DialogItem>> = items
        .iter()
        .map(|item| unsafe { dialog_item(item) })
        .collect();
    let at = |place: i32| u16::try_from(place).ok();
    let size = (dialog.width > 0 && dialog.height > 0).then(|| {
        let side = |size: u32| u16::try_from(size).unwrap_or(u16::MAX);
        (side(dialog.width), side(dialog.height))
    });
    let request = DialogRequest {
        title: text(dialog.title),
        warning: dialog.flags & DIALOG_WARNING != 0,
        items: items?,
        handled: dialog.handler.is_some(),
        keys: dialog.flags & DIALOG_KEYS != 0,
        x: at(dialog.x),
        y: at(dialog.y),
        size,
    };
    let handler = dialog
        .handler
        .map(|handler| (handler, dialog.handler_state));
    Some((request, handler))
}

/// One item read; `None` for a kind not known or a list's lines at null.
///
/// # Safety
/// `line_count` lines at `lines` (any pointer when 0).
unsafe fn dialog_item(item: &NcDialogItem) -> Option<DialogItem> {
    let kind = match item.kind {
        ITEM_TEXT => ItemKind::Text,
        ITEM_EDIT => ItemKind::Edit,
        ITEM_PASSWORD => ItemKind::Password,
        ITEM_CHECK => ItemKind::Check,
        ITEM_RADIO => ItemKind::Radio,
        ITEM_LIST => ItemKind::List,
        ITEM_SEPARATOR => ItemKind::Separator,
        ITEM_BUTTON => ItemKind::Button,
        ITEM_FIXED => ItemKind::Fixed,
        ITEM_LIST_BOX => ItemKind::ListBox,
        nc_plugin_api::ITEM_BOX => ItemKind::Box,
        _ => return None,
    };
    let flag = |flag: u32| item.flags & flag != 0;
    let history = Some(text(item.history)).filter(|name| !name.is_empty());
    Some(DialogItem {
        kind,
        text: text(item.text),
        state: item.state,
        width: u16::try_from(item.width).unwrap_or(u16::MAX),
        // SAFETY: as the caller promises.
        lines: unsafe { lines(item.lines, item.line_count) }?,
        history,
        use_last_history: flag(ITEM_USE_LAST_HISTORY),
        edit_path: flag(ITEM_EDIT_PATH),
        read_only: flag(nc_plugin_api::ITEM_READ_ONLY),
        mask: text(item.mask),
        height: u16::try_from(item.height).unwrap_or(u16::MAX),
        place: (item.x1, item.y1, item.x2, item.y2),
        disabled: flag(ITEM_DISABLED),
        hidden: flag(ITEM_HIDDEN),
        same_row: flag(ITEM_SAME_ROW),
        group: flag(ITEM_GROUP),
        default: flag(ITEM_DEFAULT),
        no_close: flag(ITEM_NO_CLOSE),
        three_state: flag(ITEM_THREE_STATE),
        focus: flag(ITEM_FOCUS),
    })
}

/// A list's lines read (up to [`MOST_LIST_LINES`]); `None` for lines at null.
///
/// # Safety
/// `count` lines at `lines` (any pointer when 0).
unsafe fn lines(lines: *const NcStr, count: usize) -> Option<Vec<String>> {
    let count = count.min(MOST_LIST_LINES);
    if count > 0 && lines.is_null() {
        return None;
    }
    // SAFETY: as the caller promises.
    let lines = unsafe { super::calls::slice(lines, count) };
    Some(lines.iter().map(|line| text(*line)).collect())
}

#[cfg(test)]
#[path = "dialog_calls_tests.rs"]
mod tests;
