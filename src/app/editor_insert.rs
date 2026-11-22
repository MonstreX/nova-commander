//! The editor's keys that put text from outside in at the cursor (S2d.6f): a panel's folder or name
//! (FAR's `Edit::ProcessInsPath` in the editor's line: Ctrl+[ / Ctrl+] and the rest, as in a
//! dialog's line, `edit_insert`) and Ctrl+F's full name of the file (FAR's `FileEditor` `KEY_CTRLF`).

use ratatui::crossterm::event::KeyEvent;

use super::cmdline_insert::{NameKind, full_name, put_text, with_end_slash};
use super::edit_insert::{insertion, network_folder, quote_spaces};
use super::editing::EditorAction;
use super::{App, Screen};
use crate::editor::view::edit_view::Step;

impl App {
    /// `InsertPath` and `InsertFileName`.
    pub(super) fn editor_insert(&mut self, action: &EditorAction) {
        match action {
            EditorAction::InsertPath(key) => self.insert_path_key(*key),
            EditorAction::InsertFileName => {
                if let Screen::Editor(editor) = &mut self.screen {
                    let name = editor.path.display().to_string();
                    editor.text.press(Step::InsertName(name));
                }
            }
            _ => {}
        }
    }

    /// A panel's folder or name; a disk folder's network name asked on a worker, put in when known.
    fn insert_path_key(&mut self, key: KeyEvent) {
        if let Some(folder) = network_folder(self, key) {
            let make =
                move || quote_spaces(&with_end_slash(&full_name(&folder, NameKind::Network)));
            return put_text(self, true, make, into_editor);
        }
        if let Some(text) = insertion(self, key) {
            into_editor(self, text);
        }
    }
}

fn into_editor(app: &mut App, text: String) {
    if let Screen::Editor(editor) = &mut app.screen {
        editor.text.press(Step::Insert(text));
    }
}
