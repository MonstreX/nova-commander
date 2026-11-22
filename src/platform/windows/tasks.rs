//! FAR's task list (Ctrl+W, `plist.cpp`): the windows Alt+Tab shows with their processes
//! (`is_alttab_window`, `is_cloaked_window`), a process's image, a process ended
//! (`os::process::terminate_other`), a window brought to the front (`SwitchToWindow`).

use std::io;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_PROCESS_ABORTED, HANDLE, HWND, LPARAM, MAX_PATH,
};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    QueryFullProcessImageNameW, TerminateProcess,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOTOWNER, GWL_EXSTYLE, GetAncestor, GetLastActivePopup, GetWindowLongPtrW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_RESTORE,
    SetForegroundWindow, ShowWindowAsync, WS_EX_TOOLWINDOW,
};

use crate::platform::process::TaskWindow;

fn handle(window: usize) -> HWND {
    std::ptr::with_exposed_provenance_mut(window)
}

/// A process opened; closed when dropped.
struct Opened(HANDLE);

impl Opened {
    fn new(pid: u32, access: u32) -> io::Result<Self> {
        // SAFETY: no pointers; a process that cannot be opened gives null.
        let raw = unsafe { OpenProcess(access, 0, pid) };
        if raw.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(raw))
        }
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        // SAFETY: the handle is this value's own.
        unsafe { CloseHandle(self.0) };
    }
}

/// FAR's `is_cloaked_window`: hidden by the desktop window manager (another virtual desktop's, a
/// suspended store application's); a failed question is "no".
fn is_cloaked(window: HWND) -> bool {
    let mut cloaked = 0u32;
    let size = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4);
    // SAFETY: the value is a local of the size given.
    let result = unsafe {
        DwmGetWindowAttribute(
            window,
            u32::try_from(DWMWA_CLOAKED).unwrap_or(14),
            std::ptr::from_mut(&mut cloaked).cast(),
            size,
        )
    };
    result >= 0 && cloaked != 0
}

/// FAR's `is_alttab_window` (Raymond Chen's rule): visible, the last active popup of its root owner,
/// no tool window, not cloaked.
fn is_alt_tab(window: HWND) -> bool {
    // SAFETY: no pointers; each call answers for a window gone too.
    unsafe {
        if IsWindowVisible(window) == 0 {
            return false;
        }
        let mut tried = GetAncestor(window, GA_ROOTOWNER);
        let mut walk: HWND = std::ptr::null_mut();
        while tried != walk {
            walk = tried;
            tried = GetLastActivePopup(walk);
            if IsWindowVisible(tried) != 0 {
                break;
            }
        }
        if walk != window {
            return false;
        }
        let extended = GetWindowLongPtrW(window, GWL_EXSTYLE);
        if extended & isize::try_from(WS_EX_TOOLWINDOW).unwrap_or(0) != 0 {
            return false;
        }
    }
    !is_cloaked(window)
}

/// A window's title (FAR's `os::GetWindowText`).
fn title(window: HWND) -> String {
    let mut buffer = [0u16; 1024];
    let size = i32::try_from(buffer.len()).unwrap_or(i32::MAX);
    // SAFETY: the buffer holds `size` units; a window gone gives 0.
    let length = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), size) };
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

/// The windows Alt+Tab shows, in the system's order (FAR's `EnumWindowsProc`); an error when the
/// windows cannot be gone through.
pub fn task_windows() -> io::Result<Vec<TaskWindow>> {
    let mut found: Vec<TaskWindow> = Vec::new();
    // SAFETY: the callback gets the list's address back and runs within this call.
    let done = unsafe { EnumWindows(Some(each), std::ptr::from_mut(&mut found) as LPARAM) };
    if done == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(found)
}

/// One top window seen: kept when Alt+Tab would show it and its process is known.
unsafe extern "system" fn each(window: HWND, found: LPARAM) -> i32 {
    // SAFETY: `task_windows` passed its list's address, alive through `EnumWindows`.
    let found = unsafe { &mut *(found as *mut Vec<TaskWindow>) };
    if !is_alt_tab(window) {
        return 1;
    }
    let mut pid = 0u32;
    // SAFETY: the window is the system's; the id goes to a local.
    if unsafe { GetWindowThreadProcessId(window, &raw mut pid) } == 0 {
        return 1;
    }
    found.push(TaskWindow {
        window: window as usize,
        pid,
        title: title(window),
    });
    1
}

/// A process's image file (FAR's `get_process_name`); empty when it cannot be asked.
pub fn process_image(pid: u32) -> String {
    let Ok(process) = Opened::new(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
        return String::new();
    };
    let mut buffer = vec![0u16; 32 * 1024];
    let mut size = u32::try_from(buffer.len()).unwrap_or(MAX_PATH);
    // SAFETY: the process is open; the buffer holds `size` units, the length goes back into it.
    let done = unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &raw mut size,
        )
    };
    if done == 0 {
        return String::new();
    }
    let length = usize::try_from(size).unwrap_or(0).min(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

/// FAR's `terminate_other`: the process ended with `ERROR_PROCESS_ABORTED`.
pub fn terminate_process(pid: u32) -> io::Result<()> {
    let process = Opened::new(pid, PROCESS_TERMINATE)?;
    // SAFETY: the process is open for ending.
    if unsafe { TerminateProcess(process.0, ERROR_PROCESS_ABORTED) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// FAR's `SwitchToWindow`: to the front, restored when minimized. A window gone is refused by the
/// system.
pub fn switch_to_window(window: usize) {
    let window = handle(window);
    // SAFETY: no pointers.
    unsafe {
        SetForegroundWindow(window);
        if IsIconic(window) != 0 {
            ShowWindowAsync(window, SW_RESTORE);
        }
    }
}

#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
