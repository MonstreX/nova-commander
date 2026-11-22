//! The command line and FAR's user screen (ADR-0006): Enter runs the line as a process of its own in
//! the active panel's folder; `cd`, a drive and `set` are carried out by the program and only shown.
//! While a command runs its output fills the console and every key goes to it; when it ends the
//! panels come back and are re-read.

use std::path::Path;

use super::dialog::Dialog;
use super::terminal_keys;
use super::{App, MainView};
use crate::platform;
use crate::platform::process::{ExecuteFailure, ProgramStart};
use crate::terminal::{self, Builtin, StartMode, UserScreen};
use ratatui::crossterm::event::KeyEvent;

impl App {
    /// The code `exit` asked the program to end with.
    pub fn exit_code(&self) -> i32 {
        self.environment.exit_code()
    }

    /// The console's title `title` asked for, once.
    pub fn take_user_title(&mut self) -> Option<String> {
        self.environment.take_title()
    }

    /// What the console's title says while no task runs, besides the `title` command's: the search's
    /// end while its dialog is up (FAR's `SetFarTitle`, until the panel's title comes back with it gone).
    pub fn screen_title(&self) -> Option<String> {
        match &self.overlay {
            Some(super::Overlay::FindResults(results)) => results.done_title(),
            _ => None,
        }
    }

    /// The system's environment variables changed (FAR's `update_environment`): with System > "Automatic
    /// update of environment variables" on, the next commands get them (`Environment::reload`).
    pub(super) fn pump_environment(&mut self) -> bool {
        if crate::platform::process::take_environment_change() {
            self.environment_changed();
        }
        false
    }

    pub(super) fn environment_changed(&mut self) {
        if self.prefs.update_environment {
            let fresh = crate::platform::process::system_environment();
            self.environment.reload(fresh);
        }
    }

    /// The screens are as large as the console: the panels cover them.
    pub(super) fn terminal_size(&self) -> (u16, u16) {
        (self.height.max(1), self.width.max(1))
    }

    /// The guest shell leaves the top and the bottom row to the program (`ui::guest_frame`).
    pub(super) fn guest_size(&self) -> (u16, u16) {
        (self.height.saturating_sub(2).max(1), self.width.max(1))
    }

    /// The user screen, opened on first need (and again after it failed to open).
    fn user_screen(&mut self) -> Option<&mut UserScreen> {
        if self.user_screen.is_none() {
            let (rows, cols) = self.terminal_size();
            match UserScreen::new(rows, cols) {
                Ok(screen) => self.user_screen = Some(screen),
                Err(error) => self.show_dialog(Dialog::error(
                    "Command line",
                    vec![format!("Cannot open the console for commands: {error}")],
                )),
            }
        }
        self.user_screen.as_mut()
    }

    pub(super) fn resize_terminal(&mut self) {
        let (rows, cols) = self.terminal_size();
        let (guest_rows, guest_cols) = self.guest_size();
        let results = [
            self.user_screen
                .as_mut()
                .map(|screen| screen.resize(rows, cols)),
            self.guest
                .as_mut()
                .map(|guest| guest.resize(guest_rows, guest_cols)),
        ];
        if let Some(error) = results.into_iter().flatten().find_map(Result::err) {
            self.show_error("Terminal", &error);
        }
        self.resize_detached();
    }

    /// Feeds the user screen's output; when the shown command has ended, the panels come back.
    /// Returns whether anything on screen may have changed.
    pub(super) fn pump_user_screen(&mut self) -> bool {
        let Some(screen) = &mut self.user_screen else {
            return false;
        };
        let pumped = screen.pump();
        let running = screen.has_command();
        let mut changed = match pumped {
            Ok(received) => received,
            Err(error) => {
                self.show_error("Command line", &error);
                true
            }
        };
        if self.main_view == MainView::CommandOutput && !running {
            self.main_view = MainView::Panels;
            self.reread_panels();
            changed = true;
        }
        if self.main_view != MainView::CommandOutput
            && !self.screen_busy()
            && let Some((line, failure)) =
                self.user_screen.as_mut().and_then(UserScreen::take_failure)
        {
            self.show_run_failure(&line, failure);
            changed = true;
        }
        changed
    }

    /// FAR's message after `Execute` failed.
    pub(super) fn show_run_failure(&mut self, line: &str, failure: ExecuteFailure) {
        let (title, lines) = match failure {
            ExecuteFailure::FolderRefused => (
                "Command line",
                vec![platform::process::INTERPRETER_FOLDER_REFUSAL.to_string()],
            ),
            ExecuteFailure::CannotExecute {
                interpreter: false,
                error,
            } => (
                "Error",
                vec![
                    "Cannot execute".to_string(),
                    line.to_string(),
                    system_text(&error, line),
                ],
            ),
            ExecuteFailure::CannotExecute {
                interpreter: true,
                error,
            } => (
                "Error",
                vec![
                    "Cannot invoke command interpreter".to_string(),
                    line.to_string(),
                    "Check COMSPEC environment variable".to_string(),
                    system_text(&error, line),
                ],
            ),
        };
        self.show_dialog(Dialog::error(title, lines));
    }

