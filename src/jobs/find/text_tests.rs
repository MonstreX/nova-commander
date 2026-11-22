//! The text in a file as FAR's `LookForString` finds it.

use super::*;

fn query(text: &str) -> Query {
    Query {
        text: text.to_string(),
        case_sensitive: false,
        whole_words: false,
        fuzzy: false,
        code_pages: CodePages::Auto,
    }
}

/// `query` read in these code pages.
fn in_pages(text: &str, pages: &[u32]) -> Query {
    Query {
        code_pages: CodePages::Pages(pages.to_vec()),
        ..query(text)
    }
}

/// Whether `bytes` hold `pattern` read as far as `limit`, and the percents told.
fn look(bytes: &[u8], pattern: &Pattern, limit: u64) -> (bool, Vec<u8>) {
    let mut told = Vec::new();
    let mut percent = |percent| told.push(percent);
    let reading = Reading {
        limit,
        size: u64::try_from(bytes.len()).unwrap(),
        stop: &AtomicBool::new(false),
        percent: &mut percent,
    };
    let found = contains(&mut io::Cursor::new(bytes), pattern, reading).unwrap();
    (found, told)
}

fn has(bytes: &[u8], query: &Query) -> bool {
    look(bytes, &Pattern::Text(query.clone()), 0).0
}

fn utf16le(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}

#[test]
fn the_case_counts_only_when_asked() {
    assert!(has(b"Hello World", &query("world")));
    let exact = Query {
        case_sensitive: true,
        ..query("world")
    };
    assert!(!has(b"Hello World", &exact));
    assert!(has("Привет Мир".as_bytes(), &query("мир")));
}

#[test]
fn whole_words_need_dividers_or_the_ends_of_the_file() {
    let words = Query {
        whole_words: true,
        ..query("cat")
    };
    assert!(has(b"cat", &words));
    assert!(has(b"a cat.", &words));
    assert!(has(b"(cat)", &words));
    assert!(!has(b"concatenate", &words));
    assert!(!has(b"cats", &words));
    assert!(has(b"cats and cat", &words), "a later one is whole");
}

#[test]
fn a_text_across_two_blocks_is_found() {
    let mut bytes = vec![b'x'; BLOCK - 2];
    bytes.extend_from_slice(b"needle y");
    assert!(has(&bytes, &query("needle")));
    let words = Query {
        whole_words: true,
        ..query("needle")
    };
    let mut joined = vec![b' '; BLOCK - 6];
    joined.extend_from_slice(b"needle");
    joined.extend_from_slice(b"s");
    assert!(
        !has(&joined, &words),
        "the next block goes on with the word"
    );
    joined.pop();
    joined.push(b' ');
    assert!(has(&joined, &words), "the next block starts with a divider");
}

#[test]
fn a_character_cut_by_the_block_end_is_whole_in_the_next() {
    // Not ASCII only at the start: the file is UTF-8.
    let mut bytes = "ё".as_bytes().to_vec();
    bytes.resize(BLOCK - 1, b' ');
    bytes.extend_from_slice("ж".as_bytes());
    bytes.extend_from_slice("ук".as_bytes());
    assert!(has(&bytes, &query("жук")));
}

#[test]
fn the_byte_order_mark_tells_the_code_page() {
    assert!(has(&utf16le("say Hello"), &query("hello")));
    let mut big = vec![0xFE, 0xFF];
    big.extend("Hello".encode_utf16().flat_map(u16::to_be_bytes));
    assert!(has(&big, &query("hello")));
    assert_eq!(code_page(b"\xEF\xBB\xBFx", true), (CodePage::Utf8, 3));
}

/// R7 review E4: "Automatic detection" tells UTF-16 without a byte order mark as the viewer does (FAR's
/// `GetFileCodepage` with `IsTextUnicode`).
#[test]
fn utf16_without_a_mark_is_told_by_its_zero_bytes() {
    let little: Vec<u8> = "say Hello world"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(code_page(&little, true), (CodePage::Utf16Le, 0));
    assert!(has(&little, &query("hello")));
    let big: Vec<u8> = "say Hello world"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    assert_eq!(code_page(&big, true), (CodePage::Utf16Be, 0));
    assert!(has(&big, &query("hello")));
}

#[test]
fn utf8_is_taken_when_it_is_more_than_ascii() {
    assert_eq!(code_page("тест".as_bytes(), true), (CodePage::Utf8, 0));
    let ansi = CodePage::Page(ansi_code_page());
    assert_eq!(code_page(b"plain", true), (ansi, 0));
    assert_eq!(code_page(b"\xC0\xC1 wrong", true), (ansi, 0));
    // A character cut by the end of a longer file is still UTF-8; the file ending in it is not.
    let cut = &"aж".as_bytes()[..2];
    assert_eq!(code_page(cut, false), (CodePage::Utf8, 0));
    assert_eq!(code_page(cut, true), (ansi, 0));
}

#[test]
fn each_code_page_asked_for_reads_the_file() {
    // No mark and no ASCII, so none of the zero bytes UTF-16 is told by.
    let big: Vec<u8> = "привет мир"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    assert!(
        !has(&big, &query("привет")),
        "no mark: detection takes it for ANSI"
    );
    assert!(has(&big, &in_pages("привет", &[UTF8, UTF16_LE, UTF16_BE])));
    assert!(!has(&big, &in_pages("привет", &[UTF8, UTF16_LE])));
    assert!(
        !has(b"hello", &in_pages("hello", &[])),
        "no code page reads nothing"
    );
}

