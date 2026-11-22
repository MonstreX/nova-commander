//! The search: texts with and without case, whole words, a regular expression, bytes; forward and
//! back; across the windows' ends; in UTF-16.

use super::*;

/// A file in memory read by offset.
struct Memory(Vec<u8>);

impl RandomRead for Memory {
    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        let from = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.0.len());
        let count = buffer.len().min(self.0.len() - from);
        buffer[..count].copy_from_slice(&self.0[from..from + count]);
        Ok(count)
    }

    fn len(&mut self) -> io::Result<u64> {
        Ok(self.0.len() as u64)
    }
}

fn text(needle: &str, case_sensitive: bool, whole_words: bool) -> Pattern {
    Pattern::Text {
        text: needle.to_string(),
        case_sensitive,
        whole_words,
        fuzzy: false,
    }
}

fn find_in(bytes: &[u8], pattern: &Pattern, range: Range) -> Option<Found> {
    let mut file = Memory(bytes.to_vec());
    let plain = |ch: char| ch;
    find(
        &mut file,
        (&Encoding::Utf8, pattern),
        range,
        (&plain, &|| false),
    )
    .unwrap()
}

fn forward(from: u64) -> Range {
    Range {
        from,
        to: u64::MAX,
        backward: false,
    }
}

fn back(from: u64) -> Range {
    Range {
        from,
        to: 0,
        backward: true,
    }
}

#[test]
fn a_text_is_found_forward_and_back_with_or_without_case() {
    let hay = b"one Two three two";
    assert_eq!(
        find_in(hay, &text("two", false, false), forward(0)),
        Some((4, 3))
    );
    assert_eq!(
        find_in(hay, &text("two", true, false), forward(0)),
        Some((14, 3))
    );
    assert_eq!(
        find_in(hay, &text("two", false, false), forward(5)),
        Some((14, 3))
    );
    assert_eq!(
        find_in(hay, &text("two", false, false), back(17)),
        Some((14, 3))
    );
    assert_eq!(
        find_in(hay, &text("two", false, false), back(14)),
        Some((4, 3))
    );
    assert_eq!(find_in(hay, &text("four", false, false), forward(0)), None);
    let bounded = Range {
        from: 0,
        to: 10,
        backward: false,
    };
    assert_eq!(
        find_in(hay, &text("two", true, false), bounded),
        None,
        "not before 10"
    );
}

#[test]
fn whole_words_need_dividers_on_both_sides() {
    let hay = b"cat concat cat. (cat)";
    let words = text("cat", true, true);
    assert_eq!(find_in(hay, &words, forward(0)), Some((0, 3)));
    assert_eq!(
        find_in(hay, &words, forward(1)),
        Some((11, 3)),
        "not inside concat"
    );
    assert_eq!(find_in(hay, &words, forward(12)), Some((17, 3)));
}

#[test]
fn a_regular_expression_and_bytes_are_found() {
    let hay = "x = 42; y = 1234;".as_bytes();
    let regex = Pattern::Regex(regex::Regex::new(r"\d{3,}").unwrap());
    assert_eq!(find_in(hay, &regex, forward(0)), Some((12, 4)));
    assert_eq!(
        find_in(hay, &Pattern::Bytes(b"= 4".to_vec()), forward(0)),
        Some((2, 3))
    );
    assert_eq!(
        find_in(hay, &Pattern::Bytes(b";".to_vec()), back(17)),
        Some((16, 1))
    );
}

#[test]
fn a_match_across_windows_is_found_whole() {
    let mut hay = vec![b'.'; (WINDOW - 2) as usize];
    hay.extend_from_slice(b"needle");
    hay.extend(vec![b'.'; 100]);
    let needle = text("needle", true, false);
    assert_eq!(find_in(&hay, &needle, forward(0)), Some((WINDOW - 2, 6)));
    let far_back = hay.len() as u64;
    assert_eq!(
        find_in(&hay, &needle, back(far_back)),
        Some((WINDOW - 2, 6))
    );
    // Back from deep beyond a window.
    let mut long = hay.clone();
    long.extend(vec![b'.'; (WINDOW * 2) as usize]);
    assert_eq!(
        find_in(&long, &needle, back(long.len() as u64)),
        Some((WINDOW - 2, 6))
    );
}

#[test]
fn utf8_and_utf16_matches_are_byte_ranges() {
    let hay = "дом и сад".as_bytes();
    assert_eq!(
        find_in(hay, &text("САД", false, false), forward(0)),
        Some((10, 6))
    );
    let le: Vec<u8> = "a needle"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut file = Memory(le);
    let plain = |ch: char| ch;
    let found = find(
        &mut file,
        (&Encoding::Utf16Le, &text("needle", true, false)),
        forward(0),
        (&plain, &|| false),
    );
    assert_eq!(found.unwrap(), Some((4, 12)));
}

/// S2d.6e: FAR's F7 "All" - every match past the last one, each with its line, its first character
/// there, its characters and the line's text; CRLF one break, a byte order mark not a character.
#[test]
fn every_match_is_found_with_its_line() {
    let hay = "\u{FEFF}one two one\r\nnone\rlast one".as_bytes().to_vec();
    let mut file = Memory(hay);
    let plain = |ch: char| ch;
    let pattern = text("one", false, false);
    let found = find_all(&mut file, (&Encoding::Utf8, &pattern), (&plain, &|| false)).unwrap();
    assert_eq!(found, [(3, 3), (11, 3), (17, 3), (26, 3)]);
    let lines = found_lines(&mut file, (&Encoding::Utf8, 3), &found, &|| false).unwrap();
    let told: Vec<(u64, usize, usize, &str)> = (lines.iter())
        .map(|line| (line.line, line.pos, line.len, line.text.as_str()))
        .collect();
    assert_eq!(
        told,
        [
            (0, 0, 3, "one two one"),
            (0, 8, 3, "one two one"),
            (1, 1, 3, "none"),
            (2, 5, 3, "last one"),
        ]
    );
    let aaa = find_all(
        &mut Memory(b"aaaa".to_vec()),
        (&Encoding::Utf8, &text("aa", true, false)),
        (&plain, &|| false),
    );
    assert_eq!(aaa.unwrap(), [(0, 2), (2, 2)], "past the last match");
}

#[test]
fn a_stop_ends_the_search() {
    let mut file = Memory(b"abc".to_vec());
    let plain = |ch: char| ch;
    let found = find(
        &mut file,
        (&Encoding::Utf8, &text("c", true, false)),
        forward(0),
        (&plain, &|| true),
    );
    assert_eq!(found.unwrap(), None);
}
