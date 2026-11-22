//! The editor's view (S2d.4b; FAR's `Editor`): the piece table laid out as lines from the top of the
//! screen, the cursor on a line and a column of the screen - past the line's end too (FAR's default
//! "Cursor beyond end of line"), FAR's moves, the screen kept on the cursor. Its edits and blocks are
//! `edit_ops`. Every step reads what it needs first: one that needs a block not read yet changes
//! nothing and names it (the caller reads it and asks again).

use super::blocks::{Bytes, Missing};
use super::code_page::Encoding;
use super::line_count::LineCounter;
use super::lines::{Layout, Line, line_before, read_line};
use super::pieces::PieceTable;

/// The biggest document whose lines are counted for the status line (the pieces' own line counts, a
/// balanced tree of them, come later: ADR-0015 addendum).
const COUNTED: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub struct EditView {
    pub(super) doc: PieceTable,
    pub(super) encoding: Encoding,
    /// The first byte past a byte order mark.
    pub(super) floor: u64,
    /// The file's line break, put in by Enter (the first one it has; `\r\n` when none).
    pub(super) eol: String,
    /// The cursor: its line's first byte and its column on the screen.
    pub(super) line: u64,
    pub(super) col: usize,
    /// The other end of the stream block (a byte), when there is one.
    pub(super) anchor: Option<u64>,
    /// What a search found, shown marked until the next step (FAR's found text).
    pub(super) found: Option<(u64, u64)>,
    top: u64,
    left: usize,
    counter: LineCounter,
    /// The first byte changed since the highlighting last asked (ADR-0024 §4).
    changed: Option<u64>,
    /// The tab stops (the editor's settings).
    layout: Layout,
    /// How it edits (the editor's settings).
    pub options: EditOptions,
    /// FAR's lock mode (`FEDITOR_LOCKMODE`, Ctrl+L): the steps that edit are not done.
    pub locked: bool,
    /// FAR's bookmarks (Ctrl+Shift+0..9 sets one, Ctrl+0..9 goes to it).
    pub marks: [Option<Mark>; 10],
    /// FAR's overtype (`FEDITOR_OVERTYPE`, Ins): typing goes over the characters.
    pub overtype: bool,
    /// The block's other end once it no longer follows the cursor (FAR's persistent blocks: a move
    /// or an edit leaves the block where it is); none: the cursor is its end.
    fixed: Option<u64>,
    /// FAR's vertical block, when there is one (no stream block then).
    pub(super) column: Option<Column>,
    /// The hex mode (Ctrl+F4, ADR-0027), when on.
    hex: Option<hex::Hex>,
}

impl EditView {
    /// The editor on `doc` in `encoding` from `floor`, breaking lines with `eol`.
    pub fn new(doc: PieceTable, (encoding, floor): (Encoding, u64), eol: &str) -> Self {
        Self {
            doc,
            encoding,
            floor,
            eol: eol.to_string(),
            line: floor,
            col: 0,
            anchor: None,
            found: None,
            top: floor,
            left: 0,
            counter: LineCounter::default(),
            changed: None,
            layout: Layout::default(),
            options: EditOptions::default(),
            locked: false,
            marks: [None; 10],
            overtype: false,
            fixed: None,
            column: None,
            hex: None,
        }
    }

    pub fn doc(&self) -> &PieceTable {
        &self.doc
    }

    pub fn doc_mut(&mut self) -> &mut PieceTable {
        &mut self.doc
    }

    pub fn encoding(&self) -> &Encoding {
        &self.encoding
    }

    pub(super) fn read(&self, start: u64) -> Result<Line, Missing> {
        read_line(&self.doc, &self.encoding, start, self.layout)
    }

    /// The bytes of `line`'s break (none at the end of the document).
    pub(super) fn break_len(&self, line: &Line) -> Result<u64, Missing> {
        let unit = self.encoding.unit();
        if line.end <= line.start || line.end < unit {
            return Ok(0);
        }
        let last = self.unit_before(line.end)?;
        if last == u32::from(b'\n') && line.end >= line.start + 2 * unit {
            let before = self.unit_before(line.end - unit)?;
            if before == u32::from(b'\r') {
                return Ok(2 * unit);
            }
        }
        Ok(match last {
            0x0A | 0x0D => unit,
            _ => 0,
        })
    }

