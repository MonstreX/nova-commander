//! FAR's user screen (ADR-0006): one pseudo console for the program's whole life. Every command line
//! runs in it as a new runner process started in the panel's folder, so the output of all commands is
//! one continuous screen, a command's end is its process's exit, and no process stays in a folder.

use std::collections::VecDeque;
use std::io;
use std::path::Path;

use portable_pty::{Child, CommandBuilder, SlavePty};

use super::pty::{Pty, to_io};
use super::runner::StartMode;
use super::{Environment, ShellView, runner};
use crate::platform::process::ExecuteFailure;

pub struct UserScreen {
    pty: Pty,
    /// Kept open: every runner is started on it.
    slave: Box<dyn SlavePty + Send>,
    running: Option<Running>,
    /// Runners waiting for the running one to end (a line entered while the previous one is shown).
    waiting: VecDeque<Start>,
    /// What could not be started, with its line, until someone reports it.
    failures: VecDeque<(String, ExecuteFailure)>,
}

struct Start {
    command: CommandBuilder,
    show_only: bool,
    line: String,
}

struct Running {
    child: Box<dyn Child + Send + Sync>,
    show_only: bool,
    line: String,
}

impl UserScreen {
    pub fn new(rows: u16, cols: u16) -> io::Result<Self> {
        let (pty, slave) = Pty::open(rows, cols)?;
        Ok(Self {
            pty,
            slave,
            running: None,
            waiting: VecDeque::new(),
            failures: VecDeque::new(),
        })
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.pty.screen()
    }

    /// The whole screen, for drawing.
    pub fn view(&self) -> ShellView<'_> {
        let (rows, _) = self.screen().size();
        ShellView {
            screen: self.screen(),
            rows: (0..rows.get()).collect(),
        }
    }

    /// Runs `line` in `dir` with the program's environment changes: shows the prompt line, then the
    /// command's output. Starts after the commands already given.
    pub fn run(&mut self, dir: &Path, line: &str, environment: &Environment) -> io::Result<()> {
        self.start(dir, line, environment, false, StartMode::default())
    }

    /// Runs the file `path` as it is, nothing in its name meaning anything (Enter on a program).
    pub fn run_file(
        &mut self,
        dir: &Path,
        path: &Path,
        environment: &Environment,
    ) -> io::Result<()> {
        let mode = StartMode {
            file: true,
            ..StartMode::default()
        };
        self.start(dir, &path.to_string_lossy(), environment, false, mode)
    }

    /// Only shows the prompt line with `line`, for a command the program carried out itself.
    pub fn show(&mut self, dir: &Path, line: &str) -> io::Result<()> {
        self.start(
            dir,
            line,
            &Environment::default(),
            true,
            StartMode::default(),
        )
    }

    /// The command line that runs, or waits to run.
    pub fn running_line(&self) -> Option<&str> {
        self.running
            .iter()
            .filter(|running| !running.show_only)
            .map(|running| running.line.as_str())
            .chain(
                self.waiting
                    .iter()
                    .filter(|start| !start.show_only)
                    .map(|start| start.line.as_str()),
            )
            .next()
    }

    /// A line that could not be started, and why.
    pub fn take_failure(&mut self) -> Option<(String, ExecuteFailure)> {
        self.failures.pop_front()
    }

    fn start(
        &mut self,
        dir: &Path,
        line: &str,
        environment: &Environment,
        show_only: bool,
        mode: StartMode,
    ) -> io::Result<()> {
        let command = runner::command(dir, line, show_only, mode, environment)?;
        self.waiting.push_back(Start {
            command,
            show_only,
            line: line.to_string(),
        });
        self.start_next()
    }

    fn start_next(&mut self) -> io::Result<()> {
        if self.running.is_some() {
            return Ok(());
        }
        let Some(start) = self.waiting.pop_front() else {
            return Ok(());
        };
        let child = self.slave.spawn_command(start.command).map_err(to_io)?;
        self.running = Some(Running {
            child,
            show_only: start.show_only,
            line: start.line,
        });
        Ok(())
    }

    /// Feeds output into the screen and starts the next runner when one ended. Returns whether
    /// anything changed.
    pub fn pump(&mut self) -> io::Result<bool> {
        let received = self.pty.pump()?;
        let status = match &mut self.running {
            Some(running) => running.child.try_wait()?,
            None => None,
        };
        let ended = status.is_some();
        if let Some(status) = status {
            if let Some(running) = self.running.take()
                && let Some(failure) = runner::failure_of(status.exit_code())
            {
                self.failures.push_back((running.line, failure));
            }
            self.start_next()?;
        }
        Ok(received || ended)
    }

    /// True while a command (not only a shown line) runs or waits to run: its output is on screen and
    /// keys go to it.
    pub fn has_command(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|running| !running.show_only)
            || self.waiting.iter().any(|start| !start.show_only)
    }

    /// Keys for the running command (Ctrl+C is byte 3).
    pub fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.pty.send(bytes)
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> io::Result<()> {
        self.pty.resize(rows, cols)
    }

    #[cfg(test)]
    pub(crate) fn push_failure(&mut self, line: &str, failure: ExecuteFailure) {
        self.failures.push_back((line.to_string(), failure));
    }

    #[cfg(test)]
    pub(crate) fn pump_waiting(&mut self, timeout: std::time::Duration) -> io::Result<bool> {
        let received = self.pty.pump_waiting(timeout)?;
        Ok(self.pump()? || received)
    }
}

