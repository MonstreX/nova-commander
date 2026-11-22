//! What the block keys do to FAR's vertical block (S2d.6f): its text (`VBlock2Text`: each line's
//! columns, padded with spaces, a line break after each), taken out (`DeleteVBlock`), put in at the
//! cursor (`VPaste`: each line of the text at the cursor's column on the next line down, lines added
//! at the file's end), shifted a column (`VBlockShift`: the character beside it moved over to its other
//! side), copied or moved with Ctrl+P / Ctrl+M. Each is one undo step.

use super::columns::Column;
use super::{EditView, Missing};
use crate::editor::view::lines::Line;

impl EditView {
    /// The vertical block's text (FAR's `VBlock2Text`): each line's columns, spaces where it is
    /// short, a break after each.
    pub(super) fn column_text(&self, column: &Column) -> Result<String, Missing> {
        let mut text = String::new();
        for line in self.block_lines(Some((column.first, column.last)))? {
            let from = Self::char_at(&line, column.x);
            let to = Self::char_at(&line, column.x + column.width);
            let len = line.chars.len();
            let copied = line.chars.get(from..to.min(len)).unwrap_or_default();
            text.extend(copied);
            text.push_str(&" ".repeat(to - from - copied.len()));
            text.push_str(&self.eol);
        }
        Ok(text)
    }

    /// The byte of character `index` of `line` (its text's end past the last).
    pub(super) fn char_byte(&self, line: &Line, index: usize) -> Result<u64, Missing> {
        match line.char_cells.get(index) {
            Some(&cell) => self.offset_at(line, cell),
            None => self.text_end(line),
        }
    }

    /// FAR's `DeleteVBlock`: each line's columns taken out (the cursor to the block's top left first
    /// without persistent blocks, when it is not above it), the block let go.
    pub(super) fn delete_column(&mut self) -> Result<(), Missing> {
        let Some(column) = self.column_span()? else {
            return Ok(());
        };
        if column.width == 0 {
            self.column = None;
            return Ok(());
        }
        let lines = self.block_lines(Some((column.first, column.last)))?;
        if !self.options.persistent_blocks && self.line >= column.first {
            (self.line, self.col) = (column.first, column.x);
        }
        self.doc.begin_group();
        let done = self.delete_column_lines(&lines, &column);
        self.doc.end_group();
        self.column = None;
        done
    }

    fn delete_column_lines(&mut self, lines: &[Line], column: &Column) -> Result<(), Missing> {
        for line in lines.iter().rev() {
            let from = Self::char_at(line, column.x);
            let len = line.chars.len();
            if from >= len {
                continue;
            }
            let to = Self::char_at(line, column.x + column.width).min(len);
            let (at, past) = (self.char_byte(line, from)?, self.char_byte(line, to)?);
            let cursor = (line.start == self.line).then(|| Self::char_at(line, self.col));
            self.delete_bytes(at, past - at);
            self.changed_at(at);
            if let Some(cursor) = cursor {
                // FAR: the cursor past the columns comes back by them, not before their start.
                let cursor = match cursor > from {
                    true => cursor.saturating_sub(to - from).max(from),
                    false => cursor,
                };
                self.col = Self::cell_at(&self.read(line.start)?, cursor);
            }
        }
        Ok(())
    }

    /// The column of character `index` of `line` (past its end: one a column).
    fn cell_at(line: &Line, index: usize) -> usize {
        match line.char_cells.get(index) {
            Some(&cell) => cell,
            None => line.cells.len() + (index - line.chars.len()),
        }
    }

    /// FAR's `VPaste`: each line of `text` put in at the cursor's column, the next one on the line
    /// below (lines added past the file's end); the cursor back at the top left, the pasted block
    /// marked with persistent blocks (FAR's Shift+Ins lets it go without them).
    pub(super) fn paste_column(&mut self, text: &str) -> Result<(), Missing> {
        let stream = self.block()?;
        self.doc.begin_group();
        let done = self.paste_column_lines(text, stream);
        self.doc.end_group();
        done
    }

    fn paste_column_lines(
        &mut self,
        text: &str,
        stream: Option<(u64, u64)>,
    ) -> Result<(), Missing> {
        if let Some(block) = stream.filter(|_| !self.options.persistent_blocks) {
            self.take_out(block)?;
        }
        (self.anchor, self.fixed, self.column) = (None, None, None);
        let overtype = std::mem::replace(&mut self.overtype, false);
        let (first, start) = (self.line, self.col);
        let (mut width, mut height) = (0, 0);
        let mut rest = text;
        loop {
            let piece_end = rest.find(['\r', '\n']).unwrap_or(rest.len());
            if piece_end > 0 {
                self.put_in(&rest[..piece_end], None)?;
            }
            if piece_end == rest.len() {
                break;
            }
            let brk = if rest[piece_end..].starts_with("\r\n") {
                2
            } else {
                1
            };
            rest = &rest[piece_end + brk..];
            width = width.max(self.col.saturating_sub(start));
            height += 1;
            self.next_paste_line(start, !rest.is_empty())?;
        }
        self.overtype = overtype;
        width = width.max(self.col.saturating_sub(start));
        let mut last = first;
        for _ in 1..height.max(1) {
            match self.next_line(&self.read(last)?)? {
                Some(next) => last = next,
                None => break,
            }
        }
        (self.line, self.col) = (first, start);
        self.column = (self.options.persistent_blocks).then_some(Column {
            first,
            last,
            x: start,
            width,
            marking: false,
        });
        Ok(())
    }

