//! What the viewer asks of the app (S2d): closing with its place kept (FAR's `Viewer::SavePosition`),
//! the next file of the list, the editor, its menus and dialogs, Ctrl+F10, the settings it changes.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

use super::{App, Screen};

impl App {
    /// A key in the viewer, its place kept first (FAR's `UndoData`); out of byte editing FAR's keys
    /// over what a search found and the places (R7 review D7: Ctrl+C, Ctrl+Ins, Ctrl+Num0 copy it,
    /// Ctrl+U lets it go, Alt+BS and Ctrl+Z go back, Num5 closes), then the viewer's own.
    pub(super) fn viewer_key(&mut self, key: KeyEvent) {
        let Screen::Viewer(viewer) = &mut self.screen else {
            return;
        };
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        let keypad = key.state.contains(KeyEventState::KEYPAD);
        let shift_or_none = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;
        let back = match key.code {
            KeyCode::Backspace => key.modifiers == KeyModifiers::ALT,
            KeyCode::Char('z') => ctrl,
            _ => false,
        };
        viewer.remember_place();
        if viewer.typed_bytes().is_none() {
            match key.code {
                KeyCode::Insert | KeyCode::Char('c') if ctrl => return self.copy_found_text(),
                KeyCode::Char('0') if ctrl && keypad => return self.copy_found_text(),
                KeyCode::Char('u') if ctrl => return viewer.drop_found(),
                _ if back => return viewer.go_back(),
                KeyCode::KeypadBegin if shift_or_none => {
                    return self.handle_viewer_action(super::viewer::ViewerAction::Close);
                }
                _ => {}
            }
        }
        let action = viewer.handle_key(key);
        self.handle_viewer_action(action);
    }

    /// What the viewer's search found, put in the clipboard (FAR's `SetClipboardText`).
    fn copy_found_text(&mut self) {
        let Screen::Viewer(viewer) = &self.screen else {
            return;
        };
        if let Some(text) = viewer.found_text()
            && let Err(error) = self.clipboard.set_text(&text)
        {
            self.show_error("Viewer", &error);
        }
    }

    /// What a key in the viewer asks of the app.
    pub(super) fn handle_viewer_action(&mut self, action: super::viewer::ViewerAction) {
        use super::viewer::ViewerAction;
        match action {
            ViewerAction::None => {}
            ViewerAction::Close => {
                self.keep_viewer_position();
                self.close_window();
            }
            ViewerAction::Open(path) => self.view_next(&path),
            // FAR's viewer of a plugin's file has no F6 (`DisableEdit`): its edits would stay in the
            // copy (review C5).
            ViewerAction::Edit(path) if self.is_extracted(&path) => {}
            ViewerAction::Edit(path) => self.viewer_to_editor(&path),
            ViewerAction::ModeMenu(mode) => self.show_viewer_mode_menu(mode),
            ViewerAction::CodePageMenu(page) => self.show_code_page_menu(page),
            ViewerAction::GoTo => self.show_viewer_go_to(),
            ViewerAction::Search => self.show_viewer_search(),
            ViewerAction::SearchOn(backward) => self.viewer_search_from(backward, true),
            ViewerAction::Settings => self.ask_options(super::options::OptionsPage::ViewerLocal),
            ViewerAction::GoToFile => {
                if let Screen::Viewer(viewer) = &self.screen {
                    let path = viewer.path.clone();
                    self.panel_to_file(&path);
                }
            }
            ViewerAction::SaveBytes => self.save_viewer_bytes(false),
            ViewerAction::AskBytes => self.ask_viewer_bytes(),
            ViewerAction::Wrapped { wrap, words } => {
                self.viewer_settings.wrap = wrap;
                self.viewer_settings.word_wrap = words;
            }
            ViewerAction::Scrollbar => {
                if let Screen::Viewer(viewer) = &mut self.screen {
                    viewer.scrollbar = !viewer.scrollbar;
                    viewer.settings.show_scrollbar = viewer.scrollbar;
                    self.viewer_settings.show_scrollbar = viewer.scrollbar;
                }
            }
        }
    }

    /// F6: the editor on the file. The viewer closes once the editor is open (FAR's `FileViewer` F6): a
    /// question first (already loaded, read-only) keeps it (review C6). A new editor shows from the
    /// viewer's first row, the cursor on it, in its code page (FAR's `StartLine -2` with the viewer's
    /// `FilePos`); one already open stays where it is.
    fn viewer_to_editor(&mut self, path: &std::path::Path) {
        self.keep_viewer_position();
        let viewer = self.front_window();
        let place = match &self.screen {
            Screen::Viewer(shown) => shown.position(),
            _ => None,
        };
        let newest = self.window_lines().iter().map(|line| line.id).max();
        self.edit_file(path, true);
        if matches!(self.screen, Screen::Editor(_)) && self.front_window() != viewer {
            self.drop_window(viewer);
        }
        let new = newest.is_none_or(|newest| self.front_window() > newest);
        let (Screen::Editor(editor), Some(place), true) = (&mut self.screen, place, new) else {
            return;
        };
        editor
            .text
            .restore(&crate::settings::positions::EditorPosition {
                line: place.position,
                col: 0,
                top: place.position,
                left: 0,
                page: place.page,
                bookmarks: Vec::new(),
            });
    }

    /// The viewer's file's place kept as it is left (FAR's `Viewer::SavePosition`).
    pub(super) fn keep_viewer_position(&mut self) {
        let (Screen::Viewer(viewer), Some(store)) = (&self.screen, &self.positions) else {
            return;
        };
        if !viewer.settings.keeps_positions() {
            return;
        }
        if let Some(place) = viewer.position() {
            let file = std::path::absolute(&viewer.path).unwrap_or_else(|_| viewer.path.clone());
            // A place not kept is no failure the user has to hear of (FAR says nothing either).
            let _ = crate::settings::positions::keep_viewer_position(store, &file, place);
        }
    }
}
