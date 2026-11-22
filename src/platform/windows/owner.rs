//! A file's owner (FAR's `get_file_owner` / `set_file_owner`, `far/platform.security.cpp`): read as
//! `DOMAIN\name` (the SID's text when no account has it), set by an account's name or a SID's text with
//! the privileges to take ownership and to restore turned on for the call, as FAR does.

use std::io;
use std::path::{Component, Path, Prefix};
use std::sync::{Mutex, PoisonError};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, HANDLE, LUID, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSidToSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, EqualSid, LUID_AND_ATTRIBUTES, LookupAccountNameW, LookupAccountSidW,
    LookupPrivilegeValueW, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::{wide, wide_path};

/// An account's name found for a SID on a computer (`None`: this one).
type KnownName = (Option<String>, Vec<u8>, String);

/// The owner of `path` as FAR shows it; `None` when it cannot be read. Names found are kept, as FAR's
/// `SIDCache`: a selection of many files by one owner asks the accounts once.
pub(super) fn owner(path: &Path) -> Option<String> {
    static NAMES: Mutex<Vec<KnownName>> = Mutex::new(Vec::new());
    let owned = Owner::read(path).ok()?;
    let computer = computer(path);
    let sid = sid_bytes(owned.sid);
    let mut names = NAMES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, _, name)) =
        (names.iter()).find(|(on, bytes, _)| *on == computer && *bytes == sid)
    {
        return Some(name.clone());
    }
    let name = sid_to_name(owned.sid, computer.as_deref())?;
    names.push((computer, sid, name.clone()));
    Some(name)
}

/// Makes `name` (`DOMAIN\name`, `name` or a SID's text) the owner of `path`; nothing is written when it
/// already is.
pub fn set_owner(path: &Path, name: &str) -> io::Result<()> {
    let computer = computer(path);
    let sid = name_to_sid(name, computer.as_deref())?;
    let sid_at = sid.as_ptr().cast_mut().cast();
    if let Ok(owned) = Owner::read(path) {
        // SAFETY: both SIDs are valid: one in `sid`, the other in the descriptor `owned` holds.
        if unsafe { EqualSid(owned.sid, sid_at) } != 0 {
            return Ok(());
        }
    }
    let _privileges = [
        Privilege::enable("SeTakeOwnershipPrivilege"),
        Privilege::enable("SeRestorePrivilege"),
    ];
    let name = wide_path(path);
    // SAFETY: `name` is NUL-terminated; `sid` outlives the call; the group, DACL and SACL are unused.
    let result = unsafe {
        SetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            sid_at,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    win32(result)
}

/// The owner's SID inside the descriptor the system gave, freed with it.
struct Owner {
    sid: PSID,
    descriptor: PSECURITY_DESCRIPTOR,
}

impl Owner {
    fn read(path: &Path) -> io::Result<Self> {
        let name = wide_path(path);
        let mut sid: PSID = std::ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `name` is NUL-terminated; the out pointers are live locals; the group and ACLs are
        // not asked for.
        let result = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &raw mut sid,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut descriptor,
            )
        };
        win32(result)?;
        if sid.is_null() {
            // SAFETY: the descriptor came from the call above and is freed once, here.
            unsafe { LocalFree(descriptor) };
            return Err(io::Error::other("the file has no owner"));
        }
        Ok(Self { sid, descriptor })
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: the descriptor came from `GetNamedSecurityInfoW` and is freed once, here.
        unsafe { LocalFree(self.descriptor) };
    }
}

/// The computer that keeps the accounts of a network path (FAR's `ExtractComputerName`).
fn computer(path: &Path) -> Option<String> {
    match path.components().next()? {
        Component::Prefix(prefix) => match prefix.kind() {
            Prefix::UNC(server, _) | Prefix::VerbatimUNC(server, _) => {
                Some(server.to_string_lossy().into_owned())
            }
            _ => None,
        },
        _ => None,
    }
}

