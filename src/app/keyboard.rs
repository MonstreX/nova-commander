//! Shortcuts by key, not by character, as FAR binds them: with a Russian layout Ctrl+Х is Ctrl+[ and
//! Ctrl+А is Ctrl+F. A key typing a character reports the character of the current layout, so such
//! keys are turned back into the Latin key before anything handles them. AltGr arrives as Ctrl+Alt; when it types a
//! character (`[` on German or Polish keyboards) the press is text, not a shortcut.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Tells a typed character from a key release. ConPTY types a character the current layout lacks
/// (`&` or `$` with a Russian one, pasted into the terminal) as an Alt code: Alt down, numpad digits,
/// Alt up carrying the character. Where crossterm reads the console (not Windows, `console_input`) it
/// reports only that last record, as a release of the character with no press before it and no
/// modifiers; a key's own release follows its press. A release with Ctrl or Alt and no press is a
/// hotkey another program took (its key up still comes here), not text. On Windows the records are
/// read as FAR reads them: an Alt code comes as a press, character releases do not come.
#[derive(Default)]
pub struct Releases {
    /// Characters pressed and not yet released.
    pressed: Vec<char>,
    /// Other keys pressed and not yet released, with their last press: a press of one of them again
    /// soon after is the key held down.
    held: Vec<(KeyCode, Instant)>,
}

impl Releases {
    /// The key as an action: presses and auto-repeats as they are, an Alt code as the press of its
    /// character, nothing for a key release.
    pub fn action(&mut self, key: KeyEvent) -> Option<KeyEvent> {
        match (key.kind, key.code) {
            (KeyEventKind::Release, KeyCode::Char(_)) if self.pressed.is_empty() => {
                key.modifiers.is_empty().then_some(KeyEvent {
                    kind: KeyEventKind::Press,
                    ..key
                })
            }
            (KeyEventKind::Release, KeyCode::Char(ch)) => {
                // Shift may go up first: Shift+1 presses `!` and releases `1`.
                let index = self
                    .pressed
                    .iter()
                    .position(|&pressed| pressed == ch)
                    .or_else(|| {
                        let lower: Vec<char> = ch.to_lowercase().collect();
                        self.pressed
                            .iter()
                            .position(|pressed| pressed.to_lowercase().eq(lower.iter().copied()))
                    })
                    .unwrap_or(0);
                self.pressed.swap_remove(index);
                None
            }
            (KeyEventKind::Release, code) => {
                self.held.retain(|(held, _)| *held != code);
                None
            }
            (_, KeyCode::Char(ch)) => {
                if !self.pressed.contains(&ch) {
                    self.pressed.push(ch);
                }
                Some(key)
            }
            (_, code) if is_holdable(code) => {
                let now = Instant::now();
                let held = self.held.iter_mut().find(|(held, _)| *held == code);
                let repeat = held
                    .as_ref()
                    .is_some_and(|(_, last)| now.duration_since(*last) < REPEAT_GAP);
                match held {
                    Some((_, last)) => *last = now,
                    None => self.held.push((code, now)),
                }
                Some(if repeat {
                    KeyEvent {
                        kind: KeyEventKind::Repeat,
                        ..key
                    }
                } else {
                    key
                })
            }
            _ => Some(key),
        }
    }

    /// Another window got the keyboard: the keys held here are released there.
    pub fn forget_held(&mut self) {
        self.pressed.clear();
        self.held.clear();
    }
}

/// The longest gap between two presses of a key held down: the system repeats every 30 ms or so after
/// half a second at most. A console that reports no releases thus loses no second press made later.
const REPEAT_GAP: Duration = Duration::from_millis(600);

/// Keys whose auto-repeat matters (FAR's `IsRepeatedKey` before deleting).
fn is_holdable(code: KeyCode) -> bool {
    matches!(code, KeyCode::F(_) | KeyCode::Delete)
}

/// The key typing a character: its Latin character and whether Shift is needed.
pub type KeyLookup = fn(char) -> Option<(char, bool)>;

/// What the keyboard layouts say about a character (the system's, or a fake one in tests).
pub struct Layouts {
    /// The key typing a character: its Latin character and whether Shift is needed.
    pub key_for_char: KeyLookup,
    /// Whether a Ctrl+Alt press of the character is AltGr typing it.
    pub is_altgr_text: fn(char) -> bool,
}

pub const SYSTEM: Layouts = Layouts {
    key_for_char: crate::platform::input::key_for_char,
    is_altgr_text: crate::platform::input::is_altgr_text,
};

