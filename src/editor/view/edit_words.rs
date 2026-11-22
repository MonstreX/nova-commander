//! FAR's line keys in the editor (S2d.6f; `Editor::ProcessKeyInternal` and the line's
//! `Edit::ProcessKey`): Ctrl+BS the word before the cursor (at a line's start: BS), Ctrl+Del /
//! Ctrl+T the word after it, Ctrl+Shift+BS the line to the cursor, Ctrl+K the line from it; Ins's
//! overtype: a character typed over the one at the cursor.

use super::{EditView, Line, Missing};

/// FAR's `IsWordDiv` with the editor's `WordDiv`: NUL or a divider (the blanks are the stop rule's).
fn divider(ch: char) -> bool {
    ch == '\0' || crate::editor::view::search::WORD_DIVIDERS.contains(ch)
}

fn blank(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

impl EditView {
    /// The first byte of character `index` of `line` (its text's end past the last one).
    fn char_offset(&self, line: &Line, index: usize) -> Result<u64, Missing> {
        match line.char_cells.get(index) {
            Some(&cell) => Ok(line.offsets[cell]),
            None => self.text_end(line),
        }
    }

    /// Characters `from`..`to` of the cursor's line taken out, the cursor at `from`.
    fn take_chars(&mut self, line: &Line, (from, to): (usize, usize)) -> Result<(), Missing> {
        let (start, end) = (self.char_offset(line, from)?, self.char_offset(line, to)?);
        if end > start {
            self.delete_bytes(start, end - start);
            self.changed_at(start);
        }
        self.col = Self::col_of(&self.read(self.line)?, start);
        Ok(())
    }

    /// FAR's Ctrl+BS: the block (without persistent blocks), at a line's start BS, else the word
    /// before the cursor up to where blanks begin or end or a divider stands.
    pub(super) fn delete_word_left(&mut self, block: Option<(u64, u64)>) -> Result<(), Missing> {
        let (chars, cursor) = self.cursor_line()?;
        let at_start = cursor == 0 && self.line > self.floor;
        if block.is_some() || at_start {
            return self.backspace(block);
        }
        let line = self.read(self.line)?;
        let end = cursor.min(chars.len());
        let mut at = end;
        while at > 0 {
            let stop = at > 1 && blank(chars[at - 1]) != blank(chars[at - 2]);
            at -= 1;
            if at == 0 || stop || divider(chars[at - 1]) {
                break;
            }
        }
        self.take_chars(&line, (at, end))
    }

    /// FAR's Ctrl+Del / Ctrl+T: the word after the cursor up to the next word or a divider; nothing at
    /// the line's end.
    pub(super) fn delete_word_right(&mut self) -> Result<(), Missing> {
        let (chars, cursor) = self.cursor_line()?;
        if cursor >= chars.len() {
            return Ok(());
        }
        let mut rest: Vec<char> = chars[cursor..].to_vec();
        let mut taken = 0;
        loop {
            let stop = rest.len() > 1 && blank(rest[0]) && !blank(rest[1]);
            rest.remove(0);
            taken += 1;
            if rest.is_empty() || stop || divider(rest[0]) {
                break;
            }
        }
        let line = self.read(self.line)?;
        self.take_chars(&line, (cursor, cursor + taken))
    }

    /// FAR's Ctrl+Shift+BS (from the line's start, the cursor at it) and Ctrl+K (to the line's end).
    pub(super) fn delete_line_part(&mut self, to_end: bool) -> Result<(), Missing> {
        let (chars, cursor) = self.cursor_line()?;
        let cursor = cursor.min(chars.len());
        let line = self.read(self.line)?;
        let col = self.col;
        let range = if to_end {
            (cursor, chars.len())
        } else {
            (0, cursor)
        };
        self.take_chars(&line, range)?;
        if to_end {
            self.col = col;
        }
        Ok(())
    }

    /// FAR's overtype (`FEDITOR_OVERTYPE`): the character at the cursor taken out before `ch` goes
    /// in; past the line's end, or over a block, typed as ever.
    pub(super) fn type_over(&mut self, ch: char, block: Option<(u64, u64)>) -> Result<(), Missing> {
        // The character taken and the one typed one undo step (review A2).
        self.doc.begin_group();
        let typed = self.type_over_grouped(ch, block);
        self.doc.end_group();
        typed
    }

    fn type_over_grouped(&mut self, ch: char, block: Option<(u64, u64)>) -> Result<(), Missing> {
        if block.is_none() {
            let (chars, cursor) = self.cursor_line()?;
            if cursor < chars.len() {
                let line = self.read(self.line)?;
                let (start, end) = (
                    self.char_offset(&line, cursor)?,
                    self.char_offset(&line, cursor + 1)?,
                );
                self.delete_bytes(start, end - start);
                self.changed_at(start);
                self.col = Self::col_of(&self.read(self.line)?, start);
            }
        }
        self.typed(&ch.to_string(), block)
    }
}
