//! The editor's hex mode (ADR-0027, Ctrl+F4; Volkov Commander's editor): the document's bytes in VC's
//! rows - the offset, 16 bytes in fours, their characters - over the same piece table, only the rows
//! shown read. The cursor stands on a byte (`hex_cursor`); typing overwrites it as one undo step, so
//! the changed mark, undo and saving are the editor's own. Ctrl+F4 again shows the text at the byte.

use super::{EditScreen, EditView, Step};
use crate::editor::view::blocks::{Bytes, Missing};
use crate::editor::view::hex_cursor::{HexCursor, HexMove, byte_char};

/// The bytes of a row.
const ROW: u64 = 16;
/// Columns from the row's first byte to its characters: 16 bytes of two digits, a space between
/// them, another after every four, two spaces before the characters.
const CHARS_AT: usize = 16 * 2 + 15 + 3 + 2;

/// The hex mode's cursor and its first row's first byte.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Hex {
    cursor: HexCursor,
    top: u64,
}

/// The digits of the rows' offsets: eight, more for a document past 4 GB.
fn address_digits(len: u64) -> usize {
    let digits = (u64::BITS - len.leading_zeros()).div_ceil(4) as usize;
    digits.max(8)
}

/// The column of byte `index` of a row: its hex digits', its character's.
fn hex_column(prefix: usize, index: usize) -> usize {
    prefix + 3 * index + index / 4
}

fn char_column(prefix: usize, index: usize) -> usize {
    prefix + CHARS_AT + index
}

/// VC's row of `bytes` from `start`: `{offset}: `, the bytes in fours, two spaces, their characters.
fn nc_row(start: u64, bytes: &[u8], digits: usize, encoding: &super::Encoding) -> String {
    let mut row = format!("{start:0digits$X}: ");
    for index in 0..ROW as usize {
        match bytes.get(index) {
            Some(byte) => row.push_str(&format!("{byte:02X}")),
            None => row.push_str("  "),
        }
        if index + 1 < ROW as usize {
            row.push(' ');
            if index % 4 == 3 {
                row.push(' ');
            }
        }
    }
    row.push_str("  ");
    row.extend(bytes.iter().map(|byte| byte_char(*byte, encoding)));
    row
}

/// The hex move of a step (none: not a move).
fn hex_move(step: &Step) -> Option<HexMove> {
    let step = match step {
        Step::Up | Step::ScrollUp => HexMove::Up,
        Step::Down | Step::ScrollDown => HexMove::Down,
        Step::Left | Step::Backspace => HexMove::Left,
        Step::Right => HexMove::Right,
        Step::Home => HexMove::Home,
        Step::End => HexMove::End,
        Step::PageUp => HexMove::PageUp,
        Step::PageDown => HexMove::PageDown,
        Step::Start | Step::FirstLine => HexMove::Start,
        Step::Finish | Step::LastLine => HexMove::Finish,
        _ => return None,
    };
    Some(step)
}

impl EditView {
    /// The hex mode's cursor (none in the text mode).
    pub fn hex_cursor(&self) -> Option<HexCursor> {
        self.hex.map(|hex| hex.cursor)
    }

    /// The hex mode's first row's byte (none in the text mode).
    pub fn hex_top(&self) -> Option<u64> {
        self.hex.map(|hex| hex.top)
    }

    /// The byte under the hex mode's cursor (none in the text mode, an empty document).
    pub fn hex_byte(&self) -> Option<u8> {
        let hex = self.hex?;
        let bytes = self.doc.at(hex.cursor.pos).ok()?;
        bytes
            .first()
            .copied()
            .filter(|_| hex.cursor.pos < self.doc.len())
    }

    /// The hex mode on the cursor's byte (`at`: on that byte, in the text column when true), the
    /// cursor's row kept on the screen's row it had; the block let go.
    pub(super) fn enter_hex(
        &mut self,
        at: Option<(u64, bool)>,
        height: usize,
    ) -> Result<(), Missing> {
        let pos = match at {
            Some((pos, _)) => pos,
            None => self.cursor_pos()?,
        };
        let row = (self.rows(height).iter())
            .position(|(start, _)| *start == self.line)
            .unwrap_or(0) as u64;
        let mut cursor = HexCursor::at(pos, self.doc.len());
        cursor.text = at.is_some_and(|(_, text)| text);
        let first = cursor.pos - cursor.pos % ROW;
        self.unmark();
        self.found = None;
        self.hex = Some(Hex {
            cursor,
            top: first.saturating_sub(row * ROW),
        });
        Ok(())
    }

    /// The text mode on the hex cursor's byte, its row kept on the screen's row it had.
    fn leave_hex(&mut self, hex: Hex, height: usize) -> Result<(), Missing> {
        let pos = hex.cursor.pos.max(self.floor);
        let line = self.line_holding(pos)?;
        let col = Self::col_of(&self.read(line)?, pos);
        let rows = hex.cursor.pos.saturating_sub(hex.top) / ROW;
        let mut top = line;
        for _ in 0..rows.min(height as u64) {
            top = self.previous_line(top)?.unwrap_or(top);
        }
        (self.line, self.col, self.top) = (line, col, top);
        self.hex = None;
        Ok(())
    }