    fn unit_before(&self, pos: u64) -> Result<u32, Missing> {
        let unit = usize::try_from(self.encoding.unit()).unwrap_or(1);
        let bytes = self.doc.before(pos)?;
        if bytes.len() >= unit {
            return Ok(self.encoding.unit_value(&bytes[bytes.len() - unit..]));
        }
        // A unit across two pieces: put together.
        let mut joined = self.doc.before(pos - bytes.len() as u64)?.to_vec();
        joined.extend_from_slice(bytes);
        Ok(self
            .encoding
            .unit_value(&joined[joined.len().saturating_sub(unit)..]))
    }

    /// The first byte past `line`'s text (before its break).
    pub(super) fn text_end(&self, line: &Line) -> Result<u64, Missing> {
        Ok(line.end - self.break_len(line)?)
    }

    /// The byte at column `col` of `line`: its text's end past the last.
    pub(super) fn offset_at(&self, line: &Line, col: usize) -> Result<u64, Missing> {
        match line.offsets.get(col) {
            Some(at) => Ok(*at),
            None => self.text_end(line),
        }
    }

    /// The column of byte `pos` in `line`.
    pub(super) fn col_of(line: &Line, pos: u64) -> usize {
        (line.offsets.iter())
            .position(|at| *at >= pos)
            .unwrap_or(line.cells.len())
    }

    /// The cursor's byte (its line's text's end when the cursor is past it; the hex mode's).
    pub fn cursor_pos(&self) -> Result<u64, Missing> {
        if let Some(cursor) = self.hex_cursor() {
            return Ok(cursor.pos);
        }
        let line = self.read(self.line)?;
        self.offset_at(&line, self.col)
    }

    /// The start of the line that holds byte `pos` (FAR: the line a step put the cursor on).
    pub(super) fn line_holding(&self, pos: u64) -> Result<u64, Missing> {
        let pos = pos.clamp(self.floor, self.doc.len());
        if pos <= self.floor {
            return Ok(self.floor);
        }
        let start = line_before(&self.doc, &self.encoding, pos, self.floor, self.layout)?;
        let line = self.read(start)?;
        // `pos` is the start of the line after it (the line ends with a break there).
        if line.end <= pos && line.end > start && self.break_len(&line)? > 0 {
            return Ok(pos);
        }
        Ok(start)
    }

    /// The line after `line`, if there is one (the empty one after a last break too).
    fn next_line(&self, line: &Line) -> Result<Option<u64>, Missing> {
        let broken = self.break_len(line)? > 0;
        Ok((line.end > line.start && (line.end < self.doc.len() || broken)).then_some(line.end))
    }

    fn previous_line(&self, start: u64) -> Result<Option<u64>, Missing> {
        if start <= self.floor {
            return Ok(None);
        }
        line_before(&self.doc, &self.encoding, start, self.floor, self.layout).map(Some)
    }

    /// The last line's start (the empty one after a last break).
    fn last_line(&self) -> Result<u64, Missing> {
        let len = self.doc.len();
        if len <= self.floor {
            return Ok(self.floor);
        }
        let start = line_before(&self.doc, &self.encoding, len, self.floor, self.layout)?;
        let line = self.read(start)?;
        Ok(if self.break_len(&line)? > 0 {
            len
        } else {
            start
        })
    }

    /// FAR's word dividers for Ctrl+Left / Ctrl+Right.
    fn divider(ch: char) -> bool {
        ch.is_whitespace() || "~!%^&*()+|{}:\"<>?`-=\\[];',./".contains(ch)
    }

