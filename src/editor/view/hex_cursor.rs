//! Byte editing (ADR-0027; Volkov Commander's hex editor): a cursor on a byte - in the hex column on
//! one of its halves, or in the text column (Tab) - its moves over rows of bytes, and what typing
//! writes: a hex digit the half the cursor is on, a character its bytes in the code page. Typing only
//! overwrites: nothing is put in or taken out, nothing is typed past the last byte.

use super::code_page::{Encoding, shown};

/// A move of the cursor (VC's editor keys in hex).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexMove {
    Up,
    Down,
    /// In the hex column half a byte, in the text column a byte.
    Left,
    Right,
    /// The row's first / last byte.
    Home,
    End,
    PageUp,
    PageDown,
    /// The first / last byte.
    Start,
    Finish,
}

/// The cursor on a byte: on its low half (the hex column), in the text column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HexCursor {
    pub pos: u64,
    pub low: bool,
    pub text: bool,
}

/// The last byte a cursor stands on (the first of an empty file).
fn last(len: u64) -> u64 {
    len.saturating_sub(1)
}

impl HexCursor {
    /// The cursor on byte `pos` of `len` (the last when past it), its high half.
    pub fn at(pos: u64, len: u64) -> Self {
        Self {
            pos: pos.min(last(len)),
            ..Self::default()
        }
    }

    /// A move over rows of `size` bytes, `rows` on the screen, in `len` bytes.
    pub fn moved(&mut self, step: HexMove, size: u64, rows: u64, len: u64) {
        let size = size.max(1);
        let last = last(len);
        let row = self.pos - self.pos % size;
        let page = size * rows.saturating_sub(1).max(1);
        let pos = self.pos;
        match step {
            HexMove::Left => return self.left(),
            HexMove::Right => return self.right(len),
            HexMove::Up => self.pos = pos.checked_sub(size).unwrap_or(pos),
            HexMove::Down if row + size <= last => self.pos = (pos + size).min(last),
            HexMove::Down => {}
            HexMove::PageUp => self.pos = pos.checked_sub(page).unwrap_or(pos % size),
            HexMove::PageDown => self.pos = (pos + page).min(last),
            HexMove::Home => self.pos = row,
            HexMove::End => self.pos = (row + size - 1).min(last),
            HexMove::Start => self.pos = 0,
            HexMove::Finish => self.pos = last,
        }
        self.low = false;
    }

    /// Left: the high half of the byte from its low one, else the byte before (its low half).
    fn left(&mut self) {
        if self.low {
            self.low = false;
        } else if self.pos > 0 {
            self.pos -= 1;
            self.low = !self.text;
        }
    }

    /// Right: the low half of the byte from its high one, else the byte after (its high half).
    fn right(&mut self, len: u64) {
        if !self.text && !self.low {
            self.low = true;
        } else if self.pos < last(len) {
            self.pos += 1;
            self.low = false;
        }
    }

    /// Tab: the other column, on the byte's high half.
    pub fn switch_column(&mut self) {
        self.text = !self.text;
        self.low = false;
    }

    /// What `ch` typed writes at the cursor over `old`, the byte there, in `len` bytes of `encoding`:
    /// the first byte written and the bytes (none: not a hex digit, past the end, a character whose
    /// bytes go past it); the cursor moved on past them.
    pub fn typed(
        &mut self,
        ch: char,
        old: u8,
        encoding: &Encoding,
        len: u64,
    ) -> Option<(u64, Vec<u8>)> {
        let at = self.pos;
        if at >= len {
            return None;
        }
        if self.text {
            let bytes = encoding.encode(&ch.to_string());
            let end = at + bytes.len() as u64;
            if bytes.is_empty() || end > len {
                return None;
            }
            self.pos = end.min(last(len));
            return Some((at, bytes));
        }
        let digit = u8::try_from(ch.to_digit(16)?).ok()?;
        let byte = match self.low {
            true => (old & 0xF0) | digit,
            false => (digit << 4) | (old & 0x0F),
        };
        self.right(len);
        Some((at, vec![byte]))
    }
}

/// A byte as the text column shows it: a zero as `.` (VC's), a byte of a one-byte code page as its
/// character, an ASCII one in the others; `.` for what is no character alone.
pub fn byte_char(byte: u8, encoding: &Encoding) -> char {
    if byte == 0 {
        return '.';
    }
    match encoding {
        Encoding::Page(_, decoder) => match decoder.decode(&[byte]) {
            (ch, 1) => shown(ch),
            _ => '.',
        },
        _ if byte.is_ascii() => shown(char::from(byte)),
        _ => '.',
    }
}

#[cfg(test)]
#[path = "hex_cursor_tests.rs"]
mod tests;
