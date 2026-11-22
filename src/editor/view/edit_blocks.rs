//! FAR's block keys in the editor (S2d.6f; `Editor::ProcessKeyInternal`): Ctrl+D takes the block
//! out, Ctrl+P puts a copy of it at the cursor, Ctrl+M moves it there (FAR's Ctrl+Ins / Shift+Del then
//! Shift+Ins through an inner clipboard), Alt+U / Alt+I shift its lines (or the cursor's line) a
//! column left or right (`BlockLeft`, `BlockRight`: one undo step, the block kept).

use super::{EditView, Line, Missing, Step};

impl EditView {
    /// The block keys' steps, a vertical block's own when there is one (Del and BS take it out with
    /// "Del removes blocks", FAR's `DeleteBlock`); none: not one of them.
    pub(super) fn block_step(
        &mut self,
        step: &Step,
        block: Option<(u64, u64)>,
    ) -> Option<Result<(), Missing>> {
        let column = self.column.is_some();
        Some(match step {
            Step::DeleteBlock | Step::Cut if column => self.delete_column(),
            Step::Delete | Step::Backspace if column && self.options.del_removes_blocks => {
                self.delete_column()
            }
            Step::DeleteBlock => self.delete_block(),
            Step::Cut => block.map_or(Ok(()), |block| self.take_out(block)),
            Step::CopyBlock(moving) if column => self.copy_column(*moving),
            Step::CopyBlock(moving) => self.copy_block(*moving),
            Step::ShiftBlock(right) if column => self.shift_column(*right),
            Step::ShiftBlock(right) => self.shift_block(*right),
            Step::PasteColumn(text) => self.paste_column(text),
            _ => return self.plugin_step(step),
        })
    }

    /// Ctrl+D: the block taken out.
    pub(super) fn delete_block(&mut self) -> Result<(), Missing> {
        match self.block()? {
            Some(block) => self.take_out(block),
            None => Ok(()),
        }
    }

    /// Ctrl+P / Ctrl+M: the block's text put in at the cursor (the moved block's taken out first, the
    /// cursor kept on its text); FAR's paste: the copy is the block with persistent blocks, else none.
    pub(super) fn copy_block(&mut self, moving: bool) -> Result<(), Missing> {
        let (Some((from, to)), Some(text)) = (self.block()?, self.block_text()?) else {
            return Ok(());
        };
        if moving {
            let line = self.read(self.line)?;
            let (pos, pad) = self.insert_point(&line)?;
            let target = match pos {
                pos if pos >= to => pos - (to - from),
                pos if pos > from => from,
                pos => pos,
            };
            self.take_out((from, to))?;
            self.line = self.line_holding(target)?;
            self.col = Self::col_of(&self.read(self.line)?, target) + pad.chars().count();
        }
        self.anchor = None;
        let line = self.read(self.line)?;
        let (pos, pad) = self.insert_point(&line)?;
        let start = pos + self.encoding.encode(&pad).len() as u64;
        self.put_in(&text, None)?;
        if self.options.persistent_blocks {
            self.mark_from(start);
            self.fix_block()?;
        }
        Ok(())
    }

    /// Alt+U / Alt+I: FAR's `BlockLeft` / `BlockRight` - each line of the block with something of it
    /// marked (no block: the cursor's line) a column left (its leading blank out, a tab as one column
    /// less of spaces) or right (a space before a line that is not empty); one undo step.
    pub(super) fn shift_block(&mut self, right: bool) -> Result<(), Missing> {
        let block = self.block()?;
        let lines = self.block_lines(block)?;
        self.doc.begin_group();
        let done = self.shift_lines(&lines, block, right);
        self.doc.end_group();
        done
    }

    /// The lines of `shift_block`, bottom up; the cursor kept on its text.
    fn shift_lines(
        &mut self,
        lines: &[Line],
        block: Option<(u64, u64)>,
        right: bool,
    ) -> Result<(), Missing> {
        let cursor_line = self.line;
        let (mut line_shift, mut col) = (0_i64, self.col);
        for line in lines.iter().rev() {
            if block.is_some_and(|(_, to)| to <= line.start) {
                continue;
            }
            let delta = self.shift_line(line, right)?;
            if delta == 0 {
                continue;
            }
            if line.start < cursor_line {
                line_shift += delta;
            } else if line.start == cursor_line {
                col = if right {
                    col + 1
                } else {
                    col.saturating_sub(1)
                };
            }
        }
        self.line = cursor_line.saturating_add_signed(line_shift);
        self.col = col;
        Ok(())
    }

    /// The lines a block covers, top down (none: the cursor's).
    pub(super) fn block_lines(&self, block: Option<(u64, u64)>) -> Result<Vec<Line>, Missing> {
        let (first, last) = match block {
            Some((from, to)) => (self.line_holding(from)?, self.line_holding(to)?),
            None => (self.line, self.line),
        };
        let mut lines = Vec::new();
        let mut at = first;
        loop {
            let line = self.read(at)?;
            let next = self.next_line(&line)?;
            lines.push(line);
            match next {
                Some(next) if at < last => at = next,
                _ => return Ok(lines),
            }
        }
    }

    /// One line shifted; the bytes it grew by (less: shrank).
    fn shift_line(&mut self, line: &Line, right: bool) -> Result<i64, Missing> {
        let first = line.chars.first().copied();
        let (taken, put) = match (right, first) {
            (true, Some(_)) => (String::new(), " ".to_string()),
            (false, Some(' ')) => (" ".to_string(), String::new()),
            (false, Some('\t')) => ("\t".to_string(), " ".repeat(self.tab_size() - 1)),
            _ => return Ok(0),
        };
        let (taken, put) = (self.encoding.encode(&taken), self.encoding.encode(&put));
        if !taken.is_empty() {
            self.delete_bytes(line.start, taken.len() as u64);
        }
        if !put.is_empty() {
            self.insert_bytes(line.start, &put);
        }
        self.changed_at(line.start);
        Ok(put.len() as i64 - taken.len() as i64)
    }
}