    /// A step that moves the cursor (none: not a move).
    pub(super) fn move_by(&mut self, step: &Step, height: usize) -> Result<bool, Missing> {
        let line = self.read(self.line)?;
        match step {
            Step::Up => self.line = self.previous_line(self.line)?.unwrap_or(self.line),
            Step::Down => self.line = self.next_line(&line)?.unwrap_or(self.line),
            Step::Left => self.left_one(&line)?,
            Step::Right => self.right_one(&line),
            Step::Home => self.col = self.home_col(&line),
            Step::End => self.col = line.cells.len(),
            Step::PageUp | Step::PageDown => self.page(step == &Step::PageDown, height)?,
            Step::Start => (self.line, self.col) = (self.floor, 0),
            Step::Finish => {
                self.line = self.last_line()?;
                self.col = self.read(self.line)?.cells.len();
            }
            Step::WordLeft | Step::WordRight => self.word(&line, step == &Step::WordRight)?,
            _ if self.move_more(&line, step, height.max(1))? => {}
            _ => return Ok(false),
        }
        if !self.options.cursor_beyond_eol {
            self.keep_in_line(&line, step)?;
        }
        Ok(true)
    }

    /// FAR's "Cursor beyond end of line" off: Right at a line's end goes to the next one's start, any
    /// other move stops at the line's end.
    fn keep_in_line(&mut self, before: &Line, step: &Step) -> Result<(), Missing> {
        let len = self.read(self.line)?.cells.len();
        if self.col <= len {
            return Ok(());
        }
        if step == &Step::Right
            && let Some(next) = self.next_line(before)?
        {
            (self.line, self.col) = (next, 0);
            return Ok(());
        }
        self.col = len;
        Ok(())
    }

    fn left_one(&mut self, line: &Line) -> Result<(), Missing> {
        if self.col > line.cells.len() {
            self.col -= 1;
        } else if self.col > 0 {
            let at = line.offsets[self.col - 1];
            self.col = Self::col_of(line, at);
        } else if let Some(previous) = self.previous_line(self.line)? {
            self.line = previous;
            self.col = self.read(previous)?.cells.len();
        }
        Ok(())
    }

    fn right_one(&mut self, line: &Line) {
        let Some(at) = line.offsets.get(self.col) else {
            // Past the end: FAR's "Cursor beyond end of line".
            self.col += 1;
            return;
        };
        self.col = (self.col..line.cells.len())
            .find(|&next| line.offsets[next] != *at)
            .unwrap_or(line.cells.len());
    }

    /// PgUp / PgDn: the cursor and the screen a screen less one row.
    fn page(&mut self, down: bool, height: usize) -> Result<(), Missing> {
        for _ in 0..height.saturating_sub(1).max(1) {
            let line = self.read(self.line)?;
            let next = match down {
                true => self.next_line(&line)?,
                false => self.previous_line(self.line)?,
            };
            let Some(next) = next else { break };
            self.line = next;
            let top = self.read(self.top)?;
            let moved = match down {
                true => self.next_line(&top)?,
                false => self.previous_line(self.top)?,
            };
            self.top = moved.unwrap_or(self.top);
        }
        Ok(())
    }

    /// Ctrl+Left / Ctrl+Right: to the next word's start, over the line's ends.
    fn word(&mut self, line: &Line, right: bool) -> Result<(), Missing> {
        let cells = &line.cells;
        let mut col = self.col.min(cells.len());
        if right {
            if col == cells.len() {
                return self.move_by(&Step::Down, 1).map(|_| self.col = 0);
            }
            while col < cells.len() && !Self::divider(cells[col]) {
                col += 1;
            }
            while col < cells.len() && Self::divider(cells[col]) {
                col += 1;
            }
        } else {
            if col == 0 {
                return self.move_by(&Step::Left, 1).map(|_| ());
            }
            while col > 0 && Self::divider(cells[col - 1]) {
                col -= 1;
            }
            while col > 0 && !Self::divider(cells[col - 1]) {
                col -= 1;
            }
        }
        self.col = col;
        Ok(())
    }

    /// The screen kept on the cursor: its line within the rows (the top moved as little as it takes),
    /// its column within the columns.
    pub fn scroll(&mut self, width: usize, height: usize) -> Result<(), Missing> {
        let height = height.max(1);
        if self.hex.is_some() {
            self.hex_scroll(height);
        } else if self.line < self.top {
            self.top = self.line;
        } else {
            let mut start = self.top;
            let mut rows = 1;
            while start < self.line && rows <= height {
                let line = self.read(start)?;
                match self.next_line(&line)? {
                    Some(next) => start = next,
                    None => break,
                }
                rows += 1;
            }
            if rows > height {
                let mut top = self.line;
                for _ in 1..height {
                    top = self.previous_line(top)?.unwrap_or(top);
                }
                self.top = top;
            }
        }
        let width = width.max(1);
        if self.col < self.left {
            self.left = self.col;
        } else if self.col >= self.left + width {
            self.left = self.col + 1 - width;
        }
        Ok(())
    }

