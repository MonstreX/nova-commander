//! FAR's `LookForString` for a text (`far/findfile.cpp`): the file is read in blocks of 64 KB, each
//! turned into characters in each code page asked for and looked through, the end of a block kept for a
//! text across two (a character cut by it too). "Automatic detection" is FAR's "Default code page"
//! (`GetFileCodepage`, ADR-0028's `vfs::text::detect` as the viewer's): the one its byte order mark
//! says, else UTF-16 told by the zero bytes of its first 32 KB, else UTF-8 when they are UTF-8 with
//! other than ASCII in them, else the system's ANSI code page; a code page chosen, or several
//! (FAR's `InitInFileSearch` for "All standard code pages"), reads the file as it is, no byte order mark
//! skipped. "Whole words": the text with a word divider or the start or end of the file
//! on both sides (FAR's `IsWordDiv`: a space, the end of a line, NUL and FAR's `WordDiv`). "Fuzzy
//! search": the text and the file folded alike (FAR's `fuzzy_searcher`). Hex: the bytes as they are,
//! across blocks too. Only the first bytes when asked (FAR's `SearchInFirst`); the percent of the file
//! read is told as it goes.

use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::platform::locale::{ansi_code_page, decode_page, encode_page, fuzzy_fold};

/// FAR's `readBufferSize`.
const BLOCK: usize = 65536;
/// What FAR's `GetFileCodepage` looks at.
const DETECT: usize = 32768;
/// The Unicode code pages by Windows' numbers (`CP_UTF8`, FAR's `CP_UTF16LE`, `CP_UTF16BE`).
pub const UTF8: u32 = 65001;
pub const UTF16_LE: u32 = 1200;
pub const UTF16_BE: u32 = 1201;
/// FAR's default `WordDiv` (ADR-0028).
use crate::vfs::text::WORD_DIVIDERS;

/// The text to look for and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub case_sensitive: bool,
    pub whole_words: bool,
    pub fuzzy: bool,
    pub code_pages: CodePages,
}

/// The code pages a file is read in for a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodePages {
    /// FAR's "Automatic detection": the file's own.
    Auto,
    /// Each of these, by Windows' numbers.
    Pages(Vec<u32>),
}

/// What a file is looked through for: FAR's "Containing" as text or as hex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    Text(Query),
    Bytes(Vec<u8>),
}

/// How a file is read: as far as `limit` (0: all of it) of its `size` bytes, `percent` told when it
/// changes, until `stop` is set.
pub struct Reading<'a> {
    pub limit: u64,
    pub size: u64,
    pub stop: &'a AtomicBool,
    pub percent: &'a mut dyn FnMut(u8),
}

/// A code page a file is read in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodePage {
    Utf8,
    Utf16Le,
    Utf16Be,
    /// Any other the system has, by its number.
    Page(u32),
}

impl CodePage {
    pub fn from_id(id: u32) -> Self {
        match id {
            UTF8 => Self::Utf8,
            UTF16_LE => Self::Utf16Le,
            UTF16_BE => Self::Utf16Be,
            id => Self::Page(id),
        }
    }
}

/// FAR's `encoding::get_bytes`: `text` in code page `id`.
pub fn encode_text(id: u32, text: &str) -> Vec<u8> {
    match CodePage::from_id(id) {
        CodePage::Utf8 => text.as_bytes().to_vec(),
        CodePage::Utf16Le => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        CodePage::Utf16Be => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
        CodePage::Page(id) => encode_page(id, text),
    }
}

/// FAR's `encoding::get_chars`: `bytes` in code page `id` as text.
pub fn decode_bytes(id: u32, bytes: &[u8]) -> String {
    let mut chars = Vec::new();
    Decoder::new(CodePage::from_id(id)).decode(bytes, true, &mut chars);
    chars.into_iter().collect()
}

/// FAR's `FindFiles::IsWordDiv`.
pub fn is_word_divider(ch: char) -> bool {
    ch == '\0' || ch.is_whitespace() || WORD_DIVIDERS.contains(ch)
}

