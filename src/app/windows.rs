//! FAR's windows (`manager.cpp`, ADR-0010 step 3): the panels, each viewer and each editor are windows.
//! One is in front (`App::screen`); the others wait behind in the order they were last in front, so
//! closing the window in front brings back the one in front before it (FAR's `DeleteCommit` activating
//! `m_windows.back()`). The Screens list and Ctrl+Tab go through them in the order they were opened
//! (FAR's `GetSortedWindows`, by `ID`). A viewer or an editor opened from Find file's results, the editor
//! of the user menu's file and the editor of a plugin's file are modal (FAR's `ExecuteModal`, no
//! `EnableSwitch`): F12 and Ctrl+Tab do nothing in them. Quitting asks about each window that has
//! unsaved changes, as closing it would (FAR's `ExitAll`), and stays when one of them is kept.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Screen};

/// The panels' number: the first window.
pub(super) const PANELS: u64 = 0;

/// The windows behind the one in front.
pub struct Windows {
    /// The number of the window in front (FAR's `ID`: the order windows are opened in).
    front: u64,
    /// The windows behind, the one in front most lately last; the panels among them while a viewer or
    /// an editor is in front.
    behind: Vec<(u64, Screen)>,
    /// The next window's number.
    next: u64,
    /// Quitting waits for the answer about this window's unsaved changes.
    quitting: Option<u64>,
}

impl Default for Windows {
    fn default() -> Self {
        Self {
            front: PANELS,
            behind: Vec::new(),
            next: PANELS + 1,
            quitting: None,
        }
    }
}

/// A viewer's or an editor's line in the Screens list (FAR's `GetTypeAndName`, `IsFileModified`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WindowLine {
    pub id: u64,
    /// FAR's "View" or "Edit".
    pub kind: &'static str,
    pub modified: bool,
    pub name: String,
}

/// What closing the window would ask about: an editor's changes, a viewer's bytes typed.
fn unsaved(screen: &Screen) -> bool {
    match screen {
        Screen::Viewer(viewer) => viewer.typed_bytes().is_some_and(|bytes| !bytes.is_empty()),
        Screen::Editor(editor) => editor.text.is_changed(),
        Screen::Panels => false,
    }
}

fn line_of(id: u64, screen: &Screen) -> Option<WindowLine> {
    let (kind, path) = match screen {
        Screen::Viewer(viewer) => ("View", &viewer.path),
        Screen::Editor(editor) => ("Edit", &editor.path),
        Screen::Panels => return None,
    };
    Some(WindowLine {
        id,
        kind,
        modified: unsaved(screen),
        name: path.display().to_string(),
    })
}

impl App {
    /// Every window with its number, the one in front first, then those behind from the one in front
    /// most lately (FAR's `m_windows` from its end).
    fn all_windows(&self) -> impl Iterator<Item = (u64, &Screen)> {
        let behind = self.windows.behind.iter().rev();
        std::iter::once((self.windows.front, &self.screen))
            .chain(behind.map(|(id, screen)| (*id, screen)))
    }

    /// A new window in front; the one in front goes behind.
    pub(super) fn show_window(&mut self, screen: Screen) {
        let id = self.windows.next;
        self.windows.next += 1;
        let old = std::mem::replace(&mut self.screen, screen);
        self.windows.behind.push((self.windows.front, old));
        self.windows.front = id;
    }

    /// The window in front closed: the one in front before it comes back.
    pub(super) fn close_window(&mut self) {
        let (id, screen) = (self.windows.behind.pop()).unwrap_or((PANELS, Screen::Panels));
        self.screen = screen;
        self.windows.front = id;
    }

    /// Every viewer's and editor's highlighting chooses its type again ("Reload schema library").
    pub(super) fn syntax_again_everywhere(&mut self) {
        let behind = self.windows.behind.iter_mut().map(|(_, screen)| screen);
        for screen in std::iter::once(&mut self.screen).chain(behind) {
            match screen {
                Screen::Viewer(viewer) => viewer.syntax_again(),
                Screen::Editor(editor) => editor.text.syntax_mut().again(),
                Screen::Panels => {}
            }
        }
    }

    /// The window `id` behind closed (the viewer an editor took the place of).
    pub(super) fn drop_window(&mut self, id: u64) {
        self.windows
            .behind
            .retain(|(each, _)| *each != id || id == PANELS);
    }

