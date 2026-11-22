//! The console's input as FAR reads it: its records one by one (`ReadConsoleInputW`), under the input
//! mode of FAR's `interf.cpp`.

use std::ffi::c_void;
use std::io;
use std::ptr::null;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    CONSOLE_SCREEN_BUFFER_INFO, DOUBLE_CLICK, ENABLE_ECHO_INPUT, ENABLE_EXTENDED_FLAGS,
    ENABLE_LINE_INPUT, ENABLE_MOUSE_INPUT, ENABLE_PROCESSED_INPUT, ENABLE_QUICK_EDIT_MODE,
    ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_WINDOW_INPUT, FOCUS_EVENT, FROM_LEFT_1ST_BUTTON_PRESSED,
    FROM_LEFT_2ND_BUTTON_PRESSED, GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle,
    INPUT_RECORD, KEY_EVENT, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, MOUSE_EVENT, MOUSE_EVENT_RECORD,
    MOUSE_HWHEELED, MOUSE_MOVED, MOUSE_WHEELED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED,
    RIGHTMOST_BUTTON_PRESSED, ReadConsoleInputW, SHIFT_PRESSED, STD_OUTPUT_HANDLE, SetConsoleMode,
    WINDOW_BUFFER_SIZE_EVENT,
};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

use crate::platform::input::{
    ConsoleRecord, ControlKeys, KeyRecord, MouseButtons, MouseKind, MouseRecord,
};

/// The program reads the console itself here.
pub const READS_CONSOLE: bool = true;

/// No mode is saved in [`SAVED_MODE`].
const NOT_SAVED: u64 = u64::MAX;
/// The mode the console's input had before the program's: an atomic, the panic hook reads it too.
static SAVED_MODE: AtomicU64 = AtomicU64::new(NOT_SAVED);

/// `CONIN$`, whatever the standard input was redirected to; open for the whole run.
fn input_handle() -> io::Result<HANDLE> {
    static OPENED: OnceLock<usize> = OnceLock::new();
    let handle = *OPENED.get_or_init(|| {
        let name: Vec<u16> = "CONIN$\0".encode_utf16().collect();
        // SAFETY: `name` is NUL-terminated and outlives the call; the null security attributes and
        // template are allowed.
        let opened = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if opened == INVALID_HANDLE_VALUE {
            0
        } else {
            opened as usize
        }
    });
    if handle == 0 {
        return Err(io::Error::other("the console's input cannot be opened"));
    }
    Ok(handle as *mut c_void)
}

/// FAR's `SetFarConsoleMode`: resizes are reported, the keys come as they are; with the mouse its
/// records come and the console's own selection (Quick Edit) is off.
const fn program_mode(mode: u32, mouse: bool) -> u32 {
    let mode = (mode | ENABLE_WINDOW_INPUT)
        & !(ENABLE_PROCESSED_INPUT
            | ENABLE_LINE_INPUT
            | ENABLE_ECHO_INPUT
            | ENABLE_VIRTUAL_TERMINAL_INPUT
            | ENABLE_MOUSE_INPUT);
    if mouse {
        (mode | ENABLE_MOUSE_INPUT | ENABLE_EXTENDED_FLAGS) & !ENABLE_QUICK_EDIT_MODE
    } else {
        mode
    }
}