/// The key as FAR would see it: AltGr text loses its Ctrl+Alt, other layouts' shortcuts become Latin.
pub fn normalize(key: KeyEvent, layouts: &Layouts) -> KeyEvent {
    let ctrl_alt = KeyModifiers::CONTROL | KeyModifiers::ALT;
    match key.code {
        KeyCode::Char(ch) if key.modifiers.contains(ctrl_alt) && (layouts.is_altgr_text)(ch) => {
            KeyEvent {
                modifiers: key.modifiers - ctrl_alt,
                ..key
            }
        }
        _ => lower_ctrl_letter(latin_shortcut(key, layouts.key_for_char)),
    }
}

/// crossterm makes the letter of a Ctrl press upper case while Shift (or CapsLock) is on; FAR names
/// the key, so Ctrl+Shift+C is `c` with Ctrl+Shift and Ctrl+C with CapsLock is Ctrl+C.
fn lower_ctrl_letter(key: KeyEvent) -> KeyEvent {
    match key.code {
        KeyCode::Char(ch)
            if key.modifiers.contains(KeyModifiers::CONTROL) && ch.is_ascii_uppercase() =>
        {
            KeyEvent {
                code: KeyCode::Char(ch.to_ascii_lowercase()),
                ..key
            }
        }
        _ => key,
    }
}

