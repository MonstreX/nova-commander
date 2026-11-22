//! A running command sent to the background (FAR's `ConsoleDetachKey`, ADR-0012 3d step 6). FAR gives the
//! program its console window; a pseudo console cannot be handed over, so the command keeps its user
//! screen and becomes a line of the Screens list (F12). The next command gets a new user screen.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::terminal_keys;
use super::{App, MainView};
use crate::terminal::{ShellView, UserScreen};

/// A command in the background, with the user screen it runs on.
pub struct DetachedCommand {
    screen: UserScreen,
    pub line: String,
    /// It has ended: its output can still be read once.
    pub ended: bool,
    /// Its console failed and that was reported: once is enough.
    broken: bool,
}

/// Ctrl+Shift+Tab, FAR's key; Windows Terminal keeps that one for its tabs, so Ctrl+Shift+F12 as well.
pub(super) fn is_detach_key(key: KeyEvent) -> bool {
    key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT
        && matches!(key.code, KeyCode::Tab | KeyCode::BackTab | KeyCode::F(12))
}

impl App {
    /// Bytes for the program of a command brought back from the background.
    pub(super) fn send_to_detached(&mut self, index: usize, bytes: &[u8]) -> std::io::Result<()> {
        match self.detached.get_mut(index) {
            Some(command) => command.screen.send(bytes),
            None => Ok(()),
        }
    }

    /// The running command goes to the background and the panels come back.
    pub(super) fn detach_command(&mut self) {
        let Some(screen) = self.user_screen.take() else {
            return;
        };
        let line = screen.running_line().unwrap_or_default().to_string();
        self.detached.push(DetachedCommand {
            screen,
            line,
            ended: false,
            broken: false,
        });
        self.main_view = MainView::Panels;
        self.reread_panels();
        // A user menu item's next line would run beside it: what is left of the item stops.
        self.stop_menu_run();
    }

    /// The commands in the background that still run.
    pub(super) fn detached_running(&self) -> usize {
        self.detached
            .iter()
            .filter(|command| !command.ended)
            .count()
    }