/// Saves the console's input mode and sets the program's.
pub fn begin_console_input(mouse: bool) -> io::Result<()> {
    let handle = input_handle()?;
    let mut mode = 0u32;
    // SAFETY: `handle` is the open console input and `mode` a live local.
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    SAVED_MODE.store(u64::from(mode), Ordering::SeqCst);
    // SAFETY: `handle` is the open console input.
    if unsafe { SetConsoleMode(handle, program_mode(mode, mouse)) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The mouse on or off, as Options > Interface says (FAR's `SetFarConsoleMode`).
pub fn set_console_mouse(mouse: bool) -> io::Result<()> {
    let handle = input_handle()?;
    let mut mode = 0u32;
    // SAFETY: `handle` is the open console input and `mode` a live local.
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `handle` is the open console input.
    if unsafe { SetConsoleMode(handle, program_mode(mode, mouse)) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The mode [`begin_console_input`] found, back; nothing when none is saved. Called on every way out,
/// the panic hook too, so it never fails and never panics.
pub fn end_console_input() {
    let Ok(mode) = u32::try_from(SAVED_MODE.swap(NOT_SAVED, Ordering::SeqCst)) else {
        return;
    };
    if let Ok(handle) = input_handle() {
        // SAFETY: `handle` is the open console input. A failure leaves what can only be left.
        unsafe { SetConsoleMode(handle, mode) };
    }
}

/// The next record; `None` when none came within `wait`.
pub fn read_console(wait: Duration) -> io::Result<Option<ConsoleRecord>> {
    let handle = input_handle()?;
    let millis = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX - 1);
    // SAFETY: `handle` is the open console input, which stays open.
    match unsafe { WaitForSingleObject(handle, millis) } {
        WAIT_OBJECT_0 => {}
        WAIT_TIMEOUT => return Ok(None),
        _ => return Err(io::Error::last_os_error()),
    }
    // SAFETY: an all-zero INPUT_RECORD is a valid value of this plain C struct.
    let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
    let mut count = 0u32;
    // SAFETY: the buffer is one live record, as the length says; `count` is a live local.
    if unsafe { ReadConsoleInputW(handle, &mut record, 1, &mut count) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((count > 0).then(|| convert(&record)))
}

fn convert(record: &INPUT_RECORD) -> ConsoleRecord {
    match u32::from(record.EventType) {
        KEY_EVENT => {
            // SAFETY: `EventType` says the union holds a key event.
            let key = unsafe { record.Event.KeyEvent };
            ConsoleRecord::Key(KeyRecord {
                down: key.bKeyDown != 0,
                virtual_key: key.wVirtualKeyCode,
                // SAFETY: both union members are the same 16 bits, any value is valid.
                unit: unsafe { key.uChar.UnicodeChar },
                control: control_keys(key.dwControlKeyState),
            })
        }
        // SAFETY: `EventType` says the union holds a mouse event.
        MOUSE_EVENT => ConsoleRecord::Mouse(mouse_record(&unsafe { record.Event.MouseEvent })),
        WINDOW_BUFFER_SIZE_EVENT => ConsoleRecord::Resized,
        // SAFETY: `EventType` says the union holds a focus event.
        FOCUS_EVENT => ConsoleRecord::Focus(unsafe { record.Event.FocusEvent.bSetFocus } != 0),
        _ => ConsoleRecord::Other,
    }
}

fn mouse_record(mouse: &MOUSE_EVENT_RECORD) -> MouseRecord {
    let state = mouse.dwButtonState;
    // The wheel's distance is the high word, signed.
    let [low, high] = (state >> 16).to_le_bytes()[..2] else {
        unreachable!()
    };
    let delta = i16::from_le_bytes([low, high]);
    let kind = match mouse.dwEventFlags {
        DOUBLE_CLICK => MouseKind::DoubleClick,
        MOUSE_MOVED => MouseKind::Moved,
        MOUSE_WHEELED => MouseKind::Wheel(delta),
        MOUSE_HWHEELED => MouseKind::HorizontalWheel(delta),
        _ => MouseKind::Button,
    };
    let (left, top) = window_origin();
    let cell = |at: i16, origin: i16| u16::try_from(i32::from(at) - i32::from(origin)).unwrap_or(0);
    MouseRecord {
        column: cell(mouse.dwMousePosition.X, left),
        row: cell(mouse.dwMousePosition.Y, top),
        buttons: MouseButtons {
            left: state & FROM_LEFT_1ST_BUTTON_PRESSED != 0,
            right: state & RIGHTMOST_BUTTON_PRESSED != 0,
            middle: state & FROM_LEFT_2ND_BUTTON_PRESSED != 0,
        },
        control: control_keys(mouse.dwControlKeyState),
        kind,
    }
}

/// The buffer's cell at the window's top left: the mouse is reported in the buffer, the screen is
/// drawn in the window.
fn window_origin() -> (i16, i16) {
    // SAFETY: GetStdHandle has no preconditions.
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    // SAFETY: an all-zero CONSOLE_SCREEN_BUFFER_INFO is a valid value of this plain C struct.
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a live local; an invalid handle only makes the call fail.
    if unsafe { GetConsoleScreenBufferInfo(output, &mut info) } == 0 {
        return (0, 0);
    }
    (info.srWindow.Left, info.srWindow.Top)
}

fn control_keys(state: u32) -> ControlKeys {
    let on = |flag: u32| state & flag != 0;
    ControlKeys {
        shift: on(SHIFT_PRESSED),
        left_ctrl: on(LEFT_CTRL_PRESSED),
        right_ctrl: on(RIGHT_CTRL_PRESSED),
        left_alt: on(LEFT_ALT_PRESSED),
        right_alt: on(RIGHT_ALT_PRESSED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_mode_reports_resizes_and_takes_the_keys_raw() {
        let cooked = ENABLE_PROCESSED_INPUT
            | ENABLE_LINE_INPUT
            | ENABLE_ECHO_INPUT
            | ENABLE_VIRTUAL_TERMINAL_INPUT
            | ENABLE_MOUSE_INPUT;
        assert_eq!(program_mode(cooked, false), ENABLE_WINDOW_INPUT);
    }

    #[test]
    fn with_the_mouse_its_records_come_and_quick_edit_is_off() {
        let mode = program_mode(ENABLE_QUICK_EDIT_MODE | ENABLE_EXTENDED_FLAGS, true);
        assert_eq!(mode & ENABLE_MOUSE_INPUT, ENABLE_MOUSE_INPUT);
        assert_eq!(mode & ENABLE_QUICK_EDIT_MODE, 0);
        assert_eq!(mode & ENABLE_EXTENDED_FLAGS, ENABLE_EXTENDED_FLAGS);
        let off = program_mode(ENABLE_MOUSE_INPUT | ENABLE_QUICK_EDIT_MODE, false);
        assert_eq!(off & ENABLE_MOUSE_INPUT, 0, "FAR's mouse off");
    }

    #[test]
    fn the_program_mode_leaves_other_flags_alone() {
        let quick_edit = 0x40;
        assert_eq!(
            program_mode(quick_edit, false) & quick_edit,
            quick_edit,
            "Quick Edit stays without the mouse"
        );
    }

    #[test]
    fn control_keys_tell_left_from_right() {
        let keys = control_keys(LEFT_CTRL_PRESSED | RIGHT_ALT_PRESSED);
        assert!(keys.left_ctrl && keys.right_alt);
        assert!(!keys.right_ctrl && !keys.left_alt && !keys.shift);
    }

    #[test]
    fn ending_with_nothing_saved_leaves_the_console_alone() {
        SAVED_MODE.store(NOT_SAVED, Ordering::SeqCst);
        end_console_input();
        assert_eq!(SAVED_MODE.load(Ordering::SeqCst), NOT_SAVED);
    }
}
