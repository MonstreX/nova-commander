//! The viewer's and the editor's own screen keys (S2d.6f; FAR's `FileViewer::ProcessKey`,
//! `FileEditor::ProcessKey`): Ctrl+B shows or hides the key bar (FAR's `ShowKeyBar`), Ctrl+Shift+B
//! the status line (`ShowTitleBar`), both kept in the settings as FAR keeps them; Ctrl+O shows the
//! user screen until a key is pressed (FAR's `ShowBackground` and `WaitKey`: that key does nothing
//! else).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Screen};

impl App {
    /// A key the viewer's or the editor's screen takes itself; whether it was one.
    pub(super) fn text_screen_key(&mut self, key: KeyEvent) -> bool {
        if std::mem::take(&mut self.backdrop) {
            return true;
        }
        // The key after Ctrl+Q is the editor's character.
        if matches!(&self.screen, Screen::Editor(editor) if editor.quoting) {
            return false;
        }
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        let ctrl_shift = key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        match key.code {
            KeyCode::Char('o') if ctrl => self.backdrop = true,
            KeyCode::Char('b') if ctrl => self.toggle_text_bar(true),
            KeyCode::Char('b' | 'B') if ctrl_shift => self.toggle_text_bar(false),
            _ => return false,
        }
        true
    }

    /// The key bar (`keys`) or the status line shown or hidden, in this viewer or editor and for the
    /// next ones.
    fn toggle_text_bar(&mut self, keys: bool) {
        let (own, all) = match &mut self.screen {
            Screen::Viewer(viewer) => (
                (
                    &mut viewer.settings.show_key_bar,
                    &mut viewer.settings.show_title_bar,
                ),
                (
                    &mut self.viewer_settings.show_key_bar,
                    &mut self.viewer_settings.show_title_bar,
                ),
            ),
            Screen::Editor(editor) => (
                (
                    &mut editor.settings.show_key_bar,
                    &mut editor.settings.show_title_bar,
                ),
                (
                    &mut self.editor_settings.show_key_bar,
                    &mut self.editor_settings.show_title_bar,
                ),
            ),
            Screen::Panels => return,
        };
        let (own, all) = if keys { (own.0, all.0) } else { (own.1, all.1) };
        *own = !*own;
        *all = *own;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_key(KeyEvent::new(code, modifiers));
    }

    #[test]
    fn ctrl_b_and_ctrl_shift_b_hide_the_bars_for_the_next_viewers_too_and_ctrl_o_waits_a_key() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("v.txt");
        std::fs::write(&path, "text").unwrap();
        let mut app = crate::app::fileops::tests::app_in(temp.path());
        app.view_file(&path, false);
        let ctrl = KeyModifiers::CONTROL;
        press(&mut app, KeyCode::Char('b'), ctrl);
        press(&mut app, KeyCode::Char('B'), ctrl | KeyModifiers::SHIFT);
        let Screen::Viewer(viewer) = &app.screen else {
            panic!("the viewer");
        };
        assert!(!viewer.settings.show_key_bar && !viewer.settings.show_title_bar);
        assert!(!app.viewer_settings.show_key_bar && !app.viewer_settings.show_title_bar);
        press(&mut app, KeyCode::Char('o'), ctrl);
        assert!(app.backdrop, "the user screen");
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.backdrop);
        assert!(
            matches!(app.screen, Screen::Viewer(_)),
            "the key that ends it does nothing else"
        );
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        app.edit_file(&path, false);
        assert!(matches!(app.screen, Screen::Editor(_)), "the editor");
        press(&mut app, KeyCode::Char('b'), ctrl);
        assert!(!app.editor_settings.show_key_bar);
        assert!(app.editor_settings.show_title_bar);
    }
}
