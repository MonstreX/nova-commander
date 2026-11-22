//! The helper process and its named pipe on Windows (ADR-0007, FAR's `elevation.cpp`): a pipe with a
//! random name, one instance, open to Administrators only; each end makes sure of the process at the
//! other; the helper cannot impersonate-escalate through the program's pipe, nor the program through its.

use std::collections::hash_map::RandomState;
use std::ffi::{OsStr, OsString};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Write};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED,
    ERROR_PIPE_NOT_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_ANONYMOUS, SECURITY_SQOS_PRESENT, WriteFile,
};
use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, WaitNamedPipeW,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetProcessId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::{wide, wide_os};
use crate::platform::elevation::PipeAccess;

/// Generic read and write for Administrators, nobody else, nothing inherited (FAR's
/// `make_explicit_admin_access`).
const ADMINISTRATORS_ONLY: &str = "D:P(A;;GRGW;;;BA)";
/// A write this small does not wait for the other side to read (with no buffer every write would).
const PIPE_BUFFER: u32 = 1 << 16;
/// How often the wait for the client looks whether its process lives.
const ACCEPT_SLICE: Duration = Duration::from_millis(100);
/// How long a helper gets to find the pipe.
const CONNECT_WAIT_MS: u32 = 15_000;

fn pipe_path(name: &str) -> Vec<u16> {
    wide(&format!(r"\\.\pipe\{name}"))
}

/// Whether an elevated helper could do more than this process (FAR's `is_admin`). Never in a test: on
/// some machines Windows does not ask.
pub fn can_elevate() -> bool {
    !cfg!(test) && !super::system::is_elevated()
}

/// Access denied, a privilege the process lacks (making a symbolic link), or an owner it may not give
/// (`ERROR_INVALID_OWNER`): what the rights cure (ADR-0007 §1, FAR's `ElevationRequired`).
pub fn needs_rights(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 1307 | 1314))
}

/// `nova-commander-elevation-<pid>-<random>`. The standard library's hash keys come from the
/// system's random numbers; the name only has to be hard to guess, the pipe's rights are the guard.
pub fn new_pipe_name() -> String {
    let random = || RandomState::new().build_hasher().finish();
    format!(
        "nova-commander-elevation-{}-{:016x}{:016x}",
        std::process::id(),
        random(),
        random()
    )
}

/// A path's exact form for the pipe: its UTF-16 code units, nothing lost.
pub fn encode_path(path: &Path) -> Vec<u8> {
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// `None` for bytes that are no such form. A NUL is none: a call would stop at it, at a shorter name.
pub fn decode_path(bytes: &[u8]) -> Option<PathBuf> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    (!units.contains(&0)).then(|| OsString::from_wide(&units).into())
}

/// An owned kernel handle.
#[derive(Debug)]
struct Owned(HANDLE);

