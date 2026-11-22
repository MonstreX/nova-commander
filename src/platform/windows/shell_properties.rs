//! "System properties" of FAR's "Attributes" dialog (`ShellSetFileAttributesImpl`): Windows' own
//! properties window of a file or a folder, with its Security tab (ADR-0011 §4).

use std::io;
use std::path::Path;

use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::UI::Shell::{SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::com::Apartment;
use super::{wide, wide_path};

/// Opens the shell's properties window of `path` beside the console and returns at once (FAR's
/// `ShellExecuteEx` with the verb "properties"); a folder's name ends with a separator, as FAR gives it.
pub fn show_properties(path: &Path, folder: bool) -> io::Result<()> {
    let _apartment = Apartment::enter();
    let mut name = path.as_os_str().to_os_string();
    if folder && !name.to_string_lossy().ends_with('\\') {
        name.push("\\");
    }
    let file = wide_path(Path::new(&name));
    let verb = wide("properties");
    let directory = std::env::current_dir().map(|dir| wide_path(&dir)).ok();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).unwrap_or(0),
        fMask: SEE_MASK_INVOKEIDLIST,
        // SAFETY: no arguments; a process without a console gets a null window.
        hwnd: unsafe { GetConsoleWindow() },
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpDirectory: directory
            .as_ref()
            .map_or(std::ptr::null(), |dir| dir.as_ptr()),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: `info` is a fully initialised SHELLEXECUTEINFOW of the right size; its strings are
    // NUL-terminated and live until the call returns.
    if unsafe { ShellExecuteExW(&raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
