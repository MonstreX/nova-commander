//! FAR's quick search on the tree (`TreeList::FastFind`, `FindPartName`): Alt+letter opens the same
//! "Search" box as on the files; the cursor goes to the first folder whose own name starts with the text
//! (wildcards allowed; a trailing `\` changes nothing, every line is a folder), from the cursor on to the
//! end, then from the top; Ctrl+Enter / Ctrl+Shift+Enter the next / previous. The box closed, "Auto change
//! folder" takes the other panel there (FAR presses Enter after `FastFind`).

use super::super::App;
use super::TreePanel;
use crate::jobs::tree_worker::Order;
use crate::vfs;

impl TreePanel {
    /// FAR's `FindPartName` (the root has no name of its own to match): the line found, centred.
    pub(super) fn find(&mut self, text: &str, next: bool, forward: bool) -> bool {
        let prefix = text.strip_suffix('\\').unwrap_or(text);
        let count = self.items.len();
        // An empty text is `*`: Backspace and Ctrl+Y empty it, as FAR's `Name + '*'` (`treelist.cpp`).
        if count == 0 {
            return false;
        }
        let start = self.cursor.min(count - 1);
        let order: Vec<usize> = if forward {
            (start + usize::from(next)..count).chain(0..start).collect()
        } else {
            let first = if next {
                start.checked_sub(1)
            } else {
                Some(start)
            };
            let down: Vec<usize> = first.map_or(Vec::new(), |first| (0..=first).rev().collect());
            down.into_iter().chain((start + 1..count).rev()).collect()
        };
        let found = order.into_iter().find(|&at| {
            at > 0
                && (self.items[at].path.file_name())
                    .is_some_and(|name| vfs::matches_prefix(prefix, &name.to_string_lossy()))
        });
        let Some(at) = found else {
            return false;
        };
        self.cursor = at;
        // FAR's `m_CurTopFile = m_CurFile - (height - 2) / 2`, the list's rows being height - 4.
        self.top = at.saturating_sub((self.rows + 2) / 2);
        self.correct();
        self.waiting = None;
        self.send(Order::Focus(self.items[at].path.clone()));
        true
    }
}

impl App {
    /// The quick search's text on the tree: true when a folder was found.
    pub(in crate::app) fn tree_find(&mut self, text: &str, next: bool, forward: bool) -> bool {
        (self.tree_panel.as_mut()).is_some_and(|tree| tree.find(text, next, forward))
    }

    /// The quick search on the tree closed: "Auto change folder" takes the other panel there, even when
    /// the key that closed it went elsewhere (FAR's `ProcessKey(KEY_ENTER)` on the tree after
    /// `FastFind`), as long as the tree is still shown.
    pub(in crate::app) fn tree_search_closed(&mut self, on_tree: bool) {
        let shown = (self.tree_side()).is_some_and(|side| self.panels[side.index()].visible);
        if on_tree && shown && self.prefs.tree.auto_change_folder {
            self.tree_enter();
        }
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
