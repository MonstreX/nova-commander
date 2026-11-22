//! The editor's code editing mode (ours over FAR; owner 2026-11-22: off by default, every key FAR's;
//! on, the ways of today's code editors): Tab / Shift+Tab indent and outdent the lines of a block over
//! several lines (Shift+Tab alone the cursor's line), Enter keeps the line's indent and adds one after an
//! opening bracket (a closing one after the cursor put on a line of its own), a closing bracket typed
//! first on a line takes one indent back, Home goes to the first character that is not blank (again: to
//! the column 0), BS in the leading spaces goes back to the previous tab stop; Ctrl+/ comments the lines
//! out or in, Alt+Up / Alt+Down move them. Each one undo step.

use super::{EditView, Line, Missing, Step};
use crate::editor::view::blocks::Bytes;

#[cfg(test)]
#[path = "edit_code_tests.rs"]
mod tests;

/// The brackets an indent opens after and their closing ones.
const BRACKETS: [(char, char); 3] = [('{', '}'), ('(', ')'), ('[', ']')];

fn blank(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

impl EditView {
    /// The code mode's steps (the new keys' at any time, the others' only with the mode on); none:
    /// not one of them.
    pub(super) fn code_step(
        &mut self,
        step: &Step,
        block: Option<(u64, u64)>,
        replaced: Option<(u64, u64)>,
    ) -> Option<Result<(), Missing>> {
        let code = self.options.code_editing;
        Some(match step {
            Step::Outdent => self.indent_lines(block, false),
            Step::ToggleComment(token) => self.toggle_comment(block, token),
            Step::MoveLines(up) => self.move_lines(block, *up),
            Step::Duplicate => self.duplicate_lines(block),
            Step::CleanUp => self.clean_up(),
            Step::Complete(typed, word) => self.complete(*typed, word),
            Step::Type('\t') if code && block.is_some() => match self.spans_lines(block) {
                Ok(true) => self.indent_lines(block, true),
                Ok(false) => return None,
                Err(missing) => Err(missing),
            },
            Step::Enter if code => self.code_enter(replaced),
            Step::Type(ch) if code && !self.overtype => self.code_typed(*ch, block, replaced)?,
            Step::Backspace if code && block.is_none() => self.smart_backspace(),
            _ => return None,
        })
    }

    /// Home in the code mode: the line's first character that is not blank, from there the column 0.
    pub(super) fn home_col(&self, line: &Line) -> usize {
        if !self.options.code_editing {
            return 0;
        }
        let first = (line.chars.iter()).take_while(|ch| blank(**ch)).count();
        let cell = line.cell_of(first);
        match first == line.chars.len() || self.col == cell {
            true => 0,
            false => cell,
        }
    }

    /// Whether a block covers more than one line.
    fn spans_lines(&self, block: Option<(u64, u64)>) -> Result<bool, Missing> {
        let Some((from, to)) = block else {
            return Ok(false);
        };
        Ok(self.line_holding(from)? != self.line_holding(to)?)
    }

    /// One indent: a tab, or the tab size's spaces with "Expand tabs".
    fn indent_unit(&self) -> String {
        match self.options.expand_tabs {
            0 => "\t".to_string(),
            _ => " ".repeat(self.tab_size()),
        }
    }

    /// The block's lines (a line the block ends at the start of left out), the cursor's without one.
    pub(super) fn code_lines(&self, block: Option<(u64, u64)>) -> Result<Vec<Line>, Missing> {
        let mut lines = self.block_lines(block)?;
        if lines.len() > 1 {
            lines.retain(|line| block.is_none_or(|(_, to)| to > line.start));
        }
        Ok(lines)
    }

    /// Tab / Shift+Tab: each line an indent right (an empty one left as it is) or left (a leading
    /// tab, or up to the tab size's spaces, taken out); one undo step, the block and the cursor on
    /// their text.
    fn indent_lines(&mut self, block: Option<(u64, u64)>, right: bool) -> Result<(), Missing> {
        let lines = self.code_lines(block)?;
        self.doc.begin_group();
        let done = self.indent_each(&lines, right);
        self.doc.end_group();
        done
    }

    fn indent_each(&mut self, lines: &[Line], right: bool) -> Result<(), Missing> {
        let cursor_line = self.line;
        let mut col = self.col;
        let unit = self.indent_unit();
        for line in lines.iter().rev() {
            let cells = match right {
                true if line.chars.is_empty() => 0,
                true => {
                    let bytes = self.encoding.encode(&unit);
                    self.insert_bytes(line.start, &bytes);
                    self.tab_size()
                }
                false => self.outdent_line(line),
            };
            if cells == 0 {
                continue;
            }
            self.changed_at(line.start);
            if line.start == cursor_line {
                col = match right {
                    true => col + cells,
                    false => col.saturating_sub(cells),
                };
            }
        }
        self.col = col;
        Ok(())
    }

    /// A line's first indent taken out: the cells it took (0: none).
    fn outdent_line(&mut self, line: &Line) -> usize {
        let tab = self.tab_size();
        let (chars, cells) = match line.chars.first() {
            Some('\t') => (1, tab),
            _ => {
                let spaces = (line.chars.iter())
                    .take(tab)
                    .take_while(|ch| **ch == ' ')
                    .count();
                (spaces, spaces)
            }
        };
        if chars == 0 {
            return 0;
        }
        let taken = self
            .encoding
            .encode(&line.chars[..chars].iter().collect::<String>());
        self.delete_bytes(line.start, taken.len() as u64);
        cells
    }

    /// The characters before the cursor on `line` (past its end: all).
    pub(super) fn chars_before(&self, line: &Line) -> usize {
        let at = self.col.min(line.cells.len());
        (0..line.chars.len())
            .take_while(|&index| line.cell_of(index) < at)
            .count()
    }

    /// The byte character `index` of `line` starts at (past the last: the text's end).
    fn char_pos(&self, line: &Line, index: usize) -> Result<u64, Missing> {
        self.offset_at(line, line.cell_of(index))
    }

    /// Enter in the code mode: the new line under the indent of the cursor's, an indent more after an
    /// opening bracket, the blanks after the cursor left out; a closing bracket after the cursor on a
    /// line of its own under the first one's indent.
    fn code_enter(&mut self, block: Option<(u64, u64)>) -> Result<(), Missing> {
        if let Some(block) = block {
            self.take_out(block)?;
        }
        let line = self.read(self.line)?;
        let before = self.chars_before(&line);
        let indent: String = (line.chars[..before].iter())
            .take_while(|ch| blank(**ch))
            .collect();
        let blanks = (line.chars[before..].iter())
            .take_while(|ch| blank(**ch))
            .count();
        let last = (line.chars[..before].iter()).rev().find(|ch| !blank(**ch));
        let opened = BRACKETS.iter().find(|(open, _)| Some(open) == last);
        let next = line.chars.get(before + blanks);
        let closed = opened.is_some_and(|(_, close)| Some(close) == next);
        let pos = self.char_pos(&line, before)?;
        let after = self.char_pos(&line, before + blanks)?;
        if after > pos {
            self.delete_bytes(pos, after - pos);
        }
        let more = if opened.is_some() {
            self.indent_unit()
        } else {
            String::new()
        };
        let first = format!("{}{indent}{more}", self.eol);
        let rest = if closed {
            format!("{}{indent}", self.eol)
        } else {
            String::new()
        };
        let first = self.encoding.encode(&first);
        let rest = self.encoding.encode(&rest);
        self.insert_bytes(pos, &[first.as_slice(), rest.as_slice()].concat());
        self.changed_at(pos);
        let cursor = pos + first.len() as u64;
        let broken = self.line;
        self.line = self.line_holding(cursor)?;
        self.column_line_added(broken, self.line);
        self.col = Self::col_of(&self.read(self.line)?, cursor);
        Ok(())
    }

    /// A closing bracket typed with only blanks before it: the line an indent back first.
    pub(super) fn type_closer(
        &mut self,
        ch: char,
        block: Option<(u64, u64)>,
    ) -> Result<(), Missing> {
        let line = self.read(self.line)?;
        let before = self.chars_before(&line);
        let only_blanks = before > 0 && line.chars[..before].iter().all(|ch| blank(*ch));
        if block.is_some() || !only_blanks || self.col > line.cells.len() {
            return self.typed(&ch.to_string(), block);
        }
        self.doc.begin_group();
        let done = self
            .indent_each(std::slice::from_ref(&line), false)
            .and_then(|()| self.typed(&ch.to_string(), None));
        self.doc.end_group();
        done
    }

    /// BS in the code mode: in the line's leading spaces back to the previous tab stop, else FAR's.
    fn smart_backspace(&mut self) -> Result<(), Missing> {
        let line = self.read(self.line)?;
        if self.backspace_pair(&line)? {
            return Ok(());
        }
        let col = self.col;
        let spaces = (line.chars.iter()).take_while(|ch| **ch == ' ').count();
        if col < 2 || col > spaces {
            return self.backspace(None);
        }
        let count = (col - 1) % self.tab_size() + 1;
        let from = line.offsets[col - count];
        let to = self.offset_at(&line, col)?;
        self.delete_bytes(from, to - from);
        self.changed_at(from);
        self.col = col - count;
        Ok(())
    }

    /// Ctrl+/: the lines (not the blank ones) commented out with `token` at their least indent, or,
    /// when every one is, the token (and a blank after it) taken out; one undo step.
    fn toggle_comment(&mut self, block: Option<(u64, u64)>, token: &str) -> Result<(), Missing> {
        let lines: Vec<Line> = (self.code_lines(block)?.into_iter())
            .filter(|line| line.chars.iter().any(|ch| !blank(*ch)))
            .collect();
        let token_chars: Vec<char> = token.chars().collect();
        if lines.is_empty() || token_chars.is_empty() {
            return Ok(());
        }
        let indent = |line: &Line| line.chars.iter().take_while(|ch| blank(**ch)).count();
        let commented = lines
            .iter()
            .all(|line| line.chars[indent(line)..].starts_with(&token_chars));
        let least = lines.iter().map(indent).min().unwrap_or(0);
        self.doc.begin_group();
        let done = self.comment_each(&lines, &token_chars, commented, least);
        self.doc.end_group();
        done
    }

    fn comment_each(
        &mut self,
        lines: &[Line],
        token: &[char],
        commented: bool,
        least: usize,
    ) -> Result<(), Missing> {
        let cursor_line = self.line;
        let mut col = self.col;
        for line in lines.iter().rev() {
            let at = match commented {
                true => line.chars.iter().take_while(|ch| blank(**ch)).count(),
                false => least,
            };
            let pos = self.char_pos(line, at)?;
            let cell = line.cell_of(at);
            let cells = if commented {
                let spaced = line.chars.get(at + token.len()) == Some(&' ');
                let count = token.len() + usize::from(spaced);
                let end = self.char_pos(line, at + count)?;
                self.delete_bytes(pos, end - pos);
                -(count as i64)
            } else {
                let text: String = token.iter().chain([' '].iter()).collect();
                self.insert_bytes(pos, &self.encoding.encode(&text));
                token.len() as i64 + 1
            };
            self.changed_at(pos);
            if line.start == cursor_line && col >= cell {
                col = col.saturating_add_signed(cells as isize).max(cell);
            }
        }
        self.col = col;
        Ok(())
    }

    /// The document's bytes `from..to`.
    pub(super) fn bytes_between(&self, from: u64, to: u64) -> Result<Vec<u8>, Missing> {
        let mut bytes = Vec::new();
        let mut at = from;
        while at < to {
            let piece = self.doc.at(at)?;
            if piece.is_empty() {
                break;
            }
            let take = piece
                .len()
                .min(usize::try_from(to - at).unwrap_or(usize::MAX));
            bytes.extend_from_slice(&piece[..take]);
            at += take as u64;
        }
        Ok(bytes)
    }

    /// Alt+Up / Alt+Down: the block's lines (the cursor's without one) swapped with the line above or
    /// below, the cursor and the block going with them; one undo step.
    fn move_lines(&mut self, block: Option<(u64, u64)>, up: bool) -> Result<(), Missing> {
        let lines = self.code_lines(block)?;
        let (Some(first), Some(last)) = (lines.first(), lines.last()) else {
            return Ok(());
        };
        let (start, end) = (first.start, last.end);
        let neighbour = match up {
            true => self.previous_line(start)?,
            false => self.next_line(last)?,
        };
        let Some(neighbour) = neighbour else {
            return Ok(());
        };
        let other = self.read(neighbour)?;
        // The lines in their new order, the one without a break (the file's last) given one and the
        // other's last taken off.
        // `bottom`: the last line now, the one that may have no break; `cut`: the line whose break goes
        // when it has none (the new last one but one).
        let (top, bottom, cut) = match up {
            true => (other.clone(), last.clone(), other.clone()),
            false => (first.clone(), other.clone(), last.clone()),
        };
        let (moved_first, moved_last) = match up {
            true => ((start, end), (other.start, start)),
            false => ((other.start, other.end), (start, end)),
        };
        let mut upper = self.bytes_between(moved_first.0, moved_first.1)?;
        let mut lower = self.bytes_between(moved_last.0, moved_last.1)?;
        let eol = self.encoding.encode(&self.eol);
        if self.break_len(&bottom)? == 0 {
            upper.extend_from_slice(&eol);
            let cut = usize::try_from(self.break_len(&cut)?).unwrap_or(0);
            lower.truncate(lower.len().saturating_sub(cut));
        }
        let shift = match up {
            true => -((start - other.start) as i64),
            false => upper.len() as i64,
        };
        let range = (top.start, bottom.end);
        self.replace_lines(range, &[upper, lower].concat(), (start, end), shift)
    }

    /// The lines `range` written as `text`, the places in `moved` moved by `shift`.
    fn replace_lines(
        &mut self,
        range: (u64, u64),
        text: &[u8],
        moved: (u64, u64),
        shift: i64,
    ) -> Result<(), Missing> {
        let inside = |at: u64| at >= moved.0 && at <= moved.1;
        let move_one = |at: u64| match inside(at) {
            true => at.saturating_add_signed(shift),
            false => at,
        };
        let (line, anchor, fixed) = (self.line, self.anchor, self.fixed);
        self.doc.begin_group();
        self.delete_bytes(range.0, range.1 - range.0);
        self.insert_bytes(range.0, text);
        self.doc.end_group();
        self.changed_at(range.0);
        self.line = move_one(line);
        self.anchor = anchor.map(move_one);
        self.fixed = fixed.map(move_one);
        self.column = None;
        Ok(())
    }
}
