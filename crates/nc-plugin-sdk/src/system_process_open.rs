//! What FAR's proclist asks of one process (`Plist.cpp`, `perfthread.cpp`): its image's path, its command
//! line, its bitness, its windows' objects, its owner, its priority class; ended (`KillProcess`). A
//! process the user may not open is opened again with the debug privilege where the user holds it (FAR's
//! `OpenProcessForced`, `DebugToken`): the plugin's thread impersonates a copy of the program's token with
//! the privilege on for that call only.

use std::io;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_NOT_ALL_ASSIGNED, FILETIME, GetLastError, HANDLE, LUID,
    LocalFree,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, DuplicateTokenEx, GetTokenInformation, LUID_AND_ATTRIBUTES,
    LookupAccountSidW, LookupPrivilegeValueW, PSID, SE_PRIVILEGE_ENABLED, SecurityImpersonation,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_DUPLICATE, TOKEN_IMPERSONATE, TOKEN_PRIVILEGES, TOKEN_QUERY,
    TOKEN_USER, TokenImpersonation, TokenUser,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, GetGuiResources, GetPriorityClass, GetProcessTimes,
    IsWow64Process, OpenProcess, OpenProcessToken, OpenThreadToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    PROCESS_TERMINATE, PROCESS_VM_READ, QueryFullProcessImageNameW, SetPriorityClass,
    SetThreadToken, TerminateProcess,
};

/// A process opened; closed when dropped.
pub(super) struct Opened(pub(super) HANDLE);

impl Drop for Opened {
    fn drop(&mut self) {
        // SAFETY: the handle is this value's own, open since it was made.
        unsafe { CloseHandle(self.0) };
    }
}

/// `pid` opened with `rights` (FAR's `OpenProcessForced`): denied, once more with the debug privilege.
pub(super) fn open(pid: u32, rights: u32) -> io::Result<Opened> {
    let attempt = || {
        // SAFETY: no pointers; a handle or null.
        let handle = unsafe { OpenProcess(rights, 0, pid) };
        if handle.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Opened(handle))
        }
    };
    match attempt() {
        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED.cast_signed()) => {
            with_debug(attempt).unwrap_or(Err(error))
        }
        other => other,
    }
}

/// What a process tells of itself; each part `None` (0) when it could not be read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessDetails {
    /// Its image's whole path.
    pub path: Option<String>,
    pub command_line: Option<String>,
    /// A 32-bit process on 64-bit Windows.
    pub wow64: bool,
    /// Its GDI and USER objects (FAR's `GetGuiResources`).
    pub gdi_objects: u32,
    pub user_objects: u32,
    /// Its start in 100 ns units since 1601.
    pub created: Option<i64>,
    pub owner: Option<Owner>,
}

/// The account a process runs as.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Owner {
    pub user: String,
    pub domain: String,
    /// `S-1-5-...`.
    pub sid: String,
}

/// What `pid` tells of itself; `None` when it cannot be opened at all.
pub fn process_details(pid: u32) -> Option<ProcessDetails> {
    let process = open(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)
        .or_else(|_| open(pid, PROCESS_QUERY_LIMITED_INFORMATION))
        .ok()?;
    let handle = process.0;
    let mut wow64 = 0;
    // SAFETY: the handle is open; the answer goes to a local.
    let wow64 = unsafe { IsWow64Process(handle, &raw mut wow64) } != 0 && wow64 != 0;
    // SAFETY: as above; no pointers.
    let (gdi_objects, user_objects) =
        unsafe { (GetGuiResources(handle, 0), GetGuiResources(handle, 1)) };
    Some(ProcessDetails {
        path: image_path(handle),
        command_line: command_line(handle),
        wow64,
        gdi_objects,
        user_objects,
        created: created(handle),
        owner: owner(handle),
    })
}

fn image_path(handle: HANDLE) -> Option<String> {
    let mut buffer = vec![0u16; 32 * 1024];
    let mut length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: the buffer holds `length` units; the length written back.
    let done = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &raw mut length,
        )
    };
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    (done != 0 && length > 0).then(|| String::from_utf16_lossy(&buffer[..length]))
}

