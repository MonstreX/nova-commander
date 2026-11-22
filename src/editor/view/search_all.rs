//! Every match of a document with its line (FAR's F7 "All", `Editor::DoSearchReplace` with its
//! `find_all_list`; S2d.6e): the matches one after another from the start, each search going on past
//! the last match (`CalculateSearchNextPositionInTheLine`), then the document read once more from its
//! first byte to tell each match's line, its first character there and its characters, and the line's
//! text for the list.

use std::io;

use super::{Encoding, Found, OVERLAP, Pattern, WINDOW, decoded, read_all, window_matches};
use crate::editor::view::lines::MAX_LINE;
use crate::vfs::RandomRead;

/// A match as FAR's "All" list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundLine {
    /// Its first byte and its bytes.
    pub at: u64,
    pub size: u64,
    /// Its line and its first character there (from 0; FAR's `FindCoord`), its characters in the line.
    pub line: u64,
    pub pos: usize,
    pub len: usize,
    /// The line's text.
    pub text: String,
}

/// Every match from the document's start, until `stop` (what was found by then).
pub fn find_all(
    file: &mut dyn RandomRead,
    (encoding, pattern): (&Encoding, &Pattern),
    (fold, stop): (&dyn Fn(char) -> char, &dyn Fn() -> bool),
) -> io::Result<Vec<Found>> {
    let len = file.len()?;
    let what = (encoding, pattern, len);
    let mut all = Vec::new();
    let mut next = 0;
    let mut lower = 0;
    while lower < len && !stop() {
        let end = (lower + WINDOW + OVERLAP).min(len);
        for (at, size) in window_matches(file, what, (lower, end), fold)? {
            if at >= next {
                all.push((at, size));
                next = at + size.max(1);
            }
        }
        lower += WINDOW;
    }
    Ok(all)
}

/// The lines of `found` (in order), the document read from `floor` (past a byte order mark), until
/// `stop`. A line ends at its break (`\r\n`, `\n`, `\r`) or after the longest line's characters, as
/// the view lays it out.
pub fn found_lines(
    file: &mut dyn RandomRead,
    (encoding, floor): (&Encoding, u64),
    found: &[Found],
    stop: &dyn Fn() -> bool,
) -> io::Result<Vec<FoundLine>> {
    let len = file.len()?;
    let mut walk = Walk::new(found);
    let mut base = floor;
    while base < len && !walk.done() && !stop() {
        let end = (base + WINDOW).min(len);
        let mut bytes = vec![0; usize::try_from(end - base).unwrap_or(0)];
        let read = read_all(file, base, &mut bytes)?;
        bytes.truncate(read);
        let whole = base + read as u64 >= len;
        let chars = decoded(encoding, base, &bytes, whole);
        // A character cut by the window's end is read whole with the next one.
        let take = if whole {
            chars.len()
        } else {
            chars.len().saturating_sub(1)
        };
        for &(ch, at) in &chars[..take] {
            walk.char(ch, at);
        }
        match chars.get(take) {
            Some(&(_, at)) if at > base => base = at,
            _ => base = end,
        }
    }
    walk.end_line();
    Ok(walk.out)
}

/// The document's characters walked in order: the line they make, the matches in it.
struct Walk<'a> {
    found: std::iter::Peekable<std::slice::Iter<'a, Found>>,
    line: u64,
    text: String,
    offsets: Vec<u64>,
    /// The line's matches and their first characters.
    open: Vec<(Found, usize)>,
    /// A CR ended the line: an LF next is its break too.
    after_cr: bool,
    out: Vec<FoundLine>,
}

impl<'a> Walk<'a> {
    fn new(found: &'a [Found]) -> Self {
        Self {
            found: found.iter().peekable(),
            line: 0,
            text: String::new(),
            offsets: Vec::new(),
            open: Vec::new(),
            after_cr: false,
            out: Vec::new(),
        }
    }

    /// Every match told.
    fn done(&mut self) -> bool {
        self.found.peek().is_none() && self.open.is_empty()
    }

    fn char(&mut self, ch: char, at: u64) {
        if std::mem::take(&mut self.after_cr) && ch == '\n' {
            return;
        }
        if self.offsets.len() >= MAX_LINE {
            self.end_line();
        }
        while let Some(&&found) = self.found.peek().filter(|found| found.0 <= at) {
            self.open.push((found, self.offsets.len()));
            self.found.next();
        }
        if ch == '\n' || ch == '\r' {
            self.after_cr = ch == '\r';
            self.end_line();
            return;
        }
        self.text.push(ch);
        self.offsets.push(at);
    }

    /// The line ends: its matches told, the next one begins.
    fn end_line(&mut self) {
        for ((at, size), pos) in std::mem::take(&mut self.open) {
            let len = (self.offsets.iter().skip(pos))
                .take_while(|offset| **offset < at + size)
                .count();
            self.out.push(FoundLine {
                at,
                size,
                line: self.line,
                pos,
                len,
                text: self.text.clone(),
            });
        }
        self.text.clear();
        self.offsets.clear();
        self.line += 1;
    }
}
