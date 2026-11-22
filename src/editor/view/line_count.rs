//! The editor's line numbers (FAR's status line's `Ln n/total`): the breaks of the document counted
//! from its start as its bytes are read, a checkpoint kept every 64 KB of it, so a line near one is
//! counted at once; an edit forgets the checkpoints past it. A count that needs a block not read yet
//! names it (the reader brings it and the count goes on): a big file's total comes as it is read.

use std::collections::BTreeMap;

use super::blocks::{Bytes, Missing};
use super::code_page::Encoding;

/// A checkpoint every so many bytes of the document.
const EVERY: u64 = 64 * 1024;

#[derive(Debug, Default, Clone)]
pub struct LineCounter {
    /// The line (from 0) a checkpoint's byte is in, and whether a `\r` ended just before it.
    checkpoints: BTreeMap<u64, (u64, bool)>,
    /// The line last found by its number and its first byte (FAR's `m_it_LastGetLine`; review D5: a
    /// plugin reading line after line counted each from the cursor); gone with any edit.
    pub last_line: Option<(u64, u64)>,
}

/// The breaks a run of units adds after the unit `prev`, the unit `next` after it counted too (its
/// `\n` is no break after a `\r`): FAR's lines, `\r\n` one break, `\r` and `\n` one each.
pub fn breaks_between(prev: Option<u32>, run: &[u32], next: Option<u32>) -> i64 {
    let mut before = prev;
    let mut count = 0;
    for &unit in run.iter().chain(next.iter()) {
        let lone_lf = unit == 0x0A && before != Some(0x0D);
        if unit == 0x0D || lone_lf {
            count += 1;
        }
        before = Some(unit);
    }
    count
}

impl LineCounter {
    /// How many checkpoints there are (tests).
    #[cfg(test)]
    pub fn checkpoints_kept(&self) -> usize {
        self.checkpoints.len()
    }

    /// An edit at `pos`: what was counted past it is counted again.
    pub fn forget_from(&mut self, pos: u64) {
        self.last_line = None;
        self.checkpoints.split_off(&(pos + 1));
    }

    /// An edit at `pos` that took `removed` bytes out and put `added` in, `lines` more breaks (fewer
    /// when negative): the counts past it moved with it, not counted again (review A7: a key typed near
    /// a big document's start counted it to its end again).
    pub fn shift(&mut self, pos: u64, (removed, added): (u64, u64), lines: i64) {
        self.last_line = None;
        let tail = self.checkpoints.split_off(&(pos + 1));
        for (at, (line, after_cr)) in tail {
            if at <= pos + removed {
                continue;
            }
            let line = line.saturating_add_signed(lines);
            self.checkpoints
                .insert(at - removed + added, (line, after_cr));
        }
    }

    /// The line (from 0) byte `pos` is in, from `floor` (past a byte order mark).
    pub fn line_of(
        &mut self,
        bytes: &dyn Bytes,
        encoding: &Encoding,
        (floor, pos): (u64, u64),
    ) -> Result<u64, Missing> {
        let pos = pos.min(bytes.len());
        let (mut at, (mut line, mut after_cr)) = (self.checkpoints.range(..=pos).next_back())
            .map_or((floor, (0, false)), |(at, counted)| (*at, *counted));
        let unit = encoding.unit();
        while at < pos {
            let piece = bytes.at(at)?;
            if piece.is_empty() {
                break;
            }
            let take = usize::try_from(pos - at)
                .unwrap_or(usize::MAX)
                .min(piece.len());
            let whole = take - take % unit as usize;
            if whole == 0 {
                break;
            }
            for chunk in piece[..whole].chunks(unit as usize) {
                count(encoding.unit_value(chunk), (&mut line, &mut after_cr));
                at += unit;
                if at.is_multiple_of(EVERY) {
                    self.checkpoints.insert(at, (line, after_cr));
                }
            }
        }
        Ok(line)
    }

    /// The first byte of the line numbered `number` (from 0), from `floor`; none past the last line.
    /// It is walked from the last checkpoint above it and keeps them as it goes, so a walk stopped
    /// for a block goes on from as far as it came (R7 review F4: Alt+F8 walked a big file from its
    /// start each time, its first blocks gone again before its last came, for ever).
    pub fn start_of(
        &mut self,
        bytes: &dyn Bytes,
        encoding: &Encoding,
        (floor, number): (u64, u64),
    ) -> Result<Option<u64>, Missing> {
        let (mut at, (mut line, mut after_cr)) = (self.checkpoints.iter().rev())
            .find(|(_, (line, _))| *line < number)
            .map_or((floor, (0, false)), |(at, counted)| (*at, *counted));
        let unit = encoding.unit();
        loop {
            let piece = bytes.at(at)?;
            let whole = piece.len() - piece.len() % unit as usize;
            if whole == 0 {
                return Ok((line == number).then_some(at));
            }
            for chunk in piece[..whole].chunks(unit as usize) {
                let value = encoding.unit_value(chunk);
                // A line starts past its break: `\r\n`'s `\n` too.
                if line == number && !(after_cr && value == 0x0A) {
                    return Ok(Some(at));
                }
                count(value, (&mut line, &mut after_cr));
                at += unit;
                if at.is_multiple_of(EVERY) {
                    self.checkpoints.insert(at, (line, after_cr));
                }
            }
        }
    }
}

