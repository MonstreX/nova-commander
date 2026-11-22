//! The mouse as a terminal reports it to the program inside (xterm's protocols), when that program
//! asked for it: FAR gives a program its console, here the program's console is a pseudo console and
//! it learns of the mouse through these sequences.

use vt100::{MouseProtocolEncoding, MouseProtocolMode, Screen};

/// What xterm's first number says of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEvent {
    Press(MouseButton),
    Release(MouseButton),
    /// The mouse moved with this button held.
    Drag(MouseButton),
    /// The mouse moved with no button held.
    Move,
    WheelUp,
    WheelDown,
    WheelLeft,
    WheelRight,
}

/// An event at a cell of the program's screen, from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseReport {
    pub column: u16,
    pub row: u16,
    pub event: MouseEvent,
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// The largest value the byte encoding can carry after its offset of 32.
const LAST_BYTE: u32 = 255;
/// The largest value the UTF-8 encoding can carry.
const LAST_UTF8: u32 = 2047;

fn button_code(button: MouseButton) -> u32 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

/// Whether a program in this mode hears of the event.
fn wanted(mode: MouseProtocolMode, event: MouseEvent) -> bool {
    match event {
        MouseEvent::Press(_)
        | MouseEvent::WheelUp
        | MouseEvent::WheelDown
        | MouseEvent::WheelLeft
        | MouseEvent::WheelRight => mode != MouseProtocolMode::None,
        MouseEvent::Release(_) => {
            !matches!(mode, MouseProtocolMode::None | MouseProtocolMode::Press)
        }
        MouseEvent::Drag(_) => matches!(
            mode,
            MouseProtocolMode::ButtonMotion | MouseProtocolMode::AnyMotion
        ),
        MouseEvent::Move => mode == MouseProtocolMode::AnyMotion,
    }
}

/// The first number of the report; `sgr`: a release keeps its button there.
fn event_code(event: MouseEvent, sgr: bool) -> u32 {
    match event {
        MouseEvent::Press(button) => button_code(button),
        MouseEvent::Release(button) if sgr => button_code(button),
        MouseEvent::Release(_) => 3,
        MouseEvent::Drag(button) => button_code(button) + 32,
        MouseEvent::Move => 3 + 32,
        MouseEvent::WheelUp => 64,
        MouseEvent::WheelDown => 65,
        MouseEvent::WheelLeft => 66,
        MouseEvent::WheelRight => 67,
    }
}