    /// The block's bytes, in order (none without one).
    pub fn block(&self) -> Result<Option<(u64, u64)>, Missing> {
        let Some(anchor) = self.anchor else {
            return Ok(None);
        };
        let pos = match self.fixed {
            Some(end) => end,
            None => self.cursor_pos()?,
        };
        Ok((anchor != pos).then(|| (anchor.min(pos), anchor.max(pos))))
    }

    /// A block begun at `at`, its other end the cursor (a vertical block let go).
    pub(super) fn mark_from(&mut self, at: u64) {
        (self.anchor, self.fixed, self.column) = (Some(at), None, None);
    }

    /// The stream block and the vertical one let go.
    pub(super) fn unmark(&mut self) {
        (self.anchor, self.column) = (None, None);
    }

    /// Whether the block is a vertical one (its text goes to the clipboard as one).
    pub fn is_column(&self) -> bool {
        self.column.is_some()
    }

    /// FAR's persistent block left where it is: its end no longer the cursor.
    pub(super) fn fix_block(&mut self) -> Result<(), Missing> {
        if self.anchor.is_some() && self.fixed.is_none() {
            self.fixed = Some(self.cursor_pos()?);
        }
        Ok(())
    }

    /// What the status line tells: the cursor's line (from 1) and the lines (0: not counted, a
    /// document past `COUNTED` - its whole read for them would push the screen's blocks out); its
    /// column and its character's place in the line (from 1).
    pub fn place(&mut self) -> Result<(u64, u64, usize, usize), Missing> {
        let line = self.read(self.line)?;
        let (number, total) = self.counts()?;
        let chars = line.offsets[..self.col.min(line.cells.len())]
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let char = chars + self.col.saturating_sub(line.cells.len());
        Ok((number, total, self.col + 1, char + 1))
    }

    /// The cursor's line and the lines, from 1; 0 for a document past `COUNTED`.
    fn counts(&mut self) -> Result<(u64, u64), Missing> {
        if self.doc.len() > COUNTED {
            return Ok((0, 0));
        }
        let (floor, end) = (self.floor, self.doc.len());
        let number = (self.counter).line_of(&self.doc, &self.encoding, (floor, self.line))?;
        let total = (self.counter).line_of(&self.doc, &self.encoding, (floor, end))?;
        Ok((number + 1, total + 1))
    }

    /// The document changed from `pos` on (its counts moved with the edit: `insert_bytes`).
    pub(super) fn changed_at(&mut self, pos: u64) {
        self.changed = Some(self.changed.map_or(pos, |old| old.min(pos)));
    }

    pub(super) fn tab_size(&self) -> usize {
        self.layout.tab.max(1)
    }

    /// FAR's "Show white space": spaces, tabs and line breaks shown.
    pub fn set_white(&mut self, white: bool) {
        self.layout.white = white;
    }

    /// The first row's line number from 1 (0 for a document past `COUNTED`).
    pub fn top_number(&mut self) -> Result<u64, Missing> {
        if self.doc.len() > COUNTED {
            return Ok(0);
        }
        let number = (self.counter).line_of(&self.doc, &self.encoding, (self.floor, self.top))?;
        Ok(number + 1)
    }

    /// The editor's tab size (FAR's `TabSize`): the lines laid out anew.
    pub fn set_tab(&mut self, tab: usize) {
        self.layout.tab = tab.max(1);
        self.changed_at(self.floor);
    }

    /// The first byte changed since the last ask (the highlighting's states past it go).
    pub fn take_changed(&mut self) -> Option<u64> {
        self.changed.take()
    }