    /// The window `id` to the front (FAR's `ActivateWindow`); false when there is none.
    pub(super) fn bring_window(&mut self, id: u64) -> bool {
        if id == self.windows.front {
            return true;
        }
        let behind = &mut self.windows.behind;
        let Some(at) = behind.iter().position(|(each, _)| *each == id) else {
            return false;
        };
        let (_, screen) = behind.remove(at);
        let old = std::mem::replace(&mut self.screen, screen);
        self.windows.behind.push((self.windows.front, old));
        self.windows.front = id;
        true
    }

    /// The number of the window in front.
    pub(super) fn front_window(&self) -> u64 {
        self.windows.front
    }

    /// The viewers and the editors in the order they were opened.
    pub(super) fn window_lines(&self) -> Vec<WindowLine> {
        let mut lines: Vec<WindowLine> = (self.all_windows())
            .filter_map(|(id, screen)| line_of(id, screen))
            .collect();
        lines.sort_by_key(|line| line.id);
        lines
    }

    /// Whether a viewer or an editor has `path` open (a plugin's file's copy stays while one has).
    pub(super) fn window_open_on(&self, path: &std::path::Path) -> bool {
        (self.all_windows()).any(|(_, screen)| match screen {
            Screen::Viewer(viewer) => viewer.path == path,
            Screen::Editor(editor) => editor.path == path,
            Screen::Panels => false,
        })
    }

    /// FAR's `GetCanLoseFocus` of the window in front: false for a modal viewer or editor, and while
    /// the editor waits for the character after Ctrl+Q.
    pub fn front_can_lose_focus(&self) -> bool {
        // The key after Ctrl+O only ends the user screen (review C7: F12 or Ctrl+Tab went on).
        if self.backdrop {
            return false;
        }
        match &self.screen {
            Screen::Panels => true,
            Screen::Viewer(_) => self.find.results.is_none(),
            // An action asked ahead and not done yet (a save, a close) is done in front first
            // (review C10: it went off whenever that editor came back).
            Screen::Editor(editor) => {
                self.find.results.is_none()
                    && self.user_menu.reread.is_none()
                    && !editor.quoting
                    && editor.pending.is_none()
                    && !self.edits_extracted(&editor.path)
            }
        }
    }

    /// Whether the screen is not drawn yet: a viewer or an editor just opened waits for its text and
    /// its colours (a short while at most), so that it shows coloured at once, as FAR's; a dialog over
    /// it is drawn.
    pub fn holds_paint(&self) -> bool {
        if self.overlay.is_some() {
            return false;
        }
        match &self.screen {
            Screen::Viewer(viewer) => viewer.holds_paint(),
            Screen::Editor(editor) => editor.text.syntax().holds_paint(),
            Screen::Panels => false,
        }
    }

    /// In a viewer or an editor that can lose the focus: F12 the Screens list (FAR's `Manager`'s key),
    /// Alt+F11 the view and edit history (FAR's `ShowViewEditHistory`; a file chosen opens a new window).
    pub(super) fn window_key(&mut self, key: KeyEvent) -> bool {
        let f12 = key.code == KeyCode::F(12) && key.modifiers == KeyModifiers::NONE;
        let alt_f11 = key.code == KeyCode::F(11) && key.modifiers == KeyModifiers::ALT;
        let window = !matches!(self.screen, Screen::Panels);
        if !(f12 || alt_f11) || !window || self.overlay.is_some() || !self.front_can_lose_focus() {
            return false;
        }
        if f12 {
            self.show_screens();
        } else {
            self.open_history_menu(crate::settings::history::List::Views);
        }
        true
    }

    /// The editor window on `path` (FAR's `FindWindowByFile`: the full names, case not counted).
    fn editor_window_on(&self, path: &std::path::Path) -> Option<u64> {
        let key = |path: &std::path::Path| {
            let full = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
            full.to_string_lossy().to_lowercase()
        };
        let wanted = key(path);
        (self.all_windows()).find_map(|(id, screen)| match screen {
            Screen::Editor(editor) if key(&editor.path) == wanted => Some(id),
            _ => None,
        })
    }

