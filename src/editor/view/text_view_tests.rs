//! A view's screen and its keys, on files in memory and on one of 10 GB made up as it is read.

use super::*;
use crate::editor::view::BLOCK;

fn view_of(bytes: &[u8]) -> TextView {
    let mut view = TextView::new(bytes.len() as u64);
    for (index, chunk) in bytes.chunks(BLOCK as usize).enumerate() {
        view.insert(index as u64, chunk.to_vec());
    }
    view.set_encoding(Encoding::Utf8, 0);
    view
}

fn numbered(count: usize) -> Vec<u8> {
    (1..=count)
        .flat_map(|n| format!("line {n}\n").into_bytes())
        .collect()
}

fn rows(view: &TextView, width: usize, height: usize) -> Vec<String> {
    view.screen(width, height).rows
}

#[test]
fn the_screen_shows_rows_from_the_top_and_knows_the_end() {
    let view = view_of(b"one\ntwo\nthree");
    let screen = view.screen(10, 5);
    assert_eq!(screen.rows, ["one", "two", "three"]);
    assert!(screen.at_end);
    assert!(!view.screen(10, 2).at_end);
}

#[test]
fn long_lines_wrap_or_are_cut() {
    let mut view = view_of(b"abcdefgh\nxy");
    assert_eq!(rows(&view, 3, 5), ["abc", "def", "gh", "xy"]);
    view.wrap = false;
    assert_eq!(rows(&view, 3, 5), ["abc", "xy"]);
    view.apply(Move::Right(2), 3, 5).unwrap();
    assert_eq!(rows(&view, 3, 5), ["cde", ""]);
    view.apply(Move::LineStarts, 3, 5).unwrap();
    assert_eq!(rows(&view, 3, 5)[0], "abc");
}

#[test]
fn up_and_down_go_a_row_and_stop_at_the_ends() {
    let mut view = view_of(&numbered(5));
    view.apply(Move::Up, 10, 3).unwrap();
    assert_eq!(rows(&view, 10, 3)[0], "line 1", "nothing above the start");
    view.apply(Move::Down, 10, 3).unwrap();
    assert_eq!(rows(&view, 10, 3)[0], "line 2");
    for _ in 0..5 {
        view.apply(Move::Down, 10, 3).unwrap();
    }
    assert_eq!(
        rows(&view, 10, 3),
        ["line 3", "line 4", "line 5"],
        "Down stops with the end on screen"
    );
    view.apply(Move::Up, 10, 3).unwrap();
    assert_eq!(rows(&view, 10, 3)[0], "line 2");
}

/// S2d.6e: FAR's word wrap (Shift+F2) - the rows at the words, stepped over and painted from their
/// own first cell.
#[test]
fn rows_wrapped_at_the_words_are_shown_and_stepped() {
    let mut view = view_of(b"one two three four\nz\n");
    view.set_words(true);
    assert_eq!(rows(&view, 9, 5), ["one two", "three", "four", "z"]);
    let screen = view.screen(9, 5);
    assert_eq!(
        view.rows(&screen, 9)[1],
        (0, 8),
        "the second row from cell 8"
    );
    view.apply(Move::Down, 9, 2).unwrap();
    assert_eq!(rows(&view, 9, 2), ["three", "four"]);
    view.apply(Move::Down, 9, 2).unwrap();
    view.apply(Move::Up, 9, 2).unwrap();
    assert_eq!(rows(&view, 9, 2)[0], "three");
    view.set_wrap(false);
    view.set_words(true);
    assert!(view.wrap, "Shift+F2 wraps");
}

#[test]
fn wrapped_rows_are_stepped_one_by_one() {
    let mut view = view_of(b"abcdef\nxy\nz\n");
    view.apply(Move::Down, 3, 2).unwrap();
    assert_eq!(rows(&view, 3, 2), ["def", "xy"]);
    view.apply(Move::Down, 3, 2).unwrap();
    view.apply(Move::Up, 3, 2).unwrap();
    view.apply(Move::Up, 3, 2).unwrap();
    assert_eq!(rows(&view, 3, 2), ["abc", "def"]);
}

