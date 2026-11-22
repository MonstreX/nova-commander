//! The editor's bookmarks (FAR's `m_SavePos`, `Editor::SetBookmark` / `GotoBookmark`; S2d.6f): ten
//! places, each the cursor's line by its first byte, its column, the first column shown and the
//! cursor's row on the screen. The document's bytes inserted or taken out before a bookmark move it, as
//! FAR moves its lines (by bytes here: a document past the line count has no line numbers, ADR-0015);
//! going to one finds the line holding its byte.

use super::{EditView, Missing};
use crate::editor::view::blocks::Bytes;
use crate::editor::view::line_count::breaks_between;

/// A bookmark (FAR's `InternalEditorBookmark`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mark {
    /// The cursor's line by its first byte.
    pub line: u64,
    pub col: usize,
    /// The first column shown (FAR's `LeftPos`).
    pub left: usize,
    /// The cursor's row from the screen's top (FAR's `ScreenLine`).
    pub row: usize,
}

/// The most rows counted from the screen's top to the cursor.
const ROWS: usize = 1000;

impl EditView {
    /// FAR's `SetBookmark`: the cursor's place kept as bookmark `digit`.
    pub(super) fn set_mark(&mut self, digit: usize) -> Result<(), Missing> {
        let mut row = 0;
        let mut start = self.top;
        while start < self.line && row < ROWS {
            match self.next_line(&self.read(start)?)? {
                Some(next) => start = next,
                None => break,
            }
            row += 1;
        }
        if let Some(mark) = self.marks.get_mut(digit) {
            *mark = Some(Mark {
                line: self.line,
                col: self.col,
                left: self.left,
                row,
            });
        }
        Ok(())
    }

    /// FAR's `GotoBookmark`: the cursor at bookmark `digit`, the line on its row; the block let go
    /// without persistent blocks. A bookmark not set does nothing.
    pub(super) fn go_to_mark(&mut self, digit: usize) -> Result<(), Missing> {
        let Some(Some(mark)) = self.marks.get(digit).copied() else {
            return Ok(());
        };
        let line = self.line_holding(mark.line)?;
        let mut top = line;
        for _ in 0..mark.row {
            match self.previous_line(top)? {
                Some(above) => top = above,
                None => break,
            }
        }
        (self.line, self.col, self.left, self.top) = (line, mark.col, mark.left, top);
        if !self.options.persistent_blocks {
            self.unmark();
        }
        Ok(())
    }

    /// `bytes` put into the document at `pos`: the bookmarks past it moved on.
    pub(super) fn insert_bytes(&mut self, pos: u64, bytes: &[u8]) {
        let unit = self.encoding.unit();
        let (prev, next) = (
            self.unit_near(pos.checked_sub(unit)),
            self.unit_near(Some(pos)),
        );
        let added = self.units_of(bytes);
        self.doc.insert(pos, bytes);
        let len = bytes.len() as u64;
        match (prev, next) {
            (Some(prev), Some(next)) => {
                let lines = breaks_between(prev, &added, next) - breaks_between(prev, &[], next);
                self.counter.shift(pos, (0, len), lines);
            }
            _ => self.counter.forget_from(pos),
        }
        self.move_places(|at: &mut u64| {
            if *at > pos {
                *at += len;
            }
        });
    }

    /// `count` bytes taken out of the document at `pos`: the bookmarks past them moved back, those in
    /// them to `pos`.
    pub(super) fn delete_bytes(&mut self, pos: u64, count: u64) {
        let unit = self.encoding.unit();
        let prev = self.unit_near(pos.checked_sub(unit));
        let next = self.unit_near(Some(pos + count));
        let removed = self.units(pos, pos + count);
        self.doc.delete(pos, count);
        match (prev, removed, next) {
            (Some(prev), Some(removed), Some(next)) => {
                let lines = breaks_between(prev, &[], next) - breaks_between(prev, &removed, next);
                self.counter.shift(pos, (count, 0), lines);
            }
            _ => self.counter.forget_from(pos),
        }
        self.move_places(|at: &mut u64| {
            if *at > pos {
                *at = at.saturating_sub(count).max(pos);
            }
        });
    }

    /// The places kept by bytes moved with an edit: the bookmarks, the block's ends, and (review A4)
    /// the cursor's line, the screen's first one and the vertical block's lines when the edit is
    /// before them (a plugin's line set above the cursor, a vertical block's edits).
    fn move_places(&mut self, moved: impl Fn(&mut u64)) {
        self.marks
            .iter_mut()
            .flatten()
            .for_each(|mark| moved(&mut mark.line));
        self.anchor
            .iter_mut()
            .chain(self.fixed.iter_mut())
            .for_each(&moved);
        moved(&mut self.line);
        moved(&mut self.top);
        if let Some(column) = self.column.as_mut() {
            moved(&mut column.first);
            moved(&mut column.last);
        }
    }

    /// The text units of `bytes` in the document's encoding.
    fn units_of(&self, bytes: &[u8]) -> Vec<u32> {
        let unit = usize::try_from(self.encoding.unit()).unwrap_or(1);
        bytes
            .chunks(unit)
            .map(|chunk| self.encoding.unit_value(chunk))
            .collect()
    }

    /// The units of the document's bytes `from..to`; none when a block of them is not read.
    fn units(&self, from: u64, to: u64) -> Option<Vec<u32>> {
        let mut bytes = Vec::new();
        let mut at = from;
        while at < to.min(self.doc.len()) {
            let piece = self.doc.at(at).ok()?;
            if piece.is_empty() {
                break;
            }
            let take = usize::try_from(to - at)
                .unwrap_or(usize::MAX)
                .min(piece.len());
            bytes.extend_from_slice(&piece[..take]);
            at += take as u64;
        }
        Some(self.units_of(&bytes))
    }

    /// The unit at `at` (`Some(None)` before the text or past its end); none when its block is not read.
    fn unit_near(&self, at: Option<u64>) -> Option<Option<u32>> {
        let Some(at) = at.filter(|at| *at >= self.floor && *at < self.doc.len()) else {
            return Some(None);
        };
        let units = self.units(at, at + self.encoding.unit())?;
        Some(units.first().copied())
    }
}
