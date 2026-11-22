//! Lines laid out from a byte, and the line before found by looking back.

use super::*;
use crate::editor::view::BLOCK;
use crate::editor::view::blocks::Blocks;

pub(in crate::editor::view) fn loaded(bytes: &[u8]) -> Blocks {
    let mut blocks = Blocks::new(bytes.len() as u64);
    for (index, chunk) in bytes.chunks(BLOCK as usize).enumerate() {
        blocks.insert(index as u64, chunk.to_vec());
    }
    blocks
}

fn line(bytes: &[u8], start: u64) -> Line {
    read_line(&loaded(bytes), &Encoding::Utf8, start, Layout::default()).unwrap()
}

fn text(line: &Line) -> String {
    line.cells.iter().collect()
}

#[test]
fn a_line_ends_at_any_break() {
    let bytes = b"one\r\ntwo\nthree\rfour";
    let first = line(bytes, 0);
    assert_eq!((text(&first), first.end), ("one".to_string(), 5));
    let second = line(bytes, 5);
    assert_eq!((text(&second), second.end), ("two".to_string(), 9));
    let third = line(bytes, 9);
    assert_eq!((text(&third), third.end), ("three".to_string(), 15));
    let last = line(bytes, 15);
    assert_eq!((text(&last), last.end), ("four".to_string(), 19));
}

#[test]
fn tabs_go_to_their_stops_and_controls_show_as_glyphs() {
    let tabbed = line(b"ab\tc\x01", 0);
    assert_eq!(text(&tabbed), "ab      c☺");
    assert_eq!(tabbed.offsets[2..8], [2; 6], "the tab's cells are its byte");
    assert_eq!(tabbed.offsets[8], 3);
}

#[test]
fn a_line_without_breaks_is_cut_at_the_longest() {
    let bytes = vec![b'x'; MAX_LINE + 5];
    let first = line(&bytes, 0);
    assert_eq!(first.cells.len(), MAX_LINE);
    assert_eq!(first.end, MAX_LINE as u64);
    assert_eq!(line(&bytes, first.end).cells.len(), 5);
}

#[test]
fn rows_wrap_a_line_and_find_a_byte() {
    let wrapped = line(b"abcdefg", 0);
    assert_eq!(wrapped.rows(3, false), 3);
    assert_eq!(wrapped.row_start(1, 3, false), 3);
    assert_eq!(wrapped.row_of(4, 3, false), 1);
    assert_eq!(wrapped.row_of(6, 3, false), 2);
    assert_eq!(wrapped.text(3, 3), "def");
    assert_eq!(line(b"", 0).rows(3, false), 1, "an empty line is a row");
    assert_eq!(line(b"", 0).row_bounds(3, true), [(0, 0)]);
}

/// S2d.6e: FAR's word wrap - after a blank or a divider, the blanks at the row's end not shown and at
/// the next one's start skipped; a word longer than the row cut at the width.
#[test]
fn rows_wrap_at_the_words() {
    let text = line(b"one two three", 0);
    assert_eq!(text.row_bounds(8, true), [(0, 7), (8, 13)]);
    assert_eq!(text.row_start(1, 8, true), 8);
    assert_eq!(text.row_of(9, 8, true), 1);
    assert_eq!(text.rows(8, true), 2);
    let long = line(b"abcdefghij", 0);
    assert_eq!(long.row_bounds(4, true), [(0, 4), (4, 8), (8, 10)]);
    let spaced = line(b"ab   cd", 0);
    assert_eq!(
        spaced.row_bounds(3, true),
        [(0, 2), (5, 7)],
        "a blank past the row"
    );
    let divided = line(b"a,b,cdef", 0);
    assert_eq!(
        divided.row_bounds(5, true),
        [(0, 4), (4, 8)],
        "after the divider"
    );
    assert_eq!(
        line(b"ab  ", 0).row_bounds(2, true),
        [(0, 2)],
        "trailing blanks skipped"
    );
}

#[test]
fn a_character_across_two_blocks_is_whole() {
    let mut bytes = vec![b'a'; BLOCK as usize - 1];
    bytes.extend_from_slice("я\n".as_bytes());
    let blocks = loaded(&bytes);
    let cut = read_line(&blocks, &Encoding::Utf8, BLOCK - 1, Layout::default()).unwrap();
    assert_eq!(cut.cells, ['я']);
    assert_eq!(cut.end, BLOCK + 2);
}

#[test]
fn the_block_a_line_needs_is_named() {
    let mut blocks = Blocks::new(2 * BLOCK);
    blocks.insert(0, vec![b'a'; BLOCK as usize]);
    assert_eq!(
        read_line(&blocks, &Encoding::Utf8, BLOCK - 5, Layout::default()),
        Err(Missing(1)),
        "the line goes on past the first block"
    );
}

fn before(bytes: &[u8], pos: u64) -> u64 {
    line_before(&loaded(bytes), &Encoding::Utf8, pos, 0, Layout::default()).unwrap()
}

#[test]
fn the_line_before_starts_after_the_break_before_it() {
    let bytes = b"one\r\ntwo\n\nfour";
    assert_eq!(before(bytes, 5), 0, "before two");
    assert_eq!(before(bytes, 9), 5, "before the empty line");
    assert_eq!(before(bytes, 10), 9, "the empty line");
    assert_eq!(before(bytes, 14), 10, "the last line, no break at the end");
    assert_eq!(before(b"a\rb", 2), 0, "a lone CR");
}

