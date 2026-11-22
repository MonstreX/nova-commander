//! The viewer's byte editing in hex (ADR-0027, F2 there): the hex cursor (`hex_cursor`) over the rows
//! of FAR's hex mode and the bytes typed kept beside the file's blocks - shown in place of the file's,
//! marked, until they are written into the file (the viewer never holds the file).

use std::collections::BTreeMap;

use super::{Missing, Mode, Move, Screen, TextView};
use crate::editor::view::hex_cursor::HexCursor;

/// Columns before a hex row's first byte (`{:010X}: `).
const BYTES_AT: usize = 12;

/// The byte editing: the cursor and the bytes typed, by their offsets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ByteEdit {
    pub cursor: HexCursor,
    pub changed: BTreeMap<u64, u8>,
}

impl TextView {
    /// The byte editing on in hex, the cursor on the first row's first byte, the rows from a multiple
    /// of a row's bytes; false in the other modes.
    pub fn edit_bytes(&mut self) -> bool {
        if self.mode != Mode::Hex {
            return false;
        }
        let size = self.hex_bytes.max(1) as u64;
        self.pos -= self.pos % size;
        self.edit = Some(ByteEdit {
            cursor: HexCursor::at(self.pos, self.len()),
            changed: BTreeMap::new(),
        });
        true
    }

    /// The byte editing, when on.
    pub fn byte_edit(&self) -> Option<&ByteEdit> {
        self.edit.as_ref()
    }

    /// The byte editing off, the bytes typed let go.
    pub fn stop_editing(&mut self) {
        self.edit = None;
    }

    /// The bytes typed written into the file: the blocks held show them, the editing goes on.
    pub fn bytes_written(&mut self) {
        if let Some(edit) = self.edit.as_mut() {
            self.blocks.patch(&edit.changed);
            edit.changed.clear();
        }
    }

    /// A step of the byte editing (none: not one, or not editing): the cursor's moves, Tab, typing over
    /// a byte; the screen kept on the cursor.
    pub(super) fn byte_step(&mut self, step: Move, height: usize) -> Option<Result<(), Missing>> {
        let mut edit = self.edit.clone()?;
        let (size, len) = (self.hex_bytes.max(1) as u64, self.len());
        match step {
            Move::Byte(moved) => edit.cursor.moved(moved, size, height as u64, len),
            Move::ByteColumn => edit.cursor.switch_column(),
            Move::TypeByte(ch) => {
                if let Err(missing) = self.type_byte(&mut edit, ch) {
                    return Some(Err(missing));
                }
            }
            _ => return None,
        }
        let row = edit.cursor.pos - edit.cursor.pos % size;
        let rows = height.max(1) as u64;
        if row < self.pos {
            self.pos = row;
        } else if row >= self.pos + rows * size {
            self.pos = row - (rows - 1) * size;
        }
        self.edit = Some(edit);
        Some(Ok(()))
    }

    /// `ch` typed over the cursor's byte: the bytes it makes kept (a byte typed back to the file's no
    /// longer changed), the cursor on past them.
    fn type_byte(&self, edit: &mut ByteEdit, ch: char) -> Result<(), Missing> {
        let len = self.len();
        let pos = edit.cursor.pos;
        if pos >= len {
            return Ok(());
        }
        let encoding = self.encoding.as_ref().ok_or(Missing(0))?;
        let file = |at: u64| {
            self.bytes(at, 1)
                .map(|bytes| bytes.first().copied().unwrap_or(0))
        };
        let old = match edit.changed.get(&pos) {
            Some(byte) => *byte,
            None => file(pos)?,
        };
        let mut cursor = edit.cursor;
        let Some((at, bytes)) = cursor.typed(ch, old, encoding, len) else {
            return Ok(());
        };
        let mut originals = Vec::with_capacity(bytes.len());
        for index in 0..bytes.len() as u64 {
            originals.push(file(at + index)?);
        }
        for ((offset, byte), original) in (at..).zip(bytes).zip(originals) {
            if byte == original {
                edit.changed.remove(&offset);
            } else {
                edit.changed.insert(offset, byte);
            }
        }
        edit.cursor = cursor;
        Ok(())
    }

    /// The bytes of a row from `start` with the bytes typed in place of the file's.
    pub(super) fn with_typed(&self, start: u64, mut bytes: Vec<u8>) -> Vec<u8> {
        if let Some(edit) = &self.edit {
            let end = start + bytes.len() as u64;
            for (&at, &byte) in edit.changed.range(start..end) {
                bytes[(at - start) as usize] = byte;
            }
        }
        bytes
    }

    /// A hex row's cursor and bytes typed on the screen: the row `count` bytes from `start` is the
    /// screen's last, `width` wide.
    pub(super) fn edit_cells(&self, screen: &mut Screen, start: u64, count: usize, width: usize) {
        let Some(edit) = &self.edit else {
            return;
        };
        let bytes = self.hex_bytes;
        let left = self.shown_left(width);
        let hex = |index: usize| BYTES_AT + 3 * index + 2 * (index / 8);
        // The characters after the bytes, `│ ` between stripes of eight and a space.
        let text = |index: usize| BYTES_AT + 3 * bytes + 2 * ((bytes.max(1) - 1) / 8) + 1 + index;
        let shown = |from: usize, to: usize| {
            let (from, to) = (
                from.saturating_sub(left),
                to.saturating_sub(left).min(width),
            );
            (to > from).then_some((from, to))
        };
        let end = start + count as u64;
        let mut cells = Vec::new();
        for &at in edit.changed.range(start..end).map(|(at, _)| at) {
            let index = (at - start) as usize;
            cells.extend(shown(hex(index), hex(index) + 2));
            cells.extend(shown(text(index), text(index) + 1));
        }
        screen.spans.push(cells);
        let pos = edit.cursor.pos;
        if (start..start + bytes as u64).contains(&pos) {
            let index = (pos - start) as usize;
            let col = match edit.cursor.text {
                true => text(index),
                false => hex(index) + usize::from(edit.cursor.low),
            };
            let row = screen.rows.len() - 1;
            screen.cursor = (col >= left && col - left < width).then(|| (row, col - left));
        }
    }
}
