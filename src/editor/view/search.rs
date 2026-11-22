//! The viewer's search (FAR's `Viewer::search_text_forward` and the others): a text in the file's
//! code page (case sensitive or not, whole words, fuzzy, a regular expression) or bytes, looked for in
//! windows read by offset from a place forward or back, never the whole file at once. Each window is
//! decoded with each character's byte kept, so a match is a byte range: the found text is selected by
//! it. A match that a window's end cuts is found in the next one (the windows overlap).

use std::io;

use super::code_page::Encoding;
use crate::vfs::RandomRead;

/// The bytes a window reads.
const WINDOW: u64 = 256 * 1024;
/// How far windows overlap: a match this long is never cut (FAR's own buffer is 8192 characters).
const OVERLAP: u64 = 16 * 1024;
/// FAR's default `WordDiv` (ADR-0028: one for the view, Find and the line editors).
pub use crate::vfs::text::WORD_DIVIDERS;

/// What is looked for.
#[derive(Debug, Clone)]
pub enum Pattern {
    Text {
        text: String,
        case_sensitive: bool,
        whole_words: bool,
        /// Characters folded alike (FAR's fuzzy search): by `fold`.
        fuzzy: bool,
    },
    Regex(regex::Regex),
    Bytes(Vec<u8>),
}

/// Where to look: the starts `from` (included) toward `to` - forward `from < to`, back `to <= from`
/// with the match starting before `from`.
#[derive(Debug, Clone, Copy)]
pub struct Range {
    pub from: u64,
    pub to: u64,
    pub backward: bool,
}

/// A match: its first byte and its bytes.
pub type Found = (u64, u64);

/// FAR's `IsWordDiv`: a space, the end of a line, NUL or one of `WordDiv`.
fn divider(ch: char) -> bool {
    ch == '\0' || ch.is_whitespace() || WORD_DIVIDERS.contains(ch) || ch == '\u{FFFD}'
}

/// The characters of `bytes` (read at `base`) with each one's first byte; `whole`: the file ends there.
fn decoded(encoding: &Encoding, base: u64, bytes: &[u8], whole: bool) -> Vec<(char, u64)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match encoding.decode(&bytes[at..], !whole) {
            Some((ch, len)) => {
                out.push((ch, base + at as u64));
                at += len.max(1);
            }
            None => break,
        }
    }
    out
}

/// The matches of `pattern` among `chars` (a window) as character ranges, in order.
fn text_matches(
    chars: &[(char, u64)],
    needle: &[char],
    case_sensitive: bool,
    fold: &dyn Fn(char) -> char,
) -> Vec<(usize, usize)> {
    let fold = |ch: char| {
        let ch = if case_sensitive {
            ch
        } else {
            ch.to_lowercase().next().unwrap_or(ch)
        };
        fold(ch)
    };
    let needle: Vec<char> = needle.iter().map(|ch| fold(*ch)).collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return Vec::new();
    }
    let hay: Vec<char> = chars.iter().map(|(ch, _)| fold(*ch)).collect();
    (0..=hay.len() - needle.len())
        .filter(|&at| hay[at..at + needle.len()] == needle[..])
        .map(|at| (at, at + needle.len()))
        .collect()
}

/// The regular expression's matches among `chars` as character ranges.
fn regex_matches(chars: &[(char, u64)], regex: &regex::Regex) -> Vec<(usize, usize)> {
    let text: String = chars.iter().map(|(ch, _)| *ch).collect();
    let mut index_of = Vec::with_capacity(text.len() + 1);
    for (index, (ch, _)) in chars.iter().enumerate() {
        index_of.extend(std::iter::repeat_n(index, ch.len_utf8()));
    }
    index_of.push(chars.len());
    regex
        .find_iter(&text)
        .filter(|found| !found.is_empty())
        .map(|found| (index_of[found.start()], index_of[found.end()]))
        .collect()
}

