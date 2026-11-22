//! The stubs of processes and the console.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::unsupported;
use crate::platform::files::Attributes;
use crate::platform::process::{ExecuteFailure, Execution, ProgramStart, ShellKind};

/// A placeholder: no shell is offered here until the port.
pub const DEFAULT_SHELL: ShellKind = ShellKind::Cmd;

pub fn shells() -> &'static [ShellKind] {
    &[]
}

pub fn git_bash() -> Option<std::path::PathBuf> {
    None
}

pub fn execute(_request: &Execution) -> Result<i32, ExecuteFailure> {
    Err(ExecuteFailure::CannotExecute {
        interpreter: false,
        error: io::Error::new(
            io::ErrorKind::Unsupported,
            "not available on this system yet",
        ),
    })
}

pub fn forget_variables(_names: &[&str]) {}

pub fn console_code_pages() -> Option<(u32, u32)> {
    None
}

pub fn set_console_code_pages(_input: u32, _output: u32) -> bool {
    false
}

pub fn console_cursor_column() -> Option<u16> {
    None
}

pub fn console_cursor_size() -> Option<u32> {
    None
}

pub fn set_console_cursor_size(_percent: u32) -> std::io::Result<()> {
    Ok(())
}

/// No task list here until the port.
pub fn task_windows() -> io::Result<Vec<crate::platform::process::TaskWindow>> {
    unsupported()
}

pub fn process_image(_pid: u32) -> String {
    String::new()
}

pub fn terminate_process(_pid: u32) -> io::Result<()> {
    unsupported()
}

pub fn switch_to_window(_window: usize) {}

/// No console window of the system's here.
pub fn console_window() -> usize {
    0
}

pub fn console_title() -> Option<String> {
    None
}

pub fn survive_ctrl_break() {}

pub fn reopen_in_plain_console() -> bool {
    false
}

pub fn console_colors() -> Option<[(u8, u8, u8); 16]> {
    None
}

/// A placeholder until the port (`sh` keeps `$VAR` names apart by case).
pub const VARIABLE_NAMES_DIFFER_BY_CASE: bool = true;

/// `/d` is a folder on Unix.
pub const CD_TAKES_DRIVE_SWITCH: bool = false;

pub const INTERPRETER_FOLDER_REFUSAL: &str = "The command interpreter cannot run commands here.";

pub fn completion_programs(_start: &str, _path: &std::ffi::OsStr) -> Vec<String> {
    Vec::new()
}

pub fn completion_variables(_start: &str, _names: &[String]) -> Vec<String> {
    Vec::new()
}

/// The port expands `$NAME`; until then the text stays as written.
pub fn expand_variables(text: &str, _value: &dyn Fn(&str) -> Option<String>) -> String {
    text.to_string()
}

pub fn flash_console_window() {}

pub fn without_console(command: &mut Command) -> &mut Command {
    command
}

/// No way to break a thread's call here yet: it is let go (stage 7).
pub fn break_thread_io<T>(_thread: &std::thread::JoinHandle<T>) {}

pub fn open_with_association(_path: &Path) -> io::Result<()> {
    unsupported()
}

pub fn pass_ctrl_c_to_children() -> io::Result<()> {
    unsupported()
}

pub fn program_start(_name: &str, _attributes: Attributes) -> Option<ProgramStart> {
    None
}

/// The port reads the environment again from the login's files; until then nothing changes.
pub fn take_environment_change() -> bool {
    false
}

pub fn system_environment() -> Vec<(String, String)> {
    Vec::new()
}

pub fn system_folder() -> PathBuf {
    PathBuf::from("/")
}