fn sid_to_name(sid: PSID, computer: Option<&str>) -> Option<String> {
    let computer = computer.map(wide);
    let computer_at = computer
        .as_ref()
        .map_or(std::ptr::null(), |name| name.as_ptr());
    let mut name = vec![0u16; 256];
    let mut domain = vec![0u16; 256];
    loop {
        let mut name_length = u32::try_from(name.len()).unwrap_or(0);
        let mut domain_length = u32::try_from(domain.len()).unwrap_or(0);
        let mut kind = 0;
        // SAFETY: the buffers hold the lengths given; `sid` is valid; `computer_at` is null or
        // NUL-terminated.
        let found = unsafe {
            LookupAccountSidW(
                computer_at,
                sid,
                name.as_mut_ptr(),
                &raw mut name_length,
                domain.as_mut_ptr(),
                &raw mut domain_length,
                &raw mut kind,
            )
        } != 0;
        if found {
            let text = |units: &[u16], length: u32| {
                String::from_utf16_lossy(&units[..usize::try_from(length).unwrap_or(0)])
            };
            let account = text(&name, name_length);
            return Some(match text(&domain, domain_length) {
                domain if domain.is_empty() => account,
                domain => format!("{domain}\\{account}"),
            });
        }
        if io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
            return sid_text(sid);
        }
        grow(&mut name, name_length);
        grow(&mut domain, domain_length);
    }
}

/// A SID as text (`S-1-5-...`).
fn sid_text(sid: PSID) -> Option<String> {
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: `sid` is valid; `text` receives a buffer the system allocates.
    if unsafe { ConvertSidToStringSidW(sid, &raw mut text) } == 0 {
        return None;
    }
    let mut length = 0;
    // SAFETY: the system's buffer is NUL-terminated: reading stops at its NUL.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: `length` units were just read from the buffer.
    let units = unsafe { std::slice::from_raw_parts(text, length) };
    let result = String::from_utf16_lossy(units);
    // SAFETY: the buffer came from `ConvertSidToStringSidW` and is freed once, here.
    unsafe { LocalFree(text.cast()) };
    Some(result)
}

/// The SID of an account's name, or of a SID's text, as bytes.
fn name_to_sid(name: &str, computer: Option<&str>) -> io::Result<Vec<u8>> {
    let account = wide(name);
    let computer = computer.map(wide);
    let computer_at = computer
        .as_ref()
        .map_or(std::ptr::null(), |name| name.as_ptr());
    let mut sid = vec![0u8; 68];
    let mut domain = vec![0u16; 256];
    loop {
        let mut sid_length = u32::try_from(sid.len()).unwrap_or(0);
        let mut domain_length = u32::try_from(domain.len()).unwrap_or(0);
        let mut kind = 0;
        // SAFETY: the buffers hold the lengths given; the names are NUL-terminated or null.
        let found = unsafe {
            LookupAccountNameW(
                computer_at,
                account.as_ptr(),
                sid.as_mut_ptr().cast(),
                &raw mut sid_length,
                domain.as_mut_ptr(),
                &raw mut domain_length,
                &raw mut kind,
            )
        } != 0;
        if found {
            return Ok(sid);
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
            return sid_from_text(&account).ok_or(error);
        }
        grow(&mut sid, sid_length);
        grow(&mut domain, domain_length);
    }
}

fn sid_from_text(text: &[u16]) -> Option<Vec<u8>> {
    let mut sid: PSID = std::ptr::null_mut();
    // SAFETY: `text` is NUL-terminated; `sid` receives a buffer the system allocates.
    if unsafe { ConvertStringSidToSidW(text.as_ptr(), &raw mut sid) } == 0 {
        return None;
    }
    let bytes = sid_bytes(sid);
    // SAFETY: the buffer came from `ConvertStringSidToSidW` and is freed once, here.
    unsafe { LocalFree(sid) };
    Some(bytes)
}

