//! A dialog run: its items handed to the host, the handler called with each event the host sends, the
//! changes it asks for through [`Controls`], the items' last values read back.

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::{Closed, Dialog, DialogEvent, Value};
use crate::api::{
    self, DIALOG_KEYS, DIALOG_WARNING, EVENT_BUTTON, EVENT_CHANGED, EVENT_CLOSE, EVENT_DRAW,
    EVENT_FOCUS, EVENT_INIT, EVENT_KEY, EVENT_MOUSE, EVENT_UNFOCUS, NcDialog, NcDialogChange,
    NcDialogEvent, NcDialogItem, NcDialogValue, NcStr,
};
use crate::host::{flag, index};
use crate::{Host, panic_text};

/// The dialog as its handler sees it: the items' values, and changes to make.
pub struct Controls<'a> {
    host: &'a Host,
    handle: *mut c_void,
    values: &'a [NcDialogValue],
    /// The states and texts this call has set, by item: read back as FAR's `DM_GETCHECK` and
    /// `DM_GETTEXT` read the dialog as it is now.
    set: Vec<(usize, Value)>,
}

impl Controls<'_> {
    /// Item `item`'s value: as the event came, with what this call has set since (FAR's
    /// `DM_GETCHECK`, `DM_GETTEXT`, `DM_LISTGETCURPOS`).
    pub fn value(&self, item: usize) -> Value {
        if let Some((_, value)) = self.set.iter().find(|(at, _)| *at == item) {
            return value.clone();
        }
        self.values
            .get(item)
            .map_or_else(Value::default, |value| Value {
                state: value.state,
                // SAFETY: the host's texts live through the handler's call.
                text: unsafe { value.text.as_str() }.to_string(),
            })
    }

    /// What this call set of item `item` kept for [`Self::value`].
    fn remember(&mut self, item: usize, change: impl FnOnce(&mut Value)) {
        let mut value = self.value(item);
        change(&mut value);
        self.set.retain(|(at, _)| *at != item);
        self.set.push((item, value));
    }

    fn change(&mut self, item: usize, what: u32, state: i32, text: &str) -> bool {
        self.change_lines(item, (what, state, text), &[])
    }

    fn change_lines(
        &mut self,
        item: usize,
        (what, state, text): (u32, i32, &str),
        lines: &[NcStr],
    ) -> bool {
        let change = NcDialogChange {
            item,
            what,
            state,
            text: NcStr::new(text),
            lines: lines.as_ptr(),
            line_count: lines.len(),
        };
        let table = self.host.table();
        // SAFETY: the host's own function with its context and the handle of the call running.
        unsafe { (table.dialog_change)(table.context, self.handle, &raw const change) == 0 }
    }

    /// The item's text (an edit line's, a label). False: no such item.
    pub fn set_text(&mut self, item: usize, text: &str) -> bool {
        let done = self.change(item, api::CHANGE_TEXT, 0, text);
        if done {
            self.remember(item, |value| value.text = text.to_string());
        }
        done
    }

    /// A check box's or radio button's state, a list's chosen line.
    pub fn set_state(&mut self, item: usize, state: i32) -> bool {
        let done = self.change(item, api::CHANGE_STATE, state, "");
        if done {
            self.remember(item, |value| value.state = state);
        }
        done
    }

    pub fn set_checked(&mut self, item: usize, checked: bool) -> bool {
        self.set_state(item, i32::from(checked))
    }

    pub fn enable(&mut self, item: usize, enabled: bool) -> bool {
        let what = if enabled {
            api::CHANGE_ENABLE
        } else {
            api::CHANGE_DISABLE
        };
        self.change(item, what, 0, "")
    }

    pub fn show(&mut self, item: usize, shown: bool) -> bool {
        let what = if shown {
            api::CHANGE_SHOW
        } else {
            api::CHANGE_HIDE
        };
        self.change(item, what, 0, "")
    }

    pub fn focus(&mut self, item: usize) -> bool {
        self.change(item, api::CHANGE_FOCUS, 0, "")
    }

    /// A list's or list box's lines (FAR's `DM_LISTSET`).
    pub fn set_lines(&mut self, item: usize, lines: &[&str]) -> bool {
        let lines: Vec<NcStr> = lines.iter().map(|line| NcStr::new(line)).collect();
        self.change_lines(item, (api::CHANGE_LINES, 0, ""), &lines)
    }
}

/// The state of a value the host has not written.
const NOT_WRITTEN: i32 = i32::MIN;

/// The handler and the host it changes the dialog through.
struct Handling<F> {
    handler: F,
    host: Host,
}

