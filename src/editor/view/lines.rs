//! A file's lines laid out from wherever the screen starts (ADR-0015 §3): a line ends at its break
//! (`\r\n`, `\n`, `\r`) or after FAR's longest line (`ViewerOptions::MaxLineSize`, 10 000 characters),
//! so a file with no breaks is still read a screen at a time. Going up finds the start of the line
//! before by looking back from it, never from the file's start.

use super::blocks::{Bytes, Missing};
use super::code_page::{Encoding, shown};

/// The characters of the longest line; the rest of it goes on as the next one (FAR's default).
pub const MAX_LINE: usize = 10_000;
/// Columns between tab stops (FAR's viewer default).
pub const TAB: usize = 8;
/// How lines are laid out (FAR's `TabSize` and `MaxLineSize`, the viewer's settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Columns between tab stops.
    pub tab: usize,
    /// The characters of the longest line; the rest of it goes on as the next one.
    pub max_line: usize,
    /// What a NUL is shown as (FAR's `ZeroChar` with `Visible0x00`); none: its glyph.
    pub zero: Option<char>,
    /// FAR's "Show white space" in the editor: a space as `·`, a tab's first cell as `→`.
    pub white: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            tab: TAB,
            max_line: MAX_LINE,
            zero: None,
            white: false,
        }
    }
}

impl Layout {
    /// How far back the start of a line is looked for: the longest line in the widest characters.
    fn window(self) -> u64 {
        self.max_line as u64 * 4
    }
}
const LF: u32 = b'\n' as u32;
const CR: u32 = b'\r' as u32;

/// One line as the screen shows it: its cells (tabs as spaces, control characters as FAR's glyphs) and
/// the byte each cell comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub start: u64,
    /// Where the next line starts: past this one's break.
    pub end: u64,
    pub cells: Vec<char>,
    pub offsets: Vec<u64>,
    /// The characters as the file has them (a tab a tab), for the syntax highlighting (ADR-0024).
    pub chars: Vec<char>,
    /// The first cell of each character (a combining one: the cell it joins).
    pub char_cells: Vec<usize>,
    /// The combining characters joined to the cells before them, by those cells (ADR-0025).
    pub joined: Vec<(usize, char)>,
}

impl Line {
    /// The rows it takes wrapped to `width`, each one's first cell and the cell past what it shows: by
    /// the width (FAR's `m_Wrap`), or at the words (`m_WordWrap`, FAR's `Viewer::ReadString`). An
    /// empty line takes one row.
    pub fn row_bounds(&self, width: usize, words: bool) -> Vec<(usize, usize)> {
        let width = width.max(1);
        let len = self.cells.len();
        if words {
            return word_rows(&self.cells, width);
        }
        if !self.is_wide() {
            return (0..len.div_ceil(width).max(1))
                .map(|row| (row * width, ((row + 1) * width).min(len)))
                .collect();
        }
        // A wide character the row's end would cut goes on the next row (FAR's full-width layout).
        let mut rows = Vec::new();
        let mut start = 0;
        loop {
            let mut end = (start + width).min(len);
            if end < len && self.cells[end] == WIDE_TAIL && end > start + 1 {
                end -= 1;
            }
            rows.push((start, end));
            if end >= len {
                return rows;
            }
            start = end;
        }
    }

    /// Whether a character two cells wide is in it.
    fn is_wide(&self) -> bool {
        self.cells.contains(&WIDE_TAIL)
    }

    /// The rows it takes wrapped to `width` (an empty line takes one).
    pub fn rows(&self, width: usize, words: bool) -> usize {
        if words || self.is_wide() {
            return self.row_bounds(width, words).len();
        }
        self.cells.len().div_ceil(width.max(1)).max(1)
    }

    /// The byte row `row` of `width` starts at.
    pub fn row_start(&self, row: usize, width: usize, words: bool) -> u64 {
        let cell = match words || self.is_wide() {
            true => (self.row_bounds(width, words).get(row)).map_or(usize::MAX, |bounds| bounds.0),
            false => row * width.max(1),
        };
        self.offsets.get(cell).copied().unwrap_or(self.start)
    }

    /// The row of `width` that shows `pos`.
    pub fn row_of(&self, pos: u64, width: usize, words: bool) -> usize {
        let starts: Vec<u64> = (self.row_bounds(width, words).iter())
            .map(|bounds| self.offsets.get(bounds.0).copied().unwrap_or(self.start))
            .collect();
        (0..starts.len())
            .find(|&row| row + 1 == starts.len() || starts[row + 1] > pos)
            .unwrap_or(0)
    }

    /// The cells from `from`, at most `width` of them.
    /// The cell character `index` starts at (the line's end past its last one).
    pub fn cell_of(&self, index: usize) -> usize {
        self.char_cells
            .get(index)
            .copied()
            .unwrap_or(self.cells.len())
    }

