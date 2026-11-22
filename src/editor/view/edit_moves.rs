//! FAR's editor moves beyond the basic set (S2d.6f; `Editor::ProcessKeyInternal`): Ctrl+Up /
//! Ctrl+Down scroll the screen with the cursor (`ScrollUp`, `ScrollDown`), Ctrl+N / Ctrl+E put the
//! cursor on the screen's first / last row, Ctrl+PgUp / Ctrl+PgDn go to the first / last line keeping
//! the column; a click puts the cursor on the row and column clicked (`Editor::ProcessMouse`,
//! `Edit::ProcessMouse`'s `SetTabCurPos`).

use super::{EditView, Line, Missing, Step};

impl EditView {
    /// One of these moves done; false for any other step.
    pub(super) fn move_more(
        &mut self,
        line: &Line,
        step: &Step,
        height: usize,
    ) -> Result<bool, Missing> {
        match step {
            Step::ScrollUp => self.scroll_up()?,
            Step::ScrollDown => self.scroll_down(line, height)?,
            Step::ScreenTop => self.line = self.top,
            Step::Click(row, col) => {
                (self.line, self.col) = (self.lines_down(self.top, *row)?, self.left + col);
            }
            Step::ScreenBottom => self.line = self.lines_down(self.top, height - 1)?,
            Step::FirstLine => (self.line, self.top) = (self.floor, self.floor),
            Step::LastLine => {
                self.line = self.last_line()?;
                let mut top = self.line;
                for _ in 1..height.max(1) {
                    top = self.previous_line(top)?.unwrap_or(top);
                }
                (self.top, self.left) = (top, 0);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// FAR's `ScrollUp`: the screen and the cursor a line up; with the first line on the screen, the
    /// cursor alone.
    fn scroll_up(&mut self) -> Result<(), Missing> {
        let Some(above) = self.previous_line(self.line)? else {
            return Ok(());
        };
        if let Some(top) = self.previous_line(self.top)? {
            self.top = top;
        }
        self.line = above;
        Ok(())
    }

    /// FAR's `ScrollDown`: the screen and the cursor a line down; with the last line on the screen,
    /// the cursor alone (FAR's `AllowEmptySpaceAfterEof` off); nothing on the last line.
    fn scroll_down(&mut self, line: &Line, height: usize) -> Result<(), Missing> {
        let Some(below) = self.next_line(line)? else {
            return Ok(());
        };
        let bottom = self.lines_down(self.top, height)?;
        let room = bottom != self.lines_down(self.top, height - 1)?;
        if room && let Some(top) = self.next_line(&self.read(self.top)?)? {
            self.top = top;
        }
        self.line = below;
        Ok(())
    }

    /// The line `count` lines below `start`, or the last one.
    fn lines_down(&self, start: u64, count: usize) -> Result<u64, Missing> {
        let mut at = start;
        for _ in 0..count {
            match self.next_line(&self.read(at)?)? {
                Some(next) => at = next,
                None => break,
            }
        }
        Ok(at)
    }
}