/// The host's call of the handler.
///
/// # Safety
/// `state` is the `Handling<F>` [`Host::dialog_with`] handed; the event and its values live through the
/// call.
unsafe extern "C" fn handle<F>(
    state: *mut c_void,
    dialog: *mut c_void,
    event: *const NcDialogEvent,
) -> i32
where
    F: FnMut(DialogEvent, &mut Controls) -> bool,
{
    // SAFETY: as the caller promises; one call at a time on the plugin's thread.
    let (handling, event) = unsafe { (&mut *state.cast::<Handling<F>>(), &*event) };
    let values = if event.count == 0 || event.values.is_null() {
        &[][..]
    } else {
        // SAFETY: the host hands `count` values at `values`, alive through the call.
        unsafe { std::slice::from_raw_parts(event.values, event.count) }
    };
    let Some(kind) = event_of(event) else {
        return 0;
    };
    let host = handling.host;
    let mut controls = Controls {
        host: &host,
        handle: dialog,
        values,
        set: Vec::new(),
    };
    // Its `false` stops a closing dialog, a key, a click.
    let stops = matches!(
        kind,
        DialogEvent::Close(_) | DialogEvent::Key { .. } | DialogEvent::Mouse { .. }
    );
    match catch_unwind(AssertUnwindSafe(|| (handling.handler)(kind, &mut controls))) {
        Ok(go_on) => i32::from(stops && !go_on),
        Err(_) => {
            // A panic in the handler changes nothing and lets the dialog close; its log says why.
            host.log(&panic_text());
            0
        }
    }
}

/// The host's event as the handler hears it; `None` for one this build does not know.
fn event_of(event: &NcDialogEvent) -> Option<DialogEvent> {
    let item = index(event.item);
    // SAFETY: the host's key lives through the handler's call.
    let key = || unsafe { event.key.as_str() }.to_string();
    Some(match event.kind {
        EVENT_INIT => DialogEvent::Init,
        EVENT_CHANGED => DialogEvent::Changed(item.unwrap_or(0)),
        EVENT_BUTTON => DialogEvent::Button(item.unwrap_or(0)),
        EVENT_CLOSE => DialogEvent::Close(item),
        EVENT_KEY => DialogEvent::Key { item, key: key() },
        EVENT_MOUSE => DialogEvent::Mouse {
            item,
            key: key(),
            x: event.x,
            y: event.y,
        },
        EVENT_FOCUS => DialogEvent::Focus(item?),
        EVENT_UNFOCUS => DialogEvent::Unfocus(item?),
        EVENT_DRAW => DialogEvent::Draw {
            columns: event.x,
            rows: event.y,
        },
        _ => return None,
    })
}

impl Host {
    /// The dialog without a handler: how it ended.
    pub fn dialog(&self, dialog: &Dialog) -> Closed {
        self.run_dialog(dialog, None)
    }

    /// The dialog with `handler` hearing its events: for [`DialogEvent::Close`] its `false` keeps the
    /// dialog open, for [`DialogEvent::Key`] and [`DialogEvent::Mouse`] it takes the input; for the others
    /// its answer does not matter.
    pub fn dialog_with<F>(&self, dialog: &Dialog, handler: F) -> Closed
    where
        F: FnMut(DialogEvent, &mut Controls) -> bool,
    {
        let mut handling = Handling {
            handler,
            host: *self,
        };
        let state = (&raw mut handling).cast::<c_void>();
        self.run_dialog(dialog, Some((handle::<F>, state)))
    }

    fn run_dialog(
        &self,
        dialog: &Dialog,
        handler: Option<(api::DialogHandler, *mut c_void)>,
    ) -> Closed {
        let lines: Vec<Vec<NcStr>> = (dialog.items.iter())
            .map(|item| item.lines.iter().map(|line| NcStr::new(line)).collect())
            .collect();
        let items: Vec<NcDialogItem> = (dialog.items.iter().zip(&lines))
            .map(|(item, lines)| NcDialogItem {
                kind: item.kind_code(),
                flags: item.flags(),
                text: NcStr::new(&item.text),
                state: item.state,
                width: item.width,
                lines: lines.as_ptr(),
                line_count: lines.len(),
                history: NcStr::new(&item.history),
                mask: NcStr::new(&item.mask),
                height: item.height,
                x1: item.place.0,
                y1: item.place.1,
                x2: item.place.2,
                y2: item.place.3,
            })
            .collect();
        let (width, height) = dialog.size.unwrap_or((0, 0));
        let raw = NcDialog {
            size: size_of::<NcDialog>(),
            flags: flag(dialog.warning, DIALOG_WARNING) | flag(dialog.keys, DIALOG_KEYS),
            title: NcStr::new(&dialog.title),
            items: items.as_ptr(),
            count: items.len(),
            handler: handler.map(|(handler, _)| handler),
            handler_state: handler.map_or(std::ptr::null_mut(), |(_, state)| state),
            x: dialog.x.map_or(-1, i32::from),
            y: dialog.y.map_or(-1, i32::from),
            width,
            height,
        };
        // A state no item has: the values the host did not write stay so.
        let mut values = vec![
            NcDialogValue {
                state: NOT_WRITTEN,
                text: NcStr::EMPTY,
            };
            items.len()
        ];
        let table = self.table();
        // SAFETY: the host's own function with its context, on the plugin's thread; the dialog, its items,
        // lines and the handler's state live through the call; `values` has room for `count` values.
        let button = unsafe {
            (table.dialog)(
                table.context,
                &raw const raw,
                values.as_mut_ptr(),
                values.len(),
            )
        };
        if values.iter().all(|value| value.state == NOT_WRITTEN) {
            values.clear();
        }
        let values = (values.iter())
            .map(|value| Value {
                // One the host did not write is as an item's default.
                state: if value.state == NOT_WRITTEN {
                    0
                } else {
                    value.state
                },
                // SAFETY: the host's texts live until the plugin's next call to it.
                text: unsafe { value.text.as_str() }.to_string(),
            })
            .collect();
        Closed {
            button: index(button),
            values,
        }
    }
}