impl Drop for UserScreen {
    fn drop(&mut self) {
        // The command may already have ended; there is nothing useful to do with a kill error here.
        if let Some(running) = &mut self.running {
            let _ = running.child.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Feeds output until `done` or 20 s have passed (blocking receive, no sleeping).
    fn drive(screen: &mut UserScreen, done: impl Fn(&UserScreen) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            screen.pump_waiting(Duration::from_millis(50)).unwrap();
            if done(screen) {
                return true;
            }
        }
        false
    }

    fn text(screen: &UserScreen) -> String {
        screen.screen().contents()
    }

    #[test]
    fn commands_run_one_after_another_in_their_folders() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        std::fs::write(first.join("marker-one.txt"), "").unwrap();
        std::fs::write(second.join("marker-two.txt"), "").unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();
        let environment = Environment::default();

        screen.run(&first, "dir /b", &environment).unwrap();
        screen.run(&second, "dir /b", &environment).unwrap();
        assert!(screen.has_command());

        assert!(drive(&mut screen, |screen| !screen.has_command()
            && text(screen).contains("marker-two.txt")));
        let shown = text(&screen);
        let one = shown.find(&format!("{}>dir /b", first.display())).unwrap();
        let two = shown.find(&format!("{}>dir /b", second.display())).unwrap();
        assert!(one < shown.find("marker-one.txt").unwrap());
        assert!(shown.find("marker-one.txt").unwrap() < two);
        assert!(!shown[two..].contains("marker-one.txt"));
    }

    #[test]
    fn no_process_is_left_in_the_folder_after_a_command() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("busy");
        std::fs::create_dir(&folder).unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();

        screen
            .run(&folder, "echo done-in-busy", &Environment::default())
            .unwrap();
        assert!(drive(&mut screen, |screen| !screen.has_command()));

