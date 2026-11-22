//! The console's records as the program's input (FAR's `keyboard.cpp`): the keys in the form the rest
//! of `app` knows, what the records tell beside them (the numeric keypad, AltGr, the modifiers held),
//! and crossterm where `platform` does not read the console.

use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
};
use ratatui::crossterm::terminal;

use crate::platform::input::{
    self, ConsoleRecord, ControlKeys, KeyRecord, Modifiers, MouseButtons, MouseKind, MouseRecord,
    key as vk, latin_for_virtual_key,
};

/// What came from the console.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Key(KeyEvent),
    /// The modifiers held changed; comes before the key of the same record.
    Held(Modifiers),
    Resize(u16, u16),
    Focus(bool),
    Mouse(Mouse),
}

/// The mouse at a cell of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mouse {
    pub column: u16,
    pub row: u16,
    /// The buttons held now.
    pub buttons: MouseButtons,
    /// The buttons that went down with this record.
    pub pressed: MouseButtons,
    pub action: MouseAction,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    /// A button went down or up.
    Button,
    DoubleClick,
    Moved,
    /// Notches of the wheel, away from the user positive (FAR's `KEY_MSWHEEL_UP`).
    Wheel(i32),
    /// Notches of the tilt wheel, to the right positive.
    HorizontalWheel(i32),
}

/// Reads the console.
#[derive(Default)]
pub struct Reader {
    translator: Translator,
    queue: VecDeque<Input>,
}

impl Reader {
    /// The next input; `None` when nothing came within `wait`.
    pub fn next(&mut self, wait: Duration) -> io::Result<Option<Input>> {
        if input::READS_CONSOLE {
            self.next_of_console(wait)
        } else {
            next_of_crossterm(wait)
        }
    }

    fn next_of_console(&mut self, mut wait: Duration) -> io::Result<Option<Input>> {
        loop {
            if let Some(input) = self.queue.pop_front() {
                return Ok(Some(input));
            }
            let Some(record) = input::read_console(wait)? else {
                return Ok(None);
            };
            wait = Duration::ZERO;
            if record == ConsoleRecord::Resized {
                // FAR asks too: the record gives the buffer's size, not the window's.
                let (width, height) = terminal::size()?;
                self.queue.push_back(Input::Resize(width, height));
            }
            self.queue.extend(self.translator.translate(&record));
        }
    }
}

fn next_of_crossterm(wait: Duration) -> io::Result<Option<Input>> {
    if !event::poll(wait)? {
        return Ok(None);
    }
    Ok(match event::read()? {
        Event::Key(key) => Some(Input::Key(key)),
        Event::Resize(width, height) => Some(Input::Resize(width, height)),
        Event::FocusGained => Some(Input::Focus(true)),
        Event::FocusLost => Some(Input::Focus(false)),
        _ => None,
    })
}

/// Turns records into input; what it keeps between them is the modifiers and half a surrogate pair.
#[derive(Default)]
pub struct Translator {
    held: Modifiers,
    high_surrogate: Option<u16>,
    buttons: MouseButtons,
    wheel: WheelTicks,
    horizontal_wheel: WheelTicks,
}

impl Translator {
    pub fn translate(&mut self, record: &ConsoleRecord) -> Vec<Input> {
        let mut inputs = Vec::new();
        match record {
            ConsoleRecord::Key(key) => {
                self.note_held(key.control.modifiers(), &mut inputs);
                inputs.extend(self.key(key).map(Input::Key));
            }
            ConsoleRecord::Mouse(mouse) => {
                self.note_held(mouse.control.modifiers(), &mut inputs);
                inputs.extend(self.mouse(mouse).map(Input::Mouse));
            }
            ConsoleRecord::Focus(focused) => {
                // The keys held here are released in the other window.
                if !focused {
                    self.note_held(Modifiers::default(), &mut inputs);
                }
                inputs.push(Input::Focus(*focused));
            }
            ConsoleRecord::Resized | ConsoleRecord::Other => {}
        }
        inputs
    }

