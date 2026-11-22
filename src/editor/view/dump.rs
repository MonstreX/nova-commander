//! Hex and dump (FAR's `Viewer::ShowHex`, `ShowDump`, their keys in `Viewer::ProcessKey`): rows of
//! bytes from a byte offset, no lines - each row `line_size` bytes on from the one before. Hex shows
//! `0000000000: ` the offset, the bytes in stripes of eight (`│` between), then their characters; dump
//! only the characters, a screen's width of them. A character cut by the row's end shows `›` for each
//! byte left on either row (FAR's `continue_char`), one not decoded `�`.

use super::{Missing, Mode, Move, Screen, TextView, Top, line_before};
use crate::editor::view::code_page::{Encoding, shown};

/// FAR's default bytes of a hex line (`m_HexModeBytesPerLine`).
pub const HEX_BYTES: usize = 16;
/// FAR's hex stripes (`s_HexModeBytesPerStripe`) and widths of its line's parts.
const STRIPE: usize = 8;
const ADDRESS_WIDTH: usize = 11;
const BYTE_WIDTH: usize = 3;
const STRIPE_PADDING: usize = 2;
const CHARS_LEADER: usize = 2;
/// A character's bytes cut by the row's end (FAR's `encoding::continue_char`).
const CONTINUED: char = '›';
const REPLACED: char = '\u{FFFD}';

/// FAR's `hex_mode_chars_area_width`.
fn chars_width(bytes: usize, unit: usize) -> usize {
    (bytes + unit / 2) / unit
}

/// FAR's `hex_mode_line_width_from_bytes`.
pub(super) fn hex_line_width(bytes: usize, unit: usize) -> usize {
    ADDRESS_WIDTH
        + BYTE_WIDTH * bytes
        + STRIPE_PADDING * ((bytes.max(1) - 1) / STRIPE)
        + CHARS_LEADER
        + chars_width(bytes, unit)
}

/// FAR's `hex_mode_bytes_from_line_width`: the most bytes a line of `width` holds (a stripe at least).
pub(super) fn hex_bytes_in(width: usize, unit: usize) -> usize {
    if width <= hex_line_width(STRIPE, unit) {
        return STRIPE;
    }
    let bytes_and_chars = width - ADDRESS_WIDTH;
    let per_stripe = STRIPE * BYTE_WIDTH + STRIPE_PADDING + chars_width(STRIPE, unit);
    let stripes = bytes_and_chars / per_stripe;
    let left = bytes_and_chars.saturating_sub(CHARS_LEADER + per_stripe * stripes);
    // With two-byte characters a byte takes 3.5 columns: counted doubled.
    let more = (2 * left) / (2 * BYTE_WIDTH + 2 / unit);
    STRIPE * stripes + more
}

/// FAR's `ChangeHexModeBytesPerLine`: `bytes` moved by `amount` to the next multiple of it, within a
/// stripe and what `width` holds; unchanged when nothing fits.
pub(super) fn hex_bytes_moved(bytes: usize, amount: i32, width: usize, unit: usize) -> usize {
    let step = amount.unsigned_abs() as usize;
    if step == 0 {
        return bytes;
    }
    let least = STRIPE.max(step);
    let most = hex_bytes_in(width, unit) / step * step;
    if least > most {
        return bytes;
    }
    let next_grid = |at: usize| (at / step + 1) * step;
    let next = next_grid(bytes);
    let rounded = if amount > 0 {
        next
    } else {
        next - next_grid(next - bytes)
    };
    rounded.clamp(least, most)
}

/// The characters of `bytes` (`count` of them the row's own, the rest read on for a character cut by
/// its end), `width` of them, after `tail` bytes of one the row before began (FAR's `txt_dump`); the
/// bytes of a character this row leaves to the next.
pub(super) fn characters(
    encoding: &Encoding,
    bytes: &[u8],
    count: usize,
    width: usize,
    mut tail: usize,
) -> (String, usize) {
    let mut out = String::new();
    let mut shown_count = 0;
    let unit = encoding.unit() as usize;
    if unit == 2 {
        for pair in bytes[..count].chunks(2) {
            let ch = match pair {
                [one, two] => {
                    let value = encoding.unit_value(&[*one, *two]);
                    char::from_u32(value).map_or(REPLACED, shown)
                }
                _ => REPLACED,
            };
            out.push(ch);
            shown_count += 1;
        }
        tail = 0;
    } else {
        let mut at = 0;
        while at < count && shown_count < width {
            if tail > 0 {
                tail -= 1;
                out.push(CONTINUED);
                shown_count += 1;
                at += 1;
                continue;
            }
            let (ch, len) = encoding
                .decode(&bytes[at..], false)
                .map_or((REPLACED, 1), |(ch, len)| (shown(ch), len.max(1)));
            out.push(ch);
            shown_count += 1;
            let padding = (len - 1).min(width - shown_count);
            out.extend(std::iter::repeat_n(CONTINUED, padding));
            shown_count += padding;
            tail = len - 1 - padding;
            at += len;
        }
    }
    out.extend(std::iter::repeat_n(' ', width.saturating_sub(shown_count)));
    (out, tail)
}

