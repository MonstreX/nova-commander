//! The console this process is attached to and the command interpreter (ADR-0006): Ctrl+C for the
//! programs the command runner starts, where the cursor is, `%COMSPEC%`; the console's title and a
//! flash of its taskbar button (ADR-0010); opening in the classic console when Windows chose a
//! Windows Terminal tab (owner, 2026-11-08).

use std::ffi::OsString;
use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;

use windows_sys::Win32::System::Console::{
    CONSOLE_CURSOR_INFO, CONSOLE_SCREEN_BUFFER_INFO, CONSOLE_SCREEN_BUFFER_INFOEX,
    CTRL_BREAK_EVENT, CTRL_C_EVENT, GetConsoleCP, GetConsoleCursorInfo, GetConsoleOutputCP,
    GetConsoleProcessList, GetConsoleScreenBufferInfo, GetConsoleScreenBufferInfoEx,
    GetConsoleTitleW, GetConsoleWindow, GetStdHandle, STD_OUTPUT_HANDLE, SetConsoleCP,
    SetConsoleCtrlHandler, SetConsoleCursorInfo, SetConsoleOutputCP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FLASHW_ALL, FLASHW_TIMERNOFG, FLASHWINFO, FlashWindowEx, GA_ROOTOWNER, GetAncestor,
    GetClassNameW, GetForegroundWindow,
};
use windows_sys::core::BOOL;

/// The height of the console's cursor in percent of the cell (FAR's `CONSOLE_CURSOR_INFO::dwSize`);
/// none without a console.
pub fn console_cursor_size() -> Option<u32> {
    let mut info = CONSOLE_CURSOR_INFO {
        dwSize: 0,
        bVisible: 0,
    };
    // SAFETY: GetStdHandle takes no pointers.
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    // SAFETY: `info` is a writable CONSOLE_CURSOR_INFO; a handle that is not a console fails.
    let read = unsafe { GetConsoleCursorInfo(output, &raw mut info) };
    (read != 0).then_some(info.dwSize)
}

