//! The guest shell (ADR-0006 §2): a real shell chosen in the settings (cmd, PowerShell, WSL) shown with
//! Alt+Shift+T, started on first use and kept until the program quits. Every key goes to it, except
//! Alt+Shift+T and Esc at its empty prompt, which bring the panels back. On entry it moves to the
//! active panel's folder (only while it waits at its prompt) with the command line typed at its prompt;
//! on leaving, what is typed there comes back to the command line and its `cd` moves the panel.

use std::path::PathBuf;
use std::time::Instant;

use super::dialog::Dialog;
use super::terminal_keys;
use super::{App, MainView};
use crate::terminal::{Session, Shell};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    /// Starts the chosen guest shell in the active panel's folder.
    fn start_guest(&mut self) {
        // Drop the old shell first so its process is gone before the new one starts.
        self.guest = None;
        let (rows, cols) = self.guest_size();
        let cwd = self.active_folder();
        match Session::spawn(self.shell.clone(), &cwd, rows, cols) {
            Ok(session) => self.guest = Some(session),
            Err(error) => self.show_dialog(Dialog::error(
                "Guest shell",
                vec![format!("Cannot start {}: {error}", self.shell.title())],
            )),
        }
    }

    /// The shell list (Commands > Choose shell): the next Alt+Shift+T starts the chosen one; a guest
    /// shown now is replaced at once.
    pub(super) fn choose_guest_shell(&mut self, shell: Shell) {
        self.shell = shell;
        self.guest = None;
        if self.main_view == MainView::Guest {
            self.start_guest();
        }
    }

    /// Feeds the guest's output; when it exited (`exit`), it is gone and the panels come back.
    /// Returns whether anything on screen may have changed.
    pub(super) fn pump_guest(&mut self) -> bool {
        let Some(guest) = &mut self.guest else {
            return false;
        };
        let changed = match guest.pump() {
            Ok(received) => received,
            Err(error) => {
                self.show_error("Guest shell", &error);
                true
            }
        };
        if self.guest.as_mut().is_some_and(|guest| !guest.is_running()) {
            // A fresh shell starts with the next Alt+Shift+T.
            self.guest = None;
            if self.main_view == MainView::Guest {
                self.main_view = MainView::Panels;
            }
            return true;
        }
        changed
    }

    /// Alt+Shift+T: the guest shell, or back to the panels (a running program keeps running).
    pub(super) fn toggle_guest(&mut self) {
        if self.main_view == MainView::Guest {
            self.main_view = MainView::Panels;
            self.take_guest_input();
            self.reread_panels();
            return;
        }
        self.main_view = MainView::Guest;
        if self.guest.is_none() {
            self.start_guest();
        }
        self.give_guest_input();
    }

    /// Back to the panels: what is typed at the shell's prompt moves to the command line, and a `cd`
    /// done in the shell moves the active panel, as FAR's command line does.
    fn take_guest_input(&mut self) {
        let given = self.given_input.take();
        let Some(guest) = &mut self.guest else {
            return;
        };
        let moved = guest.moved_dir();
        // Alt+Shift+T pressed again before the shell echoed what it was given (or before its `cd`
        // let it be typed): that text is still the line.
        let held = guest.cancel_typing();
        let unechoed = given.filter(|(_, _, at)| held || !guest.output_since(*at));
        let typed = match unechoed {
            Some((text, cursor, _)) => Some((text, cursor)),
            None => guest.typed_input(),
        };
        let mut sent = Ok(());
        if let Some((text, cursor)) = typed.filter(|(text, _)| !text.is_empty()) {
            self.command_line.set_with_cursor(&text, cursor);
            sent = guest.clear_input();
        }
        // Out of the user's folders while the panels are shown, so they can be moved and deleted.
        if let Err(error) = sent.and_then(|()| guest.leave_dir()) {
            self.show_error("Guest shell", &error);
        }
        if let Some(dir) = moved {
            self.follow_guest_folder(dir);
        }
    }

    /// The folder the guest shell moved to: the files panel goes there once the folder is seen to exist.
    /// The look is disk work and goes on a worker (a gone network folder must not freeze the screen);
    /// a folder that is not there (a prompt that only looked like a path) leaves the panel as it is.
    pub(super) fn follow_guest_folder(&mut self, dir: PathBuf) {
        self.wait_for(Vec::new(), move || {
            let exists = dir.is_dir();
            Box::new(move |app: &mut App| {
                if !exists {
                    return;
                }
                let side = app.command_side();
                let path = crate::vfs::VfsPath::local(&dir);
                if let Err(error) = app.panels[side.index()].change_dir(&path, None) {
                    app.show_error("Change directory", &error);
                }
            })
        });
        self.label_waiting("Looking for the folder…");
    }

    /// The command line moves into the shell's input line, the cursor at the same place; only at the
    /// shell's prompt (a running program would take it as its input). The shell first moves to the
    /// active panel's folder.
    fn give_guest_input(&mut self) {
        let dir = self.active_folder();
        let Some(guest) = &mut self.guest else {
            return;
        };
        // Sent out of the folder on leaving and maybe still on its way: it reads its lines in order.
        if !guest.at_prompt() && !guest.is_parked() {
            return;
        }
        let back = self.command_line.text().chars().count() - self.command_line.cursor();
        let left = terminal_keys::encode(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        let mut bytes = self.command_line.text().as_bytes().to_vec();
        for _ in 0..back {
            bytes.extend_from_slice(left.as_deref().unwrap_or_default());
        }
        match guest.enter_dir_and_type(&dir, &bytes) {
            Ok(()) if self.command_line.is_empty() => {}
            Ok(()) => {
                let cursor = self.command_line.cursor();
                let text = self.command_line.take();
                self.given_input = Some((text, cursor, Instant::now()));
            }
            Err(error) => self.show_error("Guest shell", &error),
        }
    }

    /// A key while the guest shell is shown: Alt+Shift+T, and Esc while the shell waits for a command
    /// with nothing typed (Volkov Commander's Esc), go back; everything else goes to the shell.
    pub(super) fn handle_guest_key(&mut self, key: KeyEvent) {
        if is_guest_key(key) || self.esc_returns_to_panels(key) {
            self.toggle_guest();
            return;
        }
        let Some(bytes) = terminal_keys::encode(key) else {
            return;
        };
        if self.guest.is_none() {
            self.start_guest();
        }
        if let Some(guest) = &mut self.guest
            && let Err(error) = guest.send(&bytes)
        {
            self.show_error("Guest shell", &error);
        }
    }

    fn esc_returns_to_panels(&self, key: KeyEvent) -> bool {
        key.code == KeyCode::Esc
            && key.modifiers.is_empty()
            && self.guest.as_ref().is_some_and(|guest| {
                guest.at_prompt()
                    && !guest.is_changing_dir()
                    && guest.typed_input().is_some_and(|(text, _)| text.is_empty())
            })
    }
}

