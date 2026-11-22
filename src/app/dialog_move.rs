//! Moving the dialog in front (FAR's `Dialog::ProcessMoveDialog` and `ProcessDrag`): Ctrl+F5 starts
//! moving it with the keys (FAR's `DMODE_KEYDRAGGED`: the arrows by one, Ctrl+Left / Ctrl+Right by
//! ten, Ctrl+Up / Ctrl+Down by five, Home / End / PgUp / PgDn to the screen's edges, Enter or Ctrl+F5
//! leaves it there, Esc puts it back); the left button pressed on a place of it that is no item drags
//! it until let go, the right one puts it back (`DMODE_MOUSEDRAGGED`). The place is a shift from the
//! middle of the screen, kept inside the screen by the drawing (FAR lets a dialog go off it but for a
//! column); each dialog shown comes up in the middle again.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::App;
use super::console_input::Mouse;

/// Far enough to reach any edge: the drawing keeps the dialog inside the screen.
const TO_THE_EDGE: i16 = i16::MAX / 4;

/// Where the dialog in front stands, and how it is being moved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DialogPlace {
    /// Columns and rows from the middle of the screen.
    pub offset: (i16, i16),
    /// Ctrl+F5: moved by the keys; the place it had.
    pub keys: Option<(i16, i16)>,
    /// Dragged by the mouse: the cell it was taken at and the place it had.
    drag: Option<((u16, u16), (i16, i16))>,
    /// A plugin's dialog's own column and row (FAR's `X1`, `Y1`; none: centred that way): the next
    /// draw turns them into `offset`.
    pub at: Option<(Option<u16>, Option<u16>)>,
}

impl App {
    /// Ctrl+F5, and the keys while the dialog in front is moved; true when the key was for that.
    pub(super) fn dialog_move_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let place = &mut self.dialog_place;
        let Some(old) = place.keys else {
            if key.code == KeyCode::F(5) && ctrl && place.drag.is_none() {
                place.keys = Some(place.offset);
                return true;
            }
            return false;
        };
        let step = match key.code {
            KeyCode::Left => (if ctrl { -10 } else { -1 }, 0),
            KeyCode::Right => (if ctrl { 10 } else { 1 }, 0),
            KeyCode::Up => (0, if ctrl { -5 } else { -1 }),
            KeyCode::Down => (0, if ctrl { 5 } else { 1 }),
            KeyCode::Home => (-TO_THE_EDGE, 0),
            KeyCode::End => (TO_THE_EDGE, 0),
            KeyCode::PageUp => (0, -TO_THE_EDGE),
            KeyCode::PageDown => (0, TO_THE_EDGE),
            KeyCode::Enter => {
                place.keys = None;
                (0, 0)
            }
            KeyCode::F(5) if ctrl => {
                place.keys = None;
                (0, 0)
            }
            KeyCode::Esc => {
                *place = DialogPlace {
                    offset: old,
                    ..DialogPlace::default()
                };
                (0, 0)
            }
            // FAR takes every key while the dialog moves.
            _ => (0, 0),
        };
        place.offset = (
            place.offset.0.saturating_add(step.0),
            place.offset.1.saturating_add(step.1),
        );
        true
    }

    /// A record while the mouse drags the dialog in front; false when it does not.
    pub(super) fn dialog_drag_mouse(&mut self, mouse: &Mouse) -> bool {
        let place = &mut self.dialog_place;
        let Some(((column, row), old)) = place.drag else {
            return false;
        };
        if mouse.buttons.right {
            *place = DialogPlace {
                offset: old,
                ..DialogPlace::default()
            };
        } else if !mouse.buttons.left {
            place.drag = None;
        } else {
            let shift =
                |now: u16, then: u16| i16::try_from(i32::from(now) - i32::from(then)).unwrap_or(0);
            place.offset = (
                old.0.saturating_add(shift(mouse.column, column)),
                old.1.saturating_add(shift(mouse.row, row)),
            );
        }
        // The next draw tells where it is now.
        self.mouse.dialog = None;
        true
    }

    /// The left button pressed on a place of the dialog that is no item: it is dragged from there.
    pub(super) fn start_dialog_drag(&mut self, mouse: &Mouse) {
        if self.dialog_place.keys.is_none() {
            self.dialog_place.drag = Some(((mouse.column, mouse.row), self.dialog_place.offset));
        }
    }

    /// A dialog comes up or goes: the next one stands in the middle.
    pub(super) fn forget_dialog_place(&mut self) {
        self.dialog_place = DialogPlace::default();
    }
}

#[cfg(test)]
#[path = "dialog_move_tests.rs"]
mod tests;