    /// What a command in the background shows, for drawing.
    pub fn detached_view(&self, index: usize) -> Option<ShellView<'_>> {
        self.detached
            .get(index)
            .map(|command| command.screen.view())
    }

    /// Feeds the commands in the background; one that ended stays to be read, and the panels are read
    /// again. Reports wait for the screen, as those of other work do. Returns whether anything on screen
    /// may have changed.
    pub(super) fn pump_detached(&mut self) -> bool {
        let mut changed = false;
        let mut ended_now = false;
        let mut failed = None;
        for (index, command) in self.detached.iter_mut().enumerate() {
            match command.screen.pump() {
                Ok(received) => changed |= received,
                Err(error) if !command.broken => failed = failed.or(Some((index, error))),
                Err(_) => {}
            }
            if !command.ended && !command.screen.has_command() {
                command.ended = true;
                ended_now = true;
            }
        }
        if ended_now {
            self.work.task_ended_signal = true;
            self.reread_panels();
            changed = true;
        }
        if self.main_view == MainView::CommandOutput || self.screen_busy() {
            return changed;
        }
        if let Some((index, error)) = failed {
            self.detached[index].broken = true;
            self.show_error("Command line", &error);
            return true;
        }
        let failure = self
            .detached
            .iter_mut()
            .find_map(|command| command.screen.take_failure());
        if let Some((line, failure)) = failure {
            self.show_run_failure(&line, failure);
            return true;
        }
        changed
    }

    /// Enter on its line in the Screens list.
    pub(super) fn show_detached(&mut self, index: usize) {
        if index < self.detached.len() {
            self.main_view = MainView::Detached(index);
        }
    }

    /// Del on its line in the Screens list: one that has ended leaves. The one on screen keeps its place.
    pub(super) fn forget_detached(&mut self, index: usize) {
        if !self
            .detached
            .get(index)
            .is_some_and(|command| command.ended)
        {
            return;
        }
        self.detached.remove(index);
        if let MainView::Detached(shown) = self.main_view {
            self.main_view = match shown.cmp(&index) {
                std::cmp::Ordering::Less => MainView::Detached(shown),
                std::cmp::Ordering::Equal => MainView::Panels,
                std::cmp::Ordering::Greater => MainView::Detached(shown - 1),
            };
        }
    }

    /// A key while a command in the background is shown: the detach key goes back to the panels, the
    /// rest goes to the command. Once it has ended any key closes it.
    pub(super) fn handle_detached_key(&mut self, index: usize, key: KeyEvent) {
        if index >= self.detached.len() {
            self.main_view = MainView::Panels;
            return;
        }
        if self
            .detached
            .get(index)
            .is_some_and(|command| command.ended)
        {
            self.detached.remove(index);
            self.main_view = MainView::Panels;
            return;
        }
        if is_detach_key(key) {
            self.main_view = MainView::Panels;
            return;
        }
        let Some(bytes) = terminal_keys::encode(key) else {
            return;
        };
        if let Some(command) = self.detached.get_mut(index)
            && let Err(error) = command.screen.send(&bytes)
        {
            self.show_error("Command line", &error);
        }
    }

    pub(super) fn resize_detached(&mut self) {
        let (rows, cols) = self.terminal_size();
        let results: Vec<_> = self
            .detached
            .iter_mut()
            .map(|command| command.screen.resize(rows, cols))
            .collect();
        if let Some(error) = results.into_iter().find_map(Result::err) {
            self.show_error("Terminal", &error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Overlay;
    use crate::app::dialog::{Dialog, ListAction};
    use crate::app::screens::ScreenEntry;
    use crate::app::user_screen_link::tests::tick_until;
    use std::path::Path;

    fn app_in(dir: &Path) -> App {
        App::new(dir, 100, 30, &crate::settings::Settings::default())
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_key(KeyEvent::new(code, modifiers));
    }

    fn ctrl_shift() -> KeyModifiers {
        KeyModifiers::CONTROL | KeyModifiers::SHIFT
    }

    /// A command waiting for its input, sent to the background.
    fn detached_question(app: &mut App) {
        app.command_line.set("set /p NC_ANSWER=Question:");
        app.run_command_line();
        let asked = tick_until(app, |app| {
            app.user_screen
                .as_ref()
                .is_some_and(|screen| screen.screen().contents().contains("Question:"))
        });
        assert!(asked);
        press(app, KeyCode::BackTab, ctrl_shift());
    }

    fn text_of(app: &App, index: usize) -> String {
        app.detached[index].screen.screen().contents()
    }

    #[test]
    fn the_keys_are_ctrl_shift_tab_and_ctrl_shift_f12() {
        for code in [KeyCode::Tab, KeyCode::BackTab, KeyCode::F(12)] {
            assert!(is_detach_key(KeyEvent::new(code, ctrl_shift())), "{code:?}");
        }
        assert!(!is_detach_key(KeyEvent::new(
            KeyCode::BackTab,
            KeyModifiers::SHIFT
        )));
        assert!(!is_detach_key(KeyEvent::new(
            KeyCode::F(12),
            KeyModifiers::CONTROL
        )));
    }

    #[test]
    fn a_detached_command_keeps_running_and_the_panels_come_back() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        detached_question(&mut app);

        assert_eq!(app.main_view, MainView::Panels);
        assert!(app.user_screen.is_none(), "the next command gets a new one");
        assert_eq!(app.detached.len(), 1);
        assert_eq!(app.detached[0].line, "set /p NC_ANSWER=Question:");
        assert_eq!(app.detached_running(), 1);
    }

    #[test]
    fn its_line_in_the_screens_list_shows_it_and_keys_go_to_it() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        detached_question(&mut app);

        press(&mut app, KeyCode::F(12), KeyModifiers::NONE);
        let Some(Overlay::Dialog(Dialog::List {
            items,
            action: ListAction::Screens(entries),
            ..
        })) = &app.overlay
        else {
            panic!("no list");
        };
        assert!(items[1].contains("/p NC_ANSW"), "{items:?}");
        assert_eq!(entries[1], ScreenEntry::Command(0));
        app.overlay = None;

        app.show_detached(0);
        assert_eq!(app.main_view, MainView::Detached(0));
        press(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.command_line.is_empty(), "nothing reaches the line");

        assert!(tick_until(&mut app, |app| app.detached[0].ended));
        // The last output may come a moment after the process has ended.
        assert!(tick_until(&mut app, |app| text_of(app, 0).contains("Question:y")));
        assert_eq!(
            app.main_view,
            MainView::Detached(0),
            "its output can be read"
        );
        press(&mut app, KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(app.main_view, MainView::Panels);
        assert!(app.detached.is_empty());
    }

    #[test]
    fn the_detach_key_leaves_it_running_and_shown_no_more() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        detached_question(&mut app);
        app.show_detached(0);

        press(&mut app, KeyCode::F(12), ctrl_shift());

        assert_eq!(app.main_view, MainView::Panels);
        assert_eq!(app.detached_running(), 1);
    }

    #[test]
    fn only_a_command_that_ended_can_be_forgotten() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        detached_question(&mut app);

        app.forget_detached(0);
        assert_eq!(app.detached.len(), 1, "it still runs");

        app.detached[0].ended = true;
        app.forget_detached(0);
        assert!(app.detached.is_empty());
    }

    #[test]
    fn another_command_can_run_while_one_is_in_the_background() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        detached_question(&mut app);

        app.command_line.set("echo second-7f2");
        app.run_command_line();

        assert_eq!(app.main_view, MainView::CommandOutput);
        assert!(tick_until(&mut app, |app| app.main_view == MainView::Panels));
        assert!(
            !text_of(&app, 0).contains("second-7f2"),
            "a screen of its own"
        );
    }

    #[test]
    fn enter_and_del_in_the_screens_list_reach_the_command() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        detached_question(&mut app);
        let entries = [ScreenEntry::Panels, ScreenEntry::Command(0)];

        app.choose_screen(&entries, 1);
        assert_eq!(app.main_view, MainView::Detached(0));
        app.main_view = MainView::Panels;

        app.detached[0].ended = true;
        app.remove_screen(&entries, 1);
        assert!(app.detached.is_empty());
    }

    #[test]
    fn removing_a_command_keeps_the_one_on_screen() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        detached_question(&mut app);
        detached_question(&mut app);
        app.detached[0].ended = true;
        app.main_view = MainView::Detached(1);

        app.forget_detached(0);

        assert_eq!(app.main_view, MainView::Detached(0));
        assert_eq!(app.detached.len(), 1);
    }

    #[test]
    fn a_view_of_nothing_goes_back_to_the_panels() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        app.main_view = MainView::Detached(3);
        press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(app.main_view, MainView::Panels);
    }

    #[test]
    fn a_failure_waits_for_the_screen_and_comes_one_at_a_time() {
        use crate::platform::process::ExecuteFailure;
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        detached_question(&mut app);
        let screen = &mut app.detached[0].screen;
        screen.push_failure("first", ExecuteFailure::FolderRefused);
        screen.push_failure("second", ExecuteFailure::FolderRefused);

        app.show_dialog(Dialog::error("Busy", vec!["keep me".to_string()]));
        app.pump_detached();
        let Some(Overlay::Dialog(Dialog::Message { title, .. })) = &app.overlay else {
            panic!("no dialog");
        };
        assert_eq!(title, "Busy", "not replaced");

        app.overlay = None;
        app.pump_detached();
        assert!(app.overlay.is_some(), "now it is reported");
        app.overlay = None;
        app.pump_detached();
        assert!(app.overlay.is_some(), "and the next one after it");
        app.overlay = None;
        app.pump_detached();
        assert!(app.overlay.is_none());
    }

    #[test]
    fn a_resize_reaches_the_commands_in_the_background() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        detached_question(&mut app);

        app.resize(80, 20);

        let (rows, cols) = app.detached[0].screen.screen().size();
        assert_eq!((rows.get(), cols.get()), (20, 80));
    }

    #[test]
    fn quitting_with_a_command_in_the_background_asks() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        detached_question(&mut app);

        app.ask_quit();
        let Some(Overlay::Dialog(Dialog::Confirm { lines, .. })) = &app.overlay else {
            panic!("no question");
        };
        assert!(
            lines.iter().any(|line| line.contains("1 command")),
            "{lines:?}"
        );
    }
}
