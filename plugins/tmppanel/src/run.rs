//! `tmp:<command` (FAR's TmpPanel `run_command`, `TmpPanel.cpp:237-302`): `%COMSPEC% /c command`, `%VAR%`
//! expanded, run in the program's folder - or in `dir` for a command written `|dir|command` - its output
//! and errors (one pipe, in the order written, as FAR's one file) read as a list file's lines; a `|dir|`
//! that is not there runs nothing (FAR's `CreateProcess` fails); the plugin waits for it as FAR does, with
//! no limit. Ours differs where FAR's way is a fault: the output comes through a pipe, not a temporary file;
//! `|dir|` is not handed to the command interpreter, which would take its bars for pipes; the command gets
//! no keyboard (its input is empty) and no window of the console, so it cannot draw over the program's
//! screen; while it runs a message names it instead of the console's title, and Esc on it stops the command
//! with all it started (ADR-0019 §5): nothing is taken then.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use nc_plugin_sdk::CodePage;

use crate::{ITEM, TmpPanel, listfile};

/// The command's working folder and the command itself: `|dir|command` split (FAR's `ParseParam`).
pub fn work_folder(command: &str, folder: Option<&Path>) -> (Option<PathBuf>, String) {
    match listfile::parse_param(command) {
        Some((dir, rest)) if !dir.is_empty() => (
            Some(PathBuf::from(listfile::expand_env(dir))),
            rest.to_string(),
        ),
        _ => (folder.map(Path::to_path_buf), command.to_string()),
    }
}

/// The command interpreter (`%COMSPEC%`, `cmd.exe` without it).
fn interpreter() -> String {
    std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
}

impl TmpPanel {
    /// The lines `command` wrote; none when it could not run.
    pub(crate) fn run_output(&self, command: &str, folder: Option<&Path>) -> Option<Vec<String>> {
        let (dir, command) = work_folder(command, folder);
        let line = listfile::expand_env(&command);
        self.host.busy(ITEM, &[&line]);
        let host = self.host;
        let bytes = output(&line, dir.as_deref(), &|| host.cancelled())?;
        let oem = move |line: &[u8]| {
            host.decode(CodePage::Oem, line)
                .unwrap_or_else(|| String::from_utf8_lossy(line).into_owned())
        };
        Some(listfile::lines(&bytes, &oem))
    }
}

/// How often a command that runs looks whether it was asked to stop.
const LOOK: Duration = Duration::from_millis(50);

/// What `line` writes to its output and its errors, in their order; none when it cannot start (in `dir`, a
/// folder that is not there included) or `stopped` says so while it runs (the command and what it started
/// are ended then).
pub fn output(line: &str, dir: Option<&Path>, stopped: &dyn Fn() -> bool) -> Option<Vec<u8>> {
    let (mut reader, writer) = std::io::pipe().ok()?;
    let mut process = Command::new(interpreter());
    process
        .arg("/c")
        .stdin(Stdio::null())
        .stdout(writer.try_clone().ok()?)
        .stderr(writer);
    hand_over(&mut process, line);
    if let Some(dir) = dir {
        if !dir.is_dir() {
            return None;
        }
        process.current_dir(dir);
    }
    let mut child = process.spawn().ok()?;
    // The pipe's other ends are the command's alone now: its end ends the reading.
    drop(process);
    let (read, came) = channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let whole = std::io::Read::read_to_end(&mut reader, &mut bytes).is_ok();
        let _ = read.send(whole.then_some(bytes));
    });
    loop {
        match came.recv_timeout(LOOK) {
            Ok(bytes) => {
                child.wait().ok()?;
                return bytes;
            }
            Err(RecvTimeoutError::Timeout) if stopped() => {
                end_tree(&mut child);
                return None;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

/// The command ended with every process it started (the interpreter's children keep the pipe open).
#[cfg(windows)]
fn end_tree(child: &mut Child) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &child.id().to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(windows))]
fn end_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The command line as the interpreter reads it (`cmd /c` takes the rest as it is written), with no
/// window of the console.
#[cfg(windows)]
fn hand_over(process: &mut Command, line: &str) {
    use std::os::windows::process::CommandExt;
    /// `CREATE_NO_WINDOW`: the command never draws on the program's console.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    process.raw_arg(line).creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hand_over(process: &mut Command, line: &str) {
    process.arg(line);
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
