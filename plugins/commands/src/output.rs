//! A command's output as FAR's farcmds takes it (`<command`, `OpenFromCommandLine`'s start of a process,
//! `MakeExecuteString`, `PartCmdLine`): `|folder|` first runs it there; the program named found as
//! `whereis:` finds it (no folders) and quoted when its path has a space; the whole given to `%COMSPEC% /c`.
//! The standard output and errors go to files as asked (both as one, one of them, each its own); the
//! program runs with no console of its own (FAR's hide mode: its own console output is not kept), in a job
//! the plugin ends on Esc with all it started (ADR-0019). Every 2 s a window tells the output's size so far
//! (FAR's `ThreadWhatUpdateScreen`).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use nc_plugin_sdk::Host;

use crate::parse::Catch;
use crate::{search, text};

/// `text` split at its working folder (`|folder|command`): the folder, expanded, and the rest.
pub fn working_folder(text: &str) -> (Option<String>, &str) {
    if let Some(rest) = text.strip_prefix('|')
        && let Some(end) = rest.find('|')
    {
        let folder = crate::text::unquote(&crate::text::expand(&rest[..end]));
        return (Some(folder), rest[end + 1..].trim_start());
    }
    (None, text)
}

/// The program and its parameters (FAR's `PartCmdLine`): split at the first space, `/`, `<`, `>`, `|` or
/// `&` out of quotes after the first letter; the program unquoted.
pub fn split(text: &str) -> (String, Option<String>) {
    let text = text::expand(text);
    let text = text.trim();
    let mut quoted = false;
    let at = text.char_indices().find_map(|(at, letter)| {
        if letter == '"' {
            quoted = !quoted;
        }
        let splits = matches!(letter, '>' | '<' | '|' | ' ' | '/' | '&');
        (at > 0 && !quoted && splits).then_some(at)
    });
    match at {
        Some(at) => (text::unquote(&text[..at]), Some(text[at..].to_string())),
        None => (text::unquote(text), None),
    }
}

/// What `%COMSPEC% /c` is given (FAR's `MakeExecuteString`): the program found, quoted with a space in
/// its path, quotes around the whole when the parameters have quotes.
pub fn command_line(text: &str, current: &Path, program: &Path) -> String {
    let (mut name, parameters) = split(text);
    if !name.contains('\\')
        && let Some(found) = search::find(&name, current, program, false)
    {
        name = found.display().to_string();
    }
    let name = match name.contains(' ') {
        true => format!("\"{name}\""),
        false => name,
    };
    let quoted = parameters.as_deref().is_some_and(|text| text.contains('"'));
    let parameters = parameters.map_or_else(String::new, |text| format!(" {}", text.trim()));
    let whole = format!("{name}{parameters}");
    text::expand(&match quoted {
        true => format!("\"{whole}\""),
        false => whole,
    })
}

/// Where the streams go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Files {
    pub out: Option<PathBuf>,
    pub err: Option<PathBuf>,
}

/// The streams taken for `catch` (`separate` when each may be its own: `view:`, `edit:`, `clip:`): the
/// standard output's, the errors' (none when both go to the output's file).
pub fn streams(catch: Catch, separate: bool) -> (bool, bool) {
    match catch {
        Catch::Output => (true, false),
        Catch::Errors => (false, true),
        Catch::Apart if separate => (true, true),
        Catch::Both | Catch::Apart => (true, false),
    }
}

/// A new temporary file's name for a command's output.
pub fn temporary(which: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos());
    std::env::temp_dir().join(format!(
        "vc-output-{}-{stamp}-{which}.txt",
        std::process::id()
    ))
}

/// The bytes of `path` (none: 0).
#[cfg(any(windows, test))]
fn size(path: Option<&PathBuf>) -> u64 {
    path.and_then(|path| std::fs::metadata(path).ok())
        .map_or(0, |meta| meta.len())
}

/// FAR's sizes line: the output's and the errors' (`12/3`), one of them, or nothing yet (told while the
/// job is waited for: Windows).
#[cfg(any(windows, test))]
fn sizes(files: &Files) -> String {
    match (size(files.out.as_ref()), size(files.err.as_ref())) {
        (0, 0) => String::new(),
        (out, 0) => out.to_string(),
        (0, err) => err.to_string(),
        (out, err) => format!("{out}/{err}"),
    }
}

/// How a command's run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ran {
    /// It ended by itself.
    Done,
    /// Esc ended it with all it started.
    Stopped,
    /// It did not start: why.
    Failed(String),
}

/// `command` run in `folder` with its streams to `files` (`both`: the errors to the output's file too).
/// A file that cannot be made: nothing run (FAR's check of its handles). Esc ends it with all it started.
pub fn run(
    host: &Host,
    title: &str,
    command: &str,
    folder: &Path,
    files: &Files,
    both: bool,
) -> Ran {
    let open = |path: &Option<PathBuf>| -> Result<Option<File>, String> {
        match path {
            Some(path) => File::create(path)
                .map(Some)
                .map_err(|error| format!("{}: {error}", path.display())),
            None => Ok(None),
        }
    };
    let (out, err) = match (open(&files.out), open(&files.err)) {
        (Ok(out), Ok(err)) => (out, err),
        (Err(why), _) | (_, Err(why)) => return Ran::Failed(why),
    };
    let err = match (both, &out) {
        (true, Some(out)) => out.try_clone().ok(),
        _ => err,
    };
    let stream = |file: Option<File>| file.map_or_else(Stdio::null, Stdio::from);
    let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    let mut process = std::process::Command::new(shell);
    process
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(stream(out))
        .stderr(stream(err));
    started(host, title, command, files, &mut process)
}

#[cfg(windows)]
fn started(
    host: &Host,
    title: &str,
    command: &str,
    files: &Files,
    process: &mut std::process::Command,
) -> Ran {
    use std::os::windows::process::CommandExt;
    use std::time::Duration;
    process.raw_arg(format!("/c {command}"));
    let mut job = match nc_plugin_sdk::system::Job::start(process) {
        Ok(job) => job,
        Err(error) => return Ran::Failed(error.to_string()),
    };
    let mut waited = 0u32;
    loop {
        if host.cancelled() {
            job.end();
            return Ran::Stopped;
        }
        match job.wait_for(Duration::from_millis(200)) {
            Ok(None) => {}
            Ok(Some(_)) | Err(_) => return Ran::Done,
        }
        waited += 1;
        if waited.is_multiple_of(10) {
            host.busy(title, &[command, &sizes(files)]);
        }
    }
}

#[cfg(not(windows))]
fn started(
    _host: &Host,
    _title: &str,
    command: &str,
    _files: &Files,
    process: &mut std::process::Command,
) -> Ran {
    process.arg("-c").arg(command);
    match process.status() {
        Ok(_) => Ran::Done,
        Err(error) => Ran::Failed(error.to_string()),
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