    /// A step the hex mode takes: Ctrl+F4 and the save's `HexAt` turning it on, any step while it is
    /// on (none: the text mode's step).
    pub(super) fn hex_dispatch(
        &mut self,
        step: &Step,
        height: usize,
    ) -> Option<Result<(), Missing>> {
        match step {
            Step::HexAt(pos, text) => Some(self.enter_hex(Some((*pos, *text)), height)),
            _ if self.hex.is_some() => Some(self.hex_step(step, height)),
            Step::Hex => Some(self.enter_hex(None, height)),
            _ => None,
        }
    }

    /// A step in the hex mode: the moves, Tab, typing over a byte, undo and redo, what a search found;
    /// Ctrl+F4 the text mode. The rest does nothing there.
    pub(super) fn hex_step(&mut self, step: &Step, height: usize) -> Result<(), Missing> {
        let Some(mut hex) = self.hex else {
            return Ok(());
        };
        let len = self.doc.len();
        if let Some(moved) = hex_move(step) {
            hex.cursor.moved(moved, ROW, height.max(1) as u64, len);
        } else {
            match step {
                Step::Hex => return self.leave_hex(hex, height),
                Step::Type('\t') => hex.cursor.switch_column(),
                Step::Type(ch) => self.overwrite(&mut hex.cursor, *ch)?,
                Step::Undo | Step::Redo => self.hex_undone(&mut hex.cursor, step == &Step::Redo),
                Step::Show(at, _) => hex.cursor = HexCursor::at(*at, len),
                _ => {}
            }
        }
        self.hex = Some(hex);
        Ok(())
    }

    /// `ch` typed over the cursor's byte (its bytes in the text column) as one undo step.
    fn overwrite(&mut self, cursor: &mut HexCursor, ch: char) -> Result<(), Missing> {
        let len = self.doc.len();
        if cursor.pos >= len {
            return Ok(());
        }
        let old = self.doc.at(cursor.pos)?[0];
        let mut moved = *cursor;
        let Some((at, bytes)) = moved.typed(ch, old, &self.encoding, len) else {
            return Ok(());
        };
        self.doc.begin_group();
        self.delete_bytes(at, bytes.len() as u64);
        self.insert_bytes(at, &bytes);
        self.doc.end_group();
        self.changed_at(at);
        *cursor = moved;
        Ok(())
    }

    /// Ctrl+Z / Ctrl+Shift+Z in the hex mode: the cursor on the byte the step was made at.
    fn hex_undone(&mut self, cursor: &mut HexCursor, redo: bool) {
        let at = match redo {
            true => self.doc.redo(),
            false => self.doc.undo(),
        };
        if let Some(at) = at {
            self.counter_reset();
            *cursor = HexCursor {
                text: cursor.text,
                ..HexCursor::at(at, self.doc.len())
            };
        }
    }

    /// The hex mode's screen kept on the cursor's row.
    pub(super) fn hex_scroll(&mut self, height: usize) {
        if let Some(hex) = self.hex.as_mut() {
            let row = hex.cursor.pos - hex.cursor.pos % ROW;
            let rows = height.max(1) as u64;
            if row < hex.top {
                hex.top = row;
            } else if row >= hex.top + rows * ROW {
                hex.top = row - (rows - 1) * ROW;
            }
        }
    }

    /// Up to `count` bytes from `from`, across pieces.
    fn bytes_from(&self, from: u64, count: usize) -> Result<Vec<u8>, Missing> {
        let mut out = Vec::with_capacity(count);
        while out.len() < count {
            let piece = self.doc.at(from + out.len() as u64)?;
            if piece.is_empty() {
                break;
            }
            out.extend_from_slice(&piece[..piece.len().min(count - out.len())]);
        }
        Ok(out)
    }

    /// The hex mode's rows of a `width` by `height` screen, the cursor on them, the bytes changed and
    /// not saved marked.
    pub(super) fn hex_screen(&self, hex: &Hex, width: usize, height: usize) -> EditScreen {
        let len = self.doc.len();
        let digits = address_digits(len);
        let prefix = digits + 2;
        let mut screen = EditScreen::default();
        for index in 0..height as u64 {
            let start = hex.top + index * ROW;
            if start >= len.max(1) {
                break;
            }
            let count = (len.saturating_sub(start)).min(ROW) as usize;
            let bytes = match self.bytes_from(start, count) {
                Ok(bytes) => bytes,
                Err(missing) => {
                    screen.missing = Some(missing);
                    break;
                }
            };
            let cursor = hex.cursor.pos;
            if (start..start + ROW).contains(&cursor) {
                let at = (cursor - start) as usize;
                let col = match hex.cursor.text {
                    true => char_column(prefix, at),
                    false => hex_column(prefix, at) + usize::from(hex.cursor.low),
                };
                screen.cursor = (col < width).then_some((screen.rows.len(), col));
            }
            let row = nc_row(start, &bytes, digits, &self.encoding);
            screen.rows.push(row.chars().take(width).collect());
            screen.marks.push(None);
            screen.spans.push(self.changed_cells(start, count, prefix));
        }
        screen
    }

    /// The cells of a row's bytes typed and not saved: their digits and their characters.
    fn changed_cells(&self, start: u64, count: usize, prefix: usize) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();
        for (from, to) in self.doc.added(start, start + count as u64) {
            let (first, last) = ((from - start) as usize, (to - start - 1) as usize);
            cells.push((hex_column(prefix, first), hex_column(prefix, last) + 2));
            cells.push((char_column(prefix, first), char_column(prefix, last) + 1));
        }
        cells
    }
}

#[cfg(test)]
#[path = "edit_hex_tests.rs"]
mod tests;
