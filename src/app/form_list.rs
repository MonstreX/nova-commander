//! The open list of a drop-down field (FAR's combo box menu, a `VMenu` of `CBoxMaxHeight` lines):
//! Up / Down / Home / End and PgUp / PgDn by the lines shown over the separators, Enter or a line's
//! hotkey to choose, Esc to leave it as it was; Space and Ins put a mark on a line that takes one and
//! take it away (Find file's code pages, FAR's `LIF_CHECKED`), the cursor going on to the next line.

use std::cell::Cell;

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{ListLine, hotkey_of};

/// FAR's `Dialogs.CBoxMaxHeight`: the lines an open list shows, scrolled past them.
pub const LIST_ROWS: usize = 8;

/// A drop-down field's list while it is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenList {
    /// The line under the cursor.
    pub cursor: usize,
    /// The first line shown, kept by the drawing (FAR's `TopPos`).
    pub top: Cell<usize>,
}

impl OpenList {
    pub fn at(cursor: usize) -> Self {
        Self {
            cursor,
            top: Cell::new(0),
        }
    }
}

impl ListLine {
    /// A line the cursor passes over.
    pub fn is_separator(&self) -> bool {
        matches!(self, Self::Separator | Self::Section(_))
    }

    /// The text of a line that can be chosen or marked.
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Choice(label)
            | Self::Grayed(label)
            | Self::Toggle { label, .. }
            | Self::Marked { label, .. } => Some(label),
            Self::Separator | Self::Section(_) => None,
        }
    }
}

/// A key in an open drop-down list. Returns whether the list closes.
pub(super) fn list_key(
    lines: &mut [ListLine],
    open: &mut OpenList,
    chosen: &mut usize,
    key: KeyEvent,
) -> bool {
    if lines.is_empty() {
        return matches!(key.code, KeyCode::Esc | KeyCode::Enter);
    }
    let cursor = &mut open.cursor;
    match key.code {
        KeyCode::Esc => return true,
        KeyCode::Insert | KeyCode::Char(' ') if mark(lines, *cursor) => {
            // FAR's `DM_LISTSETCURPOS` to the next line; the last one stays.
            if let Some(next) = selectable(lines, *cursor + 1, true) {
                *cursor = next;
            }
        }
        KeyCode::Enter => return choose(lines, *cursor, chosen),
        KeyCode::Char(ch) => {
            let ch = ch.to_ascii_lowercase();
            if let Some(at) = lines.iter().position(|line| line_hotkey(line) == Some(ch)) {
                *cursor = at;
                return choose(lines, at, chosen);
            }
        }
        _ => move_cursor(lines, open, key),
    }
    false
}

/// Up / Down / Home / End round the ends and PgUp / PgDn by the lines shown, over the separators; other
/// keys do nothing.
pub(super) fn move_cursor(lines: &[ListLine], open: &mut OpenList, key: KeyEvent) {
    move_page(lines, open, key, LIST_ROWS);
}

/// As [`move_cursor`] with `rows` lines shown.
pub(super) fn move_page(lines: &[ListLine], open: &mut OpenList, key: KeyEvent, rows: usize) {
    let count = lines.len();
    if count == 0 {
        return;
    }
    let page = rows.min(count);
    let cursor = &mut open.cursor;
    match key.code {
        KeyCode::Down => *cursor = step(lines, *cursor, true),
        KeyCode::Up => *cursor = step(lines, *cursor, false),
        KeyCode::Home => *cursor = step(lines, count - 1, true),
        KeyCode::End => *cursor = step(lines, 0, false),
        // FAR's `KEY_PGUP` / `KEY_PGDN`: the lines shown, not past the ends, then off a separator.
        KeyCode::PageUp => {
            let at = cursor.saturating_sub(page);
            *cursor = (selectable(lines, at, true).or_else(|| selectable(lines, at, false)))
                .unwrap_or(*cursor);
        }
        KeyCode::PageDown => {
            let at = (*cursor + page).min(count - 1);
            *cursor = (selectable(lines, at, false).or_else(|| selectable(lines, at, true)))
                .unwrap_or(*cursor);
        }
        _ => {}
    }
}

/// The next (or previous) line that is no separator, round the ends.
fn step(lines: &[ListLine], from: usize, forward: bool) -> usize {
    let count = lines.len();
    let mut at = from;
    for _ in 0..count {
        at = if forward {
            (at + 1) % count
        } else {
            (at + count - 1) % count
        };
        if !lines[at].is_separator() {
            break;
        }
    }
    at
}

/// Line `at`, or the first one after it (before it) that is no separator, not round the ends.
fn selectable(lines: &[ListLine], at: usize, forward: bool) -> Option<usize> {
    if forward {
        (at..lines.len()).find(|&line| !lines[line].is_separator())
    } else {
        (0..=at.min(lines.len().saturating_sub(1)))
            .rev()
            .find(|&line| !lines[line].is_separator())
    }
}

/// Flips the mark of line `at` when it takes one.
fn mark(lines: &mut [ListLine], at: usize) -> bool {
    match lines.get_mut(at) {
        Some(ListLine::Marked { checked, .. }) => {
            *checked = !*checked;
            true
        }
        _ => false,
    }
}

/// Line `at` chosen: a toggle line flips its mark and keeps the earlier choice.
fn choose(lines: &mut [ListLine], at: usize, chosen: &mut usize) -> bool {
    match lines.get_mut(at) {
        Some(ListLine::Toggle { checked, .. }) => *checked = !*checked,
        Some(ListLine::Choice(_) | ListLine::Marked { .. }) => *chosen = at,
        _ => return false,
    }
    true
}

fn line_hotkey(line: &ListLine) -> Option<char> {
    hotkey_of(line.label()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grey_line_stops_the_cursor_and_is_never_chosen() {
        let mut lines = vec![
            ListLine::Grayed("&All".to_string()),
            ListLine::Choice("&Root".to_string()),
        ];
        let (mut open, mut chosen) = (OpenList::at(1), 1);
        move_cursor(&lines, &mut open, KeyEvent::from(KeyCode::Up));
        assert_eq!(open.cursor, 0, "the cursor stops on it");
        let enter = KeyEvent::from(KeyCode::Enter);
        assert!(
            !list_key(&mut lines, &mut open, &mut chosen, enter),
            "the list stays"
        );
        assert_eq!(chosen, 1);
        let hotkey = KeyEvent::from(KeyCode::Char('a'));
        assert!(!list_key(&mut lines, &mut open, &mut chosen, hotkey));
        assert_eq!(chosen, 1, "nor by its hotkey");
    }
}
