//! FAR's vertical blocks in the editor (S2d.6f; `Editor::ProcessKeyInternal` with Alt+Shift or Alt
//! and a move, `ProcessVBlockMarking`, `AdjustVBlock`): the block is its first and last lines and a
//! run of screen columns; the cursor stays on one of its corners while it is marked. Its lines are
//! kept by their first bytes, moved by the text put in or taken out before them as the bookmarks are;
//! Enter on its last line takes the new line in and Ctrl+Y takes a line out, as FAR's counts do.

use super::{EditView, Missing, Step};
use crate::editor::view::lines::Line;

/// FAR's vertical block (`BTYPE_COLUMN`): its first and last lines by their first bytes, its first
/// column and its columns (FAR's `VBlockX`, `VBlockSizeX`; its lines are FAR's `VBlockSizeY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    pub first: u64,
    pub last: u64,
    pub x: usize,
    pub width: usize,
    /// FAR's `FEDITOR_MARKINGVBLOCK`: the next Alt+Shift move goes on marking this block.
    pub marking: bool,
}

impl EditView {
    /// Alt+Shift (or Alt) with a move, Ctrl+Alt with a word or the file's end: FAR's vertical block
    /// marked from where the cursor was.
    pub(super) fn column_move(&mut self, step: &Step, height: usize) -> Result<(), Missing> {
        match step {
            Step::Left => self.column_left().map(drop),
            Step::Right => self.column_right().map(drop),
            Step::Up => self.column_up(),
            Step::Down => self.column_down(),
            Step::Home => {
                while self.col > 0 && self.column_left()? {}
                Ok(())
            }
            Step::End => self.column_end(),
            Step::PageUp | Step::PageDown => {
                for _ in 0..height.saturating_sub(1).max(1) {
                    match step {
                        Step::PageUp => self.column_up()?,
                        _ => self.column_down()?,
                    }
                }
                Ok(())
            }
            Step::WordLeft => self.column_word_left(),
            Step::WordRight => self.column_word_right(),
            Step::Start | Step::Finish => self.column_to_end(step == &Step::Finish),
            _ => Ok(()),
        }
    }

    /// FAR's `ProcessVBlockMarking`: the block marked, or a new one begun at the cursor (the stream
    /// block let go).
    fn column_marking(&mut self) -> Column {
        match self.column {
            Some(column) if column.marking => column,
            _ => {
                (self.anchor, self.fixed) = (None, None);
                Column {
                    first: self.line,
                    last: self.line,
                    x: self.col,
                    width: 0,
                    marking: true,
                }
            }
        }
    }

    /// The block `delta` columns wider (less: narrower) at its right side, or at its left one (FAR's
    /// `VBlockX` / `VBlockSizeX`; a block passed over turns).
    fn column_widened(column: &mut Column, at_right: bool, delta: i64) {
        let (mut x, mut width) = (column.x as i64, column.width as i64);
        if at_right {
            width += delta;
        } else {
            x -= delta;
            width += delta;
        }
        if width < 0 {
            width = -width;
            x -= width;
        }
        (column.x, column.width) = (x.max(0) as usize, width.max(0) as usize);
    }

    /// Alt+Shift+Left: the cursor a character left within its line, the block with it.
    fn column_left(&mut self) -> Result<bool, Missing> {
        if self.col == 0 {
            return Ok(false);
        }
        let mut column = self.column_marking();
        let line = self.read(self.line)?;
        let before = self.col;
        self.left_one(&line)?;
        let delta = (before - self.col) as i64;
        if before > column.x {
            Self::column_widened(&mut column, true, -delta);
        } else {
            Self::column_widened(&mut column, false, delta);
        }
        self.column = Some(column);
        Ok(true)
    }

    /// Alt+Shift+Right: the cursor a character right (past the line's end only with "Cursor beyond
    /// end of line"), the block with it.
    fn column_right(&mut self) -> Result<bool, Missing> {
        let line = self.read(self.line)?;
        if !self.options.cursor_beyond_eol && self.col >= line.cells.len() {
            return Ok(false);
        }
        let mut column = self.column_marking();
        let before = self.col;
        self.right_one(&line);
        let delta = (self.col - before) as i64;
        let at_right = before >= column.x + column.width;
        if at_right {
            Self::column_widened(&mut column, true, delta);
        } else {
            Self::column_widened(&mut column, false, -delta);
        }
        self.column = Some(column);
        Ok(true)
    }

    /// Alt+Shift+Up: the block a line taller at its top or shorter at its bottom, the cursor up.
    fn column_up(&mut self) -> Result<(), Missing> {
        let Some(previous) = self.previous_line(self.line)? else {
            return Ok(());
        };
        let mut column = self.column_marking();
        self.column = Some(column);
        if !self.options.cursor_beyond_eol && column.x >= self.read(previous)?.cells.len() {
            return Ok(());
        }
        if self.line > column.first {
            column.last = previous;
        } else {
            column.first = previous;
        }
        let before = self.col;
        self.move_by(&Step::Up, 1)?;
        Self::column_adjusted(&mut column, self.col, before);
        self.column = Some(column);
        Ok(())
    }

    /// Alt+Shift+Down: the block a line taller at its bottom or shorter at its top, the cursor down.
    fn column_down(&mut self) -> Result<(), Missing> {
        let line = self.read(self.line)?;
        let Some(next) = self.next_line(&line)? else {
            return Ok(());
        };
        let mut column = self.column_marking();
        self.column = Some(column);
        if !self.options.cursor_beyond_eol && column.x >= self.read(next)?.cells.len() {
            return Ok(());
        }
        if self.line >= column.last {
            column.last = next;
        } else {
            column.first = next;
        }
        let before = self.col;
        self.move_by(&Step::Down, 1)?;
        Self::column_adjusted(&mut column, self.col, before);
        self.column = Some(column);
        Ok(())
    }

