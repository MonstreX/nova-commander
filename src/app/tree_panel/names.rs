//! FAR's keys on the tree that give its folder away (`TreeList::ProcessKey`): Ctrl+Enter and Ctrl+F put
//! the folder's whole path in the command line (quoted as FAR's `QuoteSpace`, a space after it),
//! Ctrl+Alt+Ins puts it on the clipboard, Shift+Enter opens it in the system's file manager (FAR's
//! `OpenFolderInShell`), Ctrl+\ takes the cursor to the root and the other panel there; Alt+letter
//! starts the quick search (`search`).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::App;
use super::super::cmdline_insert::quote_for_shell;
use super::super::console_input::alt_shift_digit;
use super::super::quick_search;

impl App {
    /// The keys above on the tree; false for the others.
    pub(super) fn tree_name_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = KeyModifiers::CONTROL;
        match (key.code, key.modifiers) {
            (KeyCode::Enter, m) if m == ctrl => self.tree_path_to_line(),
            (KeyCode::Char('f'), m) if m == ctrl => self.tree_path_to_line(),
            (KeyCode::Insert, m) if m == ctrl | KeyModifiers::ALT => self.tree_path_to_clipboard(),
            (KeyCode::Enter, KeyModifiers::SHIFT) => self.open_tree_folder(),
            (KeyCode::Char('\\'), m) if m == ctrl => {
                if let Some(tree) = &mut self.tree_panel {
                    tree.cursor = 0;
                    tree.correct();
                }
                self.tree_enter();
            }
            // Alt+Shift+digit keeps a folder shortcut (`shortcuts`), as on the files.
            _ if alt_shift_digit(&key).is_some() => return false,
            _ => return quick_search::start(self, key),
        }
        true
    }

    /// The folder under the tree's cursor, the root too.
    fn tree_path(&self) -> Option<String> {
        let folder = self.tree_target().flatten()?;
        Some(folder.display().to_string())
    }

    fn tree_path_to_line(&mut self) {
        if let Some(path) = self.tree_path() {
            let text = format!("{} ", quote_for_shell(&path));
            self.command_line.insert_str(&text);
        }
    }

    fn tree_path_to_clipboard(&mut self) {
        let Some(path) = self.tree_path() else {
            return;
        };
        if let Err(error) = self.clipboard.set_text(&path) {
            self.show_error("Clipboard", &error);
        }
    }

    fn open_tree_folder(&mut self) {
        let Some(Some(folder)) = self.tree_target() else {
            return;
        };
        self.open_by_system(folder, "Open");
    }
}
