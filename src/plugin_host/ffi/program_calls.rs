//! The host's functions beyond the panels and the windows (API 3, P6.0e; FAR's `AdvControl`,
//! `FileFilterControl`, `CopyToClipboard`, `ShowHelp`): `NcHost::synchro` (any thread), `post_keys`,
//! `screen`, `filter`, `clipboard` and `show_help` (the plugin's thread), each asked of the program as the
//! panels' requests are and its answer waited for.

use std::ffi::c_void;

use nc_plugin_api::{
    NcEntry, NcScreen, NcStr, PLUGINS_FIND, PLUGINS_LOAD, PLUGINS_UNLOAD, TRANSFER_MOVE,
    TRANSFER_UPLOAD,
};

use super::calls::{MOST_VALUES, context, shared, slice};
use super::panel_calls::entry;
use super::panel_control_calls::{ask, which};
use super::text;
use crate::plugin_host::Event;
use crate::plugin_host::panel_requests::{
    PanelAnswer, PanelRequest, PluginsAction, TransferRequest,
};

/// `NcHost::synchro` (FAR's `ACTL_SYNCHRO`): any thread of the plugin's; the program sends the plugin's
/// thread the call back.
pub unsafe extern "C" fn synchro(context: *mut c_void, param: u64) {
    // SAFETY: the host's own context, handed back by the plugin; its sender is any thread's.
    let context = unsafe { shared(context) };
    // A plugin let go asks nothing of the program any more.
    if context.calls.is_let_go() {
        return;
    }
    let _ = context.events.send(Event::Synchro {
        plugin: context.plugin.clone(),
        param,
    });
}

/// `request` asked on the plugin's thread: done (0) or not (-1).
fn done(raw: *mut c_void, request: PanelRequest) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { context(raw) }) else {
        return -1;
    };
    match ask(context, request) {
        Some(PanelAnswer::Done(true)) => 0,
        _ => -1,
    }
}

/// `NcHost::post_keys` (FAR's `ACTL_POSTKEYSEQUENCE`).
pub unsafe extern "C" fn post_keys(context: *mut c_void, keys: NcStr) -> i32 {
    done(context, PanelRequest::PostKeys(text(keys)))
}

/// `NcHost::transfer` (API 9, NetBox's "Transfer on background"): the copy handed to the program's
/// background tasks.
pub unsafe extern "C" fn transfer(
    context: *mut c_void,
    instance: NcStr,
    folder: NcStr,
    names: *const NcStr,
    count: usize,
    disk: NcStr,
    flags: u32,
) -> i32 {
    // R7 review A5: no names with a count is refused, not read from address zero.
    if names.is_null() && count > 0 {
        return -1;
    }
    // SAFETY: the plugin promises `count` texts at `names`, alive through the call.
    let names = unsafe { slice(names, count) }
        .iter()
        .map(|name| text(*name));
    let request = PanelRequest::Transfer(Box::new(TransferRequest {
        instance: text(instance),
        folder: text(folder),
        names: names.collect(),
        disk: text(disk),
        upload: flags & TRANSFER_UPLOAD != 0,
        moving: flags & TRANSFER_MOVE != 0,
    }));
    done(context, request)
}

/// `NcHost::clipboard` (FAR's `CopyToClipboard`).
pub unsafe extern "C" fn clipboard(context: *mut c_void, value: NcStr) -> i32 {
    done(context, PanelRequest::Clipboard(text(value)))
}

/// `NcHost::show_help` (FAR's `ShowHelp`).
pub unsafe extern "C" fn show_help(context: *mut c_void, topic: NcStr) -> i32 {
    done(context, PanelRequest::Help(text(topic)))
}

/// `NcHost::plugins_control` (FAR's `PluginsControl`); an action this build does not know: -1.
pub unsafe extern "C" fn plugins_control(context: *mut c_void, action: u32, value: NcStr) -> i32 {
    let action = match action {
        PLUGINS_FIND => PluginsAction::Find,
        PLUGINS_LOAD => PluginsAction::Load,
        PLUGINS_UNLOAD => PluginsAction::Unload,
        _ => return -1,
    };
    done(context, PanelRequest::Plugins(action, text(value)))
}

/// `NcHost::confirmation` (FAR's `FSSF_CONFIRMATIONS`): 1, 0, -1 for a name not known.
pub unsafe extern "C" fn confirmation(context: *mut c_void, name: NcStr) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        return -1;
    };
    match ask(context, PanelRequest::Confirmation(text(name))) {
        Some(PanelAnswer::Confirms(Some(asks))) => i32::from(asks),
        _ => -1,
    }
}

/// `NcHost::match_masks` (FAR's `ProcessName` with `PN_CMPNAMELIST`): 1, 0, -1 for masks with errors.
pub unsafe extern "C" fn match_masks(context: *mut c_void, masks: NcStr, name: NcStr) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        return -1;
    };
    let request = PanelRequest::MatchMasks {
        masks: text(masks),
        name: text(name),
    };
    match ask(context, request) {
        Some(PanelAnswer::Confirms(Some(matches))) => i32::from(matches),
        _ => -1,
    }
}

/// `NcHost::screen` (FAR's `ACTL_GETFARRECT`, `ACTL_GETFARHWND`).
pub unsafe extern "C" fn screen(context: *mut c_void, out: *mut NcScreen) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        return -1;
    };
    let Some(PanelAnswer::Screen { columns, rows }) = ask(context, PanelRequest::Screen) else {
        return -1;
    };
    if !out.is_null() {
        let screen = NcScreen {
            columns: u32::from(columns),
            rows: u32::from(rows),
            window: crate::platform::process::console_window(),
        };
        // SAFETY: the plugin hands a writable place or null.
        unsafe { out.write(screen) };
    }
    0
}

/// `NcHost::filter` (FAR's `FileFilterControl` with `FFT_PANEL`).
pub unsafe extern "C" fn filter(
    context: *mut c_void,
    which_raw: u32,
    entries: *const NcEntry,
    count: usize,
    results: *mut u8,
) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        return -1;
    };
    let Some(which) = which(which_raw) else {
        return -1;
    };
    // More than the most read at once: asked in batches (a larger one would be read as none).
    if count > MOST_VALUES || (count > 0 && (results.is_null() || entries.is_null())) {
        return -1;
    }
    // SAFETY: the plugin hands `count` entries at `entries` (not null when there are any, checked above),
    // their texts alive through the call; each is copied now.
    let raw = unsafe { slice(entries, count) };
    let entries = (raw.iter().enumerate())
        .map(|(at, raw)| entry(raw, at))
        .collect();
    let Some(PanelAnswer::Matches(matches)) = ask(context, PanelRequest::Filter { which, entries })
    else {
        return -1;
    };
    if matches.len() != count {
        return -1;
    }
    for (at, taken) in matches.into_iter().take(count).enumerate() {
        // SAFETY: the plugin hands `count` writable results; `at` is below it.
        unsafe { results.add(at).write(u8::from(taken)) };
    }
    0
}

#[cfg(test)]
#[path = "program_calls_tests.rs"]
mod tests;
