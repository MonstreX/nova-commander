//! A panel's entries deleted and its folders made by the plugin itself, its own questions shown (FAR's
//! `DeleteFilesW` and `MakeDirectoryW`, API 5): the panels that set [`PanelFlags::deletes`] and
//! [`PanelFlags::makes_folders`](crate::PanelFlags); a new panel for Shift+F1 (`OFP_CREATE`); the
//! commands on the file a panel was opened from (Shift+F3, `ProcessHostFileW`).

use std::ffi::c_void;

use crate::api::NcStr;
use crate::files::{code, refuse};
use crate::panel::host_text;
use crate::{GetMode, Plugin, state};

/// The host's `count` texts at `names` (none when null).
///
/// # Safety
/// `names` holds `count` of the host's texts, alive through the call.
unsafe fn texts(names: *const NcStr, count: usize) -> Vec<String> {
    if names.is_null() || count == 0 {
        return Vec::new();
    }
    // SAFETY: the caller promises `count` texts at `names`.
    let raw = unsafe { std::slice::from_raw_parts(names, count) };
    // SAFETY: each is the host's text, alive through the call.
    (raw.iter().map(|text| unsafe { host_text(*text) }))
        .map(str::to_string)
        .collect()
}

pub(crate) unsafe extern "C" fn delete_files<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    // SAFETY: the host's texts and names live through the call.
    let (instance, path, names) =
        unsafe { (host_text(instance), host_text(path), texts(names, count)) };
    code(
        state,
        |plugin| plugin.delete_files(instance, path, &names, GetMode::of(mode)),
        |_, ()| {},
    )
}

pub(crate) unsafe extern "C" fn make_folder<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    mode: u32,
    name: *mut NcStr,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    if name.is_null() {
        return refuse(state, "No name to make".to_string());
    }
    // SAFETY: the host's texts, its name's included, live through the call.
    let (instance, path, asked) =
        unsafe { (host_text(instance), host_text(path), host_text(*name)) };
    let asked = asked.to_string();
    code(
        state,
        |plugin| plugin.make_folder(instance, path, &asked, GetMode::of(mode)),
        |state, made: String| {
            // SAFETY: the host's place for the answer; the text is kept until the plugin's next call.
            unsafe { *name = state.kept.text(made) };
        },
    )
}

/// What the plugin's commands on the file its panel was opened from did (FAR's `ProcessHostFileW`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostFileDone {
    /// Something changed (FAR's TRUE): the selection goes, both panels are read again.
    pub changed: bool,
    /// The entries handed are no longer selected, nothing changed (FAR's `PPIF_SELECTED` cleared).
    pub unselect: bool,
}

impl HostFileDone {
    /// The ABI's `HOST_FILE_*` bits.
    pub fn bits(self) -> u32 {
        let mut bits = 0;
        if self.changed {
            bits |= crate::api::HOST_FILE_CHANGED;
        }
        if self.unselect {
            bits |= crate::api::HOST_FILE_UNSELECT;
        }
        bits
    }
}

pub(crate) unsafe extern "C" fn process_host_file<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    names: *const NcStr,
    count: usize,
    mode: u32,
    done: *mut u32,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    if done.is_null() {
        return refuse(state, "No place for the answer".to_string());
    }
    // SAFETY: the host's place for the answer: nothing done until the plugin says otherwise.
    unsafe { *done = 0 };
    // SAFETY: the host's texts and names live through the call.
    let (instance, path, names) =
        unsafe { (host_text(instance), host_text(path), texts(names, count)) };
    code(
        state,
        |plugin| plugin.process_host_file(instance, path, &names, GetMode::of(mode)),
        |_, answer: HostFileDone| {
            // SAFETY: the host's place for the answer, alive through the call.
            unsafe { *done = answer.bits() };
        },
    )
}

pub(crate) unsafe extern "C" fn open_new<P: Plugin>(raw: *mut c_void, instance: *mut NcStr) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    if instance.is_null() {
        return refuse(state, "No place for the panel".to_string());
    }
    code(
        state,
        |plugin| plugin.open_new(),
        |state, opened: Option<String>| {
            let made = opened.unwrap_or_default();
            // SAFETY: the host's place for the answer; the text is kept until the plugin's next call.
            unsafe { *instance = state.kept.text(made) };
        },
    )
}

#[cfg(test)]
#[path = "own_ops_tests.rs"]
mod tests;