    /// The pasted block's next line: the line below at the block's column, or (the file's last
    /// line, more text to come) a new line with spaces up to it (FAR's End, Enter and spaces).
    fn next_paste_line(&mut self, start: usize, more: bool) -> Result<(), Missing> {
        let line = self.read(self.line)?;
        if let Some(next) = self.next_line(&line)? {
            (self.line, self.col) = (next, start);
            return Ok(());
        }
        if !more {
            return Ok(());
        }
        let end = self.text_end(&line)?;
        let bytes = self
            .encoding
            .encode(&format!("{}{}", self.eol, " ".repeat(start)));
        self.insert_bytes(end, &bytes);
        self.changed_at(end);
        self.line = self.line_holding(end + bytes.len() as u64)?;
        self.col = start;
        Ok(())
    }

    /// Ctrl+P / Ctrl+M with a vertical block: its text put in at the cursor (moved: the block taken
    /// out first, the cursor on its line back by its columns as FAR's).
    pub(super) fn copy_column(&mut self, moving: bool) -> Result<(), Missing> {
        let Some(column) = self.column_span()? else {
            return Ok(());
        };
        let text = self.column_text(&column)?;
        let inside = (column.first..=column.last).contains(&self.line);
        let col = self.col;
        self.doc.begin_group();
        let done = self.copied_column(&text, moving, (inside, col, column));
        self.doc.end_group();
        done
    }

    fn copied_column(
        &mut self,
        text: &str,
        moving: bool,
        (inside, col, column): (bool, usize, Column),
    ) -> Result<(), Missing> {
        if moving {
            self.delete_column()?;
            if inside {
                self.col = match col >= column.x + column.width {
                    true => col - column.width,
                    false => col,
                };
            }
        }
        self.paste_column(text)
    }

    /// Alt+U / Alt+I with a vertical block (FAR's `VBlockShift`): on each line the character before
    /// (after) the block moved after (before) it, the line's trailing blanks out; the block and the
    /// cursor a column over. Not left of the first column.
    pub(super) fn shift_column(&mut self, right: bool) -> Result<(), Missing> {
        let Some(mut column) = self.column_span()? else {
            return Ok(());
        };
        if (!right && column.x == 0) || column.width == 0 {
            return Ok(());
        }
        let lines = self.block_lines(Some((column.first, column.last)))?;
        self.doc.begin_group();
        let done =
            (lines.iter().rev()).try_for_each(|line| self.shift_column_line(line, &column, right));
        self.doc.end_group();
        done?;
        column.x = if right { column.x + 1 } else { column.x - 1 };
        self.col = if right {
            column.x + column.width
        } else {
            column.x
        };
        self.column = Some(column);
        Ok(())
    }

    fn shift_column_line(
        &mut self,
        line: &Line,
        column: &Column,
        right: bool,
    ) -> Result<(), Missing> {
        let (mut chars, mut cells) = (line.chars.clone(), line.char_cells.clone());
        let shown = line.cells.len();
        let span = |chars: &[char], cells: &[usize]| {
            let at = |x: usize| Self::char_in(cells, (chars.len(), shown), x);
            (at(column.x), at(column.x + column.width))
        };
        let (mut from, mut to) = span(&chars, &cells);
        if from > chars.len() || (!right && from == 0) {
            return Ok(());
        }
        let beside = if right {
            chars.get(to)
        } else {
            chars.get(from - 1)
        };
        if beside == Some(&'\t') {
            // FAR's `ReplaceTabs`: the line's tabs as the spaces they show as.
            (chars, cells) = Self::tabs_as_spaces(line);
            (from, to) = span(&chars, &cells);
        }
        let need = (to + usize::from(right)).max(chars.len());
        chars.resize(need, ' ');
        match right {
            true => chars[from..=to].rotate_right(1),
            false => chars[from - 1..to].rotate_left(1),
        }
        while chars.last().is_some_and(|ch| ch.is_whitespace()) {
            chars.pop();
        }
        if chars == line.chars {
            return Ok(());
        }
        let end = self.text_end(line)?;
        let bytes = self.encoding.encode(&chars.iter().collect::<String>());
        self.delete_bytes(line.start, end - line.start);
        self.insert_bytes(line.start, &bytes);
        self.changed_at(line.start);
        Ok(())
    }

    /// `line`'s characters with each tab the spaces it shows as, and each one's first column.
    pub(super) fn tabs_as_spaces(line: &Line) -> (Vec<char>, Vec<usize>) {
        let (mut chars, mut cells) = (Vec::new(), Vec::new());
        for (index, &ch) in line.chars.iter().enumerate() {
            let cell = line.cell_of(index);
            if ch == '\t' {
                let spaces = line.cell_of(index + 1) - cell;
                chars.extend(std::iter::repeat_n(' ', spaces));
                cells.extend(cell..cell + spaces);
            } else {
                chars.push(ch);
                cells.push(cell);
            }
        }
        (chars, cells)
    }
}
