//! The editor's screen: its rows from the top, the cursor and the block on them (a vertical block's
//! columns past a line's end too, as FAR's), FAR's line break glyphs with "Show white space".

use super::EditView;
use crate::editor::view::blocks::{Bytes, Missing};
use crate::editor::view::lines::Line;

/// What the screen shows: the rows from the top, the cursor's place on them, the block's cells.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditScreen {
    pub rows: Vec<String>,
    pub marks: Vec<Option<(usize, usize)>>,
    /// More cells marked on each row (the hex mode's bytes changed and not saved).
    pub spans: Vec<Vec<(usize, usize)>>,
    /// The cursor's row and column on the screen, when it is on it.
    pub cursor: Option<(usize, usize)>,
    pub missing: Option<Missing>,
    /// The code mode's marks (`edit_code_marks`): the bracket at the cursor and its pair (row,
    /// column), the word under the cursor each time it is on the screen (row, from, to).
    pub pairs: Vec<(usize, usize)>,
    pub words: Vec<(usize, usize, usize)>,
    /// The first column shown (the ruler's place).
    pub left: usize,
}

impl EditView {
    /// The rows of a `width` by `height` screen, the cursor and the block on them.
    pub fn screen(&self, width: usize, height: usize) -> EditScreen {
        if let Some(hex) = &self.hex {
            return self.hex_screen(hex, width, height);
        }
        let mut screen = EditScreen {
            left: self.left,
            ..EditScreen::default()
        };
        let block = self.block().ok().flatten().or(self.found);
        let column = self.column_span().ok().flatten();
        let mut start = self.top;
        let mut lines = Vec::new();
        while screen.rows.len() < height {
            let line = match self.read(start) {
                Ok(line) => line,
                Err(missing) => {
                    screen.missing = Some(missing);
                    break;
                }
            };
            if start == self.line {
                // On a wide character's tail (Up / Down keep the column): on its first cell.
                let at = line.char_start_cell(self.col);
                let col = at.checked_sub(self.left).filter(|col| *col < width);
                screen.cursor = col.map(|col| (screen.rows.len(), col));
            }
            let mut row = line.text(self.left, width);
            if self.layout.white {
                row.push_str(&self.break_glyphs(&line));
            }
            screen.rows.push(row);
            screen.marks.push(match &column {
                Some(column) => self.column_marked(start, column, width),
                None => block.and_then(|block| self.marked(&line, block, width)),
            });
            let next = self.next_line(&line);
            if self.options.code_editing {
                lines.push(line);
            }
            match next {
                Ok(Some(next)) => start = next,
                Ok(None) => break,
                Err(missing) => {
                    screen.missing = Some(missing);
                    break;
                }
            }
        }
        if self.options.code_editing {
            self.code_marks(&lines, width, &mut screen);
        }
        screen
    }

    /// FAR's line break glyphs with "Show white space": `♪` a CR, `◙` an LF.
    pub(super) fn break_glyphs(&self, line: &Line) -> String {
        let Ok(end) = self.text_end(line) else {
            return String::new();
        };
        let unit = self.encoding.unit();
        let mut glyphs = String::new();
        let mut at = end;
        while at < line.end {
            let Ok(bytes) = self.doc.at(at) else { break };
            let Some(value) = bytes
                .get(..unit as usize)
                .map(|bytes| self.encoding.unit_value(bytes))
            else {
                break;
            };
            glyphs.push(if value == 0x0D { '♪' } else { '◙' });
            at += unit;
        }
        glyphs
    }

    /// The cells of `line` the block covers, from the screen's first column.
    pub(super) fn marked(
        &self,
        line: &Line,
        (from, to): (u64, u64),
        width: usize,
    ) -> Option<(usize, usize)> {
        let first = Self::col_of(line, from).max(self.left);
        let past = Self::col_of(line, to).min(self.left + width);
        let past = if to > self.text_end(line).ok()? {
            past.max(first + 1)
        } else {
            past
        };
        (past > first && from < line.end && to > line.start)
            .then(|| (first - self.left, past - self.left))
    }
}
