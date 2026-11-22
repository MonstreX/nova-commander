//! Hex and dump as FAR shows them: the line's layout and widths, the bytes per line, the characters a
//! row's end cuts, the moves, and the place kept between the modes.

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

fn in_mode(bytes: &[u8], mode: Mode) -> TextView {
    let mut view = view_of(bytes);
    view.apply(Move::Mode(mode), 80, 5).unwrap();
    view
}

#[test]
fn the_widths_are_far_s() {
    // FAR's comment: 16 bytes take the address, three columns each, a stripe's "│ ", two, 16 chars.
    assert_eq!(hex_line_width(16, 1), 11 + 48 + 2 + 2 + 16);
    assert_eq!(hex_line_width(8, 1), 11 + 24 + 2 + 8);
    assert_eq!(hex_line_width(16, 2), 11 + 48 + 2 + 2 + 8);
    assert_eq!(hex_bytes_in(79, 1), 16);
    assert_eq!(hex_bytes_in(10, 1), 8, "a stripe at least");
    assert_eq!(hex_bytes_in(120, 1), 25);
    assert!(hex_line_width(25, 1) <= 120 && hex_line_width(26, 1) > 120);
}

#[test]
fn bytes_per_line_go_by_one_or_sixteen_to_their_multiples() {
    assert_eq!(hex_bytes_moved(16, 1, 200, 1), 17);
    assert_eq!(hex_bytes_moved(16, -1, 200, 1), 15);
    assert_eq!(hex_bytes_moved(8, -1, 200, 1), 8, "a stripe at least");
    assert_eq!(hex_bytes_moved(17, 16, 200, 1), 32);
    assert_eq!(hex_bytes_moved(17, -16, 200, 1), 16);
    assert_eq!(
        hex_bytes_moved(16, 1, 79, 1),
        16,
        "no more than the width holds"
    );
}

#[test]
fn a_hex_row_is_far_s() {
    let view = in_mode(b"0123456789abcdefXYZ", Mode::Hex);
    let screen = view.screen(80, 5);
    assert_eq!(
        screen.rows[0],
        "0000000000: 30 31 32 33 34 35 36 37 │ 38 39 61 62 63 64 65 66  0123456789abcdef"
    );
    assert!(
        screen.rows[1].starts_with("0000000010: 58 59 5A    "),
        "{:?}",
        screen.rows[1]
    );
    assert!(screen.rows[1].contains("│    "), "the missing bytes blank");
    assert!(screen.at_end);
    assert_eq!(screen.rows.len(), 2);
}

#[test]
fn a_character_cut_by_a_row_s_end_shows_its_bytes_continued() {
    let mut bytes = b"0123456789abcde".to_vec();
    bytes.extend("ж".as_bytes());
    bytes.extend(b"z");
    let view = in_mode(&bytes, Mode::Hex);
    let screen = view.screen(80, 5);
    assert!(
        screen.rows[0].ends_with("0123456789abcdeж"),
        "{:?}",
        screen.rows[0]
    );
    assert!(
        screen.rows[1].contains("  ›z"),
        "the second byte on the next row: {:?}",
        screen.rows[1]
    );
    let (text, tail) = characters(&Encoding::Utf8, &[b'a', 0xFF, b'b'], 3, 5, 0);
    assert_eq!((text.as_str(), tail), ("a\u{FFFD}b  ", 0));
}

#[test]
fn dump_rows_are_the_screen_s_width_of_characters() {
    let view = in_mode(b"abcdefghij\nklm", Mode::Dump);
    let screen = view.screen(4, 5);
    assert_eq!(screen.rows, ["abcd", "efgh", "ij◙k", "lm  "]);
    assert!(screen.at_end);
}