    fn mouse(&mut self, record: &MouseRecord) -> Option<Mouse> {
        let pressed = record.buttons.pressed_since(self.buttons);
        self.buttons = record.buttons;
        let action = match record.kind {
            MouseKind::Button => MouseAction::Button,
            MouseKind::DoubleClick => MouseAction::DoubleClick,
            MouseKind::Moved => MouseAction::Moved,
            MouseKind::Wheel(delta) => MouseAction::Wheel(self.wheel.notches(delta)?),
            MouseKind::HorizontalWheel(delta) => {
                MouseAction::HorizontalWheel(self.horizontal_wheel.notches(delta)?)
            }
        };
        Some(Mouse {
            column: record.column,
            row: record.row,
            buttons: record.buttons,
            pressed,
            action,
            modifiers: key_modifiers(record.control),
        })
    }

    fn note_held(&mut self, held: Modifiers, inputs: &mut Vec<Input>) {
        if self.held != held {
            self.held = held;
            inputs.push(Input::Held(held));
        }
    }

    fn key(&mut self, record: &KeyRecord) -> Option<KeyEvent> {
        let modifiers = key_modifiers(record.control);
        // ConPTY types what the current layout lacks as an Alt code: the character comes on Alt's up.
        if record.virtual_key == vk::MENU && !record.down && record.unit != 0 {
            let ch = self.character(record.unit)?;
            return Some(KeyEvent::new(KeyCode::Char(ch), modifiers));
        }
        if composes_alt_code(record) {
            return None;
        }
        if let Some(code) = named_key(record) {
            self.high_surrogate = None;
            let kind = if record.down {
                KeyEventKind::Press
            } else {
                KeyEventKind::Release
            };
            return Some(KeyEvent::new_with_kind(code, modifiers, kind));
        }
        // A character's release says nothing, and with no press before it it is not typing either.
        if record.down {
            self.typed(record, modifiers)
        } else {
            None
        }
    }

    fn typed(&mut self, record: &KeyRecord, modifiers: KeyModifiers) -> Option<KeyEvent> {
        let keypad = keypad_character(record.virtual_key);
        let ch = if record.unit >= FIRST_PRINTABLE {
            self.character(record.unit)?
        } else if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            // Control codes and nothing at all: the key says what it is, whatever the layout.
            keypad.or_else(|| latin_for_virtual_key(record.virtual_key))?
        } else {
            return None;
        };
        // FAR's rule for AltGr: Ctrl+Alt is what the system sends for it, the right Alt is the key.
        let altgr =
            record.control.left_ctrl && record.control.right_alt && record.unit >= FIRST_PRINTABLE;
        Some(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: if altgr {
                modifiers - (KeyModifiers::CONTROL | KeyModifiers::ALT)
            } else {
                modifiers
            },
            kind: KeyEventKind::Press,
            state: typed_state(record, keypad.is_some()),
        })
    }

    /// The character of a UTF-16 unit; the first half of a pair is kept for the second.
    fn character(&mut self, unit: u16) -> Option<char> {
        match unit {
            0xD800..=0xDBFF => {
                self.high_surrogate = Some(unit);
                None
            }
            0xDC00..=0xDFFF => {
                let high = self.high_surrogate.take()?;
                char::decode_utf16([high, unit]).next()?.ok()
            }
            _ => {
                // A first half with no second is dropped, as crossterm does.
                self.high_surrogate = None;
                char::from_u32(u32::from(unit))
            }
        }
    }
}

/// The wheel's distance not yet a notch (FAR's `StoredTicks`).
#[derive(Default)]
struct WheelTicks(i32);

/// A notch of the wheel (`WHEEL_DELTA`, FAR's `MsWheelThreshold` 0).
const WHEEL_NOTCH: i32 = 120;

