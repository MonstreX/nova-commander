//! Keys typed while the shell owns the screen (terminal mode, or a command running) are sent to it as
//! the VT sequences a terminal would send; ConPTY turns them into console input for the program.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The bytes for `key`, or `None` for keys a terminal does not send (modifier-only, media keys).
pub fn encode(key: KeyEvent) -> Option<Vec<u8>> {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    // On Windows AltGr arrives as Ctrl+Alt: the character itself is what was typed.
    if let KeyCode::Char(ch) = key.code
        && ctrl
        && alt
    {
        return Some(ch.to_string().into_bytes());
    }
    let bytes = match key.code {
        KeyCode::Char(ch) if ctrl => control_char(ch)?,
        KeyCode::Char(ch) => ch.to_string().into_bytes(),
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => b"\x7f".to_vec(),
        KeyCode::Esc => b"\x1b".to_vec(),
        code => return special_key(code, modifier_parameter(key.modifiers)),
    };
    Some(if alt {
        [b"\x1b".as_slice(), &bytes].concat()
    } else {
        bytes
    })
}

/// Ctrl+letter and the classic control punctuation (Ctrl+[ = Esc, Ctrl+\ , Ctrl+] , Ctrl+^ , Ctrl+_).
fn control_char(ch: char) -> Option<Vec<u8>> {
    let byte = match ch.to_ascii_lowercase() {
        letter @ 'a'..='z' => letter as u8 - b'a' + 1,
        '@' | ' ' | '2' => 0,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '7' | '/' => 0x1f,
        '8' | '?' => 0x7f,
        _ => return None,
    };
    Some(vec![byte])
}

/// xterm's modifier parameter: 1 + Shift(1) + Alt(2) + Ctrl(4); `None` without modifiers.
fn modifier_parameter(modifiers: KeyModifiers) -> Option<u8> {
    let value = u8::from(modifiers.contains(KeyModifiers::SHIFT))
        + 2 * u8::from(modifiers.contains(KeyModifiers::ALT))
        + 4 * u8::from(modifiers.contains(KeyModifiers::CONTROL));
    (value != 0).then_some(value + 1)
}

fn special_key(code: KeyCode, modifier: Option<u8>) -> Option<Vec<u8>> {
    let text = match code {
        KeyCode::Up => cursor_key('A', modifier),
        KeyCode::Down => cursor_key('B', modifier),
        KeyCode::Right => cursor_key('C', modifier),
        KeyCode::Left => cursor_key('D', modifier),
        KeyCode::Home => cursor_key('H', modifier),
        KeyCode::End => cursor_key('F', modifier),
        KeyCode::Insert => tilde_key(2, modifier),
        KeyCode::Delete => tilde_key(3, modifier),
        KeyCode::PageUp => tilde_key(5, modifier),
        KeyCode::PageDown => tilde_key(6, modifier),
        KeyCode::F(number) => function_key(number, modifier)?,
        _ => return None,
    };
    Some(text.into_bytes())
}

fn cursor_key(letter: char, modifier: Option<u8>) -> String {
    match modifier {
        Some(modifier) => format!("\x1b[1;{modifier}{letter}"),
        None => format!("\x1b[{letter}"),
    }
}

fn tilde_key(number: u8, modifier: Option<u8>) -> String {
    match modifier {
        Some(modifier) => format!("\x1b[{number};{modifier}~"),
        None => format!("\x1b[{number}~"),
    }
}

fn function_key(number: u8, modifier: Option<u8>) -> Option<String> {
    let ss3 = |letter: char| match modifier {
        Some(modifier) => format!("\x1b[1;{modifier}{letter}"),
        None => format!("\x1bO{letter}"),
    };
    Some(match number {
        1 => ss3('P'),
        2 => ss3('Q'),
        3 => ss3('R'),
        4 => ss3('S'),
        5 => tilde_key(15, modifier),
        6 => tilde_key(17, modifier),
        7 => tilde_key(18, modifier),
        8 => tilde_key(19, modifier),
        9 => tilde_key(20, modifier),
        10 => tilde_key(21, modifier),
        11 => tilde_key(23, modifier),
        12 => tilde_key(24, modifier),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<Vec<u8>> {
        encode(KeyEvent::new(code, modifiers))
    }

    #[test]
    fn text_and_editing_keys() {
        assert_eq!(
            key(KeyCode::Char('ж'), KeyModifiers::NONE),
            Some("ж".as_bytes().to_vec())
        );
        assert_eq!(
            key(KeyCode::Char('A'), KeyModifiers::SHIFT),
            Some(b"A".to_vec())
        );
        assert_eq!(
            key(KeyCode::Enter, KeyModifiers::NONE),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            key(KeyCode::Backspace, KeyModifiers::NONE),
            Some(b"\x7f".to_vec())
        );
        assert_eq!(
            key(KeyCode::Esc, KeyModifiers::NONE),
            Some(b"\x1b".to_vec())
        );
    }

    #[test]
    fn altgr_characters_are_sent_as_typed() {
        let altgr = KeyModifiers::CONTROL | KeyModifiers::ALT;
        assert_eq!(key(KeyCode::Char('@'), altgr), Some(b"@".to_vec()));
        assert_eq!(key(KeyCode::Char('\\'), altgr), Some(b"\\".to_vec()));
        assert_eq!(
            key(KeyCode::Char('€'), altgr),
            Some("€".as_bytes().to_vec())
        );
    }

    #[test]
    fn control_and_alt_combinations() {
        assert_eq!(
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Some(vec![3])
        );
        assert_eq!(
            key(KeyCode::Char('Z'), KeyModifiers::CONTROL),
            Some(vec![26])
        );
        assert_eq!(
            key(KeyCode::Char('['), KeyModifiers::CONTROL),
            Some(vec![0x1b])
        );
        assert_eq!(
            key(KeyCode::Char('x'), KeyModifiers::ALT),
            Some(b"\x1bx".to_vec())
        );
        assert_eq!(key(KeyCode::Char('1'), KeyModifiers::CONTROL), None);
    }

    #[test]
    fn cursor_and_function_keys_use_xterm_sequences() {
        assert_eq!(
            key(KeyCode::Up, KeyModifiers::NONE),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            key(KeyCode::Left, KeyModifiers::CONTROL),
            Some(b"\x1b[1;5D".to_vec())
        );
        assert_eq!(
            key(KeyCode::PageDown, KeyModifiers::NONE),
            Some(b"\x1b[6~".to_vec())
        );
        assert_eq!(
            key(KeyCode::Delete, KeyModifiers::SHIFT),
            Some(b"\x1b[3;2~".to_vec())
        );
        assert_eq!(
            key(KeyCode::F(1), KeyModifiers::NONE),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(
            key(KeyCode::F(10), KeyModifiers::NONE),
            Some(b"\x1b[21~".to_vec())
        );
        assert_eq!(
            key(KeyCode::F(5), KeyModifiers::SHIFT),
            Some(b"\x1b[15;2~".to_vec())
        );
        assert_eq!(key(KeyCode::F(13), KeyModifiers::NONE), None);
    }
}
