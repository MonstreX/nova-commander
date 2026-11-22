//! The command runner (ADR-0006): this program's own executable, started with `--run` in the user
//! screen's pseudo console, once per command line, in the active panel's folder. It lets Ctrl+C reach
//! the command, shows the prompt line as FAR does (`C:\dir>command`), starts the line through
//! `platform::process::execute` and exits with its exit code. It has no UI.

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use portable_pty::CommandBuilder;

use super::{Environment, builtin, pty};
use crate::platform;
use crate::platform::process::{
    ExecuteFailure, ExecuteSource, Execution, INTERPRETER_FOLDER_REFUSAL,
};

/// The first argument that turns the program into a runner.
pub const RUN_ARGUMENT: &str = "--run";
/// The command line to show and run (an environment variable: no quoting rules to get wrong).
const LINE_VARIABLE: &str = "NOVA_COMMANDER_RUN_LINE";
/// Set when the line is only shown (a command the program carried out itself, as FAR echoes `cd`).
const SHOW_ONLY_VARIABLE: &str = "NOVA_COMMANDER_SHOW_ONLY";
/// The folder to run the line in. portable-pty starts a process whose folder is gone (deleted, a drive
/// gone offline) in the home folder instead, so the runner goes there itself or refuses.
const DIR_VARIABLE: &str = "NOVA_COMMANDER_RUN_DIR";

/// `line` is a file of a panel: nothing in its name means anything.
const FILE_VARIABLE: &str = "NOVA_COMMANDER_RUN_FILE";
/// In a console window of its own, not waited for.
const NEW_WINDOW_VARIABLE: &str = "NOVA_COMMANDER_RUN_NEW_WINDOW";
/// With the administrator's rights.
const RUN_AS_VARIABLE: &str = "NOVA_COMMANDER_RUN_AS";

/// What the command must not see: forgotten as soon as they are read.
const OWN_VARIABLES: [&str; 6] = [
    LINE_VARIABLE,
    SHOW_ONLY_VARIABLE,
    DIR_VARIABLE,
    FILE_VARIABLE,
    NEW_WINDOW_VARIABLE,
    RUN_AS_VARIABLE,
];

/// A runner's exit code that is a report and not the command's: the high half says what, the low half
/// the system's error. A program would have to end with exactly these; `0xE0..` is the range of software
/// exceptions, and these three are not known to be used.
const CANNOT_EXECUTE: u32 = 0xE0FA_0000;
const FOLDER_REFUSED: u32 = 0xE0FB_0000;
const CANNOT_INVOKE_INTERPRETER: u32 = 0xE0FC_0000;
const REPORT_MASK: u32 = 0xFFFF_0000;

/// How a runner starts what it is given.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StartMode {
    pub file: bool,
    pub new_window: bool,
    pub run_as: bool,
}

/// The variables a runner is started with; `None` removes one. The runner's own come last: `set` cannot
/// change them.
fn variables(
    dir: &Path,
    line: &str,
    show_only: bool,
    mode: StartMode,
    environment: &Environment,
) -> Vec<(OsString, Option<OsString>)> {
    let flag = |on: bool| on.then(|| OsString::from("1"));
    let mut variables: Vec<(OsString, Option<OsString>)> = environment
        .changes()
        .map(|(name, value)| (name.into(), value.map(OsString::from)))
        .collect();
    variables.extend([
        (LINE_VARIABLE.into(), Some(line.into())),
        (DIR_VARIABLE.into(), Some(dir.into())),
        (SHOW_ONLY_VARIABLE.into(), flag(show_only)),
        (FILE_VARIABLE.into(), flag(mode.file)),
        (NEW_WINDOW_VARIABLE.into(), flag(mode.new_window)),
        (RUN_AS_VARIABLE.into(), flag(mode.run_as)),
    ]);
    variables
}

/// The process that shows `line` and, unless `show_only`, starts it in `dir` with the program's
/// environment and its `environment` changes.
pub(super) fn command(
    dir: &Path,
    line: &str,
    show_only: bool,
    mode: StartMode,
    environment: &Environment,
) -> io::Result<CommandBuilder> {
    let mut command = pty::command(std::env::current_exe()?);
    command.args(runner_arguments());
    command.cwd(dir);
    for (name, value) in variables(dir, line, show_only, mode, environment) {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        }
    }
    Ok(command)
}