    /// FAR's `AdjustVBlock`: the cursor's column changed by the line it went to (a tab, a shorter
    /// line) changes the block's side it is on.
    fn column_adjusted(column: &mut Column, x: usize, before: usize) {
        if x == column.x + column.width {
            return;
        }
        if x > column.x {
            column.width = x - column.x;
        } else if x < column.x {
            let right = column.x;
            column.width = if before > column.x {
                right - x
            } else {
                column.width + right - x
            };
            column.x = x;
        } else if x != before {
            column.width = 0;
        }
    }

    /// Alt+Shift+End: to the line's end, right or back left.
    fn column_end(&mut self) -> Result<(), Missing> {
        let len = self.read(self.line)?.cells.len();
        while self.col < len && self.column_right()? {}
        while self.col > len && self.column_left()? {}
        Ok(())
    }

    /// Ctrl+Alt+Left: FAR's word left for the block - the blanks and dividers before the cursor over,
    /// then the word.
    fn column_word_left(&mut self) -> Result<(), Missing> {
        let mut blanks = true;
        loop {
            let line = self.read(self.line)?;
            while self.col > line.cells.len() && self.column_left()? {}
            if self.col == 0 {
                return Ok(());
            }
            let before = line.char_cells.partition_point(|&cell| cell < self.col);
            let ch = line.chars.get(before.saturating_sub(1)).copied();
            let divider = ch.is_some_and(Self::divider);
            if divider && !blanks {
                return Ok(());
            }
            blanks &= divider;
            self.column_left()?;
        }
    }

    /// Ctrl+Alt+Right: FAR's word right for the block - the blanks and dividers at the cursor over,
    /// then the word, within the line.
    fn column_word_right(&mut self) -> Result<(), Missing> {
        let mut blanks = true;
        loop {
            let line = self.read(self.line)?;
            if self.col >= line.cells.len() {
                return Ok(());
            }
            let at = Self::char_at(&line, self.col);
            let divider = line.chars.get(at).copied().is_some_and(Self::divider);
            if divider && !blanks {
                return Ok(());
            }
            blanks &= divider;
            if !self.column_right()? {
                return Ok(());
            }
        }
    }

    /// Ctrl+Alt+PgUp / Ctrl+Alt+Home, Ctrl+Alt+PgDn / Ctrl+Alt+End: the block to the file's first or
    /// last line.
    fn column_to_end(&mut self, down: bool) -> Result<(), Missing> {
        loop {
            let before = self.line;
            match down {
                true => self.column_down()?,
                false => self.column_up()?,
            }
            if self.line == before {
                return Ok(());
            }
        }
    }

    /// FAR's `VisualPosToReal`: the character at column `x` of `line` (past its end: one a column).
    pub(super) fn char_at(line: &Line, x: usize) -> usize {
        Self::char_in(&line.char_cells, (line.chars.len(), line.cells.len()), x)
    }

    /// The character at column `x` of characters starting at `cells` (`chars` of them over `shown`
    /// cells; past them one a column): the first of those sharing its cell (a combining one joins
    /// the character before it, ADR-0025).
    pub(super) fn char_in(cells: &[usize], (chars, shown): (usize, usize), x: usize) -> usize {
        if x >= shown {
            return chars + (x - shown);
        }
        let last = cells.partition_point(|&cell| cell <= x).saturating_sub(1);
        let cell = cells.get(last).copied().unwrap_or(0);
        cells.partition_point(|&start| start < cell)
    }

    /// The vertical block with its lines' first bytes found again (an edit may have left one inside a
    /// line); none without one.
    pub(super) fn column_span(&self) -> Result<Option<Column>, Missing> {
        let Some(mut column) = self.column else {
            return Ok(None);
        };
        column.first = self.line_holding(column.first)?;
        column.last = self.line_holding(column.last)?.max(column.first);
        Ok(Some(column))
    }

    /// The cells of the line starting at `start` the vertical block covers, from the screen's first
    /// column.
    pub(super) fn column_marked(
        &self,
        start: u64,
        column: &Column,
        width: usize,
    ) -> Option<(usize, usize)> {
        if column.width == 0 || start < column.first || start > column.last {
            return None;
        }
        let first = column.x.max(self.left);
        let past = (column.x + column.width).min(self.left + width);
        (past > first).then(|| (first - self.left, past - self.left))
    }

    /// The vertical block as the cursor's line goes: Enter on its last line takes the new line in,
    /// Ctrl+Y on its last line takes that line off (on its only line: the block goes).
    pub(super) fn column_line_added(&mut self, broken: u64, new: u64) {
        if let Some(column) = self.column.as_mut()
            && column.last == broken
        {
            column.last = new;
        }
    }

    pub(super) fn column_line_taken(&mut self, line: u64) -> Result<(), Missing> {
        let Some(column) = self.column_span()? else {
            return Ok(());
        };
        if line != column.last {
            return Ok(());
        }
        if column.first == column.last {
            self.column = None;
            return Ok(());
        }
        let above = self.previous_line(line)?.unwrap_or(column.first);
        self.column = Some(Column {
            last: above,
            ..column
        });
        Ok(())
    }
}