    /// The first line on screen, the first column shown and the document's first byte (past a byte
    /// order mark).
    pub fn window(&self) -> (u64, usize, u64) {
        (self.top, self.left, self.floor)
    }

    /// The rows of a screen `height` high: each one's line and its first cell shown.
    pub fn rows(&self, height: usize) -> Vec<(u64, usize)> {
        let mut rows = Vec::with_capacity(height);
        let mut start = Some(self.top);
        while let Some(line_start) = start.filter(|_| rows.len() < height) {
            rows.push((line_start, self.left));
            start = self
                .read(line_start)
                .and_then(|line| self.next_line(&line))
                .ok()
                .flatten();
        }
        rows
    }

    /// The character under the cursor (none past its line's end).
    pub fn char_at_cursor(&self) -> Result<Option<char>, Missing> {
        let line = self.read(self.line)?;
        Ok(line.cells.get(line.char_start_cell(self.col)).copied())
    }

    /// Another code page for the document as it is (FAR's reading again in it): from the top.
    pub fn set_encoding(&mut self, encoding: Encoding) {
        self.encoding = encoding;
        self.changed_at(self.floor);
        (self.line, self.col, self.top, self.left) = (self.floor, 0, self.floor, 0);
        self.unmark();
        self.found = None;
    }

    /// What a search found: the cursor at its start (FAR's default "Cursor at the end" off), it
    /// marked until the next step.
    pub fn show_found(&mut self, at: u64, size: u64) -> Result<(), Missing> {
        // FAR's "Cursor at the end" and "Select found".
        let cursor = if self.options.cursor_at_end {
            at + size
        } else {
            at
        };
        let line = self.line_holding(cursor)?;
        self.col = Self::col_of(&self.read(line)?, cursor);
        self.line = line;
        if self.options.select_found {
            self.mark_from(if self.options.cursor_at_end {
                at
            } else {
                at + size
            });
            self.found = None;
        } else {
            self.anchor = None;
            self.found = Some((at, at + size));
        }
        Ok(())
    }

    /// The cursor's line's characters and the cursor's place among them (past the end: beyond it).
    pub fn cursor_line(&self) -> Result<(Vec<char>, usize), Missing> {
        let line = self.read(self.line)?;
        let inside = (line.char_cells.iter())
            .take_while(|cell| **cell < self.col)
            .count();
        let beyond = self.col.saturating_sub(line.cells.len());
        Ok((line.chars, inside + beyond))
    }

    /// The cursor's line's first byte and its column.
    pub fn cursor(&self) -> (u64, usize) {
        (self.line, self.col)
    }
}

impl super::LineSource for EditView {
    fn line_at(&self, start: u64) -> Result<Line, Missing> {
        self.read(start)
    }

    fn line_after(&self, line: &Line) -> Result<Option<u64>, Missing> {
        self.next_line(line)
    }

    fn line_above(&self, start: u64) -> Result<Option<u64>, Missing> {
        self.previous_line(start)
    }

    fn size(&self) -> u64 {
        self.doc.size()
    }
}

#[path = "edit_step.rs"]
mod step;
pub use step::Step;
#[path = "edit_blocks.rs"]
mod blocks;
#[path = "edit_code.rs"]
mod code;
#[path = "edit_code_marks.rs"]
mod code_marks;
#[path = "edit_code_more.rs"]
mod code_more;
#[path = "edit_column_ops.rs"]
mod column_ops;
#[path = "edit_columns.rs"]
mod columns;
pub use columns::Column;
#[path = "edit_hex.rs"]
mod hex;
#[path = "edit_marks.rs"]
mod marks;
#[path = "edit_moves.rs"]
mod moves;
#[path = "edit_ops.rs"]
mod ops;
#[path = "edit_plugin.rs"]
mod plugin;
pub use plugin::{Place, Range};
#[path = "edit_words.rs"]
mod words;
pub use marks::Mark;
#[path = "edit_options.rs"]
mod options;
#[path = "edit_screen.rs"]
mod screen;
pub use options::EditOptions;
pub use screen::EditScreen;

#[cfg(test)]
#[path = "edit_columns_tests.rs"]
mod column_tests;
#[cfg(test)]
#[path = "edit_view_tests.rs"]
mod tests;
