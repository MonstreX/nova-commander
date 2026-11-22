//! The console input's stubs: crossterm reads the input until the port.

use std::io;

use super::unsupported;
use crate::platform::input::ConsoleRecord;

/// crossterm reads the input here until the port.
pub const READS_CONSOLE: bool = false;

pub fn read_console(_wait: std::time::Duration) -> io::Result<Option<ConsoleRecord>> {
    unsupported()
}

pub fn begin_console_input(_mouse: bool) -> io::Result<()> {
    Ok(())
}

pub fn double_click_time() -> std::time::Duration {
    std::time::Duration::from_millis(500)
}

pub fn wheel_scroll_lines() -> u32 {
    3
}

pub fn wheel_scroll_chars() -> u32 {
    3
}

pub fn keyboard_repeat() -> (std::time::Duration, std::time::Duration) {
    (
        std::time::Duration::from_millis(500),
        std::time::Duration::from_millis(33),
    )
}

pub fn end_console_input() {}

pub fn set_console_mouse(_mouse: bool) -> io::Result<()> {
    Ok(())
}

pub fn is_altgr_text(_ch: char) -> bool {
    false
}

pub fn key_for_char(_ch: char) -> Option<(char, bool)> {
    None
}