#[test]
fn the_line_before_a_cut_one_is_the_piece_before_it() {
    let mut bytes = b"start\n".to_vec();
    bytes.extend(vec![b'x'; MAX_LINE * 2 + 3]);
    let first = 6;
    let second = first + MAX_LINE as u64;
    let third = second + MAX_LINE as u64;
    assert_eq!(before(&bytes, third), second);
    assert_eq!(before(&bytes, second), first);
    assert_eq!(before(&bytes, bytes.len() as u64), third);
}

#[test]
fn utf16_lines_break_on_their_units() {
    let bytes: Vec<u8> = "ab\r\ncd\nef"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let blocks = loaded(&bytes);
    let first = read_line(&blocks, &Encoding::Utf16Le, 0, Layout::default()).unwrap();
    assert_eq!((text(&first), first.end), ("ab".to_string(), 8));
    assert_eq!(
        line_before(&blocks, &Encoding::Utf16Le, 14, 0, Layout::default()).unwrap(),
        8,
        "before ef"
    );
    assert_eq!(
        line_before(&blocks, &Encoding::Utf16Le, 8, 0, Layout::default()).unwrap(),
        0
    );
}

#[test]
fn looking_back_crosses_blocks() {
    let mut bytes = vec![b'z'; BLOCK as usize - 100];
    bytes.push(b'\n');
    let start = bytes.len() as u64;
    bytes.extend(vec![b'y'; 200]);
    bytes.push(b'\n');
    assert_eq!(before(&bytes, bytes.len() as u64), start);
}

#[test]
fn a_wide_character_takes_two_cells_and_a_combining_one_none() {
    // "a", a CJK character (3 bytes, two columns), "e" with a combining acute (2 bytes), "b".
    let wide = line("a中e\u{301}b".as_bytes(), 0);
    assert_eq!(wide.cells, ['a', '中', WIDE_TAIL, 'e', 'b']);
    assert_eq!(
        wide.offsets,
        [0, 1, 1, 4, 7],
        "the tail is the character's byte"
    );
    assert_eq!(
        wide.char_cells,
        [0, 1, 3, 3, 4],
        "the combining one joins its cell"
    );
    assert_eq!(wide.joined, [(3, '\u{301}')]);
    assert_eq!(
        wide.text(0, 5),
        "a中e\u{301}b",
        "the screen's text: its own widths"
    );
    assert_eq!(
        wide.text(2, 3),
        " e\u{301}b",
        "a row from the tail: a space there"
    );
    assert_eq!(
        wide.text(0, 2),
        "a ",
        "a row that cannot hold it whole: a space"
    );
    assert_eq!(wide.char_start_cell(2), 1);
    assert_eq!(
        wide.row_bounds(2, false),
        [(0, 1), (1, 3), (3, 5)],
        "wrapped before the wide character, not through it"
    );
    assert_eq!(wide.rows(2, false), 3);
    assert_eq!(wide.row_start(1, 2, false), 1);
    let joined_after_wide = line("中\u{301}".as_bytes(), 0);
    assert_eq!(
        joined_after_wide.joined,
        [(0, '\u{301}')],
        "with the wide one's first cell"
    );
    let alone = line("\u{301}a".as_bytes(), 0);
    assert_eq!(
        alone.cells,
        ['\u{301}', 'a'],
        "at the start: a cell of its own"
    );
}

/// A double-byte page: a byte from 0x81 begins a character of two, whose trail may be such a byte too.
#[derive(Debug)]
struct Double;

impl crate::editor::view::code_page::ByteDecoder for Double {
    fn decode(&self, bytes: &[u8]) -> (char, usize) {
        match bytes {
            [lead @ 0x81..=0xFF, trail, ..] => {
                let code = 0x4E00 + u32::from(*lead & 0x7F) * 256 + u32::from(*trail);
                (char::from_u32(code).unwrap_or('?'), 2)
            }
            [byte, ..] => (char::from(*byte), 1),
            [] => ('?', 1),
        }
    }

    fn encode(&self, text: &str) -> Vec<u8> {
        text.bytes().collect()
    }

    fn is_lead(&self, byte: u8) -> bool {
        byte >= 0x81
    }
}

#[test]
fn a_line_too_long_to_look_back_over_starts_on_a_double_byte_character_s_lead() {
    let page = Encoding::Page(936, std::sync::Arc::new(Double));
    // "a", double-byte characters whose trails are lead byte values too, past the window, and "b":
    // the window's start falls on a trail byte.
    let mut bytes = vec![b'a'];
    for _ in 0..40 {
        bytes.extend([0x81, 0x82]);
    }
    bytes.push(b'b');
    let layout = Layout {
        max_line: 10,
        ..Layout::default()
    };
    let blocks = loaded(&bytes);
    let len = bytes.len() as u64;
    let start = line_before(&blocks, &page, len, 0, layout).unwrap();
    assert_eq!(
        start % 2,
        1,
        "on a lead byte (they stand at odd bytes here): {start}"
    );
    let line = read_line(&blocks, &page, start, layout).unwrap();
    assert_eq!(line.chars.last(), Some(&'b'), "{:?}", line.chars);
}
