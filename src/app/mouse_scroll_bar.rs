//! The panels' scroll bars with the mouse (FAR's `ShowPanelScrollbar` in `FileList::ProcessMouse` and
//! `TreeList::ProcessMouse`): the left button on the right frame's arrows moves the cursor as Up or Down
//! held, on the bar between them puts the cursor at that part of the list and drags it along while held.

use super::App;
use super::command::Side;
use super::console_input::Mouse;
use super::mouse::PanelArea;

/// A side's bar: its column, its first row and its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Bar {
    pub x: u16,
    pub top: u16,
    pub length: u16,
}

impl Bar {
    /// The arrow under `row`: -1 the top one, 1 the bottom one.
    fn arrow(self, row: u16) -> Option<isize> {
        if row == self.top {
            Some(-1)
        } else if row + 1 == self.top + self.length {
            Some(1)
        } else {
            None
        }
    }

    /// FAR's `m_CurFile = (count - 1) * (y - ScrollY) / (height - 2)`, within the list.
    fn place(self, row: u16, count: usize) -> usize {
        let field = usize::from(self.length.saturating_sub(2)).max(1);
        let from_top = usize::from(row.saturating_sub(self.top));
        (count.saturating_sub(1) * from_top / field).min(count.saturating_sub(1))
    }
}

impl App {
    /// The bar of `side` drawn in `area` when the option shows one: the files' beside their list, the
    /// tree's from under its top frame.
    pub(super) fn scroll_bar_of(&self, side: Side, area: &PanelArea) -> Option<Bar> {
        if !self.prefs.panel_scrollbar {
            return None;
        }
        let x = (area.x + area.width).checked_sub(1)?;
        if self.tree_side() == Some(side) {
            let length = area.height.saturating_sub(4);
            return Some(Bar {
                x,
                top: area.y + 1,
                length,
            });
        }
        let list = area.list.as_ref().filter(|_| self.shows_files(side))?;
        Some(Bar {
            x,
            top: list.top,
            length: list.rows,
        })
    }

    /// The left button going down on a bar: true when it was (the cursor moved or a hold started).
    pub(super) fn scroll_bar_mouse(&mut self, side: Side, area: &PanelArea, mouse: &Mouse) -> bool {
        let Some(bar) = self.scroll_bar_of(side, area) else {
            return false;
        };
        let down = mouse.pressed.left && super::mouse::is_button_event(mouse.action);
        let rows = bar.top..bar.top + bar.length;
        if !down || mouse.column != bar.x || !rows.contains(&mouse.row) || self.mouse.drag.is_some()
        {
            return false;
        }
        match bar.arrow(mouse.row) {
            Some(step) => self.start_holding_bar(side, step, mouse),
            None if bar.length > 2 => {
                self.active = side;
                self.mouse.thumb = Some(side);
                self.thumb_to(side, bar, mouse.row);
            }
            None => {}
        }
        true
    }

    /// The thumb dragged: the cursor follows the mouse's row while the left button is held. `None` when
    /// no thumb is held.
    pub(super) fn thumb_mouse(&mut self, mouse: &Mouse) -> Option<bool> {
        let side = self.mouse.thumb?;
        if !mouse.buttons.left {
            self.mouse.thumb = None;
            return Some(false);
        }
        let area = self.mouse.areas[side.index()].clone()?;
        let bar = self.scroll_bar_of(side, &area)?;
        self.thumb_to(side, bar, mouse.row);
        Some(true)
    }

    fn thumb_to(&mut self, side: Side, bar: Bar, row: u16) {
        // FAR goes on with the row past the bar's ends: the list's first or last item.
        let row = row.clamp(bar.top, bar.top + bar.length.saturating_sub(1));
        if self.tree_side() == Some(side) {
            let count = self.tree_panel.as_ref().map_or(0, |tree| tree.items.len());
            let to = bar.place(row, count);
            self.move_tree(|_| to, None);
            return;
        }
        let panel = &mut self.panels[side.index()];
        if !panel.entries.is_empty() {
            panel.cursor = bar.place(row, panel.entries.len());
        }
    }

    /// The button held on an arrow: still there (FAR's `while_mouse_button_pressed` on `[▲]`, `[▼]`).
    pub(super) fn still_on_arrow(&self, side: Side, step: isize, mouse: &Mouse) -> bool {
        let Some(area) = self.mouse.areas[side.index()].as_ref() else {
            return false;
        };
        self.scroll_bar_of(side, area)
            .is_some_and(|bar| mouse.column == bar.x && bar.arrow(mouse.row) == Some(step))
    }
}

#[cfg(test)]
#[path = "mouse_scroll_bar_tests.rs"]
mod tests;