    /// Enter on the command line (FAR's `CommandLine::ExecString`). An empty line only shows the
    /// prompt, as FAR scrolls the user screen.
    pub(super) fn run_command_line(&mut self) {
        let text = self.command_line.take();
        let line = text.trim();
        let dir = self.active_folder();
        if line.is_empty() {
            self.show_in_user_screen(&dir, "");
            return;
        }
        self.push_history(line);
        // FAR's `ProcessCommandLine`, before the system's own commands (`ProcessOSCommands`).
        if self.plugin_command_line(line, &dir) {
            return;
        }
        match terminal::parse_builtin(line) {
            Some(builtin) => self.carry_out(builtin, &dir, line),
            None => self.run_in_user_screen(&dir, line),
        }
    }

    /// A command the program carries out itself (FAR's `ProcessOSCommands`); only its line is shown.
    pub(super) fn carry_out(&mut self, builtin: Builtin, dir: &Path, line: &str) {
        if let Builtin::Exit(code) = builtin {
            // FAR ends at once; running tasks are not dropped without a word.
            if self.tasks.active() > 0 || self.detached_running() > 0 {
                self.ask_quit();
                self.environment.ask_exit_code(code);
            } else {
                self.environment.set_exit_code(code);
                self.finish_quit();
            }
            return;
        }
        self.show_in_user_screen(dir, line);
        match builtin {
            Builtin::ChangeDir(target) => {
                self.change_folder(&target);
            }
            Builtin::SetVariable { name, value } => self.environment.set(&name, &value),
            Builtin::PushDir(target) => {
                let current = self.command_panel().path.to_string();
                let pushed = match target {
                    None => Some(current),
                    // A folder read in the background is remembered once it is entered.
                    Some(target) if self.change_folder(&target) => {
                        let side = self.command_side().index();
                        self.panels[side].push_when_entered(current)
                    }
                    Some(_) => None,
                };
                if let Some(folder) = pushed {
                    self.environment.push_dir(&folder);
                }
            }
            Builtin::PopDir => {
                if let Some(folder) = self.environment.pop_dir() {
                    self.change_folder(&folder);
                }
            }
            Builtin::ClearDirs => self.environment.clear_dirs(),
            Builtin::SetTitle(title) => self.environment.set_title(title),
            Builtin::Exit(_) => {}
        }
    }

    /// FAR's `IntChDir`, on the panel commands act on; false when it failed.
    fn change_folder(&mut self, target: &str) -> bool {
        if let Some(root) = crate::platform::files::paths::bare_drive_root(target) {
            self.change_to_drive(&root.to_string_lossy());
            return true;
        }
        let panel = &mut self.panels[self.command_side().index()];
        let path = panel.path.resolve(target);
        match panel.change_dir(&path, None) {
            Ok(_) => true,
            Err(error) => {
                self.show_error("Change directory", &error);
                false
            }
        }
    }

    /// Shows the prompt line with `line` without running anything (FAR echoes its own commands).
    pub(super) fn show_in_user_screen(&mut self, dir: &Path, line: &str) {
        let Some(screen) = self.user_screen() else {
            return;
        };
        if let Err(error) = screen.show(dir, line) {
            self.show_error("Command line", &error);
        }
    }

    /// Runs `line` in `dir`; its output is shown until it ends.
    pub(super) fn run_in_user_screen(&mut self, dir: &Path, line: &str) {
        let _ = self.started_in_user_screen(dir, line);
    }

    /// As [`Self::run_in_user_screen`]: whether it started.
    pub(super) fn started_in_user_screen(&mut self, dir: &Path, line: &str) -> bool {
        let environment = self.environment.clone();
        let Some(screen) = self.user_screen() else {
            return false;
        };
        match screen.run(dir, line, &environment) {
            Ok(()) => {
                self.main_view = MainView::CommandOutput;
                true
            }
            Err(error) => {
                self.show_error("Command line", &error);
                false
            }
        }
    }

    /// Shift+Enter (FAR): the command line, or with an empty line the program under the cursor, runs
    /// in a console window of its own; the program does not wait for it. Another file or a folder under
    /// the cursor opens with its Windows association (a folder in Explorer).
    pub(super) fn run_in_new_window(&mut self) {
        let mode = StartMode {
            new_window: true,
            ..StartMode::default()
        };
        if let Some(line) = self.take_typed_line() {
            return self.start_detached(line, mode);
        }
        let Some(path) = self.active_panel().current_path() else {
            return;
        };
        match self.current_program(&path) {
            Some(ProgramStart::File) => self.start_detached(
                path.to_string_lossy().into_owned(),
                StartMode { file: true, ..mode },
            ),
            Some(ProgramStart::Line(line)) => self.start_detached(line, mode),
            None => self.open_by_system(path, "Open"),
        }
    }

    /// Ctrl+Alt+Enter (FAR): the command line, or the file under the cursor, with the administrator's
    /// rights; Windows asks, and the program does not wait.
    pub(super) fn run_as_administrator(&mut self) {
        if let Some((line, mode)) = self.administrator_start() {
            self.start_detached(line, mode);
        }
    }

