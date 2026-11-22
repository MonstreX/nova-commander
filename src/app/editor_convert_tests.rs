//! Shift+F2's conversion: the code page, the signature and the line breaks as the bytes go through,
//! a character split between two writes kept whole.

use super::*;

fn converted(bytes: &[&[u8]], convert: Convert) -> Vec<u8> {
    let mut out = Vec::new();
    let mut converter = Converter::new(&mut out, convert).unwrap();
    for part in bytes {
        converter.write_all(part).unwrap();
    }
    converter.finish().unwrap();
    out
}

const UTF8: Convert = Convert {
    from: 65001,
    floor: 0,
    page: 65001,
    bom: false,
    eol: None,
};

#[test]
fn the_line_breaks_are_made_one_kind() {
    let text: &[u8] = b"a\r\nb\rc\nd";
    let to = |eol| converted(&[text], Convert { eol, ..UTF8 });
    assert_eq!(to(None), text, "as they were");
    assert_eq!(to(Some("\r\n")), b"a\r\nb\r\nc\r\nd");
    assert_eq!(to(Some("\n")), b"a\nb\nc\nd");
    assert_eq!(to(Some("\r")), b"a\rb\rc\rd");
    // A `\r\n` split between two writes is one break.
    let split = converted(
        &[b"a\r", b"\nb"],
        Convert {
            eol: Some("\n"),
            ..UTF8
        },
    );
    assert_eq!(split, b"a\nb");
}

#[test]
fn the_code_page_and_the_signature_change() {
    // UTF-8 with its signature, a two-byte character split between writes, to UTF-16 LE with its own.
    let text = "\u{feff}é!".as_bytes();
    let utf16 = Convert {
        floor: 3,
        page: 1200,
        bom: true,
        ..UTF8
    };
    let out = converted(&[&text[..4], &text[4..]], utf16);
    assert_eq!(out, [0xFF, 0xFE, 0xE9, 0x00, 0x21, 0x00]);
    // And back without a signature.
    let back = Convert {
        from: 1200,
        floor: 2,
        page: 65001,
        bom: false,
        eol: None,
    };
    assert_eq!(converted(&[&out], back), "é!".as_bytes());
    assert_eq!(signature(1251), b"");
}
