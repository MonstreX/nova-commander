//! Text cut to a width (R57: one set for the dialogs and the screen), as FAR's `strmix` has them: a
//! cut text shows `…` where it lost characters. Widths count the screen's cells as the view does
//! (ADR-0025, R7 review E14): a wide character takes two, a combining one none; one that would cross
//! the edge is left out whole.

const DOTS: char = '…';

/// How many cells the screen gives `ch` (ADR-0025: `ratatui`'s widths): 0 for a combining one, 2 for a
/// wide one; a control character's glyph takes one.
pub fn char_cells(ch: char) -> usize {
    unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1)
}

/// How many cells `text` takes on the screen.
pub fn cells(text: &str) -> usize {
    text.chars().map(char_cells).sum()
}

/// The longest start of `text` that fits in `room` cells.
pub fn head(text: &str, room: usize) -> &str {
    let mut used = 0;
    for (at, ch) in text.char_indices() {
        used += char_cells(ch);
        if used > room {
            return &text[..at];
        }
    }
    text
}

/// The longest end of `text` that fits in `room` cells.
pub fn tail(text: &str, room: usize) -> &str {
    let mut used = 0;
    for (at, ch) in text.char_indices().rev() {
        used += char_cells(ch);
        if used > room {
            return &text[at + ch.len_utf8()..];
        }
    }
    text
}

/// `text` with blanks after it to `width` cells (`{:<width$}` by cells).
pub fn left_aligned(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(cells(text))))
}

/// `text` with blanks before it to `width` cells (`{:>width$}` by cells).
pub fn right_aligned(text: &str, width: usize) -> String {
    format!("{}{text}", " ".repeat(width.saturating_sub(cells(text))))
}

/// FAR's `cut_right`: the start that fits in `width`, no mark.
pub fn cut_right(text: &str, width: usize) -> String {
    head(text, width).to_string()
}

/// FAR's `truncate_right`: the start kept, `…` last.
pub fn truncate_right(text: &str, width: usize) -> String {
    cut(text, width, |room| (room, 0))
}

/// FAR's `truncate_left`: `…` first, the end kept.
pub fn truncate_left(text: &str, width: usize) -> String {
    cut(text, width, |room| (0, room))
}

/// FAR's `truncate_center`: both ends kept, `…` between (the start one shorter of an odd room).
pub fn truncate_center(text: &str, width: usize) -> String {
    cut(text, width, |room| (room / 2, room - room / 2))
}

/// FAR's `truncate_path`: the root (`C:\`, `\\server\share\`, `/`) kept as far as it fits, `…`, the end
/// in what is left.
pub fn truncate_path(path: &str, width: usize) -> String {
    let root = root_cells(path);
    cut(path, width, |room| (root.min(room), room - root.min(room)))
}

/// FAR's `fit_to_left`: cut as [`truncate_right`], padded to `width` on the right.
pub fn fit_to_left(text: &str, width: usize) -> String {
    left_aligned(&truncate_right(text, width), width)
}

/// FAR's `fit_to_right`: cut as [`truncate_right`], padded to `width` on the left.
pub fn fit_to_right(text: &str, width: usize) -> String {
    right_aligned(&truncate_right(text, width), width)
}

/// `text` whole when it fits in `width`; none without room; else `head` cells of its start, `…`,
/// `tail` of its end, as `split` shares the room left beside `…` between them.
fn cut(text: &str, width: usize, split: impl Fn(usize) -> (usize, usize)) -> String {
    if cells(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let (start, end) = split(width - 1);
    format!("{}{DOTS}{}", head(text, start), tail(text, end))
}

/// Cells of the path's prefix and root folder (`C:\`, `\\server\share\`, `/`): FAR's `ParsePath`'s
/// offset.
fn root_cells(path: &str) -> usize {
    use std::path::{Component, Path};
    Path::new(path)
        .components()
        .take_while(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
        .map(|component| match component {
            Component::RootDir => 1,
            other => cells(&other.as_os_str().to_string_lossy()),
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ends_and_the_middle() {
        assert_eq!(cut_right("abcdef", 4), "abcd");
        assert_eq!(truncate_right("abcdef", 4), "abc…");
        assert_eq!(truncate_left("abcdef", 4), "…def");
        assert_eq!(truncate_center("abcdefghij", 5), "ab…ij");
        assert_eq!(truncate_center("abcdefghij", 6), "ab…hij");
        assert_eq!(fit_to_left("ab", 4), "ab  ");
        assert_eq!(fit_to_right("abcdef", 4), "abc…");
        assert_eq!(fit_to_right("ab", 4), "  ab");
    }

    #[test]
    fn a_text_that_fits_stays_and_no_room_leaves_nothing() {
        for cutting in [
            truncate_right,
            truncate_left,
            truncate_center,
            truncate_path,
        ] {
            assert_eq!(cutting("файл", 4), "файл");
            assert_eq!(cutting("файл", 0), "");
            assert_eq!(cutting("файл", 1), "…");
        }
        assert_eq!(fit_to_left("x", 0), "");
    }

    /// R7 review E14: a wide character takes two cells, a combining one none; one that would cross the
    /// edge is left out and the rest padded.
    #[test]
    fn widths_are_the_screen_s_cells() {
        assert_eq!(cells("日本語"), 6);
        assert_eq!(cells("e\u{301}"), 1);
        assert_eq!(cut_right("日本語", 5), "日本");
        assert_eq!(truncate_right("日本語.txt", 6), "日本…");
        assert_eq!(cells(&truncate_right("日本語.txt", 6)), 5);
        assert_eq!(truncate_left("日本語.txt", 6), "….txt");
        assert_eq!(truncate_left("x日本語", 6), "…本語");
        assert_eq!(fit_to_left("日本語.txt", 6), "日本… ");
        assert_eq!(fit_to_right("日本", 6), "  日本");
        assert_eq!(truncate_right("e\u{301}e\u{301}", 2), "e\u{301}e\u{301}");
        assert_eq!(head("ab日", 3), "ab");
        assert_eq!(tail("日ab", 3), "ab");
        assert_eq!(left_aligned("日", 3), "日 ");
        assert_eq!(right_aligned("日", 1), "日", "too wide: as it is");
    }

    #[cfg(windows)]
    #[test]
    fn a_path_keeps_its_root() {
        assert_eq!(truncate_path(r"C:\one\two\three", 12), r"C:\…wo\three");
        assert_eq!(truncate_path(r"C:\one\two\three", 4), r"C:\…");
        assert_eq!(truncate_path(r"C:\one\two\three", 2), "C…");
        assert_eq!(
            truncate_path(r"dir\one\two", 6),
            r"…e\two",
            "no root: the end only"
        );
        assert_eq!(truncate_path(r"C:\日本\語語語", 8), r"C:\…語語");
    }
}
