//! A single-line text field as FAR's `Edit` (command line, dialog inputs): the cursor is a character
//! index; a selected block is replaced by typing and removed by Del/Backspace (FAR's default
//! non-persistent blocks with "Del removes blocks").

use std::ops::Range;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineInput {
    text: String,
    cursor: usize,
    /// The selected characters, never empty.
    selection: Option<Range<usize>>,
    /// Shift+movement is extending the selection (FAR's `FEDITLINE_MARKINGBLOCK`): the next one
    /// grows it; after any other key a Shift+movement starts a new one.
    marking: bool,
    /// The first character shown (FAR's `LeftPos`): it moves only when the cursor leaves the view,
    /// so a click does not scroll the text under the mouse. Kept by the renderer.
    left: std::cell::Cell<usize>,
}

impl LineInput {
    pub fn with_text(text: &str) -> Self {
        Self {
            text: text.to_string(),
            cursor: text.chars().count(),
            ..Self::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The selected characters (indices), if any.
    pub fn selection(&self) -> Option<Range<usize>> {
        self.selection.clone()
    }

    pub fn selected_text(&self) -> Option<String> {
        let range = self.selection.clone()?;
        Some(
            self.text
                .chars()
                .skip(range.start)
                .take(range.len())
                .collect(),
        )
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn chars(&self) -> Vec<char> {
        self.text.chars().collect()
    }

    fn byte_index(&self, at: usize) -> usize {
        self.text
            .char_indices()
            .nth(at)
            .map_or(self.text.len(), |(index, _)| index)
    }

    /// Removes characters `range` (clamped) and the selection; the cursor stays on its character.
    fn remove(&mut self, range: Range<usize>) {
        let range = range.start.min(self.len())..range.end.min(self.len());
        let (start, end) = (self.byte_index(range.start), self.byte_index(range.end));
        self.text.replace_range(start..end, "");
        if self.cursor >= range.end {
            self.cursor -= range.len();
        } else if self.cursor > range.start {
            self.cursor = range.start;
        }
        self.selection = None;
    }

    /// Removes the selected block, the cursor going to its start; false if nothing was selected.
    pub fn delete_selection(&mut self) -> bool {
        let Some(range) = self.selection.take() else {
            return false;
        };
        self.remove(range.clone());
        self.cursor = range.start;
        true
    }

    pub fn insert(&mut self, ch: char) {
        self.delete_selection();
        let index = self.byte_index(self.cursor);
        self.text.insert(index, ch);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, text: &str) {
        self.delete_selection();
        for ch in text.chars() {
            self.insert(ch);
        }
    }

    /// Shift+Ins: clipboard text, its line breaks turned into single spaces (FAR's `flatten_string`).
    pub fn paste(&mut self, text: &str) {
        self.insert_str(&flatten(text));
    }

    pub fn backspace(&mut self) {
        if !self.delete_selection() && self.cursor > 0 {
            self.remove(self.cursor - 1..self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if !self.delete_selection() {
            self.remove(self.cursor..self.cursor + 1);
        }
    }

    /// Ctrl+Y: the whole line.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Ctrl+K: from the cursor to the end; a selection keeps its part before the cursor.
    pub fn delete_to_end(&mut self) {
        let selection = self.selection.take();
        self.remove(self.cursor..self.len());
        self.selection = selection
            .map(|range| range.start..range.end.min(self.cursor))
            .filter(|range| !range.is_empty());
    }

    /// Ctrl+Shift+Backspace: from the start to the cursor.
    pub fn delete_to_start(&mut self) {
        self.remove(0..self.cursor);
    }

    /// Ctrl+Backspace as in FAR's `Edit`: the word before the cursor, stopping where blanks begin or
    /// end and after a word divider. A selected block goes first (FAR's first Backspace removes it)
    /// and the word before it follows unless a divider or the start is there.
    pub fn delete_word_left(&mut self) {
        if self.delete_selection() {
            let chars = self.chars();
            if self.cursor == 0 || is_word_divider(chars[self.cursor - 1]) {
                return;
            }
        }
        let chars = self.chars();
        let end = self.cursor.min(chars.len());
        let mut at = end;
        while at > 0 {
            let stop = at > 1 && is_blank(chars[at - 1]) != is_blank(chars[at - 2]);
            at -= 1;
            if at == 0 || stop || is_word_divider(chars[at - 1]) {
                break;
            }
        }
        self.remove(at..end);
    }

    /// Ctrl+Del as in FAR's `Edit`: the word after the cursor, stopping before the next word and at a
    /// word divider.
    pub fn delete_word_right(&mut self) {
        let chars = self.chars();
        let start = self.cursor;
        let mut at = start;
        while at < chars.len() {
            let stop = at + 1 < chars.len() && is_blank(chars[at]) && !is_blank(chars[at + 1]);
            at += 1;
            if at >= chars.len() || stop || is_word_divider(chars[at]) {
                break;
            }
        }
        self.remove(start..at);
    }

    /// FAR's `FixLeftPos` for a view `width` characters wide (the cursor's column after the text
    /// included): the first character to show.
    pub fn shown_from(&self, width: usize) -> usize {
        let width = width.max(1);
        let mut left = self.left.get();
        if self.cursor >= left + width {
            left = self.cursor + 1 - width;
        }
        if self.cursor < left {
            left = self.cursor;
        }
        self.left.set(left);
        left
    }

    /// A click: the cursor at `index` (at most the end), no block.
    pub fn place_cursor(&mut self, index: usize) {
        self.selection = None;
        self.marking = false;
        self.cursor = index.min(self.len());
    }

    /// FAR's double click (`KEY_OP_SELWORD`, `FindWordInString`): the word around the cursor, or one
    /// divider standing alone; the cursor stays.
    pub fn select_word_at_cursor(&mut self) {
        if let Some(range) = word_around(&self.chars(), self.cursor) {
            self.selection = Some(range);
            self.marking = false;
        }
    }

    /// Ctrl+A.
    pub fn select_all(&mut self) {
        self.selection = (!self.text.is_empty()).then(|| 0..self.len());
        self.marking = false;
    }

    /// A key without Shift arrived: the next Shift+movement starts a new block.
    pub fn end_marking(&mut self) {
        self.marking = false;
    }

    /// Moves the cursor with `step` and selects what it passed over (Shift+movement); while marking,
    /// the block grows or shrinks at its end by the cursor.
    pub fn select_with(&mut self, step: impl FnOnce(&mut Self)) {
        let from = self.cursor;
        let anchor = match (&self.selection, self.marking) {
            (Some(range), true) if range.start == from => range.end,
            (Some(range), true) if range.end == from => range.start,
            _ => from,
        };
        step(self);
        let to = self.cursor;
        self.selection = (anchor != to).then(|| anchor.min(to)..anchor.max(to));
        self.marking = true;
    }

    pub fn left(&mut self) {
        self.selection = None;
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.selection = None;
        self.cursor = (self.cursor + 1).min(self.len());
    }

    pub fn home(&mut self) {
        self.selection = None;
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.selection = None;
        self.cursor = self.len();
    }

    /// Ctrl+Left as in FAR's `Edit`: to the start of the word before the cursor.
    pub fn word_left(&mut self) {
        self.selection = None;
        self.cursor = word_start(&self.chars(), self.cursor, false);
    }

    /// Ctrl+Right as in FAR's `Edit`: to the end of the word or the start of the next one, whichever
    /// comes first.
    pub fn word_right(&mut self) {
        self.selection = None;
        self.cursor = word_end(&self.chars(), self.cursor, false);
    }

    /// Ctrl+Shift+Left: selects to the start of the word, also stopping after a word divider.
    pub fn select_word_left(&mut self) {
        self.select_with(|input| input.cursor = word_start(&input.chars(), input.cursor, true));
    }

    /// Ctrl+Shift+Right: selects to the end of the word, also stopping after a word divider.
    pub fn select_word_right(&mut self) {
        self.select_with(|input| input.cursor = word_end(&input.chars(), input.cursor, true));
    }

    /// `text` with the cursor before character `cursor` (clamped to the end).
    pub fn set_with_cursor(&mut self, text: &str, cursor: usize) {
        self.set(text);
        self.cursor = cursor.min(self.cursor);
    }

    pub fn set(&mut self, text: &str) {
        *self = Self::with_text(text);
    }

    pub fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        self.clear();
        text
    }
}

/// Where Ctrl+Left goes from `cursor`; `after_divider` also stops after a word divider (Ctrl+Shift+Left).
fn word_start(chars: &[char], cursor: usize, after_divider: bool) -> usize {
    let mut at = cursor.min(chars.len()).saturating_sub(1);
    while at > 0 {
        let (here, before) = (chars[at], chars[at - 1]);
        let word_start = !is_word_divider(here) && is_word_divider(before) && !is_blank(here);
        let gap_before = is_blank(before) || (after_divider && is_word_divider(before));
        if word_start || (!is_blank(here) && gap_before) {
            break;
        }
        at -= 1;
    }
    at
}

/// Where Ctrl+Right goes from `cursor`; `after_divider` also stops after a word divider.
fn word_end(chars: &[char], cursor: usize, after_divider: bool) -> usize {
    if cursor >= chars.len() {
        return cursor;
    }
    let mut at = cursor + 1;
    while at < chars.len() {
        let (here, before) = (chars[at], chars[at - 1]);
        let word_end = is_word_divider(here) && !is_word_divider(before);
        let gap_before = is_blank(before) || (after_divider && is_word_divider(before));
        if word_end || (!is_blank(here) && gap_before) {
            break;
        }
        at += 1;
    }
    at
}

/// FAR's `flatten_string`: line breaks at the start and the end go, each run inside becomes a space.
fn flatten(text: &str) -> String {
    let parts: Vec<&str> = text
        .split(['\r', '\n'])
        .filter(|part| !part.is_empty())
        .collect();
    parts.join(" ")
}

/// FAR's `FindWordInString`: the run of word characters around `at`; between two dividers the one
/// character there (not a blank).
pub(super) fn word_around(chars: &[char], at: usize) -> Option<Range<usize>> {
    if chars.is_empty() || at > chars.len() {
        return None;
    }
    let divides = |ch: &char| is_word_divider(*ch) || ch.is_whitespace();
    let mut begin = chars[..at].iter().rposition(divides).map_or(0, |i| i + 1);
    let mut end = chars[at..]
        .iter()
        .position(divides)
        .map_or(chars.len(), |i| at + i);
    if begin == end {
        if begin == chars.len() {
            if !chars[begin - 1].is_whitespace() {
                begin -= 1;
            }
        } else if !chars[begin].is_whitespace() {
            end += 1;
        } else if begin > 0 && !chars[begin - 1].is_whitespace() {
            begin -= 1;
        }
    }
    (begin != end).then_some(begin..end)
}

/// FAR's default word dividers (ADR-0028).
use crate::vfs::text::WORD_DIVIDERS;

fn is_word_divider(ch: char) -> bool {
    WORD_DIVIDERS.contains(ch)
}

fn is_blank(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_arrows_jump_by_words_as_far_does() {
        let mut input = LineInput::with_text("copy c:\\dir\\a.txt  d:");
        input.word_left();
        assert_eq!(input.cursor(), 19, "to 'd' of d:");
        input.word_left();
        assert_eq!(input.cursor(), 14, "to 'txt'");
        input.word_left();
        assert_eq!(input.cursor(), 12, "to 'a'");
        input.home();
        input.word_right();
        assert_eq!(input.cursor(), 5, "over 'copy ' to the next word");
        input.word_right();
        assert_eq!(input.cursor(), 6, "past 'c' at ':'");
        input.end();
        input.word_right();
        assert_eq!(input.cursor(), 21);
    }

    #[test]
    fn edits_in_the_middle_with_non_ascii_text() {
        let mut input = LineInput::with_text("пр");
        input.left();
        input.insert('и');
        assert_eq!(input.text(), "пир");
        input.backspace();
        assert_eq!(input.text(), "пр");
        input.home();
        input.delete();
        assert_eq!(input.text(), "р");
    }

    #[test]
    fn cursor_stays_in_bounds() {
        let mut input = LineInput::with_text("ab");
        input.right();
        assert_eq!(input.cursor(), 2);
        input.home();
        input.left();
        assert_eq!(input.cursor(), 0);
        input.backspace();
        assert_eq!(input.text(), "ab");
    }

    #[test]
    fn take_clears_the_field() {
        let mut input = LineInput::with_text("dir");
        input.select_all();
        assert_eq!(input.take(), "dir");
        assert!(input.is_empty());
        assert_eq!(input.cursor(), 0);
        assert_eq!(input.selection(), None);
    }

    #[test]
    fn shift_movement_grows_and_shrinks_one_block() {
        let mut input = LineInput::with_text("копия файла");
        input.select_with(LineInput::left);
        input.select_with(LineInput::left);
        assert_eq!(input.selected_text().as_deref(), Some("ла"));
        input.select_with(LineInput::right);
        assert_eq!(input.selected_text().as_deref(), Some("а"));
        input.select_with(LineInput::right);
        assert_eq!(input.selection(), None, "back where it started");

        input.select_with(LineInput::home);
        assert_eq!(input.selected_text().as_deref(), Some("копия файла"));
        input.end_marking();
        input.select_with(LineInput::right);
        assert_eq!(
            input.selected_text().as_deref(),
            Some("к"),
            "a new block after another key"
        );
        input.right();
        assert_eq!(input.selection(), None, "plain movement drops the block");
    }

    #[test]
    fn typing_and_deleting_replace_the_block() {
        let mut input = LineInput::with_text("copy a.txt b");
        input.home();
        input.select_word_right();
        assert_eq!(input.selected_text().as_deref(), Some("copy "));
        input.insert('m');
        assert_eq!(input.text(), "ma.txt b");
        input.select_all();
        input.backspace();
        assert!(input.is_empty());

        let mut input = LineInput::with_text("one two");
        input.select_with(LineInput::word_left);
        input.paste("\r\nx\r\n\r\ny\n");
        assert_eq!(input.text(), "one x y");
        input.home();
        input.select_with(LineInput::right);
        input.delete();
        assert_eq!(input.text(), "ne x y");
    }

    #[test]
    fn ctrl_shift_arrows_also_stop_after_a_divider() {
        let mut input = LineInput::with_text("c:\\\\dir");
        input.select_word_left();
        assert_eq!(input.selected_text().as_deref(), Some("dir"));
        input.select_word_left();
        assert_eq!(input.selected_text().as_deref(), Some("\\dir"));
    }

    #[test]
    fn word_and_line_deletions_as_in_far() {
        let mut input = LineInput::with_text("copy c:\\dir\\a.txt  d:");
        input.delete_word_left();
        assert_eq!(input.text(), "copy c:\\dir\\a.txt  ", "stops at the blanks");
        input.delete_word_left();
        assert_eq!(input.text(), "copy c:\\dir\\a.txt", "the blanks alone");
        input.delete_word_left();
        assert_eq!(input.text(), "copy c:\\dir\\a.", "up to the divider");
        input.home();
        input.delete_word_right();
        assert_eq!(input.text(), "c:\\dir\\a.", "the word and its blank");
        input.delete_word_right();
        assert_eq!(input.text(), ":\\dir\\a.", "up to the divider");

        let mut input = LineInput::with_text("echo one two");
        input.word_left();
        input.delete_to_end();
        assert_eq!(input.text(), "echo one ");
        input.left();
        input.delete_to_start();
        assert_eq!((input.text(), input.cursor()), (" ", 0));
        input.clear();
        assert!(input.is_empty());
    }

    #[test]
    fn a_double_click_takes_the_word_or_a_divider_alone_as_far() {
        let around = |text: &str, at: usize| {
            let mut input = LineInput::with_text(text);
            input.place_cursor(at);
            input.select_word_at_cursor();
            input.selection()
        };
        assert_eq!(around("copy abc.txt def", 6), Some(5..8));
        assert_eq!(
            around("copy abc.txt def", 5),
            Some(5..8),
            "at its first letter"
        );
        assert_eq!(around("copy abc.txt def", 16), Some(13..16), "at the end");
        assert_eq!(around("a = b", 2), Some(2..3), "a divider alone");
        assert_eq!(around("a  b", 2), None, "between blanks");
        assert_eq!(around("", 0), None);
    }
}
