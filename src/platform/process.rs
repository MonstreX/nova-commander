//! Processes and the console: the shells this system offers, the command interpreter, how a line is
//! passed to it and what it knows (its own commands, how it finds a program, its variables, the
//! folders it cannot run in), programs and documents, the console window this process lives in.

use std::io;
use std::path::Path;

pub use super::os::{
    CD_TAKES_DRIVE_SWITCH, DEFAULT_SHELL, INTERPRETER_FOLDER_REFUSAL,
    VARIABLE_NAMES_DIFFER_BY_CASE, break_thread_io, completion_programs, completion_variables,
    console_code_pages, console_colors, console_cursor_column, console_cursor_size, console_title,
    console_window, execute, expand_variables, flash_console_window, forget_variables, git_bash,
    open_with_association, pass_ctrl_c_to_children, process_image, program_start,
    reopen_in_plain_console, set_console_code_pages, set_console_cursor_size, shells,
    survive_ctrl_break, switch_to_window, system_environment, system_folder,
    take_environment_change, task_windows, terminate_process, without_console,
};

/// A window of FAR's task list (Ctrl+W): one Alt+Tab shows, its process and title. The window is the
/// system's handle as a number; it may be gone by the time it is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskWindow {
    pub window: usize,
    pub pid: u32,
    pub title: String,
}

/// What [`execute`] is to start.
#[derive(Debug, Clone, Copy)]
pub enum ExecuteSource<'a> {
    /// A typed command line, its variables already expanded.
    Line(&'a str),
    /// A file of a panel: nothing in its name means anything.
    File(&'a Path),
}

/// One start, FAR's `execute_info`.
#[derive(Debug, Clone, Copy)]
pub struct Execution<'a> {
    pub source: ExecuteSource<'a>,
    pub dir: &'a Path,
    /// In a console window of its own, not waited for (Shift+Enter).
    pub no_wait: bool,
    /// With the administrator's rights (Ctrl+Alt+Enter); never waited for.
    pub run_as: bool,
}

/// Why nothing was started.
#[derive(Debug)]
pub enum ExecuteFailure {
    /// The interpreter cannot stand in the folder ([`INTERPRETER_FOLDER_REFUSAL`]).
    FolderRefused,
    /// FAR's "Cannot execute", or with `interpreter` its "Cannot invoke command interpreter".
    CannotExecute { interpreter: bool, error: io::Error },
}

/// How a file that is a program is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramStart {
    /// As it is.
    File,
    /// By this command line, from the file's folder.
    Line(String),
}

/// A shell of the system the embedded terminal can run (ADR-0004 §4). Which ones a system offers is
/// [`shells`]; how each one speaks (its prompt, its `cd`), and the user's own shells, are the
/// terminal's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Cmd,
    Powershell,
    Pwsh,
    Wsl,
    /// Git for Windows' bash: [`git_bash`] finds it.
    GitBash,
}

impl ShellKind {
    /// Its name in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            Self::Cmd => "cmd",
            Self::Powershell => "powershell",
            Self::Pwsh => "pwsh",
            Self::Wsl => "wsl",
            Self::GitBash => "gitbash",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Cmd => "Command Prompt (cmd)",
            Self::Powershell => "Windows PowerShell",
            Self::Pwsh => "PowerShell 7 (pwsh)",
            Self::Wsl => "WSL (default distro)",
            Self::GitBash => "Git Bash",
        }
    }

    /// The program file, found along `PATH` (Git Bash's is where [`git_bash`] says).
    pub fn program(self) -> &'static str {
        match self {
            Self::Cmd => "cmd.exe",
            Self::Powershell => "powershell.exe",
            Self::Pwsh => "pwsh.exe",
            Self::Wsl => "wsl.exe",
            Self::GitBash => "bash.exe",
        }
    }
}
