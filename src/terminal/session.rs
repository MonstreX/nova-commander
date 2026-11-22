use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use portable_pty::Child;

use super::Shell;
use super::pty::{self, Pty, to_io};

mod dir_sync;
mod view;

pub use view::ShellView;

/// A running shell attached to a pseudo console, with its emulated screen (the guest shell, ADR-0006).
pub struct Session {
    shell: Shell,
    pty: Pty,
    child: Box<dyn Child + Send + Sync>,
    /// The folder the shell was started in or last moved to.
    known_dir: PathBuf,
    pending: Option<dir_sync::PendingChange>,
    /// Sent out of the user's folders while the panels are shown (`leave_dir`); entering brings it back.
    parked: bool,
    /// When the last line (with Enter) was sent; the screen shows an older prompt until it is echoed.
    last_line: Option<Instant>,
    /// The screen just before that line: a new prompt cannot be on it.
    screen_at_line: String,
    /// The `cd` lines this program typed by itself; the screen does not show them.
    own_lines: Vec<String>,
}

impl Session {
    pub fn spawn(shell: Shell, cwd: &Path, rows: u16, cols: u16) -> io::Result<Self> {
        let (pty, slave) = Pty::open(rows, cols)?;
        let mut command = pty::command(shell.program());
        command.args(shell.args());
        command.cwd(cwd);
        let child = slave.spawn_command(command).map_err(to_io)?;
        // The slave handle must be closed in this process, otherwise the reader never sees EOF.
        drop(slave);
        Ok(Self {
            shell,
            pty,
            child,
            known_dir: cwd.to_path_buf(),
            pending: None,
            parked: false,
            last_line: None,
            screen_at_line: String::new(),
            own_lines: Vec::new(),
        })
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.pty.screen()
    }

    pub fn shell(&self) -> &Shell {
        &self.shell
    }

    pub fn process_id(&self) -> Option<u32> {
        self.child.process_id()
    }

    /// Feeds pending shell output into the screen. Returns true if anything arrived.
    pub fn pump(&mut self) -> io::Result<bool> {
        let received = self.pty.pump()?;
        self.advance_pending()?;
        Ok(received)
    }

    /// Sends `text` and Enter exactly as typed.
    pub fn send_line(&mut self, text: &str) -> io::Result<()> {
        self.last_line = Some(Instant::now());
        self.screen_at_line = self.screen().contents();
        self.send(format!("{text}\r").as_bytes())
    }

    /// Whether output arrived after `instant`.
    pub fn output_since(&self, instant: Instant) -> bool {
        self.pty.output_since(instant)
    }

    /// True while the shell shows its prompt, or before it printed anything (input is queued).
    pub fn at_prompt(&self) -> bool {
        let (line, _) = self.cursor_line();
        self.screen().contents().trim().is_empty() || self.shell.is_prompt(&line)
    }

    /// The command typed at the shell's prompt and the cursor's place in it (in characters), read from
    /// the screen; `None` while the shell is not at its prompt (a program runs). Input longer than a
    /// line wraps onto the next rows and is read from them too.
    pub fn typed_input(&self) -> Option<(String, usize)> {
        let (text, cursor) = self.cursor_line();
        let prompt = self.shell.prompt_length(&text)?;
        let input: String = text.chars().skip(prompt).collect();
        let input = input.trim_end().to_string();
        let cursor = cursor.saturating_sub(prompt).min(input.chars().count());
        Some((input, cursor))
    }

    /// The screen line with the cursor, joined with the rows it wraps onto, and the cursor's place in
    /// it (in characters).
    pub(super) fn cursor_line(&self) -> (String, usize) {
        let screen = self.pty.screen();
        let (rows, cols) = screen.size();
        let (cursor_row, cursor_col) = screen.cursor_position();
        let mut first = cursor_row;
        while first > 0 && screen.row_wrapped(first - 1) {
            first -= 1;
        }
        let mut last = cursor_row;
        while last + 1 < rows.get() && screen.row_wrapped(last) {
            last += 1;
        }
        let width = usize::from(cols.get());
        let mut text = String::new();
        let rows = screen.rows(0, cols.get()).enumerate();
        for (index, row) in rows
            .skip(usize::from(first))
            .take(usize::from(last - first) + 1)
        {
            text.push_str(&row);
            // A wrapped row is full: pad what the screen trimmed so positions stay columns.
            if index < usize::from(last) {
                let padding = width.saturating_sub(row.chars().count());
                text.extend(std::iter::repeat_n(' ', padding));
            }
        }
        let cursor = usize::from(cursor_row - first) * width + usize::from(cursor_col);
        (text, cursor)
    }

    /// Empties the shell's input line (after its text moved to the command line under the panels).
    pub fn clear_input(&mut self) -> io::Result<()> {
        self.send(self.shell.clear_input_keys())
    }

    pub fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.pty.send(bytes)
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> io::Result<()> {
        self.pty.resize(rows, cols)
    }

    /// False once the shell process has exited (e.g. the user typed `exit`).
    pub fn is_running(&mut self) -> bool {
        !self.pty.is_closed() && matches!(self.child.try_wait(), Ok(None))
    }

    pub fn quiet_for(&self, duration: Duration) -> bool {
        self.pty.quiet_for(duration)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // The shell may already have exited; there is nothing useful to do with a kill error here.
        let _ = self.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blocks on the output channel (no sleeping) until `text` is on screen or 15 s have passed.
    pub(super) fn wait_for_text(session: &mut Session, text: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(15);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            if !session.pty.pump_waiting(remaining).unwrap() {
                return false;
            }
            session.advance_pending().unwrap();
            if session.screen().contents().contains(text) {
                return true;
            }
        }
        false
    }

    #[test]
    fn cmd_session_runs_a_typed_line_in_its_folder() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("marker-file-7f3a.txt"), "x").unwrap();
        let mut session = Session::spawn(Shell::default_shell(), temp.path(), 24, 100).unwrap();

        session.send_line("dir /b").unwrap();

        assert!(wait_for_text(&mut session, "marker-file-7f3a.txt"));
        assert!(session.is_running());
    }

    #[test]
    fn git_bash_goes_to_a_windows_folder_by_its_hidden_cd() {
        let Some(bash) = Shell::system(crate::platform::process::ShellKind::GitBash) else {
            return;
        };
        let start = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("marker-bash-4e1.txt"), "x").unwrap();
        let mut session = Session::spawn(bash.clone(), start.path(), 24, 120).unwrap();
        // The user's `~/.bash_history` keeps none of the test's lines.
        session.send_line("unset HISTFILE").unwrap();
        session.send_line(&bash.change_dir(other.path())).unwrap();
        session.send_line("ls").unwrap();
        assert!(wait_for_text(&mut session, "marker-bash-4e1.txt"));
        session.send_line("exit").unwrap();
    }

    /// Consumes output until none arrives for 300 ms (blocking receive, no sleeping).
    pub(super) fn wait_until_quiet(session: &mut Session) {
        while session
            .pty
            .pump_waiting(Duration::from_millis(300))
            .unwrap()
        {}
    }
}
