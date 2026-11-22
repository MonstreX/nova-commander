//! The code page told from the first block and decoded a character at a time.

use super::*;

/// A double-byte page in miniature: 0x81 leads, the pair decodes to '漢'.
#[derive(Debug)]
struct Double;

impl ByteDecoder for Double {
    fn encode(&self, text: &str) -> Vec<u8> {
        text.bytes().collect()
    }

    fn decode(&self, bytes: &[u8]) -> (char, usize) {
        match bytes {
            [0x81, 0x40, ..] => ('漢', 2),
            [0x81, ..] => ('?', 2),
            [byte, ..] => (char::from(*byte), 1),
            [] => ('?', 1),
        }
    }
}

#[test]
fn a_byte_order_mark_tells_and_is_skipped() {
    assert_eq!(detect(&[0xEF, 0xBB, 0xBF, b'a']), (Detected::Utf8, 3));
    assert_eq!(detect(&[0xFF, 0xFE, b'a', 0]), (Detected::Utf16Le, 2));
    assert_eq!(detect(&[0xFE, 0xFF, 0, b'a']), (Detected::Utf16Be, 2));
}

#[test]
fn utf8_is_told_by_what_decodes_past_ascii() {
    assert_eq!(detect("привет".as_bytes()), (Detected::Utf8, 0));
    assert_eq!(
        detect(b"plain text"),
        (Detected::System, 0),
        "ASCII: the default"
    );
    assert_eq!(
        detect(&[0xEF, 0xF0, 0xE8]),
        (Detected::System, 0),
        "cp1251 bytes"
    );
    let cut = &"привет".as_bytes()[..5];
    assert_eq!(
        detect(cut),
        (Detected::Utf8, 0),
        "a character cut by the block's end"
    );
}

#[test]
fn utf16_without_a_mark_is_told_by_its_zero_bytes() {
    let le: Vec<u8> = "hello world"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let be: Vec<u8> = "hello world"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    assert_eq!(detect(&le), (Detected::Utf16Le, 0));
    assert_eq!(detect(&be), (Detected::Utf16Be, 0));
    assert_eq!(
        detect(&[0, 0, 0, 0, 0, 0]),
        (Detected::System, 0),
        "all zeros"
    );
}

#[test]
fn characters_decode_or_wait_for_their_rest() {
    let utf8 = Encoding::Utf8;
    assert_eq!(utf8.decode("я!".as_bytes(), true), Some(('я', 2)));
    assert_eq!(
        utf8.decode(&"я".as_bytes()[..1], true),
        None,
        "more to come"
    );
    assert_eq!(
        utf8.decode(&"я".as_bytes()[..1], false),
        Some((char::REPLACEMENT_CHARACTER, 1)),
        "the file ends there"
    );
    assert_eq!(
        utf8.decode(&[0xFF, b'a'], true),
        Some((char::REPLACEMENT_CHARACTER, 1))
    );
    let le = Encoding::Utf16Le;
    assert_eq!(le.decode(&[b'a', 0, b'b', 0], true), Some(('a', 2)));
    let smile: Vec<u8> = "😀".encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert_eq!(le.decode(&smile, true), Some(('😀', 4)));
    assert_eq!(le.decode(&smile[..2], true), None);
    assert_eq!(le.unit(), 2);
    assert_eq!(le.unit_value(&[b'\n', 0]), u32::from(b'\n'));
    assert_eq!(Encoding::Utf16Be.unit_value(&[0, b'\n']), u32::from(b'\n'));
}

#[test]
fn a_system_page_takes_its_lead_bytes_with_the_next() {
    let page = Encoding::Page(932, Arc::new(Double));
    assert_eq!(page.decode(&[0x81, 0x40, b'a'], true), Some(('漢', 2)));
    assert_eq!(page.decode(&[0x81], true), None, "the trail byte to come");
    assert_eq!(
        page.decode(&[0x81], false),
        Some(('?', 1)),
        "cut by the end"
    );
    assert_eq!(page.decode(b"a", true), Some(('a', 1)));
}

#[test]
fn control_characters_show_as_far_s_glyphs() {
    assert_eq!(shown('\0'), ' ');
    assert_eq!(shown('\x01'), '☺');
    assert_eq!(shown('\x1b'), '←');
    assert_eq!(shown('\x7f'), '⌂');
    assert_eq!(shown('a'), 'a');
}