impl WheelTicks {
    /// The notches the wheel has gone with `delta` added; none until a whole one.
    fn notches(&mut self, delta: i16) -> Option<i32> {
        let delta = i32::from(delta);
        // A turn the other way forgets what was stored.
        if (delta > 0) == (self.0 > 0) {
            self.0 += delta;
        } else {
            self.0 = delta;
        }
        if self.0.abs() < WHEEL_NOTCH {
            return None;
        }
        let notches = self.0 / WHEEL_NOTCH;
        self.0 %= WHEEL_NOTCH;
        Some(notches)
    }
}

/// Units below this are control codes.
const FIRST_PRINTABLE: u16 = 0x20;

/// A digit of the main row typed with the right Ctrl alone (FAR's `KEY_RCTRL0`..`KEY_RCTRL9`: folder
/// shortcuts; Ctrl+0..9 with the left one are the panel's modes). crossterm has no such state; this bit
/// is one it never sets.
pub const RIGHT_CTRL: KeyEventState = KeyEventState::from_bits_retain(0b1000_0000);

/// Whether `key` is RCtrl+0..9; its digit.
pub fn right_ctrl_digit(key: &KeyEvent) -> Option<usize> {
    let KeyCode::Char(ch @ '0'..='9') = key.code else {
        return None;
    };
    let digit = ch.to_digit(10)?;
    (key.modifiers == KeyModifiers::CONTROL && key.state.contains(RIGHT_CTRL))
        .then(|| usize::try_from(digit).ok())
        .flatten()
}

/// Ctrl+Shift+0..9 with either Ctrl (FAR's `KEY_CTRLSHIFT0`, `KEY_RCTRLSHIFT0`), or ours Alt+Shift+0..9
/// (Windows Terminal opens its tabs with Ctrl+Shift+1..9); its digit. The panels' folder shortcuts
/// and the viewer's and editor's bookmarks are set by it.
pub fn ctrl_shift_digit(key: &KeyEvent) -> Option<usize> {
    if let Some(digit) = alt_shift_digit(key) {
        return Some(digit);
    }
    let KeyCode::Char(ch @ '0'..='9') = key.code else {
        return None;
    };
    let keypad = key.state.contains(KeyEventState::KEYPAD);
    let digit = usize::try_from(ch.to_digit(10)?).ok()?;
    (key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT && !keypad).then_some(digit)
}

/// Ctrl+0..9 with the left Ctrl (FAR's `KEY_CTRL0`..`KEY_CTRL9`: the panel's modes, the viewer's
/// and editor's bookmarks gone to); its digit.
pub fn left_ctrl_digit(key: &KeyEvent) -> Option<usize> {
    let KeyCode::Char(ch @ '0'..='9') = key.code else {
        return None;
    };
    let plain = !key.state.contains(KeyEventState::KEYPAD) && !key.state.contains(RIGHT_CTRL);
    let digit = usize::try_from(ch.to_digit(10)?).ok()?;
    (key.modifiers == KeyModifiers::CONTROL && plain).then_some(digit)
}

/// FAR's bookmark keys of the viewer and the editor (`Viewer::ProcessKey`, `Editor::ProcessKey`):
/// Ctrl+0..9 goes to one (`false`), Ctrl+Shift+0..9 or RCtrl+0..9 sets it (`true`; ours Alt+Shift+0..9
/// too); the digit.
pub fn bookmark_key(key: &KeyEvent) -> Option<(usize, bool)> {
    if let Some(digit) = ctrl_shift_digit(key).or_else(|| right_ctrl_digit(key)) {
        return Some((digit, true));
    }
    left_ctrl_digit(key).map(|digit| (digit, false))
}

/// Alt+Shift with a digit key of the main row: the digit plus one in these bits, whatever character
/// the layout types there (`!`, `"`, `№`; the character itself is what the key stays, so a shell still
/// gets it). Ours Alt+Shift+0..9 adds a folder shortcut. Bits crossterm never sets.
const DIGIT_BITS: u8 = 0b0111_1000;
const DIGIT_SHIFT: u8 = 3;