#[test]
fn pages_keep_one_row_as_far_s() {
    let mut view = view_of(&numbered(20));
    view.apply(Move::PageDown, 10, 5).unwrap();
    assert_eq!(
        rows(&view, 10, 5)[0],
        "line 5",
        "the last row shown comes first"
    );
    view.apply(Move::PageUp, 10, 5).unwrap();
    assert_eq!(rows(&view, 10, 5)[0], "line 1");
}

#[test]
fn end_puts_the_last_line_on_the_last_row() {
    let mut view = view_of(&numbered(20));
    view.apply(Move::End, 10, 5).unwrap();
    assert_eq!(
        rows(&view, 10, 5),
        ["line 17", "line 18", "line 19", "line 20"],
        "a break at the end: FAR leaves its empty row"
    );
    let mut unbroken = view_of(b"a\nb\nc\nd\ne\nf");
    unbroken.apply(Move::End, 10, 3).unwrap();
    assert_eq!(rows(&unbroken, 10, 3), ["d", "e", "f"]);
    unbroken.apply(Move::Home, 10, 3).unwrap();
    assert_eq!(rows(&unbroken, 10, 3)[0], "a");
}

#[test]
fn a_byte_order_mark_is_not_shown() {
    let mut view = view_of(b"\xEF\xBB\xBFhi\nthere\n");
    view.set_encoding(Encoding::Utf8, 3);
    assert_eq!(rows(&view, 10, 3), ["hi", "there"]);
    view.apply(Move::End, 10, 3).unwrap();
    view.apply(Move::Home, 10, 3).unwrap();
    assert_eq!(rows(&view, 10, 3)[0], "hi");
}

#[test]
fn a_new_width_keeps_the_top_s_text() {
    let mut view = view_of(b"abcdefghij\n");
    view.apply(Move::Down, 2, 2).unwrap();
    view.apply(Move::Down, 2, 2).unwrap();
    assert_eq!(rows(&view, 2, 2)[0], "ef");
    assert_eq!(
        rows(&view, 4, 2)[0],
        "efgh",
        "the row that holds the same byte"
    );
}

#[test]
fn a_missing_block_moves_nothing_and_is_named() {
    let mut view = TextView::new(3 * BLOCK);
    view.insert(0, vec![b'a'; BLOCK as usize]);
    view.set_encoding(Encoding::Utf8, 0);
    assert_eq!(view.apply(Move::End, 10, 3), Err(Missing(2)));
    assert_eq!(view.screen(10, 3).rows.len(), 3, "the first block's rows");
}

#[test]
fn a_file_that_grows_or_shrinks_is_followed() {
    let mut view = view_of(b"a\nb\n");
    view.apply(Move::Down, 10, 1).unwrap();
    view.set_len(1);
    assert_eq!(view.screen(10, 1).missing, Some(Missing(0)), "read again");
    view.insert(0, b"a".to_vec());
    assert_eq!(rows(&view, 10, 1), ["a"], "the top went back to the start");
}

#[test]
fn a_file_that_shrank_is_shown_from_its_start() {
    let mut view = view_of(&numbered(10));
    view.apply(Move::Down, 10, 2).unwrap();
    let shorter = numbered(5);
    view.set_len(shorter.len() as u64);
    view.insert(0, shorter);
    assert_eq!(rows(&view, 10, 2)[0], "line 1");
}

#[test]
fn utf16_of_one_byte_goes_nowhere() {
    let mut view = view_of(b"x");
    view.set_encoding(Encoding::Utf16Le, 0);
    view.apply(Move::End, 20, 3).unwrap();
    view.apply(Move::Up, 20, 3).unwrap();
    assert_eq!(rows(&view, 20, 3), ["\u{FFFD}"]);
}

#[test]
fn utf16_of_an_odd_length_ends_on_its_units() {
    // Longer than the look back: the last line is found by its break, not 40 000 bytes back.
    let mut text: Vec<u8> = String::from_utf8(numbered(5000))
        .unwrap()
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    text.push(b'x');
    let mut view = view_of(&text);
    view.set_encoding(Encoding::Utf16Le, 0);
    view.apply(Move::End, 20, 3).unwrap();
    let shown = rows(&view, 20, 3);
    assert_eq!(shown[0], "line 4999", "{shown:?}");
    assert_eq!(shown[1], "line 5000");
    assert_eq!(shown[2], "\u{FFFD}", "the odd byte");
}

