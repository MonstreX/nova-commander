//! FAR's fixed edit line with a mask (`DI_FIXEDIT` with `DIF_MASKEDIT`, `far/edit.cpp`): as long as its
//! mask, the mask's other characters (separators) fixed in place, the cursor always on a place for
//! input. Typing types over, as FAR's dialogs start a fixed field (`SetOvertypeMode(true)`); after Ins it
//! inserts and pushes the rest of the field along. Del pulls the field back, Ctrl+Y blanks it. FAR's
//! masks: `9` a digit, `N` a digit or a space, `#` a digit, a space or a minus, `A` a letter, `H` a hex
//! digit, `X` anything.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskedInput {
    mask: Vec<char>,
    text: Vec<char>,
    cursor: usize,
    overtype: bool,
}

/// Whether `place` of a mask takes input (FAR's `CheckCharMask`).
fn takes_input(place: char) -> bool {
    matches!(place, '9' | 'N' | '#' | 'A' | 'H' | 'X')
}

/// Whether `ch` may stand at `place` (FAR's `CharInMask`).
fn fits(ch: char, place: char) -> bool {
    match place {
        '9' => ch.is_ascii_digit(),
        'N' => ch.is_ascii_digit() || ch == ' ',
        '#' => ch.is_ascii_digit() || ch == ' ' || ch == '-',
        'A' => ch.is_alphabetic(),
        'H' => ch.is_ascii_hexdigit(),
        'X' => !ch.is_control(),
        _ => false,
    }
}

impl MaskedInput {
    pub fn new(mask: &str, text: &str) -> Self {
        let mut input = Self {
            mask: mask.chars().collect(),
            text: Vec::new(),
            cursor: 0,
            overtype: true,
        };
        input.set_text(text);
        input
    }

    /// The field's text, as long as the mask.
    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Puts the cursor at `at` (on the nearest place for input from there on).
    pub fn set_cursor(&mut self, at: usize) {
        self.cursor = at;
        self.settle(true);
    }

    pub fn width(&self) -> usize {
        self.mask.len()
    }

    /// `text` laid over the mask place by place; what does not fit a place is a space. The cursor goes
    /// to the start.
    pub fn set_text(&mut self, text: &str) {
        let mut given = text.chars();
        self.text = (self.mask.iter())
            .map(|&place| {
                let ch = given.next().unwrap_or(' ');
                match place {
                    place if takes_input(place) && fits(ch, place) => ch,
                    place if takes_input(place) => ' ',
                    separator => separator,
                }
            })
            .collect();
        self.cursor = 0;
        self.settle(true);
    }