/// A runner with no console to be seen, for what starts in a window of its own (Shift+Enter,
/// Ctrl+Alt+Enter): no folder or file is looked at on the UI thread, and a failure is its exit code.
pub fn detached(
    dir: &Path,
    line: &str,
    mode: StartMode,
    environment: &Environment,
) -> io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command.args(runner_arguments()).current_dir(dir);
    for (name, value) in variables(dir, line, false, mode, environment) {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    platform::process::without_console(&mut command);
    Ok(command)
}

/// The runner's exit code for `failure`.
fn report(failure: &ExecuteFailure) -> u32 {
    match failure {
        ExecuteFailure::FolderRefused => FOLDER_REFUSED,
        ExecuteFailure::CannotExecute { interpreter, error } => {
            let kind = if *interpreter {
                CANNOT_INVOKE_INTERPRETER
            } else {
                CANNOT_EXECUTE
            };
            let code = error.raw_os_error().unwrap_or(0).unsigned_abs();
            kind | (code & !REPORT_MASK)
        }
    }
}

/// What a runner's exit code reports; `None` for a command's own code.
pub fn failure_of(code: u32) -> Option<ExecuteFailure> {
    let error = || io::Error::from_raw_os_error((code & !REPORT_MASK).cast_signed());
    match code & REPORT_MASK {
        FOLDER_REFUSED => Some(ExecuteFailure::FolderRefused),
        CANNOT_EXECUTE => Some(ExecuteFailure::CannotExecute {
            interpreter: false,
            error: error(),
        }),
        CANNOT_INVOKE_INTERPRETER => Some(ExecuteFailure::CannotExecute {
            interpreter: true,
            error: error(),
        }),
        _ => None,
    }
}

#[cfg(not(test))]
fn runner_arguments() -> &'static [&'static str] {
    &[RUN_ARGUMENT]
}

/// The test binary has no `--run`: it runs the one test that turns it into a runner.
#[cfg(test)]
fn runner_arguments() -> &'static [&'static str] {
    &[
        "terminal::runner::tests::runner_process",
        "--exact",
        "--nocapture",
        "--quiet",
        "--test-threads=1",
    ]
}

/// The runner's whole life; returns the exit code for the process.
pub fn run() -> i32 {
    let line = std::env::var(LINE_VARIABLE).unwrap_or_default();
    let show_only = std::env::var_os(SHOW_ONLY_VARIABLE).is_some();
    let mode = StartMode {
        file: std::env::var_os(FILE_VARIABLE).is_some(),
        new_window: std::env::var_os(NEW_WINDOW_VARIABLE).is_some(),
        run_as: std::env::var_os(RUN_AS_VARIABLE).is_some(),
    };
    // Without it Ctrl+C would not stop the command; the command still runs, so this is not fatal.
    let ctrl_c = platform::process::pass_ctrl_c_to_children();
    let dir = std::env::var_os(DIR_VARIABLE).map_or_else(
        || std::env::current_dir().unwrap_or_default(),
        PathBuf::from,
    );
    platform::process::forget_variables(&OWN_VARIABLES);
    let entered = std::env::set_current_dir(&dir);
    print_on_own_line(&format!("{}>{}", dir.display(), shown_line(&line, mode)));
    if let Err(error) = ctrl_c {
        print_on_own_line(&format!("Ctrl+C will not stop this command: {error}"));
    }
    if show_only || line.trim().is_empty() {
        return 0;
    }
    if let Err(error) = entered {
        print_on_own_line(&format!(
            "Cannot enter the folder ({error}); the command was not run."
        ));
        return 1;
    }
    if mode == StartMode::default() && set_code_page(&line) {
        return 0;
    }
    let code = start(&line, &dir, mode);
    // The next prompt line, and the program's command line under the panels, start on a new line.
    print_on_own_line("");
    code
}