        // A shell sitting in the folder made this fail with "access denied" (os error 5).
        super::super::rename_soon(&folder, &temp.path().join("moved")).unwrap();
    }

    #[test]
    fn a_shown_line_runs_nothing_and_quotes_reach_the_interpreter_intact() {
        let temp = tempfile::tempdir().unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();

        screen.show(temp.path(), "cd \"far away\"").unwrap();
        assert!(!screen.has_command());
        screen
            .run(
                temp.path(),
                "echo \"a b\" & echo second-part",
                &Environment::default(),
            )
            .unwrap();

        assert!(drive(&mut screen, |screen| text(screen)
            .contains("second-part")
            && !screen.has_command()));
        let shown = text(&screen);
        assert!(shown.contains(&format!("{}>cd \"far away\"", temp.path().display())));
        assert!(shown.contains("\n\"a b\" \n") || shown.contains("\n\"a b\"\n"));
    }

    #[test]
    fn environment_changes_reach_the_command() {
        let temp = tempfile::tempdir().unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();
        let mut environment = Environment::default();
        environment.set("NC_SCREEN_TEST", "value-77");
        environment.set("NC_SCREEN_GONE", "x");
        environment.set("NC_SCREEN_GONE", "");

        screen
            .run(
                temp.path(),
                "echo [%NC_SCREEN_TEST%] [%NC_SCREEN_GONE%]",
                &environment,
            )
            .unwrap();

        assert!(drive(&mut screen, |screen| text(screen)
            .contains("[value-77] [%NC_SCREEN_GONE%]")));
    }

    #[test]
    fn ctrl_c_stops_a_command_and_the_next_one_runs() {
        let temp = tempfile::tempdir().unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();
        let environment = Environment::default();
        screen
            .run(temp.path(), "ping -t 127.0.0.1", &environment)
            .unwrap();
        assert!(drive(&mut screen, |screen| text(screen).contains("127.0.0.1:")));

        screen.send(b"\x03").unwrap();
        assert!(drive(&mut screen, |screen| !screen.has_command()));
        screen
            .run(temp.path(), "echo after-ctrl-c", &environment)
            .unwrap();

        assert!(drive(&mut screen, |screen| !screen.has_command()
            && text(screen).contains("\nafter-ctrl-c")));
        // The prompt line starts a line of its own after `^C`.
        let prompt = format!("{}>echo after-ctrl-c", temp.path().display());
        assert!(text(&screen).lines().any(|line| line == prompt));
    }

    #[test]
    fn nothing_runs_in_a_folder_that_is_gone() {
        let temp = tempfile::tempdir().unwrap();
        let gone = temp.path().join("gone");
        let mut screen = UserScreen::new(30, 120).unwrap();

        // The line on screen is not its output: only running it expands `%...%`.
        screen
            .run(
                &gone,
                "echo ran-%USERNAME:~0,0%7b2",
                &Environment::default(),
            )
            .unwrap();

        assert!(drive(&mut screen, |screen| !screen.has_command()
            && text(screen).contains("the command was not run")));
        assert!(text(&screen).contains(&format!("{}>echo ran-", gone.display())));
        assert!(!text(&screen).contains("ran-7b2"));
    }

    #[test]
    fn a_batch_files_name_may_hold_what_the_interpreter_would_take_for_syntax() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("a b&c.cmd");
        std::fs::write(&path, "@echo ran-9c4\r\n").unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();

        screen
            .run_file(temp.path(), &path, &Environment::default())
            .unwrap();

        assert!(drive(&mut screen, |screen| !screen.has_command()
            && text(screen).contains("ran-9c4")));
        assert!(text(&screen).contains(">a b&c.cmd"));
        assert!(screen.take_failure().is_none());
    }

    #[test]
    fn a_programs_name_means_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("a%PATH%b.exe");
        let system = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        std::fs::copy(system.join("System32").join("hostname.exe"), &path).unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();

        screen
            .run_file(temp.path(), &path, &Environment::default())
            .unwrap();

        // With the name expanded nothing would start, and that is reported.
        assert!(drive(&mut screen, |screen| !screen.has_command()
            && text(screen).contains(">a%PATH%b.exe")));
        assert!(screen.take_failure().is_none());
    }

    #[test]
    fn a_program_is_started_without_the_interpreter() {
        let temp = tempfile::tempdir().unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();

        // `%COMSPEC%` itself, told to print a value only the started process can see as its parent's:
        // a program given on the line gets its parameters exactly as typed.
        screen
            .run(
                temp.path(),
                "cmd.exe /d /c echo \"as typed-7d1\"",
                &Environment::default(),
            )
            .unwrap();

        assert!(drive(&mut screen, |screen| !screen.has_command()
            && text(screen).contains("\"as typed-7d1\"")));
    }

    /// Not a file of an unknown type: Windows would bring up its "Open with" window for that.
    #[test]
    fn a_program_that_cannot_start_is_reported_with_its_line() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("broken.exe"), "not a program").unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();

        screen
            .run(temp.path(), "broken.exe", &Environment::default())
            .unwrap();

        assert!(drive(&mut screen, |screen| !screen.has_command()
            && !screen.failures.is_empty()));
        let (line, failure) = screen.take_failure().unwrap();
        assert_eq!(line, "broken.exe");
        assert!(matches!(
            failure,
            ExecuteFailure::CannotExecute {
                interpreter: false,
                ..
            }
        ));
    }

    #[test]
    fn what_is_not_found_goes_to_the_interpreter() {
        let temp = tempfile::tempdir().unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();

        screen
            .run(temp.path(), "no-such-program-4e1", &Environment::default())
            .unwrap();

        // cmd says so itself, in the system's language: its message names the program.
        assert!(drive(&mut screen, |screen| !screen.has_command()
            && text(screen).matches("no-such-program-4e1").count() >= 2));
        assert!(screen.take_failure().is_none());
    }

    #[test]
    fn a_gui_program_is_not_waited_for() {
        let temp = tempfile::tempdir().unwrap();
        // A GUI-subsystem program without a window that runs far longer than the test waits for the
        // command's end: until the test asks it to stop (it removes the file then) or 25 s are over.
        std::fs::write(
            temp.path().join("wait.vbs"),
            [
                "Set files = CreateObject(\"Scripting.FileSystemObject\")",
                "For i = 1 To 250",
                "If files.FileExists(\"stop\") Then files.DeleteFile \"stop\" : WScript.Quit",
                "WScript.Sleep 100",
                "Next",
            ]
            .join("\r\n"),
        )
        .unwrap();
        let mut screen = UserScreen::new(30, 120).unwrap();

        screen
            .run(temp.path(), "wscript //B wait.vbs", &Environment::default())
            .unwrap();

        assert!(drive(&mut screen, |screen| !screen.has_command()));
        // The script is still running: it stops when asked and leaves the folder free.
        let stop = temp.path().join("stop");
        std::fs::write(&stop, "").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while stop.exists() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(
            !stop.exists(),
            "the script did not run on after the command ended"
        );
    }
}