/// Whether `key` is Alt+Shift on a digit key of the main row; its digit.
pub fn alt_shift_digit(key: &KeyEvent) -> Option<usize> {
    if key.modifiers != KeyModifiers::ALT | KeyModifiers::SHIFT {
        return None;
    }
    let value = (key.state.bits() & DIGIT_BITS) >> DIGIT_SHIFT;
    usize::from(value).checked_sub(1)
}

/// The main row's digit key (`'0'`..`'9'`): its digit.
fn digit_key(virtual_key: u16) -> Option<u8> {
    (0x30..=0x39)
        .contains(&virtual_key)
        .then(|| u8::try_from(virtual_key - 0x30).ok())
        .flatten()
}

/// What a typed key's record tells beside its character: the keypad, the right Ctrl on a digit, the
/// digit under Alt+Shift.
fn typed_state(record: &KeyRecord, keypad: bool) -> KeyEventState {
    let control = record.control;
    let held = control.modifiers();
    match digit_key(record.virtual_key) {
        _ if keypad => KeyEventState::KEYPAD,
        Some(_) if control.right_ctrl && !control.left_ctrl => RIGHT_CTRL,
        Some(digit) if held.alt && held.shift && !held.ctrl => {
            KeyEventState::from_bits_retain((digit + 1) << DIGIT_SHIFT)
        }
        _ => KeyEventState::NONE,
    }
}

fn key_modifiers(control: ControlKeys) -> KeyModifiers {
    let held = control.modifiers();
    let mut modifiers = KeyModifiers::NONE;
    modifiers.set(KeyModifiers::SHIFT, held.shift);
    modifiers.set(KeyModifiers::CONTROL, held.ctrl);
    modifiers.set(KeyModifiers::ALT, held.alt);
    modifiers
}

/// Digits of the numeric keypad under Alt alone are an Alt code being typed.
fn composes_alt_code(record: &KeyRecord) -> bool {
    let held = record.control.modifiers();
    (vk::NUMPAD0..=vk::NUMPAD9).contains(&record.virtual_key)
        && held.alt
        && !held.ctrl
        && !held.shift
}

/// The keys that are no characters.
fn named_key(record: &KeyRecord) -> Option<KeyCode> {
    Some(match record.virtual_key {
        vk::BACK => KeyCode::Backspace,
        vk::ESCAPE => KeyCode::Esc,
        vk::RETURN => KeyCode::Enter,
        // FAR's `KEY_NUMPAD5`.
        vk::CLEAR => KeyCode::KeypadBegin,
        vk::LEFT => KeyCode::Left,
        vk::UP => KeyCode::Up,
        vk::RIGHT => KeyCode::Right,
        vk::DOWN => KeyCode::Down,
        vk::PRIOR => KeyCode::PageUp,
        vk::NEXT => KeyCode::PageDown,
        vk::HOME => KeyCode::Home,
        vk::END => KeyCode::End,
        vk::DELETE => KeyCode::Delete,
        vk::INSERT => KeyCode::Insert,
        vk::TAB if record.control.shift => KeyCode::BackTab,
        vk::TAB => KeyCode::Tab,
        vk::APPS => KeyCode::Menu,
        code @ vk::F1..=vk::F24 => KeyCode::F(u8::try_from(code - vk::F1 + 1).ok()?),
        _ => return None,
    })
}

/// The character of a key of the numeric keypad.
fn keypad_character(key: u16) -> Option<char> {
    match key {
        vk::NUMPAD0..=vk::NUMPAD9 => char::from_digit(u32::from(key - vk::NUMPAD0), 10),
        vk::MULTIPLY => Some('*'),
        vk::ADD => Some('+'),
        vk::SUBTRACT => Some('-'),
        vk::DECIMAL => Some('.'),
        vk::DIVIDE => Some('/'),
        _ => None,
    }
}

#[cfg(test)]
#[path = "console_input_tests.rs"]
mod console_input_tests;