// SAFETY: a kernel handle may be used from any thread; each `Owned` is used by one holder at a time.
unsafe impl Send for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle is owned here and closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Runs one overlapped call to its end; `Ok` with the bytes it moved. `timeout` in ms.
fn overlapped(
    handle: HANDLE,
    timeout: u32,
    start: impl FnOnce(*mut OVERLAPPED) -> i32,
) -> io::Result<u32> {
    // SAFETY: no name, no security attributes: a plain manual-reset event.
    let event = unsafe { CreateEventW(null(), 1, 0, null()) };
    if event.is_null() {
        return Err(io::Error::last_os_error());
    }
    let event = Owned(event);
    let mut state = OVERLAPPED {
        hEvent: event.0,
        ..Default::default()
    };
    if start(&raw mut state) == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != i32::try_from(ERROR_IO_PENDING).ok() {
            return Err(error);
        }
        // SAFETY: `event` is live.
        if unsafe { WaitForSingleObject(event.0, timeout) } == WAIT_TIMEOUT {
            // SAFETY: `state` is the operation started above, alive until the wait below ended.
            unsafe {
                CancelIoEx(handle, &raw const state);
                let mut moved = 0;
                GetOverlappedResult(handle, &raw const state, &raw mut moved, 1);
            }
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
    }
    let mut moved = 0;
    // SAFETY: the operation has ended; `state` and `moved` are live.
    if unsafe { GetOverlappedResult(handle, &raw const state, &raw mut moved, 1) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(moved)
}

/// The server's end of a pipe, one instance, before anyone connected.
#[derive(Debug)]
pub struct PipeListener {
    pipe: Owned,
}

impl PipeListener {
    /// Creates `\\.\pipe\<name>`, open to `access` only. Fails if the name is taken.
    pub fn create(name: &str, access: PipeAccess) -> io::Result<Self> {
        let path = pipe_path(name);
        let descriptor = PipeDescriptor::new(access)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        // SAFETY: `path` is NUL-terminated; `attributes` and its descriptor outlive the call.
        let pipe = unsafe {
            CreateNamedPipeW(
                path.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                PIPE_BUFFER,
                PIPE_BUFFER,
                0,
                &raw const attributes,
            )
        };
        if pipe == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { pipe: Owned(pipe) })
    }

    /// Waits for the client up to `timeout`. A client whose process id is not `expected_client`
    /// is refused with `PermissionDenied`.
    #[cfg(test)]
    pub fn accept(self, expected_client: u32, timeout: Duration) -> io::Result<PipeStream> {
        self.accept_while(expected_client, timeout, &|| true)
    }

    /// [`accept`](Self::accept), giving up at once when `alive` says the client's process is gone
    /// (FAR waits on the pipe and the process).
    pub fn accept_while(
        self,
        expected_client: u32,
        timeout: Duration,
        alive: &dyn Fn() -> bool,
    ) -> io::Result<PipeStream> {
        let pipe = self.pipe.0;
        let deadline = std::time::Instant::now() + timeout;
        let connected = loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let waited = u32::try_from(left.min(ACCEPT_SLICE).as_millis()).unwrap_or(u32::MAX);
            // SAFETY: `pipe` is live; `state` comes from `overlapped`, which keeps it alive to the end.
            let attempt = overlapped(pipe, waited, |state| unsafe {
                ConnectNamedPipe(pipe, state)
            });
            match attempt {
                Err(error)
                    if error.kind() == io::ErrorKind::TimedOut
                        && !left.is_zero()
                        && left > ACCEPT_SLICE =>
                {
                    if !alive() {
                        return Err(io::Error::from(io::ErrorKind::BrokenPipe));
                    }
                }
                attempt => break attempt,
            }
        };
        match connected {
            Ok(_) => {}
            // It connected between the creation and this call.
            Err(error) if error.raw_os_error() == i32::try_from(ERROR_PIPE_CONNECTED).ok() => {}
            Err(error) => return Err(error),
        }
        let mut client = 0;
        // SAFETY: `pipe` is live; `client` is a live local.
        if unsafe { GetNamedPipeClientProcessId(pipe, &raw mut client) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if client != expected_client {
            return Err(refused("the pipe's client is not the helper"));
        }
        Ok(PipeStream {
            pipe: self.pipe,
            overlapped: true,
        })
    }
}

fn refused(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, what)
}

/// The security descriptor of a new pipe; a null one is the creator's default rights.
struct PipeDescriptor(PSECURITY_DESCRIPTOR);

impl PipeDescriptor {
    fn new(access: PipeAccess) -> io::Result<Self> {
        match access {
            PipeAccess::Administrators => {
                let text = wide(ADMINISTRATORS_ONLY);
                let mut descriptor = null_mut();
                // SAFETY: `text` is NUL-terminated; `descriptor` is a live out pointer, the size may be null.
                let made = unsafe {
                    ConvertStringSecurityDescriptorToSecurityDescriptorW(
                        text.as_ptr(),
                        SDDL_REVISION_1,
                        &raw mut descriptor,
                        null_mut(),
                    )
                };
                if made == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Self(descriptor))
            }
            #[cfg(test)]
            PipeAccess::CurrentUser => Ok(Self(null_mut())),
        }
    }
}

impl Drop for PipeDescriptor {
    fn drop(&mut self) {
        // SAFETY: the descriptor came from the conversion above (`LocalFree` takes a null one too).
        unsafe { LocalFree(self.0) };
    }
}

/// A connected pipe. The other side's end shows as the end of the stream.
#[derive(Debug)]
pub struct PipeStream {
    pipe: Owned,
    overlapped: bool,
}

/// A pipe whose other side went: the end of the stream rather than an error.
fn ended(result: io::Result<u32>) -> io::Result<usize> {
    match result {
        Ok(moved) => Ok(moved as usize),
        Err(error)
            if error.raw_os_error() == i32::try_from(ERROR_BROKEN_PIPE).ok()
                || error.raw_os_error() == i32::try_from(ERROR_PIPE_NOT_CONNECTED).ok() =>
        {
            Ok(0)
        }
        Err(error) => Err(error),
    }
}

impl PipeStream {
    /// One read or write on the pipe, overlapped or not as it was opened; `Ok` with the bytes moved.
    fn transfer(
        &self,
        call: impl FnOnce(HANDLE, *mut u32, *mut OVERLAPPED) -> i32,
    ) -> io::Result<u32> {
        let pipe = self.pipe.0;
        if self.overlapped {
            let mut moved = 0;
            return overlapped(pipe, u32::MAX, |state| call(pipe, &raw mut moved, state));
        }
        let mut moved = 0;
        if call(pipe, &raw mut moved, null_mut()) == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(moved)
    }
}

