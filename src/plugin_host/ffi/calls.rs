//! The host's functions a plugin calls (`NcHost`), on the plugin's thread: its windows sent to the program
//! and the answer waited for, its log written. While the plugin starts the program is not listening: a
//! window is Esc at once.

use std::ffi::c_void;
use std::sync::mpsc::channel;

use nc_plugin_api::{
    INPUT_BUTTONS, INPUT_EDIT_PATH, INPUT_ENABLE_EMPTY, INPUT_EXPAND_ENV, INPUT_NO_AMPERSAND,
    INPUT_NO_LAST_HISTORY, INPUT_PASSWORD, MENU_AUTO_HOTKEYS, MENU_ITEM_CHECKED,
    MENU_ITEM_DISABLED, MENU_ITEM_GRAYED, MENU_ITEM_HIDDEN, MENU_ITEM_SEPARATOR,
    MENU_REVERSE_AUTO_HOTKEYS, MENU_SHOW_AMPERSAND, MENU_WRAP, MESSAGE_BUTTONS,
    MESSAGE_BUTTONS_ABORT_RETRY_IGNORE, MESSAGE_BUTTONS_OK, MESSAGE_BUTTONS_OK_CANCEL,
    MESSAGE_BUTTONS_RETRY_CANCEL, MESSAGE_BUTTONS_YES_NO, MESSAGE_BUTTONS_YES_NO_CANCEL,
    MESSAGE_ERROR, MESSAGE_LEFT_ALIGN, MESSAGE_WARNING, NcMenu, NcMenuItem, NcStr,
};

use super::{HostContext, message_lines, text};
use crate::plugin_host::Event;
use crate::plugin_host::windows::{
    Answer, InputRequest, MenuLine, MenuRequest, MessageRequest, Request,
};

/// The most lines of a plugin's menu and the most break keys read.
const MOST_LINES: usize = 100_000;
const MOST_KEYS: usize = 100;
/// The most lines and buttons of a message read (the program shows what fits the screen).
const MOST_MESSAGE_LINES: usize = 1000;
const MOST_BUTTONS: usize = 100;

/// The context behind the host's pointer, for what any thread may ask (the progress, the stop).
///
/// # Safety
/// `context` is the `HostContext` boxed with the plugin's table, alive while it is.
pub(super) unsafe fn shared<'a>(context: *mut c_void) -> &'a HostContext {
    // SAFETY: as the caller promises.
    unsafe { &*context.cast::<HostContext>() }
}

/// The context behind the host's pointer when the plugin calls on its own thread; none from another
/// (review R45: its cells are that thread's alone; the call then answers as failed).
///
/// # Safety
/// As [`shared`].
pub(super) unsafe fn context<'a>(context: *mut c_void) -> Option<&'a HostContext> {
    // SAFETY: as the caller promises.
    let context = unsafe { shared(context) };
    context.on_its_thread().then_some(context)
}

/// `request` shown and its answer waited for; Esc when the program is not listening (the plugin starting,
/// the program ending) or the window went without one.
fn ask(context: &HostContext, request: Request) -> Answer {
    if !context.listens() {
        return Answer::Esc;
    }
    // A window asked for takes the place of one without buttons.
    context.busy.set(false);
    let (reply, answer) = channel();
    let event = Event::Ask {
        plugin: context.plugin.clone(),
        request,
        reply,
    };
    if context.events.send(event).is_err() {
        return Answer::Esc;
    }
    answer.recv().unwrap_or(Answer::Esc)
}

fn to_answer(index: Option<usize>) -> i32 {
    index
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1)
}

/// `NcHost::message`.
pub unsafe extern "C" fn message(
    context: *mut c_void,
    flags: u32,
    title: NcStr,
    body: NcStr,
    buttons: NcStr,
) -> i32 {
    // FAR's `FMSG_ERRORTYPE`: the thread's last error as the call comes, before anything changes it.
    let error = (flags & MESSAGE_ERROR != 0)
        .then(std::io::Error::last_os_error)
        .and_then(|error| system_error(&error));
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        // Another thread of the plugin's (review R45).
        return -1;
    };
    let mut lines = message_lines(&text(body));
    lines.truncate(MOST_MESSAGE_LINES);
    let mut buttons = button_set(flags).unwrap_or_else(|| message_lines(&text(buttons)));
    buttons.truncate(MOST_BUTTONS);
    let message = MessageRequest {
        title: text(title),
        lines,
        buttons,
        warning: flags & MESSAGE_WARNING != 0,
        left_align: flags & MESSAGE_LEFT_ALIGN != 0,
        error,
    };
    if message.buttons.is_empty() {
        return busy(context, message);
    }
    match ask(context, Request::Message(message)) {
        Answer::Button(button) => to_answer(Some(button)),
        _ => -1,
    }
}