/// The bytes for `report`; `None` when the program did not ask for it. A place its encoding cannot
/// say is said as the last one it can, as xterm does.
pub fn encode(screen: &Screen, report: MouseReport) -> Option<Vec<u8>> {
    let mode = screen.mouse_protocol_mode();
    if !wanted(mode, report.event) {
        return None;
    }
    let encoding = screen.mouse_protocol_encoding();
    let mut code = event_code(report.event, encoding == MouseProtocolEncoding::Sgr);
    // The first mode reports no modifiers.
    if mode != MouseProtocolMode::Press {
        code +=
            u32::from(report.shift) * 4 + u32::from(report.alt) * 8 + u32::from(report.ctrl) * 16;
    }
    let column = u32::from(report.column) + 1;
    let row = u32::from(report.row) + 1;
    match encoding {
        MouseProtocolEncoding::Sgr => {
            let end = if matches!(report.event, MouseEvent::Release(_)) {
                'm'
            } else {
                'M'
            };
            Some(format!("\x1b[<{code};{column};{row}{end}").into_bytes())
        }
        MouseProtocolEncoding::Default => {
            let mut bytes = b"\x1b[M".to_vec();
            for value in [code, column, row] {
                bytes.push(u8::try_from((value + 32).min(LAST_BYTE)).ok()?);
            }
            Some(bytes)
        }
        MouseProtocolEncoding::Utf8 => {
            let mut text = String::from("\x1b[M");
            for value in [code, column, row] {
                text.push(char::from_u32((value + 32).min(LAST_UTF8))?);
            }
            Some(text.into_bytes())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use super::*;

    fn screen_after(modes: &str) -> vt100::Parser {
        let (rows, cols) = (NonZeroU16::new(24).unwrap(), NonZeroU16::new(80).unwrap());
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(modes.as_bytes());
        parser
    }

    fn report(event: MouseEvent) -> MouseReport {
        MouseReport {
            column: 9,
            row: 4,
            event,
            shift: false,
            alt: false,
            ctrl: false,
        }
    }

    fn sent(modes: &str, report: MouseReport) -> Option<String> {
        encode(screen_after(modes).screen(), report).map(|bytes| String::from_utf8(bytes).unwrap())
    }

    const LEFT_DOWN: MouseEvent = MouseEvent::Press(MouseButton::Left);

    #[test]
    fn a_program_that_did_not_ask_hears_nothing() {
        assert_eq!(sent("", report(LEFT_DOWN)), None);
        assert_eq!(sent("", report(MouseEvent::WheelUp)), None);
    }

    #[test]
    fn sgr_says_the_button_of_a_release_and_ends_it_with_m() {
        let modes = "\x1b[?1000h\x1b[?1006h";
        assert_eq!(sent(modes, report(LEFT_DOWN)).unwrap(), "\x1b[<0;10;5M");
        let up = MouseEvent::Release(MouseButton::Right);
        assert_eq!(sent(modes, report(up)).unwrap(), "\x1b[<2;10;5m");
    }

    #[test]
    fn the_byte_encoding_says_three_for_any_release_and_stops_at_its_last_column() {
        let modes = "\x1b[?1000h";
        assert_eq!(sent(modes, report(LEFT_DOWN)).unwrap(), "\x1b[M *%");
        let up = report(MouseEvent::Release(MouseButton::Left));
        assert_eq!(sent(modes, up).unwrap(), "\x1b[M#*%");
        let far = MouseReport {
            column: 230,
            ..report(LEFT_DOWN)
        };
        assert_eq!(
            encode(screen_after(modes).screen(), far).unwrap(),
            [0x1b, b'[', b'M', 32, 255, 37],
            "a column one byte cannot say is its last"
        );
    }

    #[test]
    fn utf8_says_a_column_past_a_byte() {
        let far = MouseReport {
            column: 299,
            ..report(LEFT_DOWN)
        };
        let expected = format!("\x1b[M {}%", char::from_u32(332).unwrap());
        assert_eq!(sent("\x1b[?1000h\x1b[?1005h", far).unwrap(), expected);
    }

    #[test]
    fn each_mode_reports_what_it_asked_for() {
        let drag = report(MouseEvent::Drag(MouseButton::Left));
        let moved = report(MouseEvent::Move);
        let up = report(MouseEvent::Release(MouseButton::Left));
        let press_only = "\x1b[?9h\x1b[?1006h";
        assert!(sent(press_only, report(LEFT_DOWN)).is_some());
        assert_eq!(sent(press_only, up), None);
        let buttons = "\x1b[?1000h\x1b[?1006h";
        assert!(sent(buttons, up).is_some());
        assert_eq!(sent(buttons, drag), None);
        let motion = "\x1b[?1002h\x1b[?1006h";
        assert_eq!(sent(motion, drag).unwrap(), "\x1b[<32;10;5M");
        assert_eq!(sent(motion, moved), None);
        let any = "\x1b[?1003h\x1b[?1006h";
        assert_eq!(sent(any, moved).unwrap(), "\x1b[<35;10;5M");
    }

    #[test]
    fn the_wheel_and_the_modifiers_have_their_numbers() {
        let modes = "\x1b[?1000h\x1b[?1006h";
        assert_eq!(
            sent(modes, report(MouseEvent::WheelUp)).unwrap(),
            "\x1b[<64;10;5M"
        );
        assert_eq!(
            sent(modes, report(MouseEvent::WheelRight)).unwrap(),
            "\x1b[<67;10;5M"
        );
        let held = MouseReport {
            shift: true,
            alt: true,
            ctrl: true,
            ..report(LEFT_DOWN)
        };
        assert_eq!(sent(modes, held).unwrap(), "\x1b[<28;10;5M");
        // X10 has no modifiers.
        assert_eq!(sent("\x1b[?9h\x1b[?1006h", held).unwrap(), "\x1b[<0;10;5M");
    }
}