impl Read for PipeStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        let start = buffer.as_mut_ptr();
        ended(self.transfer(|pipe, moved, state| {
            // SAFETY: `buffer` holds `length` bytes and outlives the call; `pipe` is live.
            unsafe { ReadFile(pipe, start, length, moved, state) }
        }))
    }
}

impl Write for PipeStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        let start = buffer.as_ptr();
        match ended(self.transfer(|pipe, moved, state| {
            // SAFETY: `buffer` holds `length` bytes and outlives the call; `pipe` is live.
            unsafe { WriteFile(pipe, start, length, moved, state) }
        })) {
            Ok(0) if length > 0 => Err(io::Error::from(io::ErrorKind::BrokenPipe)),
            result => result,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The helper's side: opens the pipe and goes on only if its server's process id is `server_pid` and
/// that process runs this same program file; `PermissionDenied` otherwise. The server gets no way to act
/// as this process (`SECURITY_ANONYMOUS`): it has fewer rights than the helper.
pub fn connect_pipe(name: &str, server_pid: u32) -> io::Result<PipeStream> {
    let path = pipe_path(name);
    // SAFETY: `path` is NUL-terminated. A pipe that is not there yet fails the open below.
    unsafe { WaitNamedPipeW(path.as_ptr(), CONNECT_WAIT_MS) };
    // SAFETY: `path` is NUL-terminated; no attributes or template.
    let pipe = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            null(),
            OPEN_EXISTING,
            SECURITY_SQOS_PRESENT | SECURITY_ANONYMOUS,
            null_mut(),
        )
    };
    if pipe == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let pipe = Owned(pipe);
    let mut server = 0;
    // SAFETY: `pipe` is live; `server` is a live local.
    if unsafe { GetNamedPipeServerProcessId(pipe.0, &raw mut server) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if server != server_pid || !runs_this_program(server_pid)? {
        return Err(refused("the pipe's server is not this program"));
    }
    Ok(PipeStream {
        pipe,
        overlapped: false,
    })
}

/// FAR's check of the parent: the same program file as this process.
fn runs_this_program(process_id: u32) -> io::Result<bool> {
    // SAFETY: plain values; a process that is gone makes the call fail.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = Owned(process);
    // SAFETY: the pseudo handle of this process needs no closing.
    let own = image_name(unsafe { GetCurrentProcess() })?;
    let other = image_name(process.0)?;
    Ok(own.eq_ignore_ascii_case(&other))
}

fn image_name(process: HANDLE) -> io::Result<OsString> {
    let mut name = [0u16; 32768];
    let mut length = name.len() as u32;
    // SAFETY: `process` is live; `name` holds `length` characters.
    let read = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            name.as_mut_ptr(),
            &raw mut length,
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(OsString::from_wide(&name[..length as usize]))
}

/// The helper process: ended with the program (FAR's job object: closing it kills what is in it).
#[derive(Debug)]
pub struct HelperProcess {
    process: Owned,
    _job: Option<super::job::KillJob>,
}

impl HelperProcess {
    pub fn process_id(&self) -> u32 {
        // SAFETY: the handle is live.
        unsafe { GetProcessId(self.process.0) }
    }

    pub fn is_running(&self) -> bool {
        // SAFETY: the handle is live; a zero wait only looks.
        unsafe { WaitForSingleObject(self.process.0, 0) == WAIT_TIMEOUT }
    }
}

/// Starts this program with `arguments` and the administrator's rights (`ShellExecuteExW`, `runas`).
/// `ERROR_CANCELLED` when the user refuses. No console and no window.
pub fn start_helper(arguments: &str) -> io::Result<HelperProcess> {
    // No test ever starts anything with the administrator's rights: on some machines Windows does
    // not ask.
    if cfg!(test) {
        return Err(io::Error::from(io::ErrorKind::Unsupported));
    }
    let program = std::env::current_exe()?;
    let file = wide_os(OsStr::new(&program));
    let parameters = wide(arguments);
    let verb = wide("runas");
    // Its own folder: as the program moves off a drive to be removed, the helper cannot.
    let directory = program.parent().map(|folder| wide_os(folder.as_os_str()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS,
        // SAFETY: no arguments; a process without a console gets a null window.
        hwnd: unsafe { GetConsoleWindow() },
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: parameters.as_ptr(),
        lpDirectory: directory.as_ref().map_or(null(), |folder| folder.as_ptr()),
        nShow: SW_HIDE,
        ..Default::default()
    };
    // SAFETY: `info` is a fully initialised SHELLEXECUTEINFOW of the right size; its strings are
    // NUL-terminated and live until the call returns.
    if unsafe { ShellExecuteExW(&raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.hProcess.is_null() {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }
    let process = Owned(info.hProcess);
    // `None` when the system does not allow it: then only the closing pipe ends the helper.
    let job = super::job::KillJob::around(process.0);
    Ok(HelperProcess { process, _job: job })
}

#[cfg(test)]
#[path = "elevation_tests.rs"]
mod tests;