/// Alt+Shift+T, on any keyboard layout (the key, not the character).
pub fn is_guest_key(key: KeyEvent) -> bool {
    let KeyCode::Char(ch) = key.code else {
        return false;
    };
    key.modifiers == KeyModifiers::ALT | KeyModifiers::SHIFT
        && (ch.eq_ignore_ascii_case(&'t')
            || (super::keyboard::SYSTEM.key_for_char)(ch).is_some_and(|(latin, _)| latin == 't'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::user_screen_link::tests::tick_until;
    use crate::app::waiting::finish_waiting;

    #[test]
    fn the_panel_follows_the_shell_only_into_a_folder_that_is_there() {
        let temp = tempfile::tempdir().unwrap();
        let sub = temp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let mut app = app_in(temp.path());
        app.follow_guest_folder(temp.path().join("gone"));
        finish_waiting(&mut app);
        assert_eq!(app.active_panel().path, temp.path(), "not there: stays");
        assert!(app.overlay.is_none(), "and says nothing");
        app.follow_guest_folder(sub.clone());
        finish_waiting(&mut app);
        assert_eq!(app.active_panel().path, sub);
    }

    fn alt_shift_t() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('T'), KeyModifiers::ALT | KeyModifiers::SHIFT)
    }

    fn guest_text(app: &App) -> String {
        app.guest
            .as_ref()
            .map(|session| session.screen().contents())
            .unwrap_or_default()
    }

    /// The text after the prompt on the shell's cursor line.
    fn guest_input(app: &App) -> Option<String> {
        app.guest
            .as_ref()
            .and_then(Session::typed_input)
            .map(|(text, _)| text)
    }

    fn app_in(dir: &std::path::Path) -> App {
        App::new(dir, 100, 30, &crate::settings::Settings::default())
    }

    fn type_text(app: &mut App, text: &str) {
        for ch in text.chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
    }

    #[test]
    fn the_guest_starts_on_first_use_and_gets_every_key() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        assert!(app.guest.is_none(), "no shell runs until it is asked for");

        app.handle_key(alt_shift_t());
        assert_eq!(app.main_view, MainView::Guest);
        type_text(&mut app, "echo typed-in-guest");
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(
            app.command_line.is_empty(),
            "nothing reaches our command line"
        );
        assert!(tick_until(&mut app, |app| {
            guest_text(app).matches("typed-in-guest").count() >= 2
        }));
        assert_eq!(app.main_view, MainView::Guest, "Ctrl+O went to the shell");

        app.handle_key(alt_shift_t());
        assert_eq!(app.main_view, MainView::Panels);
        assert!(app.guest.is_some(), "the shell keeps running");
    }

    #[test]
    fn esc_at_the_empty_prompt_returns_to_the_panels() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        app.handle_key(alt_shift_t());
        assert!(tick_until(&mut app, |app| guest_input(app).as_deref() == Some("")));

        type_text(&mut app, "x");
        assert!(tick_until(&mut app, |app| guest_input(app).as_deref() == Some("x")));
        app.handle_key(esc);
        assert_eq!(app.main_view, MainView::Guest, "typed text gets the Esc");
        // cmd's Esc empties its input line; the next Esc goes back.
        assert!(tick_until(&mut app, |app| guest_input(app).as_deref() == Some("")));
        app.handle_key(esc);
        assert_eq!(app.main_view, MainView::Panels);
    }

    #[test]
    fn typed_text_moves_between_the_guest_and_the_command_line() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        app.handle_key(alt_shift_t());
        assert!(tick_until(&mut app, |app| guest_input(app).as_deref() == Some("")));
        type_text(&mut app, "echo half");
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(tick_until(&mut app, |app| guest_input(app).as_deref()
            == Some("echo half")));

        // Back to the panels: the shell's line is now the command line, the cursor where it was.
        app.handle_key(alt_shift_t());
        assert_eq!(app.command_line.text(), "echo half");
        assert_eq!(app.command_line.cursor(), 8);
        assert!(tick_until(&mut app, |app| guest_input(app).as_deref() == Some("")));

        // Edited under the panels, it goes back into the shell's line.
        app.command_line.end();
        app.command_line.insert_str("-done");
        app.handle_key(alt_shift_t());
        assert!(app.command_line.is_empty());
        assert!(tick_until(&mut app, |app| guest_input(app).as_deref()
            == Some("echo half-done")));
    }

    #[test]
    fn a_quick_second_alt_shift_t_keeps_the_text_the_shell_has_not_echoed_yet() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        app.handle_key(alt_shift_t());
        assert!(tick_until(&mut app, |app| guest_text(app).contains('>')));
        app.handle_key(alt_shift_t());
        app.command_line.set("echo fast");
        app.command_line.left();
        app.handle_key(alt_shift_t());
        app.handle_key(alt_shift_t());
        assert_eq!(app.command_line.text(), "echo fast");
        assert_eq!(app.command_line.cursor(), 8);
    }

    #[test]
    fn the_guest_moves_to_the_panels_folder_and_its_cd_moves_the_panel() {
        let temp = tempfile::tempdir().unwrap();
        let sub = temp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let mut app = app_in(temp.path());
        app.handle_key(alt_shift_t());
        assert!(tick_until(&mut app, |app| guest_text(app).contains('>')));
        app.handle_key(alt_shift_t());
        app.active_panel_mut()
            .change_dir(&crate::vfs::VfsPath::local(&sub), None)
            .unwrap();
        app.command_line.set("echo here");

        // Shown, the shell is in the panel's folder with the command line typed at its prompt.
        app.handle_key(alt_shift_t());
        let prompt = format!("{}>echo here", sub.display());
        assert!(tick_until(&mut app, |app| guest_text(app).contains(&prompt)));

        // A `cd` done in the shell moves the panel when the panels come back.
        app.guest.as_mut().unwrap().send(b"\x1b").unwrap();
        type_text(&mut app, "cd ..");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let back = format!("{}>", temp.path().display());
        assert!(tick_until(&mut app, |app| guest_text(app)
            .lines()
            .any(|line| line == back)));
        app.handle_key(alt_shift_t());
        assert_eq!(app.active_panel().path, temp.path());
    }

    #[test]
    fn the_guest_leaves_the_folder_under_the_panels_and_comes_back_even_at_once() {
        let temp = tempfile::tempdir().unwrap();
        let start = temp.path().join("start");
        std::fs::create_dir(&start).unwrap();
        let mut app = app_in(&start);
        app.handle_key(alt_shift_t());
        assert!(tick_until(&mut app, |app| guest_input(app).as_deref() == Some("")));

        // Under the panels the folder is free to move.
        app.handle_key(alt_shift_t());
        assert!(tick_until(&mut app, |app| {
            app.guest
                .as_ref()
                .is_some_and(|guest| !guest.is_changing_dir())
        }));
        let moved = temp.path().join("moved");
        crate::terminal::rename_soon(&start, &moved).unwrap();
        app.active_panel_mut()
            .change_dir(&crate::vfs::VfsPath::local(&moved), None)
            .unwrap();

        // Out and in again at once: the shell is back in the panel's folder. (Coming back while the
        // park `cd` still runs is checked in `dir_sync`: cmd is too quick to catch it here.)
        app.handle_key(alt_shift_t());
        let prompt = format!("{}>", moved.display());
        assert!(tick_until(&mut app, |app| guest_text(app).contains(&prompt)));
        app.handle_key(alt_shift_t());
        app.handle_key(alt_shift_t());
        let back = |app: &App| {
            let text = guest_text(app);
            let last = text.lines().rev().find(|line| !line.trim().is_empty());
            last == Some(prompt.as_str())
                && app
                    .guest
                    .as_ref()
                    .is_some_and(|guest| !guest.is_changing_dir())
        };
        assert!(tick_until(&mut app, back), "{}", guest_text(&app));
    }

    #[test]
    fn alt_shift_t_is_recognised_by_its_key() {
        assert!(is_guest_key(alt_shift_t()));
        assert!(!is_guest_key(KeyEvent::new(
            KeyCode::Char('t'),
            KeyModifiers::ALT
        )));
        assert!(!is_guest_key(KeyEvent::new(
            KeyCode::Char('T'),
            KeyModifiers::ALT | KeyModifiers::SHIFT | KeyModifiers::CONTROL
        )));
    }
}