/// FAR's `FMSG_MB_*` buttons; `None` without a set (or one it does not know: the plugin's own then).
fn button_set(flags: u32) -> Option<Vec<String>> {
    let labels: &[&str] = match flags & MESSAGE_BUTTONS {
        MESSAGE_BUTTONS_OK => &["OK"],
        MESSAGE_BUTTONS_OK_CANCEL => &["OK", "Cancel"],
        MESSAGE_BUTTONS_ABORT_RETRY_IGNORE => &["Abort", "Retry", "Ignore"],
        MESSAGE_BUTTONS_YES_NO => &["Yes", "No"],
        MESSAGE_BUTTONS_YES_NO_CANCEL => &["Yes", "No", "Cancel"],
        MESSAGE_BUTTONS_RETRY_CANCEL => &["Retry", "Cancel"],
        _ => return None,
    };
    Some(labels.iter().map(ToString::to_string).collect())
}

/// FAR's system error text (`os::format_error`): "0x00000005 - Access is denied."; none for no error.
pub(super) fn system_error(error: &std::io::Error) -> Option<String> {
    let code = error.raw_os_error().filter(|code| *code != 0)?;
    let full = error.to_string();
    let suffix = format!(" (os error {code})");
    let text = full.strip_suffix(&suffix).unwrap_or(&full);
    // The system without a text for it: std's own words ("OS Error 99 (FormatMessageW() returned ...)"),
    // FAR's "Unknown error" (`format_error_impl` empty).
    let text = if text.starts_with("OS Error ") {
        ""
    } else {
        text
    };
    let text = text.replace(['\r', '\n'], " ");
    let text = match text.trim_end() {
        "" => "Unknown error",
        text => text,
    };
    Some(format!("0x{code:08X} - {text}"))
}

/// A message without buttons: shown while the plugin works on, 0 at once (-1 when nobody listens).
fn busy(context: &HostContext, message: MessageRequest) -> i32 {
    if !context.listens() {
        return -1;
    }
    let event = Event::Busy {
        plugin: context.plugin.clone(),
        message,
        call: context.calls.running(),
    };
    if context.events.send(event).is_err() {
        return -1;
    }
    context.busy.set(true);
    0
}

/// `NcHost::menu`.
pub unsafe extern "C" fn menu(context: *mut c_void, menu: *const NcMenu, key: *mut i32) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        // Another thread of the plugin's (review R45).
        return -1;
    };
    let set_key = |value: i32| {
        if !key.is_null() {
            // SAFETY: the plugin hands a writable `i32` or null.
            unsafe { key.write(value) };
        }
    };
    set_key(-1);
    // SAFETY: the plugin hands its menu, alive through the call.
    let Some(request) = (unsafe { menu_request(menu) }) else {
        context
            .log
            .write("A menu the program cannot read: not shown");
        return -1;
    };
    match ask(context, Request::Menu(request)) {
        Answer::Chosen { item, key } => {
            set_key(to_answer(key));
            to_answer(item)
        }
        _ => -1,
    }
}

/// The plugin's menu read; `None` for a null one, a smaller (older) one, or lines or keys at null.
///
/// # Safety
/// `menu` is null or the plugin's menu: `count` items at `items`, `break_count` names at `break_keys`.
unsafe fn menu_request(menu: *const NcMenu) -> Option<MenuRequest> {
    if menu.is_null() {
        return None;
    }
    // SAFETY: the size comes first in any build's menu.
    if unsafe { (*menu).size } < size_of::<NcMenu>() {
        return None;
    }
    // SAFETY: a menu at least this build's size.
    let menu = unsafe { &*menu };
    let count = menu.count.min(MOST_LINES);
    let keys = menu.break_count.min(MOST_KEYS);
    if (count > 0 && menu.items.is_null()) || (keys > 0 && menu.break_keys.is_null()) {
        return None;
    }
    // SAFETY: `count` items at `items` (not null when there are any), alive through the call.
    let items = unsafe { slice(menu.items, count) };
    // SAFETY: as the items.
    let break_keys = unsafe { slice(menu.break_keys, keys) };
    Some(MenuRequest {
        title: text(menu.title),
        bottom: text(menu.bottom),
        lines: items.iter().map(menu_line).collect(),
        selected: menu.selected,
        break_keys: break_keys.iter().map(|key| text(*key)).collect(),
        auto_hotkeys: menu.flags & MENU_AUTO_HOTKEYS != 0,
        wrap: menu.flags & MENU_WRAP != 0,
        show_ampersand: menu.flags & MENU_SHOW_AMPERSAND != 0,
        reverse_auto_hotkeys: menu.flags & MENU_REVERSE_AUTO_HOTKEYS != 0,
        x: u16::try_from(menu.x).ok(),
        y: u16::try_from(menu.y).ok(),
        max_height: usize::try_from(menu.max_height)
            .ok()
            .filter(|&most| most > 0),
    })
}

