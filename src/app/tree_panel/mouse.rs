//! FAR's mouse on the tree (`TreeList::ProcessMouse`, `Scroll`): a click on a line makes the tree active
//! and takes the cursor there, a double click or the right button is Enter (the other panel goes there),
//! as is any move with "Auto change folder"; a button held over the frame above or the separator below
//! (pressed there or brought there) moves the cursor on up or down as a key held down; the wheel scrolls
//! the list and the cursor together. The left button dragged from a folder to the other panel copies it
//! (Shift or the right button there: moves), as from the files (`mouse_drag`); the cursor goes back to
//! the folder dragged when the drag's note first shows (FAR's `GoToFile(strDragName)`).

use std::path::{Path, PathBuf};

use super::super::console_input::{Mouse, MouseAction};
use super::super::mouse::{PanelArea, is_button_event};
use super::super::{App, Side};
use super::TreePanel;
use crate::jobs::tree_worker::Order;

impl TreePanel {
    /// FAR's `Scroll`: the list and the cursor `delta` lines on; the cursor waits for the other panel's
    /// folder no more and the folder under it is read first.
    pub(in crate::app) fn scroll(&mut self, delta: isize) {
        self.cursor = self.cursor.saturating_add_signed(delta);
        self.top = self.top.saturating_add_signed(delta);
        self.correct();
        self.waiting = None;
        if let Some(item) = self.current() {
            self.send(Order::Focus(item.path.clone()));
        }
    }
}

impl App {
    /// A record over the tree on `side`; true when it was taken.
    pub(in crate::app) fn tree_mouse(
        &mut self,
        side: Side,
        area: &PanelArea,
        mouse: &Mouse,
    ) -> bool {
        if !(mouse.buttons.left || mouse.buttons.right) {
            return false;
        }
        let separator = area.bottom().saturating_sub(2);
        if mouse.row > area.y && mouse.row < separator {
            self.tree_line_mouse(side, area, mouse);
            return true;
        }
        let step = if mouse.row <= area.y + 1 { -1 } else { 1 };
        self.start_holding(side, step, mouse);
        true
    }

    /// FAR's `MoveToMouse` and what a click there does: a left press starts a drag before the Enter of
    /// "Auto change folder".
    fn tree_line_mouse(&mut self, side: Side, area: &PanelArea, mouse: &Mouse) {
        self.active = side;
        let Some(top) = self.tree_panel.as_ref().map(|tree| tree.top) else {
            return;
        };
        let to = top + usize::from(mouse.row - area.y - 1);
        let moved = self.move_tree(|_| to, None);
        if self
            .tree_panel
            .as_ref()
            .is_none_or(|tree| tree.items.is_empty())
        {
            return;
        }
        if mouse.pressed.left && mouse.action == MouseAction::Button {
            self.start_tree_drag(side, area, mouse);
        }
        let double = mouse.buttons.left && mouse.action == MouseAction::DoubleClick;
        let right = mouse.pressed.right && is_button_event(mouse.action);
        if double || right || (moved && self.prefs.tree.auto_change_folder) {
            self.tree_enter();
        }
    }

    /// One step of a button held over the tree's edge; true when the cursor moved.
    pub(in crate::app) fn tree_hold_step(&mut self, step: isize) -> bool {
        self.move_tree(|tree| tree.cursor.saturating_add_signed(step), None)
    }

    /// The hold over the tree's edge is over: "Auto change folder" takes the other panel there.
    pub(in crate::app) fn tree_hold_ended(&mut self) {
        if self.prefs.tree.auto_change_folder && self.tree_has_keys() {
            self.tree_enter();
        }
    }

    /// The wheel on the tree when it has the keys; false otherwise.
    pub(in crate::app) fn tree_wheel(&mut self, lines: isize) -> bool {
        if !self.tree_has_keys() {
            return false;
        }
        if let Some(tree) = &mut self.tree_panel {
            tree.scroll(lines);
        }
        true
    }

    /// The folder under the cursor of the tree on `side`, for a drag (none on the root).
    pub(in crate::app) fn tree_drag_folder(&self, side: Side) -> Option<PathBuf> {
        let tree = self.tree_panel.as_ref().filter(|tree| tree.side == side)?;
        let item = tree.current().filter(|_| tree.cursor > 0)?;
        Some(item.path.clone())
    }

    /// The cursor of the tree back on the folder dragged; false when it is gone from the tree.
    pub(in crate::app) fn tree_back_to(&mut self, folder: &Path) -> bool {
        let Some(at) = self
            .tree_panel
            .as_ref()
            .and_then(|tree| tree.position(folder))
        else {
            return false;
        };
        self.move_tree(|_| at, None);
        true
    }
}

#[cfg(test)]
#[path = "mouse_tests.rs"]
mod tests;