#[test]
fn hex_moves_by_lines_and_screens_and_ends_as_far() {
    let bytes: Vec<u8> = (0..100u8).collect();
    let mut view = in_mode(&bytes, Mode::Hex);
    view.apply(Move::Down, 80, 3).unwrap();
    assert_eq!(view.screen(80, 3).position, 16);
    view.apply(Move::PageDown, 80, 3).unwrap();
    assert_eq!(view.screen(80, 3).position, 48);
    view.apply(Move::Up, 80, 3).unwrap();
    assert_eq!(view.screen(80, 3).position, 32);
    // 100 bytes: the end's line starts at 96; FAR's End: that line last on the screen.
    view.apply(Move::End, 80, 3).unwrap();
    assert_eq!(view.screen(80, 3).position, 100 - 4 - 32);
    assert!(view.screen(80, 3).at_end);
    view.apply(Move::Down, 80, 3).unwrap();
    assert_eq!(view.screen(80, 3).position, 64, "nothing past the end");
    view.apply(Move::Roll(1), 80, 3).unwrap();
    assert_eq!(view.screen(80, 3).position, 65, "Ctrl+Right: a byte on");
    view.apply(Move::Home, 80, 3).unwrap();
    assert_eq!(view.screen(80, 3).position, 0);
    view.apply(Move::Roll(-1), 80, 3).unwrap();
    assert_eq!(view.screen(80, 3).position, 0);
}

#[test]
fn hex_scrolls_its_columns_and_changes_its_bytes() {
    let bytes: Vec<u8> = (0..64u8).collect();
    let mut view = in_mode(&bytes, Mode::Hex);
    view.apply(Move::Right(3), 40, 3).unwrap();
    assert!(
        view.screen(40, 3).rows[0].starts_with("0000000: "),
        "{:?}",
        view.screen(40, 3).rows[0]
    );
    view.apply(Move::Right(1000), 40, 3).unwrap();
    assert_eq!(
        view.left(),
        hex_line_width(16, 1) - 40,
        "no further than the line"
    );
    view.apply(Move::LineStarts, 40, 3).unwrap();
    view.apply(Move::Bytes(1), 200, 3).unwrap();
    assert!(
        view.screen(200, 3).rows[1].starts_with("0000000011: "),
        "17 bytes a line"
    );
}

#[test]
fn the_place_is_kept_between_text_and_hex() {
    let text: String = (1..=50).map(|n| format!("line {n}\n")).collect();
    let mut view = view_of(text.as_bytes());
    for _ in 0..3 {
        view.apply(Move::Down, 20, 5).unwrap();
    }
    let place = view.screen(20, 5).position;
    assert_eq!(place, 21, "line 4");
    view.apply(Move::Mode(Mode::Hex), 80, 5).unwrap();
    assert_eq!(view.screen(80, 5).position, place);
    view.apply(Move::Roll(3), 80, 5).unwrap();
    view.apply(Move::Mode(Mode::Text), 20, 5).unwrap();
    assert_eq!(
        view.screen(20, 5).rows[0],
        "line 4",
        "the line that shows byte 24"
    );
    view.apply(Move::Mode(Mode::Dump), 20, 5).unwrap();
    assert_eq!(view.screen(20, 5).position, 21);
}

/// Review A5: what a search found in hex is marked on its bytes' digits (FAR's `SelStart`), not from
/// the space before them.
#[test]
fn a_selection_in_hex_marks_its_bytes_digits() {
    let mut view = in_mode(b"0123456789abcdefghij", Mode::Hex);
    view.set_selection(Some((1, 2)));
    let screen = view.screen(80, 5);
    let row: Vec<char> = screen.rows[0].chars().collect();
    let (from, to) = screen.marks[0].unwrap();
    let marked: String = row[from..to].iter().collect();
    assert_eq!(marked, "31 32", "{:?}", screen.rows[0]);
    view.set_selection(Some((8, 1)));
    let screen = view.screen(80, 5);
    let row: Vec<char> = screen.rows[0].chars().collect();
    let (from, to) = screen.marks[0].unwrap();
    assert_eq!(
        row[from..to].iter().collect::<String>(),
        "38",
        "past the stripe"
    );
}