#[test]
fn a_chosen_code_page_skips_no_mark() {
    let mut marked = vec![0xFF, 0xFE];
    marked.extend("ab".encode_utf16().flat_map(u16::to_le_bytes));
    assert!(has(&marked, &in_pages("ab", &[UTF16_LE])));
    // Read as UTF-16 big endian the mark is U+FFFE and the text is other characters.
    assert!(!has(&marked, &in_pages("ab", &[UTF16_BE])));
}

#[cfg(windows)]
#[test]
fn a_code_page_of_the_system_reads_its_bytes() {
    // "привет" in Windows-1251 and in DOS 866.
    let windows = b"\xEF\xF0\xE8\xE2\xE5\xF2";
    assert!(has(windows, &in_pages("ПРИВЕТ", &[1251])));
    assert!(!has(windows, &in_pages("привет", &[866])));
    assert!(has(
        b"\xAF\xE0\xA8\xA2\xA5\xE2",
        &in_pages("привет", &[866, 1251])
    ));
}

#[cfg(windows)]
#[test]
fn a_double_byte_character_across_two_blocks_is_found() {
    // Shift-JIS "あい" with its first byte the last of a block.
    let mut bytes = vec![b'x'; BLOCK - 1];
    bytes.extend_from_slice(b"\x82\xA0\x82\xA2");
    assert!(has(&bytes, &in_pages("あい", &[932])));
    assert!(
        has(&bytes, &in_pages("あい", &[UTF16_LE, 932, UTF8])),
        "each code page keeps its own cut bytes"
    );
}

#[test]
fn text_turns_into_a_code_pages_bytes_and_back() {
    assert_eq!(encode_text(UTF16_LE, "Aя"), [0x41, 0, 0x4F, 0x04]);
    assert_eq!(encode_text(UTF16_BE, "A"), [0, 0x41]);
    assert_eq!(encode_text(UTF8, "я"), [0xD1, 0x8F]);
    assert_eq!(decode_bytes(UTF16_BE, &[0, 0x41, 0x04, 0x4F]), "Aя");
    assert_eq!(decode_bytes(UTF8, &[0xD1, 0x8F]), "я");
}

#[test]
fn a_surrogate_pair_cut_by_the_block_end_is_one_character() {
    let mut chars = Vec::new();
    let mut decoder = Decoder::new(CodePage::Utf16Le);
    let bytes: Vec<u8> = "😀".encode_utf16().flat_map(u16::to_le_bytes).collect();
    decoder.decode(&bytes[..3], false, &mut chars);
    assert!(chars.is_empty());
    decoder.decode(&bytes[3..], true, &mut chars);
    assert_eq!(chars, vec!['😀']);
}

#[test]
fn a_stopped_search_finds_nothing_more() {
    let mut bytes = vec![b'x'; BLOCK * 2];
    bytes.extend_from_slice(b"needle");
    let stop = AtomicBool::new(true);
    let reading = Reading {
        limit: 0,
        size: 0,
        stop: &stop,
        percent: &mut |_| {},
    };
    let pattern = Pattern::Text(query("needle"));
    let found = contains(&mut io::Cursor::new(bytes), &pattern, reading).unwrap();
    assert!(!found);
}

#[test]
fn hex_takes_the_bytes_as_they_are_across_blocks() {
    let mut bytes = vec![0u8; BLOCK - 1];
    bytes.extend_from_slice(&[0xDE, 0xAD, 0xBE]);
    assert!(look(&bytes, &Pattern::Bytes(vec![0xDE, 0xAD, 0xBE]), 0).0);
    assert!(!look(&bytes, &Pattern::Bytes(vec![0xAD, 0xDE]), 0).0);
    assert!(
        look(b"\xEF\xBB\xBFx", &Pattern::Bytes(vec![0xEF, 0xBB]), 0).0,
        "no mark skipped"
    );
}

#[test]
fn fuzzy_takes_letters_with_marks_as_without() {
    let fuzzy = Query {
        fuzzy: true,
        ..query("cafe")
    };
    assert!(has("Un café noir".as_bytes(), &fuzzy));
    assert!(!has("Un café noir".as_bytes(), &query("cafe")));
    let marked = Query {
        fuzzy: true,
        ..query("CAFÉ")
    };
    assert!(has(b"cafe", &marked), "the text is folded too");
}

#[test]
fn only_the_first_bytes_are_read_when_asked() {
    let mut bytes = vec![b'x'; 100];
    bytes.extend_from_slice(b"needle");
    let pattern = Pattern::Text(query("needle"));
    assert!(!look(&bytes, &pattern, 100).0);
    assert!(look(&bytes, &pattern, 106).0);
    assert!(look(&bytes, &pattern, 0).0);
}

#[test]
fn the_percent_read_is_told_as_it_changes() {
    let bytes = vec![b'x'; BLOCK * 2 + 10];
    let (_, told) = look(&bytes, &Pattern::Text(query("needle")), 0);
    assert_eq!(told, [49, 99, 100]);
    let (_, limited) = look(
        &bytes,
        &Pattern::Text(query("needle")),
        u64::try_from(BLOCK).unwrap(),
    );
    assert_eq!(limited, [100], "of what the limit takes");
}

#[test]
fn word_dividers_are_fars() {
    for ch in [' ', '\t', '\n', '\0', '.', ',', '(', '"', '\\'] {
        assert!(is_word_divider(ch), "{ch:?}");
    }
    for ch in ['a', '_', '#', '$', '@', 'я'] {
        assert!(!is_word_divider(ch), "{ch:?}");
    }
}
