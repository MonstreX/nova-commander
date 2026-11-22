//! A process's handles as FAR's proclist lists them (`handles.cpp`, `PrintHandleInfo`): every handle of the
//! process from the system's table (`SystemExtendedHandleInformation`, whose process numbers are whole -
//! FAR's `SystemHandleInformation` keeps 16 bits of them), each copied into this process to ask its
//! object's type and name: a process's number, a thread's number and description, a file's or another
//! object's name. A file's name is asked on a thread of its own (a pipe waited on may hang the call, FAR's
//! `GetFileNameThread`): not answered in 100 ms, its I/O is broken (`CancelSynchronousIo`, never FAR's
//! `TerminateThread`, ADR-0019) and the file's kind stands for its name (`<pipe>`); a thread still hanging
//! is let go, its late answer thrown away, the copy closed at once as FAR closes it. A file that hung is
//! not asked again, and no more than 16 threads are left waiting.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use windows_sys::Wdk::Foundation::{NtQueryObject, ObjectTypeInformation};
use windows_sys::Wdk::System::SystemInformation::NtQuerySystemInformation;
use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_TYPE_CHAR, FILE_TYPE_DISK, FILE_TYPE_PIPE, FILE_TYPE_REMOTE, FILE_TYPE_UNKNOWN,
    GetFileType,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessId, GetThreadDescription, GetThreadId, PROCESS_DUP_HANDLE,
};

use super::process_open::{Opened, local_text, open};

/// What a handle's object is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleName {
    None,
    Text(String),
    /// A process, by its number (the caller names it from its list).
    Process(u32),
    Thread {
        id: u32,
        description: String,
    },
}

/// One handle of a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandleInfo {
    pub value: u64,
    pub access: u32,
    /// Its object's type (`File`, `Key`, `Event`); empty when it could not be asked.
    pub type_name: String,
    pub name: HandleName,
}

/// `SystemExtendedHandleInformation`.
const EXTENDED_HANDLES: i32 = 64;
/// `ObjectNameInformation`.
const OBJECT_NAME: i32 = 1;
/// `SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX`'s size, and the table's header before them.
const ENTRY: usize = 40;
const HEADER: usize = 16;
/// The buffer is too small.
const TOO_SMALL: [u32; 3] = [0xC000_0004, 0x8000_0005, 0xC000_0023];
/// How long a file's name is waited for (FAR's 100 ms), and then once more after its I/O is broken.
const NAME_WAIT: Duration = Duration::from_millis(100);
/// A disk file's name, which no pipe's wait holds, is waited for longer on a busy machine.
const DISK_NAME_WAIT: Duration = Duration::from_secs(1);

/// One entry of the system's table.
struct Entry {
    pid: u64,
    value: u64,
    access: u32,
}

/// The system's handle table, grown until it fits.
fn table() -> io::Result<Vec<u64>> {
    let mut words = 64 * 1024;
    loop {
        let mut buffer = vec![0u64; words];
        let size = u32::try_from(words * 8).unwrap_or(u32::MAX);
        let mut needed = 0u32;
        // SAFETY: the buffer is writable for `size` bytes and aligned; the size needed goes to a local.
        let status = unsafe {
            NtQuerySystemInformation(
                EXTENDED_HANDLES,
                buffer.as_mut_ptr().cast(),
                size,
                &raw mut needed,
            )
        };
        if status >= 0 {
            return Ok(buffer);
        }
        let code = status.cast_unsigned();
        if !TOO_SMALL.contains(&code) || words >= 256 * 1024 * 1024 {
            return Err(io::Error::other(format!("NTSTATUS 0x{code:08X}")));
        }
        // Handles come and go between the calls: some room over what was said.
        words = (usize::try_from(needed).unwrap_or(0) / 8).max(words * 2) + 16 * 1024;
    }
}

/// The table's entries of `pid`.
pub(crate) fn entries_of(buffer: &[u64], pid: u32) -> Vec<(u64, u32)> {
    let count = usize::try_from(buffer.first().copied().unwrap_or(0)).unwrap_or(0);
    let word = |at: usize| buffer.get(at / 8).copied().unwrap_or(0);
    (0..count)
        .map(|index| HEADER + index * ENTRY)
        .take_while(|at| at + ENTRY <= buffer.len() * 8)
        .map(|at| {
            let flags = word(at + 24);
            Entry {
                pid: word(at + 8),
                value: word(at + 16),
                access: u32::try_from(flags & 0xFFFF_FFFF).unwrap_or(0),
            }
        })
        .filter(|entry| entry.pid == u64::from(pid))
        .map(|entry| (entry.value, entry.access))
        .collect()
}

/// A handle copied into this process; closed when dropped.
struct Copied(HANDLE);

