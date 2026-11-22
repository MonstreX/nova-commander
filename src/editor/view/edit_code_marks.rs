//! The code mode's marks on the editor's screen (owner 2026-11-22): the bracket at the cursor (or just
//! before it) and its pair among the rows shown, and the word under the cursor each time the rows show
//! it as a whole word. Only the rows on the screen are looked at, so a pair off the screen is not marked.

use super::{EditScreen, EditView, Line};

/// The brackets paired.
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

fn word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

impl EditView {
    /// The pairs and the words marked for the screen's `lines`, `width` columns from `left`.
    pub(super) fn code_marks(&self, lines: &[Line], width: usize, screen: &mut EditScreen) {
        let Some(row) = lines.iter().position(|line| line.start == self.line) else {
            return;
        };
        let at = self.chars_before(&lines[row]);
        let shown = |cell: usize| cell.checked_sub(self.left).filter(|col| *col < width);
        if let Some((from, to)) = pair_of(lines, row, at) {
            for (row, index) in [from, to] {
                if let Some(col) = shown(lines[row].cell_of(index)) {
                    screen.pairs.push((row, col));
                }
            }
        }
        let Some(word) = word_at(&lines[row].chars, at) else {
            return;
        };
        for (row, line) in lines.iter().enumerate() {
            for index in occurrences(&line.chars, &word) {
                let from = line.cell_of(index);
                let to = line.cell_of(index + word.len());
                let first = from.max(self.left);
                let past = to.min(self.left + width);
                if past > first {
                    screen
                        .words
                        .push((row, first - self.left, past - self.left));
                }
            }
        }
    }
}

/// The bracket at character `at` of row `row` (else the one just before it) and its pair: both as
/// (row, character); none without a bracket or a pair on the rows.
fn pair_of(lines: &[Line], row: usize, at: usize) -> Option<((usize, usize), (usize, usize))> {
    let chars = &lines[row].chars;
    let bracket = |index: usize| {
        let ch = *chars.get(index)?;
        PAIRS
            .iter()
            .find(|(open, close)| ch == *open || ch == *close)
            .map(|pair| (index, ch, *pair))
    };
    let (index, ch, (open, close)) = bracket(at).or_else(|| at.checked_sub(1).and_then(bracket))?;
    let found = match ch == open {
        true => forward(lines, (row, index), open, close),
        false => backward(lines, (row, index), open, close),
    }?;
    Some(((row, index), found))
}

/// The closing bracket matching the opening one at `from`.
fn forward(
    lines: &[Line],
    from: (usize, usize),
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let mut depth = 0_usize;
    for (row, line) in lines.iter().enumerate().skip(from.0) {
        let start = if row == from.0 { from.1 } else { 0 };
        for (index, ch) in line.chars.iter().enumerate().skip(start) {
            if *ch == open {
                depth += 1;
            } else if *ch == close {
                depth -= 1;
                if depth == 0 {
                    return Some((row, index));
                }
            }
        }
    }
    None
}

/// The opening bracket matching the closing one at `from`.
fn backward(
    lines: &[Line],
    from: (usize, usize),
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let mut depth = 0_usize;
    for row in (0..=from.0).rev() {
        let chars = &lines[row].chars;
        let end = if row == from.0 {
            from.1 + 1
        } else {
            chars.len()
        };
        for index in (0..end.min(chars.len())).rev() {
            if chars[index] == close {
                depth += 1;
            } else if chars[index] == open {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((row, index));
                }
            }
        }
    }
    None
}

/// The word the character `at` is in (or ends just before it).
fn word_at(chars: &[char], at: usize) -> Option<Vec<char>> {
    let inside = chars.get(at).is_some_and(|ch| word_char(*ch));
    let before = at
        .checked_sub(1)
        .is_some_and(|index| word_char(chars[index]));
    if !inside && !before {
        return None;
    }
    let start = (0..at)
        .rev()
        .take_while(|index| word_char(chars[*index]))
        .last()
        .unwrap_or(at);
    let end = (at..chars.len())
        .take_while(|index| word_char(chars[*index]))
        .last()
        .map_or(at, |last| last + 1);
    Some(chars[start..end].to_vec())
}

/// The characters where `word` stands as a whole word in `chars`.
fn occurrences(chars: &[char], word: &[char]) -> Vec<usize> {
    if word.is_empty() || chars.len() < word.len() {
        return Vec::new();
    }
    (0..=chars.len() - word.len())
        .filter(|&index| {
            chars[index..].starts_with(word)
                && (index == 0 || !word_char(chars[index - 1]))
                && chars
                    .get(index + word.len())
                    .is_none_or(|ch| !word_char(*ch))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    #[test]
    fn the_word_at_the_cursor_and_its_whole_occurrences() {
        let line = chars("let count = count_all(count);");
        assert_eq!(word_at(&line, 5), Some(chars("count")), "inside it");
        assert_eq!(word_at(&line, 9), Some(chars("count")), "just past it");
        assert_eq!(word_at(&line, 10), None, "between blanks");
        assert_eq!(
            occurrences(&line, &chars("count")),
            [4, 22],
            "not count_all"
        );
    }
}