    /// What the screen shows of `width` cells from `from`: a wide character's tail left out (the
    /// character takes it), a space for a tail the row starts on and for a wide character it cannot
    /// hold whole; the combining characters after their cells' characters.
    pub fn text(&self, from: usize, width: usize) -> String {
        let end = from.saturating_add(width).min(self.cells.len());
        let first = self.joined.partition_point(|(cell, _)| *cell < from);
        let mut joined = self.joined[first..].iter().peekable();
        let mut text = String::new();
        for at in from..end {
            while joined.next_if(|(cell, _)| *cell < at).is_some() {}
            let cell = self.cells[at];
            let cut = at + 1 == end && self.cells.get(end) == Some(&WIDE_TAIL);
            match cell {
                WIDE_TAIL if at > from => {}
                WIDE_TAIL => text.push(' '),
                _ if cut => text.push(' '),
                _ => {
                    text.push(cell);
                    while let Some((_, ch)) = joined.next_if(|(cell, _)| *cell == at) {
                        text.push(*ch);
                    }
                }
            }
        }
        text
    }

    /// The first cell of the character the cell `col` shows (a wide character's tail: its first).
    pub fn char_start_cell(&self, col: usize) -> usize {
        match self.cells.get(col) {
            Some(&WIDE_TAIL) => col.saturating_sub(1),
            _ => col,
        }
    }
}

/// The second cell of a character two cells wide (ADR-0025): the row's text leaves it out.
pub const WIDE_TAIL: char = '\u{FFFF}';

/// How many cells the screen gives `ch` (ADR-0025: `ratatui`'s widths): 0 for a combining one, 2 for a
/// wide one; a control character's glyph takes one.
fn cell_width(ch: char) -> usize {
    unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1)
}

/// FAR's word wrap (`Viewer::ReadString` with `m_WordWrap`): a row ends after its last blank or word
/// divider (`WordDiv`) that fits, or at the width when none does or the cell past it is a blank; the
/// blanks at its end are not shown, those at the next row's start are skipped.
fn word_rows(cells: &[char], width: usize) -> Vec<(usize, usize)> {
    let blank = |ch: char| ch == ' ';
    let can_wrap = |ch: char| blank(ch) || super::search::WORD_DIVIDERS.contains(ch);
    let mut rows = Vec::new();
    let mut start = 0;
    while cells.len() - start > width {
        let over = start + width;
        let point = (start..over).rev().find(|&at| can_wrap(cells[at]));
        let next = match point {
            Some(at) if !blank(cells[over]) => at + 1,
            // A wide character the width would cut goes on the next row.
            _ if cells[over] == WIDE_TAIL && over > start + 1 => over - 1,
            _ => over,
        };
        let mut end = next;
        while end > start && blank(cells[end - 1]) {
            end -= 1;
        }
        rows.push((start, end));
        start = next;
        while start < cells.len() && blank(cells[start]) {
            start += 1;
        }
        if start == cells.len() {
            return rows;
        }
    }
    rows.push((start, cells.len()));
    rows
}

/// Where a document's lines are read by their first byte: the editor's view and the viewer's, for
/// the syntax highlighting (ADR-0024).
pub trait LineSource {
    /// The line starting at `start`.
    fn line_at(&self, start: u64) -> Result<Line, Missing>;
    /// The start of the line after `line` (none after the last).
    fn line_after(&self, line: &Line) -> Result<Option<u64>, Missing>;
    /// The start of the line before the one at `start` (none before the first).
    fn line_above(&self, start: u64) -> Result<Option<u64>, Missing>;
    /// The document's bytes.
    fn size(&self) -> u64;
}

/// Characters one after another from a byte.
struct Cursor<'a> {
    blocks: &'a dyn Bytes,
    encoding: &'a Encoding,
    pos: u64,
}

impl Cursor<'_> {
    /// The next character and its byte; none at the end.
    fn next(&mut self) -> Result<Option<(char, u64)>, Missing> {
        let here = self.blocks.at(self.pos)?;
        if here.is_empty() {
            return Ok(None);
        }
        let after = self.pos + here.len() as u64;
        let more = after < self.blocks.len();
        let (ch, len) = match self.encoding.decode(here, more) {
            Some(decoded) => decoded,
            None => {
                // The character goes on in the next block.
                let next = self.blocks.at(after)?;
                let mut joined = here.to_vec();
                joined.extend_from_slice(&next[..next.len().min(4)]);
                let more = self.pos + (joined.len() as u64) < self.blocks.len();
                (self.encoding.decode(&joined, more))
                    .unwrap_or((char::REPLACEMENT_CHARACTER, here.len()))
            }
        };
        let at = self.pos;
        self.pos += len as u64;
        Ok(Some((ch, at)))
    }
}