/// The command line (`ProcessCommandLineInformation`: a `UNICODE_STRING` whose text follows it in the
/// buffer), the buffer grown to what the system says it needs.
fn command_line(handle: HANDLE) -> Option<String> {
    use windows_sys::Wdk::System::Threading::{
        NtQueryInformationProcess, ProcessCommandLineInformation,
    };
    // 2 KB first: most command lines fit, a longer one grows the buffer.
    let mut buffer = vec![0u64; 256];
    loop {
        let size = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
        let mut needed = 0u32;
        // SAFETY: the buffer is writable for `size` bytes and aligned; the size needed goes to a local.
        let status = unsafe {
            NtQueryInformationProcess(
                handle,
                ProcessCommandLineInformation,
                buffer.as_mut_ptr().cast(),
                size,
                &raw mut needed,
            )
        };
        if status >= 0 {
            return unicode_text(&buffer);
        }
        let wanted = usize::try_from(needed).unwrap_or(0).div_ceil(8);
        if !TOO_SMALL.contains(&status.cast_unsigned()) || wanted <= buffer.len() {
            return None;
        }
        buffer.resize(wanted, 0);
    }
}

/// The buffer is too small (`STATUS_INFO_LENGTH_MISMATCH`, `STATUS_BUFFER_OVERFLOW`,
/// `STATUS_BUFFER_TOO_SMALL`).
const TOO_SMALL: [u32; 3] = [0xC000_0004, 0x8000_0005, 0xC000_0023];

/// The text of the `UNICODE_STRING` at the buffer's start (its length in bytes, then its text's
/// address), read where that address points within the buffer; none when it points elsewhere.
fn unicode_text(buffer: &[u64]) -> Option<String> {
    let bytes: Vec<u8> = buffer.iter().flat_map(|word| word.to_le_bytes()).collect();
    let length = usize::try_from(buffer.first()? & 0xFFFF).ok()?;
    let address = usize::try_from(*buffer.get(1)?).ok()?;
    let offset = address.checked_sub(buffer.as_ptr() as usize)?;
    let text = bytes.get(offset..offset.checked_add(length)?)?;
    let units: Vec<u16> = (text.chunks_exact(2))
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    (!units.is_empty()).then(|| String::from_utf16_lossy(&units))
}

fn created(handle: HANDLE) -> Option<i64> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut start, mut end, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: the handle is open; the four times go to locals.
    let done = unsafe {
        GetProcessTimes(
            handle,
            &raw mut start,
            &raw mut end,
            &raw mut kernel,
            &raw mut user,
        )
    };
    let ticks = (u64::from(start.dwHighDateTime) << 32) | u64::from(start.dwLowDateTime);
    (done != 0 && ticks != 0).then(|| ticks.cast_signed())
}

/// A token opened; closed when dropped.
struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: the handle is this value's own.
        unsafe { CloseHandle(self.0) };
    }
}

/// The account of the process's token (FAR asks WMI's `GetOwner`; the token says the same at once).
fn owner(handle: HANDLE) -> Option<Owner> {
    let mut raw: HANDLE = std::ptr::null_mut();
    // SAFETY: the process is open; the token goes to a local.
    if unsafe { OpenProcessToken(handle, TOKEN_QUERY, &raw mut raw) } == 0 {
        return None;
    }
    let token = Token(raw);
    let mut buffer = vec![0u64; 128];
    let mut needed = 0u32;
    let size = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
    // SAFETY: the buffer is writable for `size` bytes and aligned for a `TOKEN_USER`.
    let done = unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &raw mut needed,
        )
    };
    if done == 0 {
        return None;
    }
    // SAFETY: the system wrote a `TOKEN_USER` at the buffer's start (aligned); its SID points into the
    // buffer, alive through the lookup.
    let sid: PSID = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    account(sid)
}