impl Drop for Copied {
    fn drop(&mut self) {
        // SAFETY: the handle is this value's own.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

fn copy(owner: &Opened, value: u64) -> Option<Copied> {
    let source: HANDLE = std::ptr::with_exposed_provenance_mut(usize::try_from(value).ok()?);
    let mut copied: HANDLE = std::ptr::null_mut();
    // SAFETY: the owner is open to copy its handles; the copy goes to a local.
    let done = unsafe {
        DuplicateHandle(
            owner.0,
            source,
            GetCurrentProcess(),
            &raw mut copied,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    (done != 0).then_some(Copied(copied))
}

/// The `UNICODE_STRING` an object's information starts with.
fn object_text(handle: HANDLE, class: i32) -> Option<String> {
    let mut buffer = vec![0u64; 1024];
    loop {
        let size = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
        let mut needed = 0u32;
        // SAFETY: the handle is this process's; the buffer is writable for `size` bytes, aligned.
        let status = unsafe {
            NtQueryObject(
                handle,
                class,
                buffer.as_mut_ptr().cast(),
                size,
                &raw mut needed,
            )
        };
        if status >= 0 {
            break;
        }
        let wanted = usize::try_from(needed).unwrap_or(0).div_ceil(8);
        if !TOO_SMALL.contains(&status.cast_unsigned()) || wanted <= buffer.len() {
            return None;
        }
        buffer.resize(wanted, 0);
    }
    let length = usize::try_from(buffer[0] & 0xFFFF).ok()?;
    let offset = usize::try_from(buffer[1])
        .ok()?
        .checked_sub(buffer.as_ptr() as usize)?;
    let bytes: Vec<u8> = buffer.iter().flat_map(|word| word.to_le_bytes()).collect();
    let text = bytes.get(offset..offset.checked_add(length)?)?;
    let units: Vec<u16> = (text.as_chunks::<2>().0.iter())
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// What a file's kind says when its name is not answered (FAR's `GetFileName`).
pub(crate) fn file_kind(kind: u32) -> String {
    match kind {
        FILE_TYPE_PIPE => "<pipe>".to_string(),
        FILE_TYPE_CHAR => "<char>".to_string(),
        FILE_TYPE_REMOTE => "<remote>".to_string(),
        FILE_TYPE_UNKNOWN => "<unknown>".to_string(),
        other => format!("<unknown> ({other})"),
    }
}

/// Naming threads given up and not yet back: at most so many (each waits on a hung call).
static ABANDONED: AtomicUsize = AtomicUsize::new(0);
const MOST_ABANDONED: usize = 16;

/// The files whose name hung once, by process and handle, with their kind: not asked again (the quick
/// view gathers the handles each second).
static HUNG: Mutex<Option<HashMap<(u32, u64), u32>>> = Mutex::new(None);

fn hung() -> std::sync::MutexGuard<'static, Option<HashMap<(u32, u64), u32>>> {
    HUNG.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A naming thread's state: running, done, or given up by the caller (who counts it in [`ABANDONED`]).
const RUNNING: u8 = 0;
const DONE: u8 = 1;
const GIVEN_UP: u8 = 2;

/// The naming thread's work: the kind, then - unless given up meanwhile - the name of the handle whose
/// value it is given (the caller owns the copy and closes it).
fn ask_name(
    value: usize,
    state: &AtomicU8,
    kinds: &mpsc::Sender<u32>,
    names: &mpsc::Sender<String>,
) {
    let handle: HANDLE = std::ptr::with_exposed_provenance_mut(value);
    // SAFETY: the caller's copy, open until it gives this thread up; a call already in the system keeps
    // its object alive past that.
    let kind = unsafe { GetFileType(handle) };
    // The caller may have given up and gone: nobody to tell.
    let _ = kinds.send(kind);
    if state.load(Ordering::SeqCst) == RUNNING {
        // As above: nobody may be waiting any more.
        let _ = names.send(object_text(handle, OBJECT_NAME).unwrap_or_default());
    }
    if state
        .compare_exchange(RUNNING, DONE, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        ABANDONED.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The thread given up: counted, unless it has just ended.
fn give_up(state: &AtomicU8) {
    // The thread let go runs on in this library: it stays loaded (R7 review A1: unloading the plugin
    // freed the code it ran, the program crashed when its call came back).
    super::keep_library_loaded();
    ABANDONED.fetch_add(1, Ordering::SeqCst);
    if state
        .compare_exchange(RUNNING, GIVEN_UP, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        ABANDONED.fetch_sub(1, Ordering::SeqCst);
    }
}

fn remember_hung(key: (u32, u64), kind: u32) {
    hung().get_or_insert_with(HashMap::new).insert(key, kind);
}

/// A file's name (FAR's `GetFileName`), asked on a thread of its own: a disk's file waited for a second,
/// another kind 100 ms, then its I/O broken and 100 ms more; not answered, its kind stands for it, the
/// thread is let go and the copy closed by the caller. A file that hung once, or too many threads let
/// go, give the kind at once.
fn file_name(copied: &Copied, key: (u32, u64)) -> String {
    let known = hung().as_ref().and_then(|files| files.get(&key).copied());
    if let Some(kind) = known {
        return file_kind(kind);
    }
    if ABANDONED.load(Ordering::SeqCst) >= MOST_ABANDONED {
        return file_kind(FILE_TYPE_UNKNOWN);
    }
    let (kinds, kind_told) = mpsc::channel();
    let (names, name_told) = mpsc::channel();
    let state = Arc::new(AtomicU8::new(RUNNING));
    let theirs = Arc::clone(&state);
    let value = copied.0 as usize;
    let asking = std::thread::Builder::new()
        .name("proclist file name".to_string())
        .spawn(move || ask_name(value, &theirs, &kinds, &names));
    let Ok(asking) = asking else {
        return String::new();
    };
    let Ok(kind) = kind_told.recv_timeout(NAME_WAIT) else {
        give_up(&state);
        remember_hung(key, FILE_TYPE_UNKNOWN);
        return file_kind(FILE_TYPE_UNKNOWN);
    };
    // A disk's file does not hang on a pipe's wait (FAR asks it at once): its name is waited for longer.
    let wait = if kind == FILE_TYPE_DISK {
        DISK_NAME_WAIT
    } else {
        NAME_WAIT
    };
    let name = name_told.recv_timeout(wait).ok().or_else(|| {
        break_io(&asking);
        name_told.recv_timeout(NAME_WAIT).ok()
    });
    match name {
        Some(name) => {
            // It has answered and ends at once; a panic in it gives no name either way.
            let _ = asking.join();
            if name.is_empty() {
                file_kind(kind)
            } else {
                name
            }
        }
        None => {
            give_up(&state);
            remember_hung(key, kind);
            file_kind(kind)
        }
    }
}

fn break_io<T>(thread: &std::thread::JoinHandle<T>) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::IO::CancelSynchronousIo;
    // SAFETY: the thread is alive or ended; its handle is valid while the `JoinHandle` is.
    unsafe { CancelSynchronousIo(thread.as_raw_handle()) };
}

fn thread_name(handle: HANDLE) -> HandleName {
    // SAFETY: the handle is this process's; no pointers.
    let id = unsafe { GetThreadId(handle) };
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: as above; the text is the system's, freed by `local_text`.
    let described = unsafe { GetThreadDescription(handle, &raw mut text) } >= 0;
    let description = if described {
        local_text(text)
    } else {
        String::new()
    };
    if id == 0 && description.is_empty() {
        return HandleName::None;
    }
    HandleName::Thread { id, description }
}

/// The name of the copied handle's object of `type_name` (FAR's `GetNameByType`); `key` the handle's
/// process and value.
fn name_of(copied: &Copied, type_name: &str, key: (u32, u64)) -> HandleName {
    match type_name.to_ascii_lowercase().as_str() {
        "" => HandleName::None,
        "process" => {
            // SAFETY: the handle is this process's; no pointers.
            let pid = unsafe { GetProcessId(copied.0) };
            if pid == 0 {
                HandleName::None
            } else {
                HandleName::Process(pid)
            }
        }
        "thread" => thread_name(copied.0),
        "file" => text_name(file_name(copied, key)),
        _ => text_name(object_text(copied.0, OBJECT_NAME).unwrap_or_default()),
    }
}

fn text_name(text: String) -> HandleName {
    if text.is_empty() {
        HandleName::None
    } else {
        HandleName::Text(text)
    }
}

/// `pid`'s handles in the system's order, asked until `stop` says to (what was asked by then). A process
/// that cannot be opened has its handles with no type or name (FAR's copy fails the same way).
pub fn process_handles(pid: u32, stop: &dyn Fn() -> bool) -> io::Result<Vec<HandleInfo>> {
    let buffer = table()?;
    let owner = open(pid, PROCESS_DUP_HANDLE).ok();
    let mut handles = Vec::new();
    for (value, access) in entries_of(&buffer, pid) {
        if stop() {
            break;
        }
        let mut info = HandleInfo {
            value,
            access,
            type_name: String::new(),
            name: HandleName::None,
        };
        if let Some(copied) = owner.as_ref().and_then(|owner| copy(owner, value)) {
            // Asked of each copy (FAR's `GetTypeToken`): a handle closed and its value used again since
            // the table was read is another object, of another type than the table's.
            info.type_name = object_text(copied.0, ObjectTypeInformation).unwrap_or_default();
            info.name = name_of(&copied, &info.type_name, (pid, value));
        }
        handles.push(info);
    }
    Ok(handles)
}

#[cfg(test)]
#[path = "system_handles_tests.rs"]
mod tests;
