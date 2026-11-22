//! More of the code editing mode (owner 2026-11-22): brackets and quotes closed as they are opened
//! (typed over when the next character is the one typed), a block wrapped in them, BS between an empty
//! pair taking both, Ctrl+Shift+D duplicating the lines, the lines' ending blanks taken out and a last
//! break given before a save.

use super::{EditView, Line, Missing};
use crate::editor::view::blocks::Bytes;

/// The brackets closed as they are opened.
const BRACKETS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
/// The quotes closed as they are opened.
const QUOTES: [char; 3] = ['"', '\'', '`'];
/// The largest document the save cleans up (its whole read for it: no more than the colours').
const CLEANED: u64 = 16 << 20;

fn word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// What closes `ch` when it opens a pair (a quote closes itself).
fn closer_of(ch: char) -> Option<char> {
    (BRACKETS
        .iter()
        .find(|(open, _)| *open == ch)
        .map(|(_, close)| *close))
    .or_else(|| QUOTES.contains(&ch).then_some(ch))
}

/// A place in the lines `from..=to` moved by `shift`, any other as it is.
fn shifted(at: u64, (from, to): (u64, u64), shift: u64) -> u64 {
    match at >= from && at <= to {
        true => at + shift,
        false => at,
    }
}

fn is_closer(ch: char) -> bool {
    BRACKETS.iter().any(|(_, close)| *close == ch)
}

impl EditView {
    /// A character typed in the code mode: a pair opened, a block wrapped, the next character typed
    /// over, a closing bracket's indent; none: typed as FAR types it.
    pub(super) fn code_typed(
        &mut self,
        ch: char,
        block: Option<(u64, u64)>,
        replaced: Option<(u64, u64)>,
    ) -> Option<Result<(), Missing>> {
        let close = closer_of(ch);
        if let (Some(close), Some(block)) = (close, block)
            && self.column.is_none()
        {
            return Some(self.wrap(block, ch, close));
        }
        if block.is_some() {
            return is_closer(ch).then(|| self.type_closer(ch, replaced));
        }
        let line = match self.read(self.line) {
            Ok(line) => line,
            Err(missing) => return Some(Err(missing)),
        };
        let at = self.chars_before(&line);
        let inside = self.col <= line.cells.len();
        let next = line.chars.get(at).copied().filter(|_| inside);
        if (is_closer(ch) || QUOTES.contains(&ch)) && next == Some(ch) {
            self.col = line.cell_of(at + 1);
            return Some(Ok(()));
        }
        if let Some(close) = close
            && self.opens_here(&line, at, next, ch)
        {
            return Some(self.type_pair(ch, close));
        }
        is_closer(ch).then(|| self.type_closer(ch, replaced))
    }

    /// Whether a pair is closed as `ch` opens at character `at`: nothing but a blank, a closing
    /// bracket or a separator after it; a quote not after a word's character or another quote.
    fn opens_here(&self, line: &Line, at: usize, next: Option<char>, ch: char) -> bool {
        let free =
            next.is_none_or(|next| next.is_whitespace() || is_closer(next) || ",;".contains(next));
        let before = at.checked_sub(1).map(|index| line.chars[index]);
        let quote_free =
            !QUOTES.contains(&ch) || before.is_none_or(|before| !word_char(before) && before != ch);
        free && quote_free
    }

    /// The pair typed, the cursor between.
    fn type_pair(&mut self, open: char, close: char) -> Result<(), Missing> {
        self.typed(&format!("{open}{close}"), None)?;
        self.col = self.col.saturating_sub(1);
        Ok(())
    }

    /// The stream block `from..to` put between `open` and `close`, the block on its text still.
    fn wrap(&mut self, (from, to): (u64, u64), open: char, close: char) -> Result<(), Missing> {
        let open = self.encoding.encode(&open.to_string());
        let close = self.encoding.encode(&close.to_string());
        let shift = open.len() as u64;
        let cursor_first = self.cursor_pos()? == from;
        self.doc.begin_group();
        self.insert_bytes(to, &close);
        self.insert_bytes(from, &open);
        self.doc.end_group();
        self.changed_at(from);
        let (cursor, other) = match cursor_first {
            true => (from + shift, to + shift),
            false => (to + shift, from + shift),
        };
        if self.fixed.is_some() {
            self.fixed = Some(cursor);
        }
        self.anchor = Some(other);
        self.line = self.line_holding(cursor)?;
        self.col = Self::col_of(&self.read(self.line)?, cursor);
        Ok(())
    }

    /// BS between an empty pair (`(|)`, `"|"`): both taken out; false: not between one.
    pub(super) fn backspace_pair(&mut self, line: &Line) -> Result<bool, Missing> {
        if self.col > line.cells.len() {
            return Ok(false);
        }
        let at = self.chars_before(line);
        let (Some(before), Some(next)) =
            (at.checked_sub(1).map(|i| line.chars[i]), line.chars.get(at))
        else {
            return Ok(false);
        };
        if closer_of(before) != Some(*next) {
            return Ok(false);
        }
        let from = self.offset_at(line, line.cell_of(at - 1))?;
        let to = self.offset_at(line, line.cell_of(at + 1))?;
        self.delete_bytes(from, to - from);
        self.changed_at(from);
        self.col = line.cell_of(at - 1);
        Ok(true)
    }

