//! FAR's scroll bar on the right side of a menu or list longer than its box (`VMenu::ProcessMouse`
//! with `ShowMenuScrollbar`; a drop-down field's list always has it): the left button held on `▲` or
//! `▼` moves one line and on at the keyboard's repeat rate, pressed on the field between them it drags
//! the selection to the place the mouse is at (FAR's `(count - 1) * (y - top) / (height - 3)`) while
//! it stays pressed, wherever the mouse goes.

use std::time::Instant;

use super::{Kind, MenuArea, MenuHold};
use crate::app::App;
use crate::app::console_input::Mouse;
use crate::platform;

impl App {
    /// A record on the scroll bar of the menu in front, or while its slider is dragged; false when it
    /// is for something else.
    pub(super) fn menu_scroll_bar(&mut self, kind: Kind, area: MenuArea, mouse: &Mouse) -> bool {
        if !mouse.buttons.left {
            // Let go after a drag: nothing more (no Enter on an item).
            return std::mem::take(&mut self.mouse.menu_thumb);
        }
        let frame = area.frame;
        let len = self.menu_len(kind);
        if self.mouse.menu_thumb {
            self.menu_thumb_to(kind, area, len, mouse.row);
            return true;
        }
        let rows = usize::from(frame.height.saturating_sub(2));
        let shown = match kind {
            Kind::DropDown => true,
            Kind::Bar | Kind::Popup | Kind::Completion => false,
            _ => self.prefs.dialogs.menu_scrollbar,
        };
        let top = frame.y + 1;
        let bottom = frame.y + frame.height.saturating_sub(2);
        let on_bar = shown
            && len > rows
            && rows >= 2
            && mouse.column + 1 == frame.x + frame.width
            && (top..=bottom).contains(&mouse.row);
        if !on_bar {
            return false;
        }
        let step = match mouse.row {
            row if row == top => -1,
            row if row == bottom => 1,
            _ => {
                self.mouse.menu_hold = None;
                self.mouse.menu_thumb = true;
                self.menu_thumb_to(kind, area, len, mouse.row);
                return true;
            }
        };
        if self.mouse.menu_hold.is_none_or(|hold| hold.step != step) {
            self.menu_step(kind, step);
            let (delay, _) = platform::input::keyboard_repeat();
            self.mouse.menu_hold = Some(MenuHold {
                step,
                next: Instant::now() + delay,
            });
        }
        true
    }

    /// The selection at the place of row `row` along the bar, on a line that can take it.
    fn menu_thumb_to(&mut self, kind: Kind, area: MenuArea, len: usize, row: u16) {
        let frame = area.frame;
        let Some(last) = len.checked_sub(1) else {
            return;
        };
        let field = usize::from(frame.height.saturating_sub(3)).max(1);
        let at = (last * usize::from(row.saturating_sub(frame.y)) / field).min(last);
        let found = (at..len)
            .find(|&index| self.menu_item_selectable(kind, index))
            .or_else(|| {
                (0..at)
                    .rev()
                    .find(|&index| self.menu_item_selectable(kind, index))
            });
        if let Some(index) = found {
            self.menu_select(kind, index);
        }
    }
}

#[cfg(test)]
#[path = "mouse_menu_bar_tests.rs"]
mod tests;