/// The most values of one list taken from a plugin (a menu's items, its break keys, a dialog's items).
pub(super) const MOST_VALUES: usize = 1 << 20;

/// `count` values at `at`; none for 0, and none for more than [`MOST_VALUES`] (review R46: a plugin's
/// error, never read).
///
/// # Safety
/// `at` points to `count` readable values (any pointer when `count` is 0 or too many).
pub(super) unsafe fn slice<'a, T>(at: *const T, count: usize) -> &'a [T] {
    // A null pointer is no slice, whatever the count (R7 review A5).
    if count == 0 || count > MOST_VALUES || at.is_null() {
        return &[];
    }
    // SAFETY: as the caller promises.
    unsafe { std::slice::from_raw_parts(at, count) }
}

fn menu_line(item: &NcMenuItem) -> MenuLine {
    MenuLine {
        text: text(item.text),
        separator: item.flags & MENU_ITEM_SEPARATOR != 0,
        checked: item.flags & MENU_ITEM_CHECKED != 0,
        disabled: item.flags & MENU_ITEM_DISABLED != 0,
        grayed: item.flags & MENU_ITEM_GRAYED != 0,
        hidden: item.flags & MENU_ITEM_HIDDEN != 0,
    }
}

/// `NcHost::input`.
pub unsafe extern "C" fn input(
    context: *mut c_void,
    title: NcStr,
    prompt: NcStr,
    initial: NcStr,
    history: NcStr,
    flags: u32,
    out: *mut NcStr,
) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        // Another thread of the plugin's (review R45).
        return -1;
    };
    let request = InputRequest {
        title: text(title),
        prompt: text(prompt),
        text: text(initial),
        enable_empty: flags & INPUT_ENABLE_EMPTY != 0,
        password: flags & INPUT_PASSWORD != 0,
        expand_env: flags & INPUT_EXPAND_ENV != 0,
        buttons: flags & INPUT_BUTTONS != 0,
        no_ampersand: flags & INPUT_NO_AMPERSAND != 0,
        history: Some(text(history)).filter(|name| !name.is_empty()),
        no_last_history: flags & INPUT_NO_LAST_HISTORY != 0,
        edit_path: flags & INPUT_EDIT_PATH != 0,
    };
    let Answer::Text(typed) = ask(context, Request::Input(request)) else {
        return -1;
    };
    let mut answer = context.answer.borrow_mut();
    *answer = typed;
    if !out.is_null() {
        // SAFETY: the plugin hands a writable `NcStr` or null; the text stays in the context until the
        // next answer replaces it.
        unsafe { out.write(NcStr::new(&answer)) };
    }
    0
}

/// `NcHost::log`.
pub unsafe extern "C" fn log(context: *mut c_void, line: NcStr) {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        // Another thread of the plugin's (review R45).
        return;
    };
    context.log.write(&text(line));
}

/// `NcHost::progress` (ADR-0019 §5): the running call's progress, for the program's window over it.
pub unsafe extern "C" fn progress(context: *mut c_void, done: u64, total: u64, item: NcStr) {
    // SAFETY: the host's own context, handed back by the plugin; its call state is any thread's.
    let context = unsafe { shared(context) };
    context.calls.tell(crate::plugin_host::CallProgress {
        done,
        total,
        text: text(item),
    });
}

/// `NcHost::cancelled` (ADR-0019 §5): 1 when the user asked the running call to stop.
pub unsafe extern "C" fn cancelled(context: *mut c_void) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin; its call state is any thread's.
    let context = unsafe { shared(context) };
    i32::from(context.calls.cancelled())
}

/// `NcHost::resume` (API 5): the stop asked of the running call forgotten (the plugin's own "Interrupt?"
/// answered No).
pub unsafe extern "C" fn resume(context: *mut c_void) {
    // SAFETY: the host's own context, handed back by the plugin; its call state is any thread's.
    let context = unsafe { shared(context) };
    context.calls.resume();
}

#[cfg(test)]
#[path = "calls_tests.rs"]
mod tests;