    /// FAR's `FileEditor::Init` for a file an editor has open: with "Reload edited file" (or that
    /// editor's changes unsaved) its question "already loaded. How to open this file?", else that
    /// editor to the front. False when no editor has it.
    pub(super) fn ask_reedit(
        &mut self,
        path: &std::path::Path,
        remember: bool,
        place: (usize, usize),
    ) -> bool {
        let Some(window) = self.editor_window_on(path) else {
            return false;
        };
        let unsaved = (self.all_windows()).any(|(id, screen)| id == window && unsaved(screen));
        if !unsaved && !self.prefs.confirm_reedit {
            self.bring_window(window);
            return true;
        }
        // From Find file's results the new editor is modal: no going to the one behind (FAR's
        // question without `FFILEEDIT_ENABLEF6`).
        // From the user menu too (review C9; its menu is read again once that editor closes).
        let modal = self.find.results.is_some() || self.user_menu.reread.is_some();
        let buttons = match modal {
            true => vec!["&New instance", "&Cancel"],
            false => vec!["&Current", "&New instance", "R&eload", "&Cancel"],
        };
        let full = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        self.show_dialog(super::Dialog::Confirm {
            title: "Editor".to_string(),
            lines: vec![
                full.display().to_string(),
                "already loaded. How to open this file?".to_string(),
            ],
            buttons,
            focused: 0,
            danger: false,
            action: super::dialog::ConfirmAction::Reedit {
                path: path.to_path_buf(),
                remember,
                place,
                window,
                modal,
            },
        });
        true
    }

    /// The answer: Current brings that editor, New instance opens another, Reload closes that one
    /// (its changes and its place dropped, FAR's `FFILEEDIT_DISABLESAVEPOS`) and opens the file again.
    pub(super) fn answer_reedit(&mut self, action: super::dialog::ConfirmAction, button: usize) {
        let super::dialog::ConfirmAction::Reedit {
            path,
            remember,
            place,
            window,
            modal,
        } = action
        else {
            return;
        };
        let choice = match (modal, button) {
            (true, 0) => 1,
            (true, _) => 3,
            (false, button) => button,
        };
        match choice {
            0 => {
                self.bring_window(window);
            }
            1 => self.open_editor_at(&path, remember, place),
            2 => {
                if self.bring_window(window) {
                    self.close_window();
                }
                self.open_editor_at(&path, remember, place);
            }
            _ => {}
        }
    }

    /// F10's questions while tasks run: Wait, Cancel all, Back; then Wait or Quit anyway for tasks that do
    /// not stop.
    pub(super) fn answer_quit_with_tasks(
        &mut self,
        quit: &super::dialog::ConfirmAction,
        button: usize,
    ) {
        match (quit, button) {
            (super::dialog::ConfirmAction::QuitWithTasks, 0) => {
                self.tasks.resume_all();
                self.work.quit_when_idle = true;
                self.environment.confirm_exit_code();
            }
            (super::dialog::ConfirmAction::QuitWithTasks, 1) => {
                self.tasks.cancel_all();
                self.work.quit_when_idle = true;
                self.environment.confirm_exit_code();
            }
            (super::dialog::ConfirmAction::QuitWithTasks, 2) => self.work.quit_when_idle = false,
            (super::dialog::ConfirmAction::QuitNotStopping, 0) => self.tasks.wait_for_stop(),
            (super::dialog::ConfirmAction::QuitNotStopping, 1) => {
                self.tasks.abandon_all();
                self.finish_quit();
            }
            (super::dialog::ConfirmAction::QuitNotStopping, _) => self.stay_after_not_stopping(),
            _ => {}
        }
    }

    /// Quitting agreed: FAR's `ExitAll` - the first window with unsaved changes (from the one in front)
    /// comes to the front and asks as closing it would; with none left the windows close (their places
    /// kept) and the program ends.
    pub(super) fn finish_quit(&mut self) {
        self.windows.quitting = None;
        let first = (self.all_windows()).find(|(_, screen)| unsaved(screen));
        if let Some((id, _)) = first {
            self.bring_window(id);
            self.windows.quitting = Some(id);
            match &mut self.screen {
                // Its own close asked ahead goes: the question is asked once (review C10).
                Screen::Editor(editor) => {
                    editor.pending = None;
                    self.close_editor();
                }
                Screen::Viewer(_) => self.ask_viewer_bytes(),
                Screen::Panels => {}
            }
            return;
        }
        while let Some(line) = self.window_lines().first() {
            self.bring_window(line.id);
            self.keep_viewer_position();
            self.keep_editor_position();
            self.close_window();
        }
        self.quit = true;
    }

    /// Once the window quitting asked about has its answer: quitting goes on when its changes are
    /// saved or dropped, and stops when they are kept (Cancel, or a save that failed).
    pub(super) fn pump_quitting(&mut self) -> bool {
        let Some(id) = self.windows.quitting else {
            return false;
        };
        if self.overlay.is_some() || self.is_waiting() {
            return false;
        }
        let kept = (self.all_windows()).any(|(each, screen)| each == id && unsaved(screen));
        if kept {
            self.windows.quitting = None;
            return false;
        }
        self.finish_quit();
        true
    }
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
