//! FAR's `DialogBuilder::LinkFlags(Parent, Target, DIF_DISABLE)`: an item disabled while the check
//! box it follows is off, set again after each key (FAR's `DIF_AUTOMATION` on the box's click).

use ratatui::crossterm::event::KeyEvent;

use super::{Form, FormOutcome, Item};
use crate::app::clipboard::Clipboard;

impl Item {
    /// Disabled while the check box at `parent` is off.
    pub fn follows(mut self, parent: usize) -> Self {
        self.follows = Some(parent);
        self
    }
}

impl Form {
    pub fn handle_key(&mut self, key: KeyEvent, clipboard: &mut dyn Clipboard) -> FormOutcome {
        let outcome = self.key(key, clipboard);
        self.apply_links();
        outcome
    }

    /// Each following item disabled as its check box says.
    pub(super) fn apply_links(&mut self) {
        for index in 0..self.items.len() {
            if let Some(parent) = self.items[index].follows {
                self.items[index].disabled = !self.is_checked(parent);
            }
        }
    }
}