    /// Ctrl+Shift+D: the block's lines (the cursor's without one) put again after them, the cursor and
    /// the block on the copy; one undo step.
    pub(super) fn duplicate_lines(&mut self, block: Option<(u64, u64)>) -> Result<(), Missing> {
        let lines = self.code_lines(block)?;
        let (Some(first), Some(last)) = (lines.first(), lines.last()) else {
            return Ok(());
        };
        let (start, end) = (first.start, last.end);
        let mut copy = self.bytes_between(start, end)?;
        if self.break_len(last)? == 0 {
            let mut broken = self.encoding.encode(&self.eol);
            broken.append(&mut copy);
            copy = broken;
        }
        let shift = copy.len() as u64;
        let (line, anchor, fixed) = (self.line, self.anchor, self.fixed);
        self.insert_bytes(end, &copy);
        self.changed_at(end);
        let moved = |at: u64| shifted(at, (start, end), shift);
        self.line = shifted(line, (start, end), shift);
        self.anchor = anchor.map(moved);
        self.fixed = fixed.map(moved);
        self.column = None;
        Ok(())
    }

    /// Ctrl+Space's words: the word before the cursor (its characters) and the document's words that
    /// start with it (the case aside), longer than it, the most frequent first; from the lines read
    /// within `NEAR` bytes of the cursor (a block not read yet ends the look there).
    pub fn completions(&self) -> (usize, Vec<String>) {
        const NEAR: u64 = 64 << 10;
        let Ok(line) = self.read(self.line) else {
            return (0, Vec::new());
        };
        let at = self.chars_before(&line);
        let typed = (line.chars[..at].iter().rev())
            .take_while(|ch| word_char(**ch))
            .count();
        if typed == 0 {
            return (0, Vec::new());
        }
        let prefix: String = line.chars[at - typed..at]
            .iter()
            .collect::<String>()
            .to_lowercase();
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let from = self.line.saturating_sub(NEAR).max(self.floor);
        let mut start = self.line_holding(from).unwrap_or(self.line);
        while start <= self.line + NEAR {
            let Ok(line) = self.read(start) else { break };
            for word in line.chars.split(|ch| !word_char(*ch)) {
                let word: String = word.iter().collect();
                if word.chars().count() > typed && word.to_lowercase().starts_with(&prefix) {
                    *counts.entry(word).or_default() += 1;
                }
            }
            match self.next_line(&line) {
                Ok(Some(next)) => start = next,
                _ => break,
            }
        }
        let mut words: Vec<(String, usize)> = counts.into_iter().collect();
        words.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        (
            typed,
            words.into_iter().take(30).map(|(word, _)| word).collect(),
        )
    }

    /// A completion chosen: the `typed` characters before the cursor replaced by `word`.
    pub(super) fn complete(&mut self, typed: usize, word: &str) -> Result<(), Missing> {
        let line = self.read(self.line)?;
        let at = self.chars_before(&line);
        let Some(first) = at.checked_sub(typed) else {
            return Ok(());
        };
        let from = self.offset_at(&line, line.cell_of(first))?;
        let to = self.offset_at(&line, line.cell_of(at))?;
        self.doc.begin_group();
        self.delete_bytes(from, to - from);
        self.changed_at(from);
        self.col = line.cell_of(first);
        let done = self.typed(word, None);
        self.doc.end_group();
        done
    }

    /// Before a save in the code mode: every line's ending blanks taken out, a last line given the
    /// file's break; one undo step. A document past `CLEANED` is left as it is (its whole read).
    pub(super) fn clean_up(&mut self) -> Result<(), Missing> {
        if self.doc.len() > CLEANED || self.doc.len() <= self.floor {
            return Ok(());
        }
        let mut blanks = Vec::new();
        let mut start = self.floor;
        let last = loop {
            let line = self.read(start)?;
            let kept = line
                .chars
                .iter()
                .rposition(|ch| !matches!(ch, ' ' | '\t'))
                .map_or(0, |at| at + 1);
            if kept < line.chars.len() {
                let from = self.offset_at(&line, line.cell_of(kept))?;
                blanks.push((from, self.text_end(&line)?));
            }
            match self.next_line(&line)? {
                Some(next) => start = next,
                None => break line,
            }
        };
        let unbroken = self.break_len(&last)? == 0 && !last.chars.is_empty();
        if blanks.is_empty() && !unbroken {
            return Ok(());
        }
        self.doc.begin_group();
        if unbroken {
            let eol = self.encoding.encode(&self.eol);
            self.insert_bytes(last.end, &eol);
        }
        for &(from, to) in blanks.iter().rev() {
            self.delete_bytes(from, to - from);
        }
        self.doc.end_group();
        self.changed_at(blanks.first().map_or(last.end, |(from, _)| *from));
        Ok(())
    }
}