/// Ctrl (+Alt, +Shift) with a character of another layout becomes the Latin key; Alt+Shift with the
/// character on the bracket keys becomes `[` or `]` (FAR's Alt+Shift+[ and Alt+Shift+]). Alt+letter
/// keeps its character: quick search types it.
fn latin_shortcut(key: KeyEvent, lookup: KeyLookup) -> KeyEvent {
    let KeyCode::Char(ch) = key.code else {
        return key;
    };
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt_shift = key
        .modifiers
        .contains(KeyModifiers::ALT | KeyModifiers::SHIFT);
    let latin = match lookup(ch) {
        // Shifted too: Ctrl+Shift+С is Ctrl+Shift+C, Ctrl+Shift+Ъ is Ctrl+Shift+].
        Some((latin, _)) if ctrl && !ch.is_ascii() => latin,
        Some((latin @ ('[' | ']'), true)) if alt_shift && !ctrl => latin,
        _ => return key,
    };
    KeyEvent {
        code: KeyCode::Char(latin),
        ..key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Russian layout: Х on the [ key, Ъ on ], А on F; the shifted ones are capitals.
    fn russian(ch: char) -> Option<(char, bool)> {
        match ch {
            'х' => Some(('[', false)),
            'Х' => Some(('[', true)),
            'ъ' => Some((']', false)),
            'Ъ' => Some((']', true)),
            'а' => Some(('f', false)),
            'А' => Some(('f', true)),
            '{' => Some(('[', true)),
            _ => None,
        }
    }

    fn key(ch: char, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), modifiers)
    }

    fn release(ch: char) -> KeyEvent {
        KeyEvent::new_with_kind(KeyCode::Char(ch), KeyModifiers::NONE, KeyEventKind::Release)
    }

    #[test]
    fn an_alt_code_is_typed_key_releases_are_not() {
        let mut releases = Releases::default();
        let a = key('a', KeyModifiers::NONE);
        assert_eq!(releases.action(a), Some(a));
        assert_eq!(releases.action(a), Some(a), "auto-repeat");
        assert_eq!(releases.action(release('a')), None);
        // `&` with a Russian layout: only the Alt up record, as a release.
        assert_eq!(
            releases.action(release('&')),
            Some(key('&', KeyModifiers::NONE))
        );
        let enter =
            KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Release);
        assert_eq!(releases.action(enter), None);
    }

    #[test]
    fn a_hotkey_release_with_nothing_held_is_not_a_shortcut() {
        let mut releases = Releases::default();
        // Another program's Ctrl+Alt+A (a global hotkey) or Ctrl+V: only the key up comes here.
        for modifiers in [
            KeyModifiers::CONTROL | KeyModifiers::ALT,
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
        ] {
            let up = KeyEvent::new_with_kind(KeyCode::Char('a'), modifiers, KeyEventKind::Release);
            assert_eq!(releases.action(up), None, "{modifiers:?}");
        }
        assert_eq!(
            releases.action(release('&')),
            Some(key('&', KeyModifiers::NONE))
        );
    }

    #[test]
    fn a_delete_key_pressed_again_before_its_release_is_a_repeat() {
        let mut releases = Releases::default();
        let f8 = KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE);
        let up = KeyEvent::new_with_kind(KeyCode::F(8), KeyModifiers::NONE, KeyEventKind::Release);
        assert_eq!(releases.action(f8).unwrap().kind, KeyEventKind::Press);
        assert_eq!(releases.action(f8).unwrap().kind, KeyEventKind::Repeat);
        assert_eq!(releases.action(up), None);
        assert_eq!(
            releases.action(f8).unwrap().kind,
            KeyEventKind::Press,
            "pressed again"
        );
        let other = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        releases.action(other);
        assert_eq!(
            releases.action(other).unwrap().kind,
            KeyEventKind::Press,
            "not tracked"
        );
    }

    #[test]
    fn keys_held_when_the_window_lost_focus_are_forgotten() {
        let mut releases = Releases::default();
        releases.action(key('x', KeyModifiers::NONE));
        // The release of x went to the other window.
        releases.forget_held();
        assert_eq!(
            releases.action(release('&')),
            Some(key('&', KeyModifiers::NONE))
        );
    }

    #[test]
    fn keys_released_out_of_order_are_not_typed() {
        let mut releases = Releases::default();
        // Rollover: b pressed before a is up.
        releases.action(key('a', KeyModifiers::NONE));
        releases.action(key('b', KeyModifiers::NONE));
        assert_eq!(releases.action(release('a')), None);
        assert_eq!(releases.action(release('b')), None);
        // Shift up before the key: `!` pressed, `1` released; `Ф` pressed, `ф` released.
        releases.action(key('!', KeyModifiers::SHIFT));
        releases.action(key('Ф', KeyModifiers::SHIFT));
        assert_eq!(releases.action(release('ф')), None);
        assert_eq!(releases.action(release('1')), None);
        assert_eq!(
            releases.action(release('$')),
            Some(key('$', KeyModifiers::NONE)),
            "nothing held: an Alt code again"
        );
    }

    #[test]
    fn ctrl_with_another_layout_becomes_the_latin_key() {
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(latin_shortcut(key('х', ctrl), russian), key('[', ctrl));
        assert_eq!(latin_shortcut(key('а', ctrl), russian), key('f', ctrl));
        let ctrl_alt = KeyModifiers::CONTROL | KeyModifiers::ALT;
        assert_eq!(
            latin_shortcut(key('а', ctrl_alt), russian),
            key('f', ctrl_alt)
        );
        let ctrl_shift = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        assert_eq!(
            latin_shortcut(key('ъ', ctrl_shift), russian),
            key(']', ctrl_shift)
        );
    }

    #[test]
    fn ctrl_letters_are_lower_case_with_shift_or_caps_lock() {
        let us = Layouts {
            key_for_char: |_| None,
            is_altgr_text: |_| false,
        };
        let ctrl_shift = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        assert_eq!(normalize(key('C', ctrl_shift), &us), key('c', ctrl_shift));
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(normalize(key('V', ctrl), &us), key('v', ctrl), "CapsLock");
        let russian_layouts = Layouts {
            key_for_char: russian,
            is_altgr_text: |_| false,
        };
        assert_eq!(
            normalize(key('А', ctrl_shift), &russian_layouts),
            key('f', ctrl_shift)
        );
        assert_eq!(
            normalize(key('A', KeyModifiers::SHIFT), &us),
            key('A', KeyModifiers::SHIFT),
            "typing keeps its case"
        );
    }

    #[test]
    fn alt_shift_brackets_in_any_layout() {
        let alt_shift = KeyModifiers::ALT | KeyModifiers::SHIFT;
        assert_eq!(
            latin_shortcut(key('Х', alt_shift), russian),
            key('[', alt_shift)
        );
        assert_eq!(
            latin_shortcut(key('{', alt_shift), russian),
            key('[', alt_shift)
        );
        // Other Alt+Shift letters stay for quick search.
        assert_eq!(
            latin_shortcut(key('А', alt_shift), russian),
            key('А', alt_shift)
        );
    }

    #[test]
    fn altgr_brackets_are_typed_ctrl_alt_brackets_are_shortcuts() {
        let german = Layouts {
            key_for_char: |_| None,
            is_altgr_text: |ch| matches!(ch, '[' | ']' | '{' | '}'),
        };
        let ctrl_alt = KeyModifiers::CONTROL | KeyModifiers::ALT;
        assert_eq!(
            normalize(key('[', ctrl_alt), &german),
            key('[', KeyModifiers::NONE)
        );
        assert_eq!(normalize(key('f', ctrl_alt), &german), key('f', ctrl_alt));
        let us = Layouts {
            key_for_char: |_| None,
            is_altgr_text: |_| false,
        };
        assert_eq!(normalize(key('[', ctrl_alt), &us), key('[', ctrl_alt));
    }

    #[test]
    fn typing_and_quick_search_keep_their_characters() {
        assert_eq!(
            latin_shortcut(key('х', KeyModifiers::NONE), russian),
            key('х', KeyModifiers::NONE)
        );
        assert_eq!(
            latin_shortcut(key('а', KeyModifiers::ALT), russian),
            key('а', KeyModifiers::ALT)
        );
        // Latin characters are already keys.
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(latin_shortcut(key('{', ctrl), russian), key('{', ctrl));
    }
}