/// FAR's `SetCursorType`: the cursor's height in percent of the cell (1 to 100), shown or hidden as
/// it is.
pub fn set_console_cursor_size(percent: u32) -> io::Result<()> {
    // SAFETY: GetStdHandle takes no pointers.
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let mut info = CONSOLE_CURSOR_INFO {
        dwSize: 0,
        bVisible: 0,
    };
    // SAFETY: `info` is a writable CONSOLE_CURSOR_INFO; a handle that is not a console fails.
    if unsafe { GetConsoleCursorInfo(output, &raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    info.dwSize = percent.clamp(1, 100);
    // SAFETY: `info` is a valid CONSOLE_CURSOR_INFO.
    if unsafe { SetConsoleCursorInfo(output, &raw const info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Given to the copy of the program started in the classic console: it does not start another.
const IN_PLAIN_CONSOLE: &str = "--in-plain-console";

/// The class of Windows Terminal's window, which owns the hidden window of a console it shows.
const TERMINAL_WINDOW: &str = "CASCADIA_HOSTING_WINDOW_CLASS";

/// A program started outside a console (Explorer, a shortcut) gets the default terminal, a tab of
/// Windows Terminal where that is the system's choice. The owner wants the plain window: this starts
/// the program again in the classic console and returns true, the caller then ends and the tab closes.
/// False when the program stays: run from a shell or from a terminal's own profile, or the classic
/// console could not be started.
pub fn reopen_in_plain_console() -> bool {
    let given: Vec<OsString> = std::env::args_os().skip(1).collect();
    if already_reopened(&given) || !given_a_terminal_tab() {
        return false;
    }
    let Ok(program) = std::env::current_exe() else {
        return false;
    };
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    Command::new(super::system_folder().join("System32\\conhost.exe"))
        .args(reopening_arguments(program.into_os_string(), given))
        // The standard streams stay as they are: redirected, the program in the new window would
        // get them too and could not start.
        .creation_flags(DETACHED_PROCESS)
        .spawn()
        .is_ok()
}

fn already_reopened(given: &[OsString]) -> bool {
    given.iter().any(|argument| argument == IN_PLAIN_CONSOLE)
}

/// What `conhost.exe` is given: after `--` everything is the program and its arguments.
fn reopening_arguments(program: OsString, given: Vec<OsString>) -> Vec<OsString> {
    let mut arguments = vec!["--".into(), program, IN_PLAIN_CONSOLE.into()];
    arguments.extend(given);
    arguments
}

/// Whether Windows gave this process a Windows Terminal tab of its own.
fn given_a_terminal_tab() -> bool {
    let mut processes = [0u32; 2];
    // SAFETY: the list has room for the two ids it is said to have.
    let attached = unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 2) };
    // SAFETY: GetConsoleWindow takes no arguments; GetAncestor fails on a null window.
    let owner = unsafe { GetAncestor(GetConsoleWindow(), GA_ROOTOWNER) };
    let mut class = [0u16; 64];
    let size = i32::try_from(class.len()).unwrap_or(0);
    // SAFETY: `class` holds `size` characters and outlives the call; a null window makes it fail.
    let length = unsafe { GetClassNameW(owner, class.as_mut_ptr(), size) };
    let length = usize::try_from(length).unwrap_or(0).min(class.len());
    is_terminal_tab(
        attached,
        std::env::var_os("WT_SESSION").is_some(),
        &String::from_utf16_lossy(&class[..length]),
    )
}

/// Alone in the console (no shell it was started from), not one of the terminal's own profiles (they
/// set `WT_SESSION`), in a window of Windows Terminal (not another pseudo console).
fn is_terminal_tab(attached: u32, in_a_profile: bool, owner_class: &str) -> bool {
    attached == 1 && !in_a_profile && owner_class == TERMINAL_WINDOW
}

/// Ctrl+Break would end the program on the spot: the console would keep its raw mode and the theme's
/// colours. The panels ignore it, as they do Ctrl+C.
pub fn survive_ctrl_break() {
    // SAFETY: `ignore_break` is a plain function that lives as long as the process.
    unsafe { SetConsoleCtrlHandler(Some(ignore_break), 1) };
}

/// Only these two: returning true for a closing console would hold the window up.
unsafe extern "system" fn ignore_break(kind: u32) -> BOOL {
    BOOL::from(kind == CTRL_BREAK_EVENT || kind == CTRL_C_EVENT)
}

/// The console's window as a number (Windows' `HWND`, for a plugin's menu of the system's to belong to,
/// FAR's `ACTL_GETFARHWND`); 0: none.
pub fn console_window() -> usize {
    // SAFETY: GetConsoleWindow takes no arguments.
    unsafe { GetConsoleWindow() as usize }
}

/// The console window's title now, to put it back when the program ends.
pub fn console_title() -> Option<String> {
    let mut buffer = vec![0u16; 1024];
    let size = u32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: `buffer` holds `size` characters and outlives the call.
    let length = unsafe { GetConsoleTitleW(buffer.as_mut_ptr(), size) };
    let length = usize::try_from(length).ok().filter(|&length| length > 0)?;
    Some(String::from_utf16_lossy(
        &buffer[..length.min(buffer.len())],
    ))
}

/// The console's 16 colours now, in the order of escape sequences (1 is red; the console's own table
/// has 1 blue). Under Windows Terminal this is the hidden console's table, not the terminal's scheme.
pub fn console_colors() -> Option<[(u8, u8, u8); 16]> {
    // SAFETY: GetStdHandle has no preconditions.
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    // SAFETY: an all-zero CONSOLE_SCREEN_BUFFER_INFOEX is a valid value of this plain C struct.
    let mut info: CONSOLE_SCREEN_BUFFER_INFOEX = unsafe { std::mem::zeroed() };
    info.cbSize = u32::try_from(std::mem::size_of::<CONSOLE_SCREEN_BUFFER_INFOEX>()).unwrap_or(0);
    // SAFETY: `info` is a live local with its size set; an invalid handle makes the call fail.
    let read = unsafe { GetConsoleScreenBufferInfoEx(output, &mut info) } != 0;
    read.then(|| {
        std::array::from_fn(|index| {
            let [red, green, blue, _] = info.ColorTable[console_table_index(index)].to_le_bytes();
            (red, green, blue)
        })
    })
}

/// Escape sequences count red, green, blue; the console's table blue, green, red.
const fn console_table_index(index: usize) -> usize {
    (index & 0b1010) | ((index & 1) << 2) | ((index & 0b100) >> 2)
}

/// FAR's `taskbar::flash`: when the console window is not in front, its taskbar button flashes until
/// it is (a task ended while the user was elsewhere). Windows Terminal hosts the console in a hidden
/// window; there this does nothing.
pub fn flash_console_window() {
    // SAFETY: neither call takes arguments.
    let (console, foreground) = unsafe { (GetConsoleWindow(), GetForegroundWindow()) };
    if console.is_null() || console == foreground {
        return;
    }
    let info = FLASHWINFO {
        cbSize: u32::try_from(std::mem::size_of::<FLASHWINFO>()).unwrap_or(0),
        hwnd: console,
        dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
        uCount: 5,
        dwTimeout: 0,
    };
    // SAFETY: `info` is a valid FLASHWINFO for the call.
    unsafe { FlashWindowEx(&raw const info) };
}

/// Programs started from now on stop on Ctrl+C, while this process survives it.
///
/// A process started in a pseudo console inherits "ignore Ctrl+C"; clearing that flag is what children
/// inherit, and the handler keeps this process alive so it can wait for them and report their end.
pub fn pass_ctrl_c_to_children() -> io::Result<()> {
    // SAFETY: a null handler with FALSE only clears the process's "ignore Ctrl+C" flag.
    let cleared = unsafe { SetConsoleCtrlHandler(None, 0) } != 0;
    // SAFETY: `survive_ctrl_c` is a plain function that lives as long as the process.
    let added = unsafe { SetConsoleCtrlHandler(Some(survive_ctrl_c), 1) } != 0;
    if cleared && added {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Handles Ctrl+C and Ctrl+Break for this process by doing nothing: the started program gets them.
unsafe extern "system" fn survive_ctrl_c(_kind: u32) -> BOOL {
    1
}

/// The cursor's column in this process's console output, `None` when the output is not a console.
pub fn console_cursor_column() -> Option<u16> {
    // SAFETY: GetStdHandle has no preconditions.
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    // SAFETY: an all-zero CONSOLE_SCREEN_BUFFER_INFO is a valid value of this plain C struct.
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a live local of the right type; an invalid handle makes the call fail.
    let read = unsafe { GetConsoleScreenBufferInfo(output, &mut info) } != 0;
    read.then(|| u16::try_from(info.dwCursorPosition.X).unwrap_or(0))
}

/// Makes `command` start with no console window to be seen.
pub fn without_console(command: &mut Command) -> &mut Command {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW)
}

/// Takes `names` out of this process's environment, so nothing it starts sees them.
pub fn forget_variables(names: &[&str]) {
    for name in names {
        // SAFETY: the environment of a Windows process is guarded by the system's own lock.
        unsafe { std::env::remove_var(name) };
    }
}

/// The input and output code pages of this process's console; `None` when it has no console.
pub fn console_code_pages() -> Option<(u32, u32)> {
    // SAFETY: no arguments; 0 means there is no console.
    let pages = unsafe { (GetConsoleCP(), GetConsoleOutputCP()) };
    (pages.0 != 0 && pages.1 != 0).then_some(pages)
}

/// Sets both code pages of this process's console; false when the system refuses them.
pub fn set_console_code_pages(input: u32, output: u32) -> bool {
    // SAFETY: plain values; an unknown code page makes the call fail.
    unsafe { SetConsoleCP(input) != 0 && SetConsoleOutputCP(output) != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_tab_windows_chose_is_left() {
        assert!(is_terminal_tab(1, false, TERMINAL_WINDOW), "from Explorer");
        assert!(!is_terminal_tab(2, false, TERMINAL_WINDOW), "from a shell");
        assert!(
            !is_terminal_tab(1, true, TERMINAL_WINDOW),
            "a terminal's profile"
        );
        assert!(
            !is_terminal_tab(1, false, "PseudoConsoleWindow"),
            "another pseudo console"
        );
        assert!(
            !is_terminal_tab(1, false, "ConsoleWindowClass"),
            "the classic console"
        );
    }

    #[test]
    fn the_program_goes_after_the_marker_and_keeps_its_arguments() {
        let arguments =
            reopening_arguments("C:\\a b\\vc.exe".into(), vec!["x y".into(), "z".into()]);
        let expected: Vec<OsString> = ["--", "C:\\a b\\vc.exe", IN_PLAIN_CONSOLE, "x y", "z"]
            .map(OsString::from)
            .to_vec();
        assert_eq!(arguments, expected);
    }

    #[test]
    fn the_reopened_program_does_not_reopen_again() {
        let arguments = reopening_arguments("vc.exe".into(), Vec::new());
        assert!(already_reopened(&arguments));
        assert!(!already_reopened(&["other".into()]));
    }

    #[test]
    fn red_and_blue_change_places_between_the_two_orders() {
        let table: Vec<usize> = (0..16).map(console_table_index).collect();
        assert_eq!(
            table,
            [0, 4, 2, 6, 1, 5, 3, 7, 8, 12, 10, 14, 9, 13, 11, 15]
        );
    }
}
