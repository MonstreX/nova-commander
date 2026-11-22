//! Keyboard input: the console's records read here, as FAR reads them (crossterm does not tell the held
//! modifiers, the numeric keypad or AltGr), and what the keyboard layouts say about a character (FAR
//! binds shortcuts to keys, not characters).

pub use super::os::{
    READS_CONSOLE, begin_console_input, double_click_time, end_console_input, is_altgr_text,
    key_for_char, keyboard_repeat, read_console, set_console_mouse, wheel_scroll_chars,
    wheel_scroll_lines,
};

/// Modifier keys held down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// The modifier keys one by one, as a console record has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControlKeys {
    pub shift: bool,
    pub left_ctrl: bool,
    pub right_ctrl: bool,
    pub left_alt: bool,
    pub right_alt: bool,
}

impl ControlKeys {
    pub fn modifiers(self) -> Modifiers {
        Modifiers {
            shift: self.shift,
            ctrl: self.left_ctrl || self.right_ctrl,
            alt: self.left_alt || self.right_alt,
        }
    }
}

/// A key going down or up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyRecord {
    pub down: bool,
    /// The key by its virtual-key code ([`key`]).
    pub virtual_key: u16,
    /// The UTF-16 unit it types in the current layout, 0 for none.
    pub unit: u16,
    pub control: ControlKeys,
}

/// The mouse buttons held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

impl MouseButtons {
    pub fn any(self) -> bool {
        self.left || self.right || self.middle
    }

    /// The buttons held here and not in `before`.
    pub fn pressed_since(self, before: Self) -> Self {
        Self {
            left: self.left && !before.left,
            right: self.right && !before.right,
            middle: self.middle && !before.middle,
        }
    }
}

/// What a mouse record tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    /// A button went down or up.
    Button,
    DoubleClick,
    Moved,
    /// The wheel by this much, 120 a notch; away from the user is positive.
    Wheel(i16),
    /// The tilt wheel; to the right is positive.
    HorizontalWheel(i16),
}

/// The mouse at a cell of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseRecord {
    pub column: u16,
    pub row: u16,
    pub buttons: MouseButtons,
    pub control: ControlKeys,
    pub kind: MouseKind,
}

/// One record of the console's input, where [`READS_CONSOLE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleRecord {
    Key(KeyRecord),
    Mouse(MouseRecord),
    Resized,
    Focus(bool),
    Other,
}

/// Virtual-key codes.
pub mod key {
    pub const BACK: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    /// Num5 without NumLock.
    pub const CLEAR: u16 = 0x0C;
    pub const RETURN: u16 = 0x0D;
    pub const MENU: u16 = 0x12;
    pub const ESCAPE: u16 = 0x1B;
    pub const PRIOR: u16 = 0x21;
    pub const NEXT: u16 = 0x22;
    pub const END: u16 = 0x23;
    pub const HOME: u16 = 0x24;
    pub const LEFT: u16 = 0x25;
    pub const UP: u16 = 0x26;
    pub const RIGHT: u16 = 0x27;
    pub const DOWN: u16 = 0x28;
    pub const INSERT: u16 = 0x2D;
    pub const DELETE: u16 = 0x2E;
    /// The context menu key (FAR's `KEY_APPS`).
    pub const APPS: u16 = 0x5D;
    pub const NUMPAD0: u16 = 0x60;
    pub const NUMPAD9: u16 = 0x69;
    pub const MULTIPLY: u16 = 0x6A;
    pub const ADD: u16 = 0x6B;
    pub const SUBTRACT: u16 = 0x6D;
    pub const DECIMAL: u16 = 0x6E;
    pub const DIVIDE: u16 = 0x6F;
    pub const F1: u16 = 0x70;
    pub const F24: u16 = 0x87;
}

/// The character on a key of the US layout, by its Windows virtual-key code: letters (lower case),
/// digits and the punctuation keys. FAR names its shortcuts after these keys (Ctrl+[, Ctrl+\, ...).
pub fn latin_for_virtual_key(key: u16) -> Option<char> {
    let ascii = |code: u16| u8::try_from(code).ok().map(char::from);
    match key {
        0x30..=0x39 => ascii(key),
        0x41..=0x5A => ascii(key + 0x20),
        0xBA => Some(';'),
        0xBB => Some('='),
        0xBC => Some(','),
        0xBD => Some('-'),
        0xBE => Some('.'),
        0xBF => Some('/'),
        0xC0 => Some('`'),
        0xDB => Some('['),
        0xDC => Some('\\'),
        0xDD => Some(']'),
        0xDE => Some('\''),
        // FAR's `VK_SPACE`: Ctrl+Shift+Space types nothing on most layouts, the key is still Space.
        0x20 => Some(' '),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_keys_name_us_layout_characters() {
        assert_eq!(latin_for_virtual_key(0x46), Some('f'));
        assert_eq!(latin_for_virtual_key(0x37), Some('7'));
        assert_eq!(latin_for_virtual_key(0xDB), Some('['));
        assert_eq!(latin_for_virtual_key(0xDD), Some(']'));
        assert_eq!(latin_for_virtual_key(0x70), None, "F1 types nothing");
        assert_eq!(latin_for_virtual_key(0x20), Some(' '), "Ctrl+Shift+Space");
    }
}
