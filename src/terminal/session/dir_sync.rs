//! Keeping the guest shell's folder and the active panel's folder one (ADR-0006 §2): on entry the
//! shell moves to the panel's folder with a hidden `cd` (only while it waits at its prompt), and a
//! `cd` done in it moves the panel.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::Session;
use crate::platform;
use crate::terminal::shell::is_same_dir;

/// A `cd` that shows a prompt of another folder and stays quiet this long has failed.
const CHANGE_DIR_SETTLE: Duration = Duration::from_millis(300);
/// The prompt on screen is trusted once output came after the last line sent and then stopped this
/// long; before that it may be the prompt the line was typed at, not the one after it.
const PROMPT_SETTLE: Duration = Duration::from_millis(100);

/// Keys waiting for the `cd` sent before them to finish.
pub(super) struct PendingChange {
    dir: PathBuf,
    /// Keys to type at the new prompt (the command line moving into the shell's input line).
    keys: Vec<u8>,
    sent: Instant,
    /// The screen when the `cd` was sent: it must change (the echo) before the `cd` can have failed.
    screen: String,
}

impl Session {
    /// The shell is about to be shown: at its prompt it moves to `dir` (if it is elsewhere) and then
    /// gets `keys` typed.
    pub fn enter_dir_and_type(&mut self, dir: &Path, keys: &[u8]) -> io::Result<()> {
        // Sent out by `leave_dir` (perhaps not even there yet): it comes back in any case; the shell
        // takes the lines in order.
        if std::mem::take(&mut self.parked) || (self.at_prompt() && self.is_in(dir) == Some(false))
        {
            return self.change_dir_then(dir, keys.to_vec());
        }
        if keys.is_empty() {
            return Ok(());
        }
        self.send(keys)
    }

    /// The panels are back: a shell waiting at its prompt moves out to the Windows folder with a hidden
    /// `cd`, so it holds none of the user's folders (Windows keeps a process's folder open; the panels
    /// could not move or delete it). The next `enter_dir_and_type` brings it back. A busy shell, or one
    /// whose prompt does not show its folder (WSL), stays where it is.
    pub fn leave_dir(&mut self) -> io::Result<()> {
        // Its prompt, with what was typed after it (the line is being cleared), or nothing printed yet
        // (the line is queued until the shell reads it).
        let blank = self.screen().contents().trim().is_empty();
        let waits = self.shell_dir().is_some() || (blank && self.shell.prompt_shows_dir());
        if self.parked || self.pending.is_some() || !waits {
            return Ok(());
        }
        self.change_dir_then(&platform::process::system_folder(), Vec::new())?;
        self.parked = true;
        Ok(())
    }

    /// Whether the shell was sent out of the user's folders and not brought back yet; the next entry
    /// brings it back even while that `cd` still runs.
    pub fn is_parked(&self) -> bool {
        self.parked
    }

    /// Takes back keys still waiting for a `cd` to finish; true if there were any.
    pub fn cancel_typing(&mut self) -> bool {
        let Some(pending) = self.pending.take() else {
            return false;
        };
        // The `cd` itself was sent and goes on.
        self.known_dir = pending.dir;
        true
    }

    /// The folder the shell moved to by itself (a `cd` typed in it, a script), once per move; the
    /// panel follows it.
    pub fn moved_dir(&mut self) -> Option<PathBuf> {
        if self.pending.is_some() || !self.at_prompt() {
            return None;
        }
        let shown = self.shell_dir()?;
        if is_same_dir(&shown, &self.known_dir) {
            return None;
        }
        self.known_dir = PathBuf::from(shown);
        Some(self.known_dir.clone())
    }

