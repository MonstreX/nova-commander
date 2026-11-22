//! The files of a plugin's panel through the ABI (ADR-0016 §13, 5c-2): one entry, reading, writing,
//! making a folder, renaming, removing. Each call's failure is its kind's code and its text in
//! `last_error`; a panic is a failure too.

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::api::{
    ERROR_OTHER, ERROR_UNSUPPORTED, NcEntry, NcStr, NcTimes, REMOVE_FILE, REMOVE_FOLDER,
    WRITE_ABORT, WRITE_APPEND, WRITE_END, WRITE_NEW,
};
use crate::panel::{Error, Times, WriteMode, host_text, raw_entry};
use crate::{Plugin, State, panic_text, state};

/// A call's result as the ABI's code: 0, or the failure's kind with its text kept for `last_error`.
pub(crate) fn code<P, T>(
    state: &mut State<P>,
    call: impl FnOnce(&mut P) -> Result<T, Error>,
    done: impl FnOnce(&mut State<P>, T),
) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| call(&mut state.plugin))) {
        Ok(Ok(value)) => {
            done(state, value);
            0
        }
        Ok(Err(error)) => {
            let code = error.code();
            state.error = error.message;
            code
        }
        Err(_) => {
            state.error = panic_text();
            ERROR_OTHER
        }
    }
}

/// A value this build does not know: the call refused as unsupported.
pub(crate) fn refuse<P>(state: &mut State<P>, why: String) -> i32 {
    state.error = why;
    ERROR_UNSUPPORTED
}

pub(crate) unsafe extern "C" fn stat<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    entry: *mut NcEntry,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    // SAFETY: the host's texts live through the call.
    let (instance, path) = unsafe { (host_text(instance), host_text(path)) };
    code(
        state,
        |plugin| plugin.stat(instance, path),
        |state, found| {
            let made = raw_entry(found, &mut state.kept);
            if !entry.is_null() {
                // SAFETY: the host's place for the answer, alive through the call; the name is kept
                // until the plugin's next call.
                unsafe { *entry = made };
            }
        },
    )
}

pub(crate) unsafe extern "C" fn read<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    offset: u64,
    buffer: *mut u8,
    size: usize,
    done: *mut usize,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    // SAFETY: the host's texts live through the call.
    let (instance, path) = unsafe { (host_text(instance), host_text(path)) };
    let room = if buffer.is_null() { 0 } else { size };
    let buffer: &mut [u8] = if room == 0 {
        &mut []
    } else {
        // SAFETY: the host hands `size` writable bytes at `buffer`, its own, through the call.
        unsafe { std::slice::from_raw_parts_mut(buffer, size) }
    };
    code(
        state,
        |plugin| plugin.read(instance, path, offset, buffer),
        |_, read| {
            if !done.is_null() {
                // SAFETY: the host's place for the answer, alive through the call.
                unsafe { *done = read.min(room) };
            }
        },
    )
}

pub(crate) unsafe extern "C" fn write<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    mode: u32,
    data: *const u8,
    size: usize,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    // SAFETY: the host's texts live through the call.
    let (instance, path) = unsafe { (host_text(instance), host_text(path)) };
    let data: &[u8] = if data.is_null() || size == 0 {
        &[]
    } else {
        // SAFETY: the host hands `size` readable bytes at `data` through the call.
        unsafe { std::slice::from_raw_parts(data, size) }
    };
    let mode = match mode {
        WRITE_NEW => WriteMode::New,
        WRITE_APPEND => WriteMode::Append,
        WRITE_END => {
            return code(state, |plugin| plugin.write_end(instance, path), |_, ()| {});
        }
        WRITE_ABORT => {
            return code(
                state,
                |plugin| plugin.write_abort(instance, path),
                |_, ()| {},
            );
        }
        // A mode of a later host: never taken for another.
        _ => return refuse(state, format!("No write mode {mode}")),
    };
    code(
        state,
        |plugin| plugin.write(instance, path, mode, data),
        |_, ()| {},
    )
}

pub(crate) unsafe extern "C" fn make_dir<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    // SAFETY: the host's texts live through the call.
    let (instance, path) = unsafe { (host_text(instance), host_text(path)) };
    code(state, |plugin| plugin.make_dir(instance, path), |_, ()| {})
}

pub(crate) unsafe extern "C" fn rename<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    from: NcStr,
    to: NcStr,
    replace: u32,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    // SAFETY: the host's texts live through the call.
    let (instance, from, to) = unsafe { (host_text(instance), host_text(from), host_text(to)) };
    code(
        state,
        |plugin| plugin.rename(instance, from, to, replace != 0),
        |_, ()| {},
    )
}

pub(crate) unsafe extern "C" fn remove<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    how: u32,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    // SAFETY: the host's texts live through the call.
    let (instance, path) = unsafe { (host_text(instance), host_text(path)) };
    let folder = match how {
        REMOVE_FILE => false,
        REMOVE_FOLDER => true,
        // A removal of a later host (a Recycle Bin): never taken for removing for good.
        _ => return refuse(state, format!("No removal {how}")),
    };
    code(
        state,
        |plugin| plugin.remove(instance, path, folder),
        |_, ()| {},
    )
}

pub(crate) unsafe extern "C" fn set_times<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    times: *const NcTimes,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    // SAFETY: the host's texts live through the call.
    let (instance, path) = unsafe { (host_text(instance), host_text(path)) };
    if times.is_null() {
        return refuse(state, format!("No times for {path}"));
    }
    // SAFETY: the host's times, alive through the call.
    let times = Times::of(unsafe { &*times });
    code(
        state,
        |plugin| plugin.set_times(instance, path, &times),
        |_, ()| {},
    )
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