/// A window `[start, end)` of the file: its matches as byte ranges, and whether its decoded end is the
/// file's (a match touching a window's end is left to the next window otherwise).
fn window_matches(
    file: &mut dyn RandomRead,
    (encoding, pattern, len): (&Encoding, &Pattern, u64),
    (start, end): (u64, u64),
    fold: &dyn Fn(char) -> char,
) -> io::Result<Vec<Found>> {
    // A character before for whole words.
    let context = start.saturating_sub(4);
    let mut bytes = vec![0; usize::try_from(end - context).unwrap_or(0)];
    let read = read_all(file, context, &mut bytes)?;
    bytes.truncate(read);
    let whole = context + read as u64 >= len;
    if let Pattern::Bytes(needle) = pattern {
        let found = memchr::memmem::find_iter(&bytes, needle)
            .map(|at| (context + at as u64, needle.len() as u64))
            .filter(|(at, _)| *at >= start);
        return Ok(found.collect());
    }
    let chars = decoded(encoding, context, &bytes, whole);
    let ranges = match pattern {
        Pattern::Text {
            text,
            case_sensitive,
            fuzzy,
            ..
        } => {
            let needle: Vec<char> = text.chars().collect();
            let plain = |ch: char| ch;
            text_matches(
                &chars,
                &needle,
                *case_sensitive,
                if *fuzzy { fold } else { &plain },
            )
        }
        Pattern::Regex(regex) => regex_matches(&chars, regex),
        Pattern::Bytes(_) => Vec::new(),
    };
    let words = matches!(
        pattern,
        Pattern::Text {
            whole_words: true,
            ..
        }
    );
    let end_of = |index: usize| {
        chars
            .get(index)
            .map_or(context + read as u64, |(_, at)| *at)
    };
    let found = ranges.into_iter().filter(|&(first, past)| {
        let begins = chars[first].1;
        // Cut by the window's end: the next window has it whole.
        let cut = past == chars.len() && !whole;
        let before = first.checked_sub(1).map(|at| chars[at].0);
        let after = chars.get(past).map(|(ch, _)| *ch);
        let bounded = !words
            || (before.is_none_or(divider)
                && after.is_none_or(divider)
                && (after.is_some() || whole));
        begins >= start && !cut && bounded
    });
    Ok(found
        .map(|(first, past)| (chars[first].1, end_of(past) - chars[first].1))
        .collect())
}

fn read_all(file: &mut dyn RandomRead, at: u64, buffer: &mut [u8]) -> io::Result<usize> {
    let mut done = 0;
    while done < buffer.len() {
        match file.read_at(at + done as u64, &mut buffer[done..])? {
            0 => break,
            read => done += read,
        }
    }
    Ok(done)
}

/// The first match from `range.from` toward `range.to` (back: the last before `from`), until `stop`.
pub fn find(
    file: &mut dyn RandomRead,
    (encoding, pattern): (&Encoding, &Pattern),
    range: Range,
    (fold, stop): (&dyn Fn(char) -> char, &dyn Fn() -> bool),
) -> io::Result<Option<Found>> {
    let len = file.len()?;
    let what = (encoding, pattern, len);
    if range.backward {
        let mut upper = range.from.min(len);
        while upper > range.to {
            if stop() {
                return Ok(None);
            }
            let lower = upper.saturating_sub(WINDOW).max(range.to);
            let end = (upper + OVERLAP).min(len);
            let found = window_matches(file, what, (lower, end), fold)?;
            if let Some(last) = found.into_iter().rev().find(|(at, _)| *at < upper) {
                return Ok(Some(last));
            }
            upper = lower;
        }
        return Ok(None);
    }
    let mut lower = range.from;
    while lower < range.to.min(len) {
        if stop() {
            return Ok(None);
        }
        let end = (lower + WINDOW + OVERLAP).min(len);
        let found = window_matches(file, what, (lower, end), fold)?;
        if let Some(first) = found.into_iter().find(|(at, _)| *at < range.to) {
            return Ok(Some(first));
        }
        lower += WINDOW;
    }
    Ok(None)
}

#[path = "search_all.rs"]
mod all;
pub use all::{FoundLine, find_all, found_lines};

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