/// A 10 GB file made up as read: lines of 15 letters and a break.
fn made_up(index: u64) -> Vec<u8> {
    (index * BLOCK..(index + 1) * BLOCK)
        .map(|at| {
            if at % 16 == 15 {
                b'\n'
            } else {
                b'a' + (at / 16 % 26) as u8
            }
        })
        .collect()
}

#[test]
fn a_huge_file_opens_ends_and_comes_back_reading_a_few_blocks() {
    const LEN: u64 = 10 * 1024 * 1024 * 1024;
    let mut view = TextView::new(LEN);
    let mut reads = 0;
    let mut settle = |view: &mut TextView, step: Option<Move>| loop {
        let result = match step {
            Some(step) => view.apply(step, 20, 10),
            None => view.screen(20, 10).missing.map_or(Ok(()), Err),
        };
        match result {
            Ok(()) => break,
            Err(Missing(index)) => {
                view.insert(index, made_up(index));
                reads += 1;
            }
        }
    };
    view.set_encoding(Encoding::Utf8, 0);
    settle(&mut view, None);
    settle(&mut view, Some(Move::End));
    settle(&mut view, None);
    let screen = view.screen(20, 10);
    let last_line = (LEN / 16 - 1) % 26;
    let letter = char::from(b'a' + last_line as u8);
    assert_eq!(screen.rows[8], letter.to_string().repeat(15));
    assert!(screen.at_end);
    settle(&mut view, Some(Move::Home));
    assert_eq!(view.screen(20, 10).rows[0], "a".repeat(15));
    assert!(reads <= 3, "{reads} blocks read for 10 GB");
}

/// The screen tells its first row's byte (FAR's `FilePos`): a line's start, or a wrapped row's.
#[test]
fn the_screen_tells_its_first_rows_byte() {
    let mut view = view_of(b"abcdefgh\nxy\n");
    assert_eq!(view.screen(3, 2).position, 0);
    view.apply(Move::Down, 3, 2).unwrap();
    assert_eq!(
        view.screen(3, 2).position,
        3,
        "the second row of the first line"
    );
    view.apply(Move::Down, 3, 2).unwrap();
    assert_eq!(view.screen(3, 2).position, 6, "its third row");
    view.apply(Move::Down, 3, 1).unwrap();
    assert_eq!(view.screen(3, 1).position, 9, "the next line");
}

/// FAR's F2: unwrapped from its line's start, wrapped again from column 0.
#[test]
fn wrap_is_set_from_the_line_s_start() {
    let mut view = view_of(b"abcdefgh\nxy\n");
    view.apply(Move::Down, 3, 2).unwrap();
    view.set_wrap(false);
    assert_eq!(rows(&view, 3, 2), ["abc", "xy"], "from the line's start");
    view.apply(Move::Right(4), 3, 2).unwrap();
    assert_eq!(view.left(), 4);
    view.set_wrap(true);
    assert_eq!(view.left(), 0);
    assert_eq!(rows(&view, 3, 2), ["abc", "def"]);
}

/// FAR's Ctrl+PgUp / Ctrl+PgDn keep the columns; Home and End go back to the first.
#[test]
fn start_and_finish_keep_the_columns() {
    let mut view = view_of(&numbered(20));
    view.set_wrap(false);
    view.apply(Move::Right(2), 10, 3).unwrap();
    view.apply(Move::Finish, 10, 3).unwrap();
    assert_eq!((view.left(), view.screen(10, 3).at_end), (2, true));
    view.apply(Move::Start, 10, 3).unwrap();
    assert_eq!(view.left(), 2);
    assert_eq!(rows(&view, 10, 3)[0], "ne 1");
    view.apply(Move::End, 10, 3).unwrap();
    assert_eq!(view.left(), 0);
}

/// Another code page: the screen from its line's start, read anew in it.
#[test]
fn another_code_page_reads_from_the_line_s_start() {
    let mut view = view_of("абвгде\n".as_bytes());
    view.apply(Move::Down, 3, 1).unwrap();
    assert_eq!(rows(&view, 3, 1), ["где"]);
    assert_eq!(view.screen(3, 1).position, 6);
    view.change_encoding(Encoding::Utf16Le);
    assert_eq!(view.screen(3, 1).position, 0);
}
