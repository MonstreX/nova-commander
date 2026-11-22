//! FAR's operations on the tree (`TreeList::ProcessKey`, `TreeList::GetSelName`): the folder under the
//! cursor is what F5, F6 and Alt+F6 copy, move or link to the other panel's folder (Shift+F5 / Shift+F6
//! with its own path in the line, FAR's `CurrentOnly`), what F8 and Shift+Del delete and what F4 and
//! Ctrl+A give the attributes of; F7 makes a folder in it. A target written without its folder is in the
//! folder that holds it (FAR's `ShellCopy` changes there). Each takes the folder as the key is pressed:
//! the cursor may move while its dialog is up (the other panel's folder read in). A hidden tree takes no
//! key (FAR's `if (!IsVisible()) return false`), an empty one does nothing with them. The tree learns
//! what was done from the operations' targets and the folders' times (ADR-0014).

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::super::App;
use super::super::transfer_dialog::{Operation, quote_target};
use crate::vfs::VfsPath;

impl App {
    /// Whether the tree has the keys: the active side, shown.
    pub(in crate::app) fn tree_has_keys(&self) -> bool {
        self.tree_is_active() && self.active_panel().visible
    }

    /// When the tree has the keys, the folder under its cursor (FAR's `TreeList::GetSelName`; none: the
    /// tree has no line yet).
    pub(in crate::app) fn tree_target(&self) -> Option<Option<PathBuf>> {
        let tree = self.tree_panel.as_ref().filter(|_| self.tree_has_keys())?;
        Some(tree.current().map(|item| item.path.clone()))
    }

    /// As [`tree_target`](Self::tree_target), but none on the root: no copy, move, link or deletion of a
    /// whole disk.
    pub(in crate::app) fn tree_folder_below_root(&self) -> Option<Option<PathBuf>> {
        let tree = self.tree_panel.as_ref().filter(|_| self.tree_has_keys())?;
        Some((tree.current().filter(|_| tree.cursor > 0)).map(|item| item.path.clone()))
    }

    /// Where a target written without its folder goes: beside the tree's folder (FAR's `ShellCopy`
    /// changes to the folder that holds it), taken from what the operation took; else the active
    /// panel's folder.
    pub(in crate::app) fn operation_base(&self, sources: &[VfsPath]) -> VfsPath {
        // A plugin's real files (ADR-0016 §13): a name typed is beside them, as a rename of the file on
        // its disk - never a copy into the plugin and the file then removed.
        let real_files = self.active_panel().path.as_local().is_none()
            && !sources.is_empty()
            && sources.iter().all(|source| source.as_local().is_some());
        let beside = (sources.first())
            .filter(|_| self.tree_has_keys() || real_files)
            .and_then(VfsPath::parent);
        beside.unwrap_or_else(|| self.active_panel().path.clone())
    }

    /// Shift+F5 / Shift+F6 on the tree (FAR's `CurrentOnly`): the folder's own path in the line.
    fn ask_tree_current(&mut self, operation: Operation) {
        let Some(Some(folder)) = self.tree_folder_below_root() else {
            return;
        };
        let move_ = operation == Operation::Move;
        let target = quote_target(folder.display().to_string(), move_);
        // FAR's tree: `ToPlugin = 0` for Shift+F5 / Shift+F6.
        self.ask_transfer_of(
            operation,
            vec![VfsPath::local(&folder)],
            &target,
            false,
            None,
        );
    }

    /// The operations' keys on the tree; false for the others. A held F8 or Shift+Del deletes once, as
    /// on the files (the cursor goes to the folder above when one is gone).
    pub(super) fn tree_operation_key(&mut self, key: KeyEvent) -> bool {
        let plain = key.modifiers.is_empty();
        let shift = key.modifiers == KeyModifiers::SHIFT;
        let alt = key.modifiers == KeyModifiers::ALT;
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        let once = key.kind != KeyEventKind::Repeat;
        match key.code {
            KeyCode::F(5) if plain => self.ask_copy(),
            KeyCode::F(6) if plain => self.ask_move(),
            KeyCode::F(6) if alt => self.ask_link(),
            KeyCode::F(5) if shift => self.ask_tree_current(Operation::Copy),
            KeyCode::F(6) if shift => self.ask_tree_current(Operation::Move),
            KeyCode::F(7) if plain => self.ask_make_dir(),
            KeyCode::F(8) if plain => {
                if once {
                    self.ask_delete(false, false);
                }
            }
            KeyCode::Delete if shift => {
                if once {
                    self.ask_delete(false, true);
                }
            }
            KeyCode::F(4) if plain => self.ask_attributes(),
            KeyCode::Char('a') if ctrl => self.ask_attributes(),
            _ => return false,
        }
        true
    }
}

/// F8 / Shift+Del from the tree: its folder `folder` as the files panel would list it, looked at on its
/// disk (off the screen's thread: it may not answer); an empty list on the root or a folder gone.
pub(in crate::app) fn delete_items_of(
    folder: Option<std::path::PathBuf>,
) -> Vec<crate::jobs::delete::Item> {
    (folder.into_iter())
        .filter_map(|folder| {
            let path = VfsPath::local(&folder);
            let provider = crate::vfs::provider_for(&path).ok()?;
            let entry = provider.stat(&path).ok()?;
            Some(crate::jobs::delete::Item { path, entry })
        })
        .collect()
}

/// Whether the tree's folder can go to the Recycle Bin (its disk's provider says).
pub(in crate::app) fn can_trash(items: &[crate::jobs::delete::Item]) -> bool {
    (items.first()).is_some_and(|item| {
        crate::vfs::provider_for(&item.path)
            .is_ok_and(|provider| provider.capabilities(&item.path).trash)
    })
}

#[cfg(test)]
#[path = "operations_tests.rs"]
mod tests;