    /// Whether keys still wait for a `cd`.
    pub fn is_changing_dir(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether the shell is in `dir`: from its prompt, or before its first prompt the folder it was
    /// started in. `None` when that cannot be known: the prompt does not show the folder, or the line
    /// sent last has not been answered by a new prompt yet (it may change the folder: `pushd`, a
    /// script).
    fn is_in(&self, dir: &Path) -> Option<bool> {
        let Some(sent) = self.last_line else {
            if self.screen().contents().trim().is_empty() {
                return Some(self.known_dir.as_path() == dir);
            }
            return self.shell_dir().map(|shown| is_same_dir(&shown, dir));
        };
        let settled = self.pending.is_none()
            && self.output_since(sent)
            && self.quiet_for(PROMPT_SETTLE)
            && self.screen().contents() != self.screen_at_line;
        if !settled {
            return None;
        }
        self.shell_dir().map(|shown| is_same_dir(&shown, dir))
    }

    fn change_dir_then(&mut self, dir: &Path, keys: Vec<u8>) -> io::Result<()> {
        let screen = self.screen().contents();
        let line = self.shell.change_dir(dir);
        self.send_line(&line)?;
        self.remember_own_line(line);
        self.pending = Some(PendingChange {
            dir: dir.to_path_buf(),
            keys,
            sent: Instant::now(),
            screen,
        });
        Ok(())
    }

    /// Types what waits once the shell's prompt shows its folder, or once the prompt settles on
    /// another folder (the `cd` failed and printed why: the line is typed where the shell is).
    pub(super) fn advance_pending(&mut self) -> io::Result<()> {
        let Some(pending) = &self.pending else {
            return Ok(());
        };
        if !self.output_since(pending.sent) || !self.at_prompt() {
            return Ok(());
        }
        let arrived = self
            .shell_dir()
            .is_some_and(|shown| is_same_dir(&shown, &pending.dir));
        let echoed = self.screen().contents() != pending.screen;
        let failed = echoed && self.quiet_for(CHANGE_DIR_SETTLE);
        if !(arrived || failed) {
            return Ok(());
        }
        let Some(pending) = self.pending.take() else {
            return Ok(());
        };
        if arrived {
            self.known_dir = pending.dir.clone();
        }
        if pending.keys.is_empty() {
            return Ok(());
        }
        self.send(&pending.keys)
    }

    /// The folder the shell's prompt shows, if it shows one.
    fn shell_dir(&self) -> Option<String> {
        let (line, _) = self.cursor_line();
        self.shell.prompt_dir(&line).map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Shell;
    use crate::terminal::session::tests::{wait_for_text, wait_until_quiet};

    /// Feeds output and the waiting command (as `pump` does) until `done` or 15 s have passed.
    fn drive(session: &mut Session, done: impl Fn(&Session) -> bool) -> io::Result<bool> {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            session.pty.pump_waiting(Duration::from_millis(50))?;
            if session.pty.is_closed() {
                return Ok(false);
            }
            session.advance_pending()?;
            if done(session) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn screen_has(text: &str) -> impl Fn(&Session) -> bool {
        move |session| session.screen().contents().contains(text)
    }

    #[test]
    fn the_shell_moves_to_the_folder_only_when_it_is_elsewhere() {
        for shell in [
            Shell::default_shell(),
            Shell::system(crate::platform::process::ShellKind::Powershell).unwrap(),
        ] {
            let start = tempfile::tempdir().unwrap();
            let other = tempfile::tempdir().unwrap();
            std::fs::write(other.path().join("other-marker-5c1.txt"), "x").unwrap();
            let mut session = Session::spawn(shell.clone(), start.path(), 30, 200).unwrap();
            assert!(wait_for_text(&mut session, ">"));
            wait_until_quiet(&mut session);

            session
                .enter_dir_and_type(start.path(), b"echo first-run\r")
                .unwrap();
            let printed = |session: &Session| {
                (session.screen().contents().lines()).any(|line| line == "first-run")
            };
            assert!(drive(&mut session, printed).unwrap());
            wait_until_quiet(&mut session);
            let own = shell.change_dir(start.path());
            assert!(!session.screen().contents().contains(&own), "already there");

            session.enter_dir_and_type(other.path(), b"dir\r").unwrap();
            assert!(drive(&mut session, screen_has("other-marker-5c1.txt")).unwrap());
            assert!(!session.is_changing_dir());
            let own = shell.change_dir(other.path());
            assert!(session.screen().contents().contains(&own));
            // The shell's own history holds the typed line. PSReadLine may take longer than a quiet
            // spell to show it, so the test waits for the line itself.
            wait_until_quiet(&mut session);
            let typed = |text: &'static str| {
                move |session: &Session| {
                    session.typed_input().map(|(typed, _)| typed).as_deref() == Some(text)
                }
            };
            session.send(b"[A").unwrap();
            drive(&mut session, typed("dir")).unwrap();
            assert_eq!(
                session.typed_input().map(|(text, _)| text).as_deref(),
                Some("dir")
            );
            // PowerShell keeps the program's own `cd` out of it (cmd cannot).
            if shell.dialect() == crate::terminal::Dialect::PowerShell {
                session.send(b"[A").unwrap();
                drive(&mut session, typed("echo first-run")).unwrap();
                assert_eq!(
                    session.typed_input().map(|(text, _)| text).as_deref(),
                    Some("echo first-run")
                );
            }
        }
    }

    #[test]
    fn leaving_frees_the_folder_and_coming_back_returns_to_it() {
        for shell in [
            Shell::default_shell(),
            Shell::system(crate::platform::process::ShellKind::Powershell).unwrap(),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let start = temp.path().join("start");
            std::fs::create_dir(&start).unwrap();
            let mut session = Session::spawn(shell.clone(), &start, 30, 200).unwrap();
            assert!(wait_for_text(&mut session, ">"));
            wait_until_quiet(&mut session);

            // Left with text typed at the prompt (the app clears it, then leaves at once).
            session.send(b"echo half").unwrap();
            let typed = |session: &Session| {
                session.typed_input().map(|(text, _)| text).as_deref() == Some("echo half")
            };
            assert!(drive(&mut session, typed).unwrap(), "{shell:?}");
            session.clear_input().unwrap();
            session.leave_dir().unwrap();
            assert!(session.is_parked(), "{shell:?}");
            assert!(drive(&mut session, |session| !session.is_changing_dir()).unwrap());
            let moved = temp.path().join("moved");
            crate::terminal::rename_soon(&start, &moved).unwrap();

            // Back, then out and back again at once, while the shell is still on its way out.
            session.enter_dir_and_type(&moved, b"").unwrap();
            let arrived = |session: &Session| {
                !session.is_changing_dir()
                    && session
                        .shell_dir()
                        .is_some_and(|dir| dir.ends_with("moved"))
            };
            assert!(drive(&mut session, arrived).unwrap(), "{shell:?}");
            session.leave_dir().unwrap();
            assert!(
                session.is_parked() && session.is_changing_dir(),
                "{shell:?}"
            );
            session
                .enter_dir_and_type(&moved, b"echo back-7e2\r")
                .unwrap();
            assert!(!session.is_parked());
            let back = |session: &Session| {
                (session.screen().contents().lines()).any(|line| line == "back-7e2")
            };
            assert!(drive(&mut session, back).unwrap(), "{shell:?}");
            wait_until_quiet(&mut session);
            let prompt = session.shell_dir();
            assert!(
                prompt.as_deref().is_some_and(|dir| dir.ends_with("moved")),
                "{shell:?}: {prompt:?}"
            );
        }
    }

    #[test]
    fn after_a_failed_cd_the_line_is_typed_where_the_shell_is() {
        let start = tempfile::tempdir().unwrap();
        let mut session = Session::spawn(Shell::default_shell(), start.path(), 30, 200).unwrap();
        assert!(wait_for_text(&mut session, ">"));
        wait_until_quiet(&mut session);
        let gone = start.path().join("no-such-folder");

        session.enter_dir_and_type(&gone, b"echo kept").unwrap();
        assert!(session.is_changing_dir());
        let typed = |session: &Session| {
            session.typed_input().map(|(text, _)| text).as_deref() == Some("echo kept")
        };
        assert!(drive(&mut session, typed).unwrap());

        assert!(!session.is_changing_dir());
        assert_eq!(session.moved_dir(), None, "the shell stayed where it was");
    }
}