/// As FAR's prompt: a file by its name, a line as typed.
fn shown_line(line: &str, mode: StartMode) -> String {
    if mode.file {
        Path::new(line).file_name().map_or_else(
            || line.to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    } else {
        line.to_string()
    }
}

/// `chcp <number>` carried out here, as FAR's: the console's code pages stay changed for what follows.
fn set_code_page(line: &str) -> bool {
    let Some(page) = builtin::code_page(line) else {
        return false;
    };
    let set = platform::process::set_console_code_pages(page, page);
    if set {
        print_on_own_line(&page.to_string());
    }
    set
}

/// Starts the line or file; a program that changed the console's code pages leaves them as they were
/// (FAR's `RestoreCP`).
fn start(line: &str, dir: &Path, mode: StartMode) -> i32 {
    let pages = platform::process::console_code_pages();
    let expanded = Environment::default().expand(line);
    let source = if mode.file {
        ExecuteSource::File(Path::new(line))
    } else {
        ExecuteSource::Line(&expanded)
    };
    let result = platform::process::execute(&Execution {
        source,
        dir,
        no_wait: mode.new_window || mode.run_as,
        run_as: mode.run_as,
    });
    if let Some((input, output)) = pages {
        // Nothing to do about a console that will not take its own code pages back.
        platform::process::set_console_code_pages(input, output);
    }
    match result {
        Ok(code) => code,
        Err(failure) => {
            if matches!(failure, ExecuteFailure::FolderRefused) {
                print_on_own_line(INTERPRETER_FOLDER_REFUSAL);
            }
            report(&failure).cast_signed()
        }
    }
}

/// Prints `text` on a line of its own: a new line first if the cursor is not at the start of one (a
/// command stopped with Ctrl+C or ending without a line break). Empty `text` only ends such a line.
fn print_on_own_line(text: &str) {
    let mut out = io::stdout().lock();
    let mut written = Ok(());
    if platform::process::console_cursor_column().is_some_and(|column| column > 0) {
        written = writeln!(out);
    }
    if !text.is_empty() {
        written = written.and_then(|()| writeln!(out, "{text}"));
    }
    // Nowhere to report a console that cannot be written to; the command's result still counts.
    let _ = written.and_then(|()| out.flush());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_goes_through_the_exit_code_and_a_commands_own_code_does_not() {
        let cannot = ExecuteFailure::CannotExecute {
            interpreter: false,
            error: io::Error::from_raw_os_error(1155),
        };
        let Some(ExecuteFailure::CannotExecute { interpreter, error }) =
            failure_of(report(&cannot))
        else {
            panic!("reported");
        };
        assert!(!interpreter);
        assert_eq!(error.raw_os_error(), Some(1155));
        let invoke = ExecuteFailure::CannotExecute {
            interpreter: true,
            error: io::Error::from_raw_os_error(2),
        };
        assert!(matches!(
            failure_of(report(&invoke)),
            Some(ExecuteFailure::CannotExecute {
                interpreter: true,
                ..
            })
        ));
        assert!(matches!(
            failure_of(report(&ExecuteFailure::FolderRefused)),
            Some(ExecuteFailure::FolderRefused)
        ));
        for own in [0, 1, 255, 0xC000_013A, 0xE06D_7363] {
            assert!(failure_of(own).is_none(), "{own:#x}");
        }
    }

    #[test]
    fn a_file_is_shown_by_its_name_and_a_line_as_typed() {
        let file = StartMode {
            file: true,
            ..StartMode::default()
        };
        assert_eq!(shown_line("C:/dir/a b.exe", file), "a b.exe");
        assert_eq!(
            shown_line("C:/dir/a b.exe -x", StartMode::default()),
            "C:/dir/a b.exe -x"
        );
    }

    #[test]
    fn the_runner_variables_cannot_be_changed_with_set() {
        let mut environment = Environment::default();
        environment.set(LINE_VARIABLE, "del *");
        environment.set(SHOW_ONLY_VARIABLE, "1");
        environment.set("NC_RUNNER_TEST", "kept");
        let dir = Path::new(r"C:\dir");
        environment.set(FILE_VARIABLE, "1");
        let run = command(dir, "echo", false, StartMode::default(), &environment).unwrap();
        assert_eq!(run.get_env(LINE_VARIABLE), Some("echo".as_ref()));
        assert_eq!(run.get_env(SHOW_ONLY_VARIABLE), None);
        assert_eq!(run.get_env(FILE_VARIABLE), None);
        assert_eq!(run.get_env("NC_RUNNER_TEST"), Some("kept".as_ref()));
        assert_eq!(run.get_env(DIR_VARIABLE), Some(dir.as_os_str()));
        let shown = command(dir, "cd x", true, StartMode::default(), &environment).unwrap();
        assert_eq!(shown.get_env(SHOW_ONLY_VARIABLE), Some("1".as_ref()));
    }

    /// The runner inside the test binary: started by [`command`], a no-op in a normal test run.
    #[test]
    fn runner_process() {
        if std::env::var_os(LINE_VARIABLE).is_some() {
            std::process::exit(run());
        }
    }
}