/// The account and domain of `sid` and its text (`LookupAccountSidW`, `ConvertSidToStringSidW`); `sid`
/// points into a buffer alive through the call.
fn account(sid: PSID) -> Option<Owner> {
    let (mut user, mut domain) = (vec![0u16; 256], vec![0u16; 256]);
    let (mut user_length, mut domain_length) = (256u32, 256u32);
    let mut kind = 0;
    // SAFETY: the SID is the token's; both buffers hold the lengths given, written back.
    let found = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid,
            user.as_mut_ptr(),
            &raw mut user_length,
            domain.as_mut_ptr(),
            &raw mut domain_length,
            &raw mut kind,
        )
    };
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: as above; the text is the system's, freed by `local_text`.
    let converted = unsafe { ConvertSidToStringSidW(sid, &raw mut text) } != 0;
    let sid = if converted {
        local_text(text)
    } else {
        String::new()
    };
    let cut = |units: &[u16], length: u32| {
        let length = usize::try_from(length).unwrap_or(0).min(units.len());
        String::from_utf16_lossy(&units[..length])
    };
    let (user, domain) = if found == 0 {
        (String::new(), String::new())
    } else {
        (cut(&user, user_length), cut(&domain, domain_length))
    };
    (!user.is_empty() || !sid.is_empty()).then_some(Owner { user, domain, sid })
}

/// The debug token's handle, made once for the library's life (FAR's `CreateToken` as it starts).
#[derive(Clone, Copy)]
struct Shared(usize);

static DEBUG_TOKEN: std::sync::OnceLock<Option<Shared>> = std::sync::OnceLock::new();

/// `f` done with the debug privilege on for this thread (FAR's `DebugToken::Enable` and `Revert`);
/// `None` when the user does not hold it, or the thread's own token cannot be kept to put back.
fn with_debug<T>(f: impl FnOnce() -> T) -> Option<T> {
    use windows_sys::Win32::Foundation::ERROR_NO_TOKEN;
    use windows_sys::Win32::Security::RevertToSelf;
    let shared = (*DEBUG_TOKEN.get_or_init(|| {
        debug_token().map(|token| {
            let shared = Shared(token.0 as usize);
            std::mem::forget(token);
            shared
        })
    }))?;
    let token: HANDLE = std::ptr::with_exposed_provenance_mut(shared.0);
    let mut saved: HANDLE = std::ptr::null_mut();
    // SAFETY: the thread's own impersonation token, if any, goes to a local.
    let had =
        unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_IMPERSONATE, 1, &raw mut saved) } != 0;
    // SAFETY: no pointers; the call's own error.
    if !had && unsafe { GetLastError() } != ERROR_NO_TOKEN {
        return None;
    }
    let saved = had.then_some(Token(saved));
    // SAFETY: the token is an impersonation token of the program's own, open for the library's life;
    // null: this thread.
    if unsafe { SetThreadToken(std::ptr::null(), token) } == 0 {
        return None;
    }
    let result = f();
    let back = saved.as_ref().map_or(std::ptr::null_mut(), |token| token.0);
    // SAFETY: as above: the thread's earlier token back, or none. Should that fail, the thread stops
    // impersonating at all rather than keep the debug privilege.
    if unsafe { SetThreadToken(std::ptr::null(), back) } == 0 {
        // SAFETY: no pointers; this thread's impersonation ended.
        unsafe { RevertToSelf() };
    }
    Some(result)
}