/// Whether `reader` holds the pattern; false once `stop` is set.
pub fn contains(
    reader: &mut dyn Read,
    pattern: &Pattern,
    reading: Reading<'_>,
) -> io::Result<bool> {
    match pattern {
        Pattern::Text(query) => {
            let detect = query.code_pages == CodePages::Auto;
            let pages = match &query.code_pages {
                CodePages::Auto => vec![CodePage::Utf8],
                CodePages::Pages(ids) => ids.iter().map(|&id| CodePage::from_id(id)).collect(),
            };
            let mut readers: Vec<(Decoder, Matcher)> = (pages.into_iter())
                .map(|page| (Decoder::new(page), Matcher::new(query)))
                .collect();
            let mut chars = Vec::new();
            each_block(reader, reading, |block, first, last| {
                let mut start = 0;
                if detect && first {
                    let (page, skip) = code_page(block, last);
                    readers[0].0 = Decoder::new(page);
                    start = skip;
                }
                readers.iter_mut().any(|(decoder, matcher)| {
                    chars.clear();
                    decoder.decode(&block[start..], last, &mut chars);
                    matcher.feed(&chars, last)
                })
            })
        }
        Pattern::Bytes(needle) => {
            let mut window: Vec<u8> = Vec::new();
            each_block(reader, reading, |block, _, _| {
                window.extend_from_slice(block);
                if window
                    .windows(needle.len().max(1))
                    .any(|at| at == &needle[..])
                {
                    return true;
                }
                let keep = window.len().min(needle.len().saturating_sub(1));
                window.drain(..window.len() - keep);
                false
            })
        }
    }
}

/// The file block by block to `look` (with whether it is the first and the last), as far as the limit;
/// true once `look` finds it.
fn each_block(
    reader: &mut dyn Read,
    reading: Reading<'_>,
    mut look: impl FnMut(&[u8], bool, bool) -> bool,
) -> io::Result<bool> {
    let limit = if reading.limit == 0 {
        u64::MAX
    } else {
        reading.limit
    };
    let mut reader = reader.take(limit);
    let total = reading.size.min(limit);
    let mut block = vec![0u8; BLOCK];
    let (mut read, mut shown, mut first) = (0u64, None, true);
    loop {
        let length = fill(&mut reader, &mut block)?;
        let last = length < BLOCK;
        read += u64::try_from(length).unwrap_or(0);
        let percent = percent_of(read, total);
        if shown != Some(percent) {
            (reading.percent)(percent);
            shown = Some(percent);
        }
        if look(&block[..length], first, last) {
            return Ok(true);
        }
        if last || reading.stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        first = false;
    }
}

/// FAR's `ToPercent`.
fn percent_of(read: u64, total: u64) -> u8 {
    if total == 0 {
        return 100;
    }
    u8::try_from((u128::from(read) * 100 / u128::from(total)).min(100)).unwrap_or(100)
}

