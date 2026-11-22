//! The mouse beside the panels as FAR has it: the top line (`FilePanels::ProcessMouse`: the left corner
//! is Ctrl+O, the rest opens the menu bar at the menu under the mouse), the key bar
//! (`KeyBar::ProcessMouse`: a key is pressed when the button is let go over it, the right button adds
//! Alt) and the command line (`Edit::ProcessMouse`: the cursor where clicked, a double click selects
//! the word, a click on the same place right after that selects the line).

use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::console_input::{Mouse, MouseAction};
use super::line_input::LineInput;
use super::menu::{MenuState, title_positions};
use super::{App, Overlay};
use crate::platform;

/// Where the renderer put the key bar: `ends` are where each key ends, from the bar's left.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyBarArea {
    pub x: u16,
    pub row: u16,
    pub width: u16,
    pub ends: Vec<u16>,
}

/// Where the renderer put the command line's text: `first` is the character shown at `x`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandLineArea {
    pub x: u16,
    pub row: u16,
    pub width: u16,
    pub first: usize,
}

impl KeyBarArea {
    /// The key under the mouse (FAR's `upper_bound` over the boundaries).
    fn key_at(&self, column: u16, row: u16) -> Option<usize> {
        if row != self.row || !(self.x..self.x + self.width).contains(&column) {
            return None;
        }
        let at = column - self.x;
        let index = self.ends.iter().filter(|&&end| end <= at).count();
        Some(index.min(self.ends.len().checked_sub(1)?))
    }
}

/// A key of the bar the mouse went down on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBarPress {
    index: usize,
    key: KeyEvent,
}

impl App {
    /// The top line: true when it took the record.
    pub(super) fn top_line_mouse(&mut self, mouse: &Mouse) -> bool {
        let pressed = mouse.pressed.left || mouse.pressed.right;
        if mouse.row != 0 || mouse.action != MouseAction::Button || !pressed {
            return false;
        }
        if mouse.column == 0 {
            self.dispatch_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        } else {
            // FAR's `HMenu::ProcessMouse`: the last title starting at or before the mouse.
            let menu = title_positions()
                .iter()
                .rposition(|&x| x <= mouse.column)
                .unwrap_or(0);
            self.overlay = Some(Overlay::Menu(self.fresh_menu(MenuState {
                menu,
                ..MenuState::default()
            })));
        }
        true
    }

    /// The left or right button goes down over a key of the bar: true when it did.
    pub(super) fn key_bar_press(&mut self, mouse: &Mouse) -> bool {
        let pressed = mouse.pressed.left || mouse.pressed.right;
        if !pressed || !matches!(mouse.action, MouseAction::Button | MouseAction::DoubleClick) {
            return false;
        }
        let Some(index) = self
            .mouse
            .key_bar
            .as_ref()
            .and_then(|bar| bar.key_at(mouse.column, mouse.row))
        else {
            return false;
        };
        let mut modifiers = mouse.modifiers;
        if mouse.buttons.right {
            modifiers |= KeyModifiers::ALT;
        }
        let number = u8::try_from(index + 1).unwrap_or(1);
        self.mouse.key_press = Some(KeyBarPress {
            index,
            key: KeyEvent::new(KeyCode::F(number), modifiers),
        });
        true
    }

    /// While a key of the bar is held every record is its; let go over the same key it is pressed.
    /// `None` when no key is held.
    pub(super) fn key_bar_release(&mut self, mouse: &Mouse) -> Option<bool> {
        let press = self.mouse.key_press?;
        if mouse.buttons.left || mouse.buttons.right {
            return Some(false);
        }
        self.mouse.key_press = None;
        let here = self
            .mouse
            .key_bar
            .as_ref()
            .and_then(|bar| bar.key_at(mouse.column, mouse.row));
        if here != Some(press.index) {
            return Some(false);
        }
        self.dispatch_key(press.key);
        Some(true)
    }

    /// The left or right button over the command line's text: true when it was there.
    pub(super) fn command_line_mouse(&mut self, mouse: &Mouse) -> bool {
        if !(mouse.buttons.left || mouse.buttons.right) {
            return false;
        }
        let Some(area) = &self.mouse.command_line else {
            return false;
        };
        if mouse.row != area.row || !(area.x..area.x + area.width).contains(&mouse.column) {
            return false;
        }
        let at = area.first + usize::from(mouse.column - area.x);
        self.command_line.place_cursor(at);
        if mouse.buttons.left {
            line_clicks(&mut self.command_line, &mut self.mouse.last_double, mouse);
        }
        true
    }
}

/// FAR's `Edit::ProcessMouse` with the left button: a double click selects the word, a click on the
/// same place within the double-click time after that the whole line. `last_double`: where and when
/// the last double click was.
pub(super) fn line_clicks(
    input: &mut LineInput,
    last_double: &mut Option<(Instant, (u16, u16))>,
    mouse: &Mouse,
) {
    let here = (mouse.column, mouse.row);
    let again = last_double.is_some_and(|(at, place)| {
        place == here && at.elapsed() <= platform::input::double_click_time()
    });
    if again && mouse.action == MouseAction::Button {
        input.select_all();
        *last_double = None;
        return;
    }
    if mouse.action == MouseAction::DoubleClick {
        input.select_word_at_cursor();
        *last_double = Some((Instant::now(), here));
    } else {
        *last_double = None;
    }
}
