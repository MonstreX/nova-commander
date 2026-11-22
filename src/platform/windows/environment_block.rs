//! The environment a process of this user would get now (FAR's `ReloadEnvironment` over
//! `os::env::provider::block`): `CreateEnvironmentBlock` of this process's token, the system's and the
//! user's variables from the registry only. FAR's block inherits its process's too, where its `set` has
//! put its changes; this program's `set` leaves the process alone (`terminal::environment`), so an
//! inherited block would bring back the values the program started with over them.

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::TOKEN_QUERY;
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// The variables of the fresh block, each name with its value; none when it cannot be made. The
/// hidden ones (`=C:`, the drives' folders of cmd) are left out: they are this process's own.
pub fn system_environment() -> Vec<(String, String)> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the current process's pseudo handle needs no closing; `token` receives a handle.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
        return Vec::new();
    }
    let mut block: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: `token` is open with TOKEN_QUERY; `block` receives the block the system allocates.
    let made = unsafe { CreateEnvironmentBlock(&raw mut block, token, 0) } != 0;
    // SAFETY: `token` was opened above and is closed once.
    unsafe { CloseHandle(token) };
    if !made || block.is_null() {
        return Vec::new();
    }
    // SAFETY: the system's block: UTF-16 strings, each ended by a NUL, the list by an empty one.
    let variables = unsafe { read_block(block.cast::<u16>()) };
    // SAFETY: the block CreateEnvironmentBlock made, destroyed once.
    unsafe { DestroyEnvironmentBlock(block) };
    variables
}

/// The `NAME=value` strings of an environment block.
///
/// # Safety
/// `block` points to UTF-16 strings, each ended by a NUL, the last followed by another NUL.
unsafe fn read_block(block: *const u16) -> Vec<(String, String)> {
    let mut variables = Vec::new();
    let mut at = block;
    loop {
        let mut length = 0;
        // SAFETY: inside the block: every string ends with a NUL before the block does.
        while unsafe { *at.add(length) } != 0 {
            length += 1;
        }
        if length == 0 {
            return variables;
        }
        // SAFETY: `length` units were just read from `at`.
        let units = unsafe { std::slice::from_raw_parts(at, length) };
        variables.extend(split(&String::from_utf16_lossy(units)));
        // SAFETY: past the string and its NUL: the next string or the block's ending NUL.
        at = unsafe { at.add(length + 1) };
    }
}

/// `NAME=value`; a name that starts with `=` is a hidden one.
fn split(text: &str) -> Option<(String, String)> {
    let (name, value) = text.split_once('=')?;
    (!name.is_empty()).then(|| (name.to_string(), value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_gives_its_variables_and_not_the_hidden_ones() {
        let block: Vec<u16> = "=C:=C:\\x\0Path=C:\\Windows;C:\\bin\0A=b=c\0\0"
            .encode_utf16()
            .collect();
        // SAFETY: a block as the system lays it out, alive for the call.
        let variables = unsafe { read_block(block.as_ptr()) };
        let pair = |name: &str, value: &str| (name.to_string(), value.to_string());
        assert_eq!(
            variables,
            [pair("Path", r"C:\Windows;C:\bin"), pair("A", "b=c")]
        );
    }

    #[test]
    fn the_users_block_has_the_systems_variables() {
        let variables = system_environment();
        let names: Vec<String> = variables
            .iter()
            .map(|(name, _)| name.to_uppercase())
            .collect();
        assert!(names.contains(&"SYSTEMROOT".to_string()), "{names:?}");
        assert!(names.contains(&"PATH".to_string()));
    }
}