    /// What Ctrl+Alt+Enter starts. A document too, as FAR's `known` source with the `runas` verb.
    fn administrator_start(&mut self) -> Option<(String, StartMode)> {
        let mode = StartMode {
            run_as: true,
            ..StartMode::default()
        };
        if let Some(line) = self.take_typed_line() {
            return Some((line, mode));
        }
        let path = self.active_panel().current_path()?;
        Some(match self.current_program(&path) {
            Some(ProgramStart::Line(line)) => (line, mode),
            _ => (
                path.to_string_lossy().into_owned(),
                StartMode { file: true, ..mode },
            ),
        })
    }

    /// The typed line, gone from the command line and kept in the history; `None` when nothing is typed.
    fn take_typed_line(&mut self) -> Option<String> {
        let typed = self.command_line.text().trim().to_string();
        if typed.is_empty() {
            return None;
        }
        self.command_line.take();
        self.push_history(&typed);
        Some(typed)
    }

    /// Starts `line` beside the UI: looking for the program is disk work, and only a failure comes back.
    fn start_detached(&mut self, line: String, mode: StartMode) {
        let dir = self.active_folder();
        let environment = self.environment.clone();
        self.wait_for(Vec::new(), move || {
            let ended = terminal::detached_runner(&dir, &line, mode, &environment)
                .and_then(|mut runner| runner.status());
            Box::new(move |app: &mut App| match ended {
                Ok(status) => {
                    let code = status.code().map_or(0, i32::cast_unsigned);
                    if let Some(failure) = terminal::runner_failure(code) {
                        app.show_run_failure(&line, failure);
                    }
                }
                Err(error) => app.show_error("Run in a new window", &error),
            })
        });
        self.label_waiting("Starting the program…");
    }

    /// While a command runs every key goes to it (Ctrl+C stops it), as in FAR; FAR's detach key sends
    /// it to the background.
    pub(super) fn handle_command_output_key(&mut self, key: KeyEvent) {
        if super::detached::is_detach_key(key) {
            return self.detach_command();
        }
        let Some(bytes) = terminal_keys::encode(key) else {
            return;
        };
        if let Some(screen) = &mut self.user_screen
            && let Err(error) = screen.send(&bytes)
        {
            self.show_error("Command line", &error);
        }
    }

    /// Enter on a file: run programs on the user screen, open other files with their association.
    pub(super) fn open_current_file(&mut self) {
        // A plugin's file it writes out: its copy first (FAR's `ProcessEnter` with `PluginMode`).
        if self.get_to_open(false) {
            return;
        }
        // The file's association for Enter first (FAR's `ProcessLocalFileTypes`), then a plugin of its
        // masks for a file that is no program (`OpenFilePlugin`).
        if self.open_by_association(super::associations::AssocMode::Execute) {
            return;
        }
        let program = (self.active_panel().current_path())
            .is_some_and(|path| self.current_program(&path).is_some());
        if program || !self.open_file_by_plugins(false) {
            self.open_without_association();
        }
    }

    /// Enter on a file as it is without associations: a program runs, a document opens.
    pub(super) fn open_without_association(&mut self) {
        let Some(path) = self.active_panel().current_path() else {
            return;
        };
        let dir = self.active_folder();
        match self.current_program(&path) {
            Some(ProgramStart::File) => self.run_file_in_user_screen(&dir, &path),
            Some(ProgramStart::Line(line)) => self.run_in_user_screen(&dir, &line),
            None => self.open_by_system(path, "Open"),
        }
    }

    /// Ctrl+PgDn on a file: its association for the key, else a plugin of its masks (FAR's
    /// `OpenFilePlugin` with `OFP_ALTERNATIVE`: an archive by its contents, S6.1a); nothing more when no
    /// plugin takes it, as in FAR.
    pub(super) fn open_current_file_alt(&mut self) {
        if self.get_to_open(true) {
            return;
        }
        if !self.open_by_association(super::associations::AssocMode::AltExecute) {
            self.open_file_by_plugins(true);
        }
    }

    fn run_file_in_user_screen(&mut self, dir: &Path, path: &Path) {
        let environment = self.environment.clone();
        let Some(screen) = self.user_screen() else {
            return;
        };
        match screen.run_file(dir, path, &environment) {
            Ok(()) => self.main_view = MainView::CommandOutput,
            Err(error) => self.show_error("Command line", &error),
        }
    }

    /// How the program under the cursor is started; `None` for other files.
    fn current_program(&self, path: &Path) -> Option<ProgramStart> {
        let attributes = self.active_panel().current()?.attributes;
        platform::process::program_start(&path.file_name()?.to_string_lossy(), attributes)
    }
}

/// The system's text leaves `%1` for the name of what failed.
fn system_text(error: &std::io::Error, line: &str) -> String {
    error.to_string().replace("%1", line)
}

#[cfg(test)]
#[path = "user_screen_link_tests.rs"]
pub(super) mod tests;