    /// A key for the field; returns whether its text changed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let before = self.text.clone();
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        let forward = match key.code {
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                false
            }
            KeyCode::Right => {
                // FAR: not past the end of what is typed.
                if self.typed_length() > self.cursor {
                    self.cursor += 1;
                }
                true
            }
            KeyCode::Home => {
                self.cursor = 0;
                true
            }
            KeyCode::End => {
                self.cursor = self.typed_length().min(self.mask.len().saturating_sub(1));
                false
            }
            KeyCode::Insert => {
                self.overtype = !self.overtype;
                true
            }
            KeyCode::Delete => {
                self.delete();
                true
            }
            KeyCode::Backspace => {
                self.backspace();
                false
            }
            KeyCode::Char('y') if control => {
                self.set_text("");
                true
            }
            KeyCode::Char(ch) if !control => {
                self.type_char(ch);
                true
            }
            _ => true,
        };
        self.settle(forward);
        self.text != before
    }

    /// The text without the spaces at its end.
    fn typed_length(&self) -> usize {
        self.text.len() - self.text.iter().rev().take_while(|ch| **ch == ' ').count()
    }

    /// FAR's `GetNextCursorPos`: the nearest place for input the way the cursor moved, else back, else
    /// the first.
    fn settle(&mut self, forward: bool) {
        let fits_here = |at: &usize| self.mask.get(*at).is_some_and(|place| takes_input(*place));
        let ahead: Option<usize> = if forward {
            (self.cursor..self.mask.len()).find(fits_here)
        } else {
            (0..=self.cursor.min(self.mask.len())).rev().find(fits_here)
        };
        let behind = || (0..=self.cursor.min(self.mask.len())).rev().find(fits_here);
        let first = || (0..self.mask.len()).find(fits_here);
        if let Some(at) = ahead.or_else(behind).or_else(first) {
            self.cursor = at;
        }
    }

    /// Places for input from `from` on.
    fn places_from(&self, from: usize) -> Vec<usize> {
        (from..self.mask.len())
            .filter(|at| takes_input(self.mask[*at]))
            .collect()
    }

    fn type_char(&mut self, ch: char) {
        let Some(&place) = self.mask.get(self.cursor) else {
            return;
        };
        if !fits(ch, place) {
            return;
        }
        if !self.overtype {
            // FAR's insert in a mask: each place takes what stood at the place before it.
            let places = self.places_from(self.cursor);
            for pair in places.windows(2).rev() {
                self.text[pair[1]] = self.text[pair[0]];
            }
        }
        self.text[self.cursor] = ch;
        self.cursor += 1;
    }

    /// FAR's Del in a mask: each place takes what stands at the next one while it fits, a space ends it.
    fn delete(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let places = self.places_from(self.cursor);
        let mut last = self.cursor;
        for pair in places.windows(2) {
            let next = self.text[pair[1]];
            if !fits(next, self.mask[pair[0]]) {
                break;
            }
            self.text[pair[0]] = next;
            last = pair[1];
        }
        self.text[last] = ' ';
    }

    /// Back to the place for input before the cursor, then Del there.
    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        while self.cursor > 0 && !takes_input(self.mask[self.cursor]) {
            self.cursor -= 1;
        }
        self.delete();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(input: &mut MaskedInput, code: KeyCode) -> bool {
        input.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn typed(input: &mut MaskedInput, text: &str) {
        for ch in text.chars() {
            press(input, KeyCode::Char(ch));
        }
    }

    #[test]
    fn the_separators_stay_and_what_does_not_fit_is_a_space() {
        let input = MaskedInput::new("99.99.9999N", "29.02.2024");
        assert_eq!(input.text(), "29.02.2024 ");
        assert_eq!(MaskedInput::new("99.99.9999N", "").text(), "  .  .     ");
        assert_eq!(MaskedInput::new("99.99", "a1x23").text(), " 1.23");
        assert_eq!(input.width(), 11);
    }

    #[test]
    fn typing_over_the_separators_fills_a_blank_date() {
        let mut input = MaskedInput::new("99.99.9999N", "");
        typed(&mut input, "15032025");
        assert_eq!(input.text(), "15.03.2025 ");
        typed(&mut input, ".x");
        assert_eq!(
            input.text(),
            "15.03.2025 ",
            "a separator or a letter is no digit"
        );
    }

    #[test]
    fn typing_types_over_and_after_ins_inserts_and_pushes_the_rest_along() {
        let mut input = MaskedInput::new("99.99.9999N", "29.02.2024");
        typed(&mut input, "15032025");
        assert_eq!(input.text(), "15.03.2025 ", "FAR's fixed field types over");
        let mut input = MaskedInput::new("99:99", "12:34");
        press(&mut input, KeyCode::Insert);
        typed(&mut input, "9");
        assert_eq!((input.text().as_str(), input.cursor()), ("91:23", 1));
        press(&mut input, KeyCode::Insert);
        typed(&mut input, "88");
        assert_eq!((input.text().as_str(), input.cursor()), ("98:83", 4));
    }

    #[test]
    fn del_pulls_back_and_backspace_steps_over_a_separator() {
        let mut input = MaskedInput::new("N9:99", " 1:23");
        press(&mut input, KeyCode::Delete);
        assert_eq!(input.text(), "12:3 ");
        press(&mut input, KeyCode::End);
        assert_eq!(input.cursor(), 4);
        press(&mut input, KeyCode::Backspace);
        assert_eq!((input.text().as_str(), input.cursor()), ("12:  ", 3));
        press(&mut input, KeyCode::Backspace);
        assert_eq!((input.text().as_str(), input.cursor()), ("1 :  ", 1));
    }

    #[test]
    fn the_cursor_skips_separators_the_way_it_moves_and_ctrl_y_blanks_the_field() {
        let mut input = MaskedInput::new("99.99", "12.34");
        press(&mut input, KeyCode::Right);
        press(&mut input, KeyCode::Right);
        assert_eq!(input.cursor(), 3);
        press(&mut input, KeyCode::Left);
        assert_eq!(input.cursor(), 1);
        let changed = input.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
        assert!(changed);
        assert_eq!((input.text().as_str(), input.cursor()), ("  .  ", 0));
        assert!(!press(&mut input, KeyCode::Right), "moving changes nothing");
    }

    #[test]
    fn a_hex_place_takes_hex_digits_only() {
        let mut input = MaskedInput::new("HH HH", "");
        for ch in ['a', 'g', 'F', '1', '9'] {
            press(&mut input, KeyCode::Char(ch));
        }
        assert_eq!(input.text(), "aF 19");
    }

    #[test]
    fn a_letter_place_takes_letters_only() {
        let mut input = MaskedInput::new("AA-9", "");
        for ch in ['1', 'z', '-', 'Ж', '5'] {
            press(&mut input, KeyCode::Char(ch));
        }
        assert_eq!(input.text(), "zЖ-5");
    }
}