impl TextView {
    /// FAR's `get_char_size`: two bytes for UTF-16, one for the rest.
    fn unit(&self) -> u64 {
        self.encoding.as_ref().map_or(1, Encoding::unit)
    }

    /// The bytes of a row (FAR's `GetModeDependentLineSize`).
    pub(super) fn line_size(&self, width: usize) -> u64 {
        match self.mode {
            Mode::Hex => self.hex_bytes as u64,
            _ => width as u64 * self.unit(),
        }
    }

    /// Up to `count` bytes from `from`, across blocks; fewer only at the file's end.
    pub(super) fn bytes(&self, from: u64, count: usize) -> Result<Vec<u8>, Missing> {
        let mut out = Vec::with_capacity(count);
        let mut at = from;
        while out.len() < count && at < self.len() {
            let piece = self.blocks.at(at)?;
            let take = piece.len().min(count - out.len());
            if take == 0 {
                break;
            }
            out.extend_from_slice(&piece[..take]);
            at += take as u64;
        }
        Ok(out)
    }

    /// The rows from `pos`: each `line_size` bytes, the file's end on the one that reaches it.
    pub(super) fn lay_out_bytes(
        &self,
        width: usize,
        height: usize,
        screen: &mut Screen,
    ) -> Result<(), Missing> {
        let encoding = self.encoding.as_ref().ok_or(Missing(0))?;
        let unit = self.unit();
        let pos = match self.mode {
            Mode::Dump => self.pos - self.pos % unit,
            _ => self.pos,
        };
        screen.position = pos;
        let size = self.line_size(width);
        // Read on past the row for a character its end cuts.
        let more = match encoding {
            Encoding::Utf8 => 3,
            Encoding::Page(..) => 1,
            _ => 0,
        };
        let mut tail = 0;
        for row in 0..height as u64 {
            let start = pos + row * size;
            if start >= self.len() {
                screen.at_end = true;
                return Ok(());
            }
            let bytes = self.with_typed(start, self.bytes(start, size as usize + more)?);
            let count = bytes.len().min(size as usize);
            let text = match self.mode {
                Mode::Hex => {
                    let (line, next) = self.hex_row(encoding, start, &bytes, count, tail);
                    tail = next;
                    let left = self.shown_left(width);
                    line.chars().skip(left).take(width).collect()
                }
                _ => {
                    let (line, next) = characters(encoding, &bytes, count, width, tail);
                    tail = next;
                    line
                }
            };
            screen.rows.push(text);
            screen.marks.push(self.marked_bytes(start, count, width));
            self.edit_cells(screen, start, count, width);
            screen.end = (start + size).min(self.len());
            if start + size >= self.len() {
                screen.at_end = true;
                return Ok(());
            }
        }
        Ok(())
    }

    /// The columns of a row of `count` bytes from `start` the selection covers: hex's bytes (past the
    /// columns scrolled away), dump's characters.
    fn marked_bytes(&self, start: u64, count: usize, width: usize) -> Option<(usize, usize)> {
        let (first, size) = self.selection?;
        let from = first.max(start);
        let to = (first + size.max(1)).min(start + count as u64);
        if from >= to {
            return None;
        }
        let (first, last) = ((from - start) as usize, (to - start - 1) as usize);
        if self.mode == Mode::Dump {
            let unit = self.unit() as usize;
            return Some((first / unit, last / unit + 1));
        }
        // FAR's `SelStart`: past the row's address and its ": " (12 columns; review A5: one short).
        let column =
            |byte: usize| ADDRESS_WIDTH + 1 + BYTE_WIDTH * byte + STRIPE_PADDING * (byte / STRIPE);
        let left = self.shown_left(width);
        let begin = column(first).saturating_sub(left);
        let end = (column(last) + 2).saturating_sub(left);
        (end > begin).then_some((begin, end.min(width)))
    }

    /// The hex rows' first column shown on a screen `width` wide.
    pub(super) fn shown_left(&self, width: usize) -> usize {
        let room = hex_line_width(self.hex_bytes, self.unit() as usize);
        self.left.min(room.saturating_sub(width))
    }

