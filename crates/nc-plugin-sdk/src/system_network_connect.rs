//! Connections as FAR's Network plugin makes and ends them (`AddConnection`, `AddConnectionWithLogon`,
//! `CancelConnection`, `ConfirmCancelConnection`, `GetFreeLetter`): a share connected as a disk, with
//! the user's credentials or a name and password given (`WNetAddConnection2`), remembered at logon or
//! not; a connection ended (`WNetCancelConnection2`); whether a disk is restored at logon
//! (`HKCU\Network\X`); the disks there are (`GetLogicalDrives`).

use std::io;

use windows_sys::Win32::Foundation::NO_ERROR;
use windows_sys::Win32::NetworkManagement::WNet::{
    CONNECT_UPDATE_PROFILE, WNetAddConnection2W, WNetCancelConnection2W,
};
use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, RegCloseKey, RegOpenKeyExW,
};

use super::{NetResource, Raw, TYPE_ANY, TYPE_DISK, failed, wide};

/// The system's codes a connection's failure is told apart by (`io::Error::raw_os_error`).
pub const ERROR_ACCESS_DENIED: i32 = 5;
pub const ERROR_INVALID_HANDLE: i32 = 6;
pub const ERROR_INVALID_PASSWORD: i32 = 86;
pub const ERROR_DEVICE_ALREADY_REMEMBERED: i32 = 1202;
pub const ERROR_SESSION_CREDENTIAL_CONFLICT: i32 = 1219;
pub const ERROR_CANCELLED: i32 = 1223;
pub const ERROR_LOGON_FAILURE: i32 = 1326;
pub const ERROR_LOGON_TYPE_NOT_GRANTED: i32 = 1385;
pub const ERROR_OPEN_FILES: i32 = 2401;
pub const ERROR_DEVICE_IN_USE: i32 = 2404;

/// A user's name and password a connection is made as (FAR's "User name", "User password").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Logon<'a> {
    /// Empty: the user's own name, with the password given.
    pub user: &'a str,
    pub password: &'a str,
}

/// The share `remote` connected as the disk `local` (`X:`; empty: with no letter, a server's `\\SERVER`
/// too), as `logon` (none: the user's own credentials), restored at logon when `remember`.
pub fn connect(
    local: &str,
    remote: &str,
    logon: Option<Logon<'_>>,
    remember: bool,
) -> io::Result<()> {
    // A letter takes a disk; with none, any resource (the system's rule for `dwType`).
    let kind = if local.is_empty() {
        TYPE_ANY
    } else {
        TYPE_DISK
    };
    let resource = NetResource {
        kind,
        local_name: local.to_string(),
        remote_name: remote.to_string(),
        ..NetResource::default()
    };
    add(&resource, logon, remember)
}

/// A connection to `resource` with the user's own credentials, asking nothing and remembering
/// nothing (FAR's `AddConnection` before it asks for a name and password).
pub fn add_connection(resource: &NetResource) -> io::Result<()> {
    add(resource, None, false)
}

/// `WNetAddConnection2` for `resource`.
fn add(resource: &NetResource, logon: Option<Logon<'_>>, remember: bool) -> io::Result<()> {
    let raw = Raw::new(resource);
    let user = logon.map(|logon| wide(logon.user));
    let password = logon.map(|logon| wide(logon.password));
    let text = |given: Option<&Vec<u16>>, empty_null: bool| {
        given
            .filter(|text| !(empty_null && text.len() <= 1))
            .map_or(std::ptr::null(), |text| text.as_ptr())
    };
    let flags = if remember { CONNECT_UPDATE_PROFILE } else { 0 };
    // SAFETY: the resource, its texts, the name and the password are alive through the call; a null
    // name or password is the user's own (FAR passes no name when none was typed).
    let result = unsafe {
        WNetAddConnection2W(
            &raw.resource,
            text(password.as_ref(), false),
            text(user.as_ref(), true),
            flags,
        )
    };
    if result == NO_ERROR {
        Ok(())
    } else {
        Err(failed(result))
    }
}

/// The connection `name` (a disk `X:` or a share's `\\SERVER\share`) ended; `forget` drops it from the
/// connections restored at logon, `force` ends it with files open on it.
pub fn disconnect(name: &str, forget: bool, force: bool) -> io::Result<()> {
    let name = wide(name);
    let flags = if forget { CONNECT_UPDATE_PROFILE } else { 0 };
    // SAFETY: `name` is NUL-terminated and alive through the call.
    let result = unsafe { WNetCancelConnection2W(name.as_ptr(), flags, i32::from(force)) };
    if result == NO_ERROR {
        Ok(())
    } else {
        Err(failed(result))
    }
}

/// Whether the disk `letter` is a connection restored at logon (`HKCU\Network\X`, FAR's
/// `ConfirmCancelConnection`).
pub fn is_remembered(letter: char) -> bool {
    let key_name = wide(&format!(r"Network\{}", letter.to_ascii_uppercase()));
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `key_name` is NUL-terminated; `key` is a valid out pointer.
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key_name.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &raw mut key,
        )
    };
    if opened != NO_ERROR {
        return false;
    }
    // SAFETY: `key` was opened above and is closed once.
    unsafe { RegCloseKey(key) };
    true
}

/// The disks there are, a bit for each letter from A (`GetLogicalDrives`).
pub fn used_letters() -> u32 {
    // SAFETY: no arguments; a mask comes back.
    unsafe { GetLogicalDrives() }
}

/// The letters of `used` free from `from` (FAR's `GetFreeLetter` starts at C, the letter menu at A).
pub fn free_letters(used: u32, from: char) -> Vec<char> {
    ('A'..='Z')
        .zip(0u32..)
        .filter(|(letter, bit)| *letter >= from && used & (1 << bit) == 0)
        .map(|(letter, _)| letter)
        .collect()
}