/// A unit counted: a break more, and whether it is a `\r` (whose `\n` is no break).
fn count(value: u32, (line, after_cr): (&mut u64, &mut bool)) {
    match value {
        0x0A if *after_cr => *after_cr = false,
        0x0A => *line += 1,
        0x0D => {
            *line += 1;
            *after_cr = true;
        }
        _ => *after_cr = false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::view::blocks::Blocks;

    fn loaded(bytes: &[u8]) -> Blocks {
        let mut blocks = Blocks::new(bytes.len() as u64);
        for (index, chunk) in bytes
            .chunks(crate::editor::view::BLOCK as usize)
            .enumerate()
        {
            blocks.insert(index as u64, chunk.to_vec());
        }
        blocks
    }

    #[test]
    fn breaks_of_every_kind_are_counted() {
        let blocks = loaded(b"a\nb\r\nc\rd");
        let mut counter = LineCounter::default();
        let line =
            |counter: &mut LineCounter, pos| counter.line_of(&blocks, &Encoding::Utf8, (0, pos));
        assert_eq!(line(&mut counter, 0), Ok(0));
        assert_eq!(line(&mut counter, 2), Ok(1));
        assert_eq!(line(&mut counter, 4), Ok(2), "\\r\\n: one break");
        assert_eq!(line(&mut counter, 5), Ok(2));
        assert_eq!(line(&mut counter, 7), Ok(3));
    }

    #[test]
    fn checkpoints_are_kept_and_forgotten() {
        let text: Vec<u8> = (0..40_000).flat_map(|_| *b"line\r\n").collect();
        let blocks = loaded(&text);
        let mut counter = LineCounter::default();
        let end = text.len() as u64;
        assert_eq!(
            counter.line_of(&blocks, &Encoding::Utf8, (0, end)),
            Ok(40_000)
        );
        assert_eq!(counter.checkpoints.len(), 3, "every 64 KB of 240 000 bytes");
        assert_eq!(
            counter.line_of(&blocks, &Encoding::Utf8, (0, 6 * 15_000)),
            Ok(15_000)
        );
        counter.forget_from(EVERY);
        assert_eq!(counter.checkpoints.len(), 1);
        let missing = Blocks::new(10);
        assert_eq!(
            LineCounter::default().line_of(&missing, &Encoding::Utf8, (0, 5)),
            Err(Missing(0))
        );
    }

    #[test]
    fn a_lines_start_is_past_its_break_of_every_kind() {
        let blocks = loaded(b"ab\ncd\r\nef\rg\n");
        let mut counter = LineCounter::default();
        let mut start = |number| counter.start_of(&blocks, &Encoding::Utf8, (0, number));
        let starts: Vec<_> = (0..6).map(&mut start).collect();
        assert_eq!(
            starts,
            [
                Ok(Some(0)),
                Ok(Some(3)),
                Ok(Some(7)),
                Ok(Some(10)),
                Ok(Some(12)),
                Ok(None)
            ],
            "the empty line past the last break is a line, none past it"
        );
        let bom = loaded(b"\xEF\xBB\xBFa\nb");
        let mut counter = LineCounter::default();
        assert_eq!(counter.start_of(&bom, &Encoding::Utf8, (3, 0)), Ok(Some(3)));
        assert_eq!(counter.start_of(&bom, &Encoding::Utf8, (3, 1)), Ok(Some(5)));
    }

    /// R7 review F4: with only the blocks past a checkpoint at hand the walk goes on from it; it never
    /// needs the file's first blocks again.
    #[test]
    fn a_walk_goes_on_from_the_checkpoints_it_kept() {
        let text: Vec<u8> = (0..60_000).flat_map(|_| *b"line\r\n").collect();
        let full = loaded(&text);
        let mut counter = LineCounter::default();
        let mut partial = Blocks::new(text.len() as u64);
        let block = crate::editor::view::BLOCK as usize;
        partial.insert(0, text[..block].to_vec());
        assert_eq!(
            counter.start_of(&partial, &Encoding::Utf8, (0, 50_000)),
            Err(Missing(1))
        );
        // Only the blocks past the first: the walk is not started over.
        let mut later = Blocks::new(text.len() as u64);
        for (index, chunk) in text.chunks(block).enumerate().skip(1) {
            later.insert(index as u64, chunk.to_vec());
        }
        assert_eq!(
            counter.start_of(&later, &Encoding::Utf8, (0, 50_000)),
            Ok(Some(6 * 50_000))
        );
        assert_eq!(
            counter.start_of(&full, &Encoding::Utf8, (0, 60_000)),
            Ok(Some(text.len() as u64))
        );
        assert_eq!(
            counter.start_of(&full, &Encoding::Utf8, (0, 60_001)),
            Ok(None)
        );
    }
}