    /// FAR's hex row: `{:010X}: `, the bytes (`│ ` between stripes), two spaces, the characters.
    fn hex_row(
        &self,
        encoding: &Encoding,
        start: u64,
        bytes: &[u8],
        count: usize,
        tail: usize,
    ) -> (String, usize) {
        let mut line = format!("{start:010X}: ");
        for at in 0..self.hex_bytes {
            match bytes[..count].get(at) {
                Some(byte) => line.push_str(&format!("{byte:02X} ")),
                None => line.push_str("   "),
            }
            if at + 1 != self.hex_bytes && (at + 1) % STRIPE == 0 {
                line.push_str("│ ");
            }
        }
        let (text, next) = characters(encoding, bytes, count, self.hex_bytes, tail);
        line.push(' ');
        line.push_str(&text);
        (line, next)
    }

    /// A key's move in hex or dump (FAR's `Viewer::ProcessKey` there).
    pub(super) fn apply_bytes(
        &mut self,
        step: Move,
        width: usize,
        height: usize,
    ) -> Result<(), Missing> {
        let size = self.line_size(width);
        let len = self.len();
        let rows = height as u64;
        let at_end = self.pos + size * rows >= len;
        match step {
            Move::Up => self.pos = self.pos.saturating_sub(size),
            Move::PageUp => self.pos = self.pos.saturating_sub(size * (rows - 1)),
            Move::Down if !at_end => self.pos += size,
            Move::PageDown if !at_end => self.pos += size * (rows - 1),
            Move::Home | Move::Start => {
                if step == Move::Home {
                    self.left = 0;
                }
                self.pos = 0;
            }
            Move::End | Move::Finish => {
                if step == Move::End {
                    self.left = 0;
                }
                self.pos = self.last_screen(size, rows);
            }
            Move::Left(by) if self.mode == Mode::Hex => self.left = self.left.saturating_sub(by),
            Move::Right(by) if self.mode == Mode::Hex => {
                let room = hex_line_width(self.hex_bytes, self.unit() as usize);
                self.left = (self.left + by).min(room.saturating_sub(width));
            }
            Move::LineStarts => self.left = 0,
            Move::Bytes(amount) if self.mode == Mode::Hex => {
                let unit = self.unit() as usize;
                self.hex_bytes = hex_bytes_moved(self.hex_bytes, amount, width, unit);
            }
            Move::Roll(chars) => {
                // A character is a byte in hex (FAR's `GetModeDependentCharSize`).
                let unit = match self.mode {
                    Mode::Hex => 1,
                    _ => self.unit(),
                };
                let moved = self.pos as i128 + i128::from(chars) * unit as i128;
                let moved = moved.clamp(0, len.saturating_sub(1) as i128) as u64;
                self.pos = moved / unit * unit;
            }
            _ => {}
        }
        Ok(())
    }

    /// FAR's End in hex and dump: the last screen, the end on its last row.
    fn last_screen(&self, size: u64, rows: u64) -> u64 {
        let unit = self.unit();
        let end = self.len() - self.len() % unit;
        let back = if end.is_multiple_of(size) {
            size * rows
        } else {
            end % size + size * (rows - 1)
        };
        end.saturating_sub(back)
    }

    /// Another mode, the place kept (FAR's `ProcessDisplayMode` and `AdjustFilePos`): text starts at
    /// the row that shows the byte the rows started at.
    pub(super) fn switch(&mut self, mode: Mode, width: usize) -> Result<(), Missing> {
        if mode == self.mode {
            return Ok(());
        }
        if self.mode == Mode::Text {
            let line = self.line(self.top.line)?;
            let row = self.top_at(&line, width);
            self.pos = line.row_start(row, width, self.words);
        } else if mode == Mode::Text {
            self.top = self.top_of(self.pos, width)?;
            self.width = width;
        }
        self.mode = mode;
        // The byte editing is hex's (ADR-0027).
        self.edit = None;
        Ok(())
    }

    /// The row of text that shows byte `pos` (FAR's `AdjustFilePos`: a character read, then up a row).
    pub(super) fn top_of(&self, pos: u64, width: usize) -> Result<Top, Missing> {
        let encoding = self.encoding.as_ref().ok_or(Missing(0))?;
        let len = self.len();
        let unit = encoding.unit();
        let pos = self.floor.max(pos - pos % unit).min(len);
        if pos >= len || pos <= self.floor {
            let line = if pos <= self.floor {
                self.floor
            } else {
                line_before(&self.blocks, encoding, len, self.floor, self.layout)?
            };
            return Ok(Top { line, row: 0 });
        }
        let after = encoding
            .decode(self.blocks.at(pos)?, false)
            .map_or(1, |(_, size)| size.max(1));
        let next = (pos + after as u64).min(len);
        let start = line_before(&self.blocks, encoding, next, self.floor, self.layout)?;
        let line = self.line(start)?;
        let row = if self.wrap {
            line.row_of(pos, width, self.words)
        } else {
            0
        };
        Ok(Top { line: start, row })
    }
}

#[cfg(test)]
#[path = "dump_tests.rs"]
mod tests;