/// The line that starts at `start`.
pub fn read_line(
    blocks: &dyn Bytes,
    encoding: &Encoding,
    start: u64,
    layout: Layout,
) -> Result<Line, Missing> {
    let mut cursor = Cursor {
        blocks,
        encoding,
        pos: start,
    };
    let mut line = Line {
        start,
        end: start,
        cells: Vec::new(),
        offsets: Vec::new(),
        chars: Vec::new(),
        char_cells: Vec::new(),
        joined: Vec::new(),
    };
    for _ in 0..layout.max_line.max(1) {
        let Some((ch, at)) = cursor.next()? else {
            break;
        };
        match ch {
            '\n' => break,
            '\r' => {
                let back = cursor.pos;
                if !matches!(cursor.next()?, Some(('\n', _))) {
                    cursor.pos = back;
                }
                break;
            }
            '\t' => {
                line.chars.push(ch);
                line.char_cells.push(line.cells.len());
                let tab = layout.tab.max(1);
                let stop = (line.cells.len() / tab + 1) * tab;
                if layout.white {
                    line.cells.push('→');
                }
                line.cells.resize(stop, ' ');
                line.offsets.resize(stop, at);
            }
            _ => put_char(&mut line, (ch, at), layout),
        }
    }
    line.end = cursor.pos;
    Ok(line)
}

/// A character of the line at byte `at` laid out in its cells: two for a wide one, none for a
/// combining one after another (ADR-0025).
fn put_char(line: &mut Line, (ch, at): (char, u64), layout: Layout) {
    let cell = match (ch, layout.zero) {
        ('\0', Some(zero)) => zero,
        (' ', _) if layout.white => '·',
        (WIDE_TAIL, _) => char::REPLACEMENT_CHARACTER,
        _ => shown(ch),
    };
    let width = cell_width(cell);
    line.chars.push(ch);
    if width == 0
        && let Some(last) = line.cells.len().checked_sub(1)
    {
        // With the character before it: a wide one's first cell.
        let base = match line.cells[last] {
            WIDE_TAIL => last - 1,
            _ => last,
        };
        line.char_cells.push(base);
        line.joined.push((base, cell));
        return;
    }
    line.char_cells.push(line.cells.len());
    line.cells.push(cell);
    line.offsets.push(at);
    if width > 1 {
        line.cells.push(WIDE_TAIL);
        line.offsets.push(at);
    }
}

/// The start of the line that ends at `pos` (`pos` > `floor`, the first byte past a byte order mark).
pub fn line_before(
    blocks: &dyn Bytes,
    encoding: &Encoding,
    pos: u64,
    floor: u64,
    layout: Layout,
) -> Result<u64, Missing> {
    let unit = encoding.unit();
    let unit_before = |at: u64| -> Result<u32, Missing> {
        let bytes = blocks.before(at)?;
        let from = bytes.len().saturating_sub(unit as usize);
        Ok(encoding.unit_value(&bytes[from..]))
    };
    // Its own break first; units counted from the floor (a UTF-16 file of an odd length).
    let mut end = floor + (pos - floor) / unit * unit;
    if end <= floor {
        // Less than a unit past the floor (a UTF-16 file of one byte): that line starts there.
        return Ok(floor);
    }
    let last = unit_before(end)?;
    if last == LF || last == CR {
        end -= unit;
        if last == LF && end > floor && unit_before(end)? == CR {
            end -= unit;
        }
    }
    let limit = end.saturating_sub(layout.window()).max(floor);
    let mut start = break_before(blocks, encoding, end, limit)?.unwrap_or(limit);
    if start == limit && limit > floor {
        start = char_start(blocks, encoding, limit)?;
    }
    // From there the pieces it is cut into, up to the one that ends at `pos`.
    loop {
        let line = read_line(blocks, encoding, start, layout)?;
        if line.end >= pos || line.end <= start {
            return Ok(start);
        }
        start = line.end;
    }
}

/// Past the last break between `limit` and `end`.
fn break_before(
    blocks: &dyn Bytes,
    encoding: &Encoding,
    end: u64,
    limit: u64,
) -> Result<Option<u64>, Missing> {
    let unit = encoding.unit();
    let mut at = end;
    while at >= limit + unit {
        let bytes = blocks.before(at)?;
        let base = at - bytes.len() as u64;
        let lowest = limit.max(base);
        while at >= lowest + unit {
            let from = usize::try_from(at - unit - base).unwrap_or(usize::MAX);
            let value = encoding.unit_value(&bytes[from..]);
            if value == LF || value == CR {
                return Ok(Some(at));
            }
            at -= unit;
        }
        if at > limit && at == base {
            continue;
        }
        break;
    }
    Ok(None)
}

/// `pos` moved past the rest of a character it falls in: UTF-8's continuation bytes; a double-byte
/// page's trail byte (an odd run of lead byte values before `pos`: they pair from the run's start).
fn char_start(blocks: &dyn Bytes, encoding: &Encoding, pos: u64) -> Result<u64, Missing> {
    if let Encoding::Page(_, decoder) = encoding {
        let leads = (blocks.before(pos)?.iter().rev())
            .take_while(|byte| decoder.is_lead(**byte))
            .count();
        return Ok(pos + (leads % 2) as u64);
    }
    if !matches!(encoding, Encoding::Utf8) {
        return Ok(pos);
    }
    let bytes = blocks.at(pos)?;
    let skipped = bytes
        .iter()
        .take(3)
        .take_while(|byte| (0x80..0xC0).contains(*byte))
        .count();
    Ok(pos + skipped as u64)
}

#[cfg(test)]
#[path = "lines_tests.rs"]
mod tests;