/// A copy of the program's token to impersonate, the debug privilege on (FAR's `CreateToken`); `None`
/// when the user does not hold the privilege.
fn debug_token() -> Option<Token> {
    let mut process: HANDLE = std::ptr::null_mut();
    // SAFETY: the program's own process; the token goes to a local.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_DUPLICATE, &raw mut process) } == 0 {
        return None;
    }
    let process = Token(process);
    let mut copy: HANDLE = std::ptr::null_mut();
    let rights = TOKEN_IMPERSONATE | TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES;
    // SAFETY: the token is open; no security attributes; the copy goes to a local.
    let copied = unsafe {
        DuplicateTokenEx(
            process.0,
            rights,
            std::ptr::null(),
            SecurityImpersonation,
            TokenImpersonation,
            &raw mut copy,
        )
    };
    if copied == 0 {
        return None;
    }
    let copy = Token(copy);
    let name: Vec<u16> = "SeDebugPrivilege".encode_utf16().chain([0]).collect();
    let mut privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: LUID {
                LowPart: 0,
                HighPart: 0,
            },
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    // SAFETY: the name is NUL-terminated; the LUID goes into the local structure.
    if unsafe {
        LookupPrivilegeValueW(
            std::ptr::null(),
            name.as_ptr(),
            &raw mut privileges.Privileges[0].Luid,
        )
    } == 0
    {
        return None;
    }
    // SAFETY: the copy is open with the right to adjust; no earlier state asked for.
    let adjusted = unsafe {
        AdjustTokenPrivileges(
            copy.0,
            0,
            &raw const privileges,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    // SAFETY: no pointers; the adjustment's own error.
    let all = adjusted != 0 && unsafe { GetLastError() } != ERROR_NOT_ALL_ASSIGNED;
    all.then_some(copy)
}

/// How `pid` could not be ended.
#[derive(Debug)]
pub enum KillError {
    /// The user may not end it; the debug privilege might let them (FAR asks "Retry with debug
    /// privileges?").
    Denied,
    Other(io::Error),
}

/// `pid` ended (FAR's `KillProcess`: `TerminateProcess` with `ERROR_PROCESS_ABORTED`), with the debug
/// privilege when `debug`.
pub fn terminate(pid: u32, debug: bool) -> Result<(), KillError> {
    const ERROR_PROCESS_ABORTED: u32 = 1067;
    let attempt = || {
        // SAFETY: no pointers; a handle or null.
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let process = Opened(handle);
        // SAFETY: the handle is open with the right to end it.
        if unsafe { TerminateProcess(process.0, ERROR_PROCESS_ABORTED) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    };
    let done = if debug {
        with_debug(attempt).unwrap_or_else(|| {
            Err(io::Error::from_raw_os_error(
                ERROR_ACCESS_DENIED.cast_signed(),
            ))
        })
    } else {
        attempt()
    };
    done.map_err(|error| match error.raw_os_error() {
        Some(code) if code == ERROR_ACCESS_DENIED.cast_signed() && !debug => KillError::Denied,
        _ => KillError::Other(error),
    })
}

/// Windows' priority classes in FAR's order (lowest first) with the base priorities they give.
pub const PRIORITY_CLASSES: [(u32, i32); 6] = [
    (0x40, 4),    // IDLE_PRIORITY_CLASS
    (0x4000, 6),  // BELOW_NORMAL_PRIORITY_CLASS
    (0x20, 8),    // NORMAL_PRIORITY_CLASS
    (0x8000, 10), // ABOVE_NORMAL_PRIORITY_CLASS
    (0x80, 13),   // HIGH_PRIORITY_CLASS
    (0x100, 24),  // REALTIME_PRIORITY_CLASS
];

/// `pid`'s priority class one step down (`up` false, FAR's Shift+F1) or up (Shift+F2); `Ok(false)` at the
/// end of the scale or for a class not among Windows' six.
pub fn step_priority(pid: u32, up: bool) -> io::Result<bool> {
    let process = open(pid, PROCESS_QUERY_INFORMATION | PROCESS_SET_INFORMATION)?;
    // SAFETY: the handle is open with the right to read its class.
    let class = unsafe { GetPriorityClass(process.0) };
    if class == 0 {
        return Err(io::Error::last_os_error());
    }
    let Some(at) = PRIORITY_CLASSES.iter().position(|(one, _)| *one == class) else {
        return Ok(false);
    };
    let next = if up {
        at.checked_add(1)
    } else {
        at.checked_sub(1)
    };
    let Some(&(next, _)) = next.and_then(|next| PRIORITY_CLASSES.get(next)) else {
        return Ok(false);
    };
    // SAFETY: the handle is open with the right to set its class.
    if unsafe { SetPriorityClass(process.0, next) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(true)
}

/// A text the system gave as a `LocalAlloc`'s, freed.
pub(crate) fn local_text(text: *mut u16) -> String {
    if text.is_null() {
        return String::new();
    }
    let mut length = 0usize;
    // SAFETY: the system's text is NUL-terminated.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: `length` units were just read; the text is the system's to free with `LocalFree`.
    let units = unsafe { std::slice::from_raw_parts(text, length) };
    let owned = String::from_utf16_lossy(units);
    // SAFETY: the system allocated it with `LocalAlloc`.
    unsafe { LocalFree(text.cast()) };
    owned
}