/// A valid SID's bytes: revision, the count of sub-authorities, 6 bytes of authority, 4 bytes per
/// sub-authority.
fn sid_bytes(sid: PSID) -> Vec<u8> {
    // SAFETY: the SID is valid (from the system); its second byte is the count.
    let count = usize::from(unsafe { *sid.cast::<u8>().add(1) });
    // SAFETY: a valid SID is exactly this long.
    unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), 8 + 4 * count) }.to_vec()
}

/// A privilege of this process turned on until the value goes; nothing when the user has not got it.
pub(super) struct Privilege {
    token: HANDLE,
    previous: TOKEN_PRIVILEGES,
}

impl Privilege {
    pub(super) fn enable(name: &str) -> Option<Self> {
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        let name = wide(name);
        // SAFETY: `name` is NUL-terminated; `luid` is a live local.
        if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &raw mut luid) } == 0 {
            return None;
        }
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the current process's pseudo handle needs no closing; `token` receives a handle.
        let opened = unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &raw mut token,
            )
        };
        if opened == 0 {
            return None;
        }
        let state = |attributes| TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: attributes,
            }],
        };
        let enabled = state(SE_PRIVILEGE_ENABLED);
        let mut previous = state(0);
        let mut length = 0;
        let size = u32::try_from(size_of::<TOKEN_PRIVILEGES>()).unwrap_or(0);
        // SAFETY: `token` is open; the states are valid one-privilege lists of `size` bytes.
        let adjusted = unsafe {
            AdjustTokenPrivileges(
                token,
                0,
                &raw const enabled,
                size,
                &raw mut previous,
                &raw mut length,
            )
        };
        // Without the privilege the call succeeds and says so in the last error.
        let held = adjusted != 0 && io::Error::last_os_error().raw_os_error() == Some(0);
        if !held {
            // SAFETY: `token` was opened above and is closed once, here.
            unsafe { CloseHandle(token) };
            return None;
        }
        Some(Self { token, previous })
    }
}

impl Drop for Privilege {
    fn drop(&mut self) {
        // SAFETY: `token` is open; `previous` is the state the enabling call returned.
        unsafe {
            AdjustTokenPrivileges(
                self.token,
                0,
                &raw const self.previous,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            CloseHandle(self.token);
        }
    }
}

/// A buffer made as long as the system asked for, and at least twice as long as it was.
fn grow<T: Clone + Default>(buffer: &mut Vec<T>, needed: u32) {
    let needed = usize::try_from(needed).unwrap_or(0);
    let length = needed.max(buffer.len() * 2);
    buffer.resize(length, T::default());
}

fn win32(result: u32) -> io::Result<()> {
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(
            i32::try_from(result).unwrap_or(0),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_file_is_owned_by_its_maker_and_setting_the_same_owner_is_a_no_op() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("a.txt");
        std::fs::write(&file, "a").unwrap();
        let owner = owner(&file).expect("an owner");
        assert!(!owner.is_empty());
        set_owner(&file, &owner).unwrap();
        assert_eq!(super::owner(&file).as_deref(), Some(owner.as_str()));
    }

    #[test]
    fn an_unknown_account_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("a.txt");
        std::fs::write(&file, "a").unwrap();
        assert!(set_owner(&file, "no-such-account-6f1c").is_err());
    }

    #[test]
    fn a_sids_text_names_its_account() {
        let sid = sid_from_text(&wide("S-1-5-32-544")).unwrap();
        let name = sid_to_name(sid.as_ptr().cast_mut().cast(), None).unwrap();
        assert!(
            name.ends_with("\\Administrators") || name.contains('\\'),
            "{name}"
        );
        assert_eq!(name_to_sid(&name, None).unwrap()[..sid.len()], sid[..]);
    }

    #[test]
    fn network_paths_name_their_computer() {
        assert_eq!(
            computer(Path::new(r"\\srv\share\x")).as_deref(),
            Some("srv")
        );
        assert_eq!(computer(Path::new(r"C:\x")), None);
    }
}
