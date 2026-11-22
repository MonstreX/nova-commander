//! The hex cursor's moves and typing (VC's editor in hex).

use super::*;

/// A one-byte page in miniature: bytes as Latin-1, 0x81 a lead byte.
#[derive(Debug)]
struct Latin;

impl super::super::code_page::ByteDecoder for Latin {
    fn encode(&self, text: &str) -> Vec<u8> {
        text.chars()
            .map(|ch| u8::try_from(ch).unwrap_or(b'?'))
            .collect()
    }

    fn decode(&self, bytes: &[u8]) -> (char, usize) {
        match bytes {
            [0x81, ..] => ('?', 2),
            [byte, ..] => (char::from(*byte), 1),
            [] => ('?', 1),
        }
    }
}

fn latin() -> Encoding {
    Encoding::Page(1252, std::sync::Arc::new(Latin))
}

#[test]
fn right_and_left_go_by_half_a_byte_in_the_hex_column() {
    let mut cursor = HexCursor::at(0, 40);
    cursor.moved(HexMove::Right, 16, 10, 40);
    assert_eq!((cursor.pos, cursor.low), (0, true));
    cursor.moved(HexMove::Right, 16, 10, 40);
    assert_eq!((cursor.pos, cursor.low), (1, false));
    cursor.moved(HexMove::Left, 16, 10, 40);
    assert_eq!((cursor.pos, cursor.low), (0, true));
    cursor.moved(HexMove::Left, 16, 10, 40);
    cursor.moved(HexMove::Left, 16, 10, 40);
    assert_eq!(
        (cursor.pos, cursor.low),
        (0, false),
        "stops at the first byte"
    );
    cursor.switch_column();
    cursor.moved(HexMove::Right, 16, 10, 40);
    assert_eq!(
        (cursor.pos, cursor.low),
        (1, false),
        "a byte in the text column"
    );
    cursor.moved(HexMove::Left, 16, 10, 40);
    assert_eq!((cursor.pos, cursor.low), (0, false));
}

#[test]
fn rows_pages_and_ends_stay_on_the_bytes() {
    let mut cursor = HexCursor::at(5, 40);
    cursor.moved(HexMove::Down, 16, 3, 40);
    assert_eq!(cursor.pos, 21);
    cursor.moved(HexMove::Down, 16, 3, 40);
    assert_eq!(cursor.pos, 37);
    cursor.moved(HexMove::Down, 16, 3, 40);
    assert_eq!(cursor.pos, 37, "the last row");
    cursor.moved(HexMove::End, 16, 3, 40);
    assert_eq!(cursor.pos, 39, "the last byte ends the last row");
    cursor.moved(HexMove::Home, 16, 3, 40);
    assert_eq!(cursor.pos, 32);
    cursor.moved(HexMove::PageUp, 16, 3, 40);
    assert_eq!(cursor.pos, 0, "two rows up");
    cursor.moved(HexMove::PageDown, 16, 3, 40);
    assert_eq!(cursor.pos, 32);
    cursor.moved(HexMove::Up, 16, 3, 40);
    assert_eq!(cursor.pos, 16);
    cursor.moved(HexMove::Finish, 16, 3, 40);
    assert_eq!(cursor.pos, 39);
    cursor.moved(HexMove::Start, 16, 3, 40);
    assert_eq!(cursor.pos, 0);
    // A row down ending short of the last row's column: on the last byte.
    let mut cursor = HexCursor::at(30, 40);
    cursor.moved(HexMove::Down, 16, 3, 40);
    assert_eq!(cursor.pos, 39);
    assert_eq!(HexCursor::at(99, 40).pos, 39);
    assert_eq!(HexCursor::at(5, 0).pos, 0);
}

#[test]
fn a_hex_digit_sets_the_high_half_then_the_low_one() {
    let mut cursor = HexCursor::at(0, 2);
    let page = latin();
    assert_eq!(cursor.typed('a', 0x12, &page, 2), Some((0, vec![0xA2])));
    assert!(cursor.low);
    assert_eq!(cursor.typed('F', 0xA2, &page, 2), Some((0, vec![0xAF])));
    assert_eq!((cursor.pos, cursor.low), (1, false));
    assert_eq!(cursor.typed('g', 0x00, &page, 2), None, "not a hex digit");
    cursor.typed('1', 0x00, &page, 2);
    cursor.typed('2', 0x10, &page, 2);
    assert_eq!(
        (cursor.pos, cursor.low),
        (1, true),
        "the last byte keeps it"
    );
    assert_eq!(
        HexCursor::at(0, 0).typed('1', 0, &page, 0),
        None,
        "nothing to type over"
    );
}

#[test]
fn a_character_in_the_text_column_writes_its_bytes() {
    let mut cursor = HexCursor::at(0, 3);
    cursor.switch_column();
    assert_eq!(cursor.typed('é', 0, &latin(), 3), Some((0, vec![0xE9])));
    assert_eq!(cursor.pos, 1);
    assert_eq!(
        cursor.typed('ж', 0, &Encoding::Utf8, 3),
        Some((1, vec![0xD0, 0xB6]))
    );
    assert_eq!(cursor.pos, 2, "the last byte");
    assert_eq!(
        cursor.typed('ж', 0, &Encoding::Utf8, 3),
        None,
        "past the end"
    );
}

#[test]
fn the_text_column_shows_a_byte_s_character() {
    assert_eq!(byte_char(0, &latin()), '.');
    assert_eq!(byte_char(b'A', &latin()), 'A');
    assert_eq!(byte_char(0xE9, &latin()), 'é');
    assert_eq!(byte_char(0x81, &latin()), '.', "a lead byte alone");
    assert_eq!(byte_char(1, &Encoding::Utf8), '☺');
    assert_eq!(byte_char(0xD0, &Encoding::Utf8), '.');
}