/// Reads until `block` is full or the file ends; how many bytes.
fn fill(reader: &mut dyn Read, block: &mut [u8]) -> io::Result<usize> {
    let mut length = 0;
    while length < block.len() {
        match reader.read(&mut block[length..]) {
            Ok(0) => break,
            Ok(read) => length += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(length)
}

/// FAR's "Default code page" of a file from its first block (`whole`: the file is all there), and how
/// many bytes its byte order mark takes.
pub fn code_page(first: &[u8], whole: bool) -> (CodePage, usize) {
    use crate::vfs::text::{Detected, detect};
    let sample = &first[..first.len().min(DETECT)];
    // A character cut by the sample's end is still UTF-8, unless the file ends there.
    let (detected, mark) = detect(sample, whole && sample.len() == first.len());
    let page = match detected {
        Detected::Utf8 => CodePage::Utf8,
        Detected::Utf16Le => CodePage::Utf16Le,
        Detected::Utf16Be => CodePage::Utf16Be,
        Detected::System => CodePage::Page(ansi_code_page()),
    };
    (page, usize::try_from(mark).unwrap_or(0))
}

/// Bytes to characters, a character cut by a block's end kept for the next block.
struct Decoder {
    page: CodePage,
    pending: Vec<u8>,
}

impl Decoder {
    fn new(page: CodePage) -> Self {
        Self {
            page,
            pending: Vec::new(),
        }
    }

    fn decode(&mut self, bytes: &[u8], last: bool, out: &mut Vec<char>) {
        let mut data = std::mem::take(&mut self.pending);
        data.extend_from_slice(bytes);
        match self.page {
            CodePage::Utf8 => self.pending = utf8(&data, last, out),
            CodePage::Utf16Le => self.pending = utf16(&data, last, u16::from_le_bytes, out),
            CodePage::Utf16Be => self.pending = utf16(&data, last, u16::from_be_bytes, out),
            CodePage::Page(id) => {
                let (text, used) = decode_page(id, &data, last);
                out.extend(text.chars());
                self.pending = data[used..].to_vec();
            }
        }
    }
}

/// UTF-8 to `out`, each wrong byte one replacement character; returns a cut character's bytes.
fn utf8(mut data: &[u8], last: bool, out: &mut Vec<char>) -> Vec<u8> {
    loop {
        match std::str::from_utf8(data) {
            Ok(text) => {
                out.extend(text.chars());
                return Vec::new();
            }
            Err(error) => {
                let (valid, rest) = data.split_at(error.valid_up_to());
                out.extend(String::from_utf8_lossy(valid).chars());
                match error.error_len() {
                    Some(wrong) => {
                        out.push(char::REPLACEMENT_CHARACTER);
                        data = &rest[wrong..];
                    }
                    None if last => {
                        out.push(char::REPLACEMENT_CHARACTER);
                        return Vec::new();
                    }
                    None => return rest.to_vec(),
                }
            }
        }
    }
}

/// UTF-16 to `out`; returns an odd byte and a high surrogate the block ends with.
fn utf16(data: &[u8], last: bool, unit: fn([u8; 2]) -> u16, out: &mut Vec<char>) -> Vec<u8> {
    let mut units: Vec<u16> = (data.as_chunks::<2>().0.iter())
        .map(|pair| unit([pair[0], pair[1]]))
        .collect();
    let mut kept = data[units.len() * 2..].to_vec();
    if !last
        && units
            .last()
            .is_some_and(|&unit| (0xD800..0xDC00).contains(&unit))
    {
        let at = (units.len() - 1) * 2;
        kept = data[at..].to_vec();
        units.pop();
    }
    let chars = char::decode_utf16(units).map(|ch| ch.unwrap_or(char::REPLACEMENT_CHARACTER));
    out.extend(chars);
    if last && !kept.is_empty() {
        out.push(char::REPLACEMENT_CHARACTER);
        kept.clear();
    }
    kept
}

/// The text looked for in the characters as they come.
struct Matcher {
    needle: Vec<char>,
    case_sensitive: bool,
    whole_words: bool,
    /// The end of what came before, kept for a text across two blocks.
    window: Vec<char>,
    /// The character before the window; none at the file's start.
    before: Option<char>,
    fuzzy: bool,
}

impl Matcher {
    fn new(query: &Query) -> Self {
        let mut matcher = Self {
            needle: Vec::new(),
            case_sensitive: query.case_sensitive,
            whole_words: query.whole_words,
            window: Vec::new(),
            before: None,
            fuzzy: query.fuzzy,
        };
        let needle: Vec<char> = query.text.chars().collect();
        matcher.needle = matcher.folded(&needle);
        matcher
    }

    /// The characters as they are compared: fuzzy folded when asked, then the case.
    fn folded(&self, chars: &[char]) -> Vec<char> {
        if self.fuzzy {
            let text: String = chars.iter().collect();
            fuzzy_fold(&text).chars().map(|ch| self.fold(ch)).collect()
        } else {
            chars.iter().map(|&ch| self.fold(ch)).collect()
        }
    }

    fn fold(&self, ch: char) -> char {
        if self.case_sensitive {
            ch
        } else {
            ch.to_lowercase().next().unwrap_or(ch)
        }
    }

    /// Whether the text is there now; `last`: nothing comes after these characters.
    fn feed(&mut self, chars: &[char], last: bool) -> bool {
        let folded = self.folded(chars);
        self.window.extend(folded);
        let length = self.needle.len();
        if length == 0 {
            return true;
        }
        let end = self.window.len();
        let mut at = 0;
        while at + length <= end {
            if self.window[at..at + length] == self.needle[..] {
                match self.word_at(at, last) {
                    Some(true) => return true,
                    // The character after it has not come yet: looked at again with it.
                    None => break,
                    Some(false) => {}
                }
            }
            at += 1;
        }
        let keep = end.min(length);
        if end > keep {
            self.before = Some(self.window[end - keep - 1]);
            self.window.drain(..end - keep);
        }
        false
    }

    /// Whether the text at `at` is whole, as asked; none when that waits for what comes next.
    fn word_at(&self, at: usize, last: bool) -> Option<bool> {
        if !self.whole_words {
            return Some(true);
        }
        let before = if at == 0 {
            self.before
        } else {
            Some(self.window[at - 1])
        };
        if !before.is_none_or(is_word_divider) {
            return Some(false);
        }
        match self.window.get(at + self.needle.len()) {
            Some(&after) => Some(is_word_divider(after)),
            None if last => Some(true),
            None => None,
        }
    }
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
