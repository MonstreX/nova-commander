//! The editor's edits and blocks (FAR's `Editor::ProcessKey` for typing, Enter, BS, Del, Ctrl+Y, the
//! stream block with Shift, Ctrl+A / Ctrl+U, the clipboard's text, undo and redo). FAR's defaults: the
//! block is not persistent (a move without Shift lets it go, typing takes its place), Del takes it out
//! ("Del removes blocks"), the cursor past a line's end types after spaces filling up to it.

use super::{EditView, Step};
use crate::editor::view::blocks::{Bytes, Missing};
use crate::editor::view::lines::Line;

/// What a step may change of the view besides the document, put back when it stops.
struct Kept {
    place: (u64, usize, u64, usize),
    anchor: Option<u64>,
    found: Option<(u64, u64)>,
    changed: Option<u64>,
    marks: [Option<super::Mark>; 10],
    overtype: bool,
    fixed: Option<u64>,
    column: Option<super::Column>,
    hex: Option<super::hex::Hex>,
    counter: super::super::line_count::LineCounter,
}

impl EditView {
    /// One of the editor's steps on a screen `height` rows high, done whole or not at all: one that
    /// stops for a block not read yet leaves the document and the view as they were, so that its
    /// retry once the block is read does not do its edits twice (ADR-0015 S2d.4b).
    pub fn apply(&mut self, step: &Step, height: usize) -> Result<(), Missing> {
        let kept = Kept {
            place: (self.line, self.col, self.top, self.left),
            anchor: self.anchor,
            found: self.found,
            changed: self.changed,
            marks: self.marks,
            overtype: self.overtype,
            fixed: self.fixed,
            column: self.column,
            hex: self.hex,
            counter: self.counter.clone(),
        };
        // Characters typed one after another are one undo step (FAR's `AddUndoData`); anything else
        // - a move too - starts a new one.
        match step {
            Step::Type(_) if !self.overtype => self.doc.type_next(),
            Step::Replace(.., true) => self.doc.join_next(),
            _ => self.doc.end_typing(),
        }
        self.doc.begin_attempt();
        let done = self.apply_step(step, height);
        if done.is_ok() {
            self.doc.end_attempt();
            return done;
        }
        // The counts of a document the step left as it was stay: its retry goes on from them (R7
        // review F4: a walk to a far line started over each time).
        if self.doc.attempt_changed() {
            self.counter = kept.counter;
        }
        self.doc.roll_back();
        (self.line, self.col, self.top, self.left) = kept.place;
        (self.anchor, self.found, self.changed) = (kept.anchor, kept.found, kept.changed);
        (self.marks, self.overtype, self.fixed) = (kept.marks, kept.overtype, kept.fixed);
        (self.column, self.hex) = (kept.column, kept.hex);
        done
    }

    /// The step itself.
    fn apply_step(&mut self, step: &Step, height: usize) -> Result<(), Missing> {
        if self.locked && step.edits() {
            return Ok(());
        }
        if let Some(done) = self.hex_dispatch(step, height) {
            return done;
        }
        self.found = None;
        if let Step::Column(inner) = step {
            return self.column_move(inner, height);
        }
        if let Some(column) = self.column.as_mut() {
            column.marking = false;
        }
        if let Step::Select(inner) = step {
            return self.select(inner, height);
        }
        let block = self.block()?;
        let persistent = self.options.persistent_blocks;
        if persistent {
            self.fix_block()?;
        }
        if self.move_by(step, height)? {
            // FAR's non-persistent blocks: any move lets the block go.
            if !persistent {
                self.unmark();
            }
            return Ok(());
        }
        // A persistent block stays where typing goes in beside it.
        let replaced = block.filter(|_| !persistent);
        let own = self.block_step(step, block);
        if let Some(done) = own.or_else(|| self.code_step(step, block, replaced)) {
            return done;
        }
        match step {
            Step::Type('\t') if self.options.expand_tabs > 0 => self.tab_as_spaces(replaced),
            Step::Type(ch) if self.overtype => self.type_over(*ch, replaced),
            Step::Type(ch) => self.typed(&ch.to_string(), replaced),
            Step::Quoted(ch) => self.put_in(&ch.to_string(), replaced),
            Step::Insert(text) => self.put_in(text, replaced),
            Step::InsertName(name) => self.insert_name(name),
            Step::DeleteWordLeft => self.delete_word_left(replaced),
            Step::DeleteWordRight => self.delete_word_right(),
            Step::DeleteToStart => self.delete_line_part(false),
            Step::DeleteToEnd => self.delete_line_part(true),
            Step::Overtype => self.switch_overtype(),
            Step::Paste(text) => {
                // FAR's `Paste`: a vertical block let go.
                self.column = None;
                self.typed(text, replaced)
            }
            Step::Enter => self.enter(replaced),
            // FAR's "BS like Del" (on): with "Del removes blocks" any block goes.
            Step::Backspace => self.backspace(
                block
                    .filter(|_| self.options.del_removes_blocks)
                    .or(replaced),
            ),
            Step::Delete => {
                let taken = block.filter(|_| self.options.del_removes_blocks);
                if taken.is_none() && !persistent {
                    self.anchor = None;
                }
                self.delete(taken)
            }
            Step::DeleteLine => self.delete_line(),
            Step::SelectAll => self.select_all(height),
            Step::Unselect => {
                self.unmark();
                Ok(())
            }
            Step::MarkLine => self.mark_line(),
            Step::MarkChars(from, to) => self.mark_chars(*from, *to),
            Step::Undo => self.undone(false),
            Step::Redo => self.undone(true),
            Step::GoTo(line, col) => self.go_to_line(*line, *col),
            Step::Show(at, size) => self.show_found(*at, *size),
            Step::Restore(line, col, top, left) => self.restore(*line, *col, (*top, *left)),
            Step::SetMark(digit) => self.set_mark(*digit),
            Step::GoToMark(digit) => self.go_to_mark(*digit),
            Step::Replace(at, size, text, _) => self.replace(*at, *size, text),
            Step::ReplaceAll(found, text, backward) => self.replace_all(found, text, *backward),
            _ => Ok(()),
        }
    }

    /// Ins: FAR's overtype on or off.
    fn switch_overtype(&mut self) -> Result<(), Missing> {
        self.overtype = !self.overtype;
        Ok(())
    }

    /// A move holding Shift: the stream block from where the cursor was (a block left where it was
    /// gives way to a new one, FAR's marking begun again).
    fn select(&mut self, inner: &Step, height: usize) -> Result<(), Missing> {
        let pos = self.cursor_pos()?;
        let anchor = match (self.anchor, self.fixed) {
            (Some(anchor), None) => anchor,
            _ => pos,
        };
        self.move_by(inner, height)?;
        self.mark_from(anchor);
        Ok(())
    }

    /// Ctrl+A: the whole document marked, the cursor at its end.
    fn select_all(&mut self, height: usize) -> Result<(), Missing> {
        self.apply_step(&Step::Start, height)?;
        self.mark_from(self.floor);
        let end = self.doc.len();
        let last = self.line_holding(end)?;
        self.line = last;
        self.col = Self::col_of(&self.read(last)?, end);
        Ok(())
    }

    /// Ctrl+F: FAR's `KEY_CTRLF` - a block not persistent taken out (a vertical one too), the name
    /// put in, any block let go.
    fn insert_name(&mut self, name: &str) -> Result<(), Missing> {
        if !self.options.persistent_blocks {
            match self.block()? {
                Some(block) => self.take_out(block)?,
                None if self.column.is_some() => self.delete_column()?,
                None => {}
            }
        }
        self.unmark();
        self.put_in(name, None)
    }

    /// The cursor's line's characters `from` to `to` marked, the block's end not the cursor.
    fn mark_chars(&mut self, from: usize, to: usize) -> Result<(), Missing> {
        let line = self.read(self.line)?;
        let (at, past) = (self.char_byte(&line, from)?, self.char_byte(&line, to)?);
        self.mark_from(at);
        self.fixed = Some(past);
        Ok(())
    }

    /// The cursor's line marked with its break, the block's end not the cursor.
    fn mark_line(&mut self) -> Result<(), Missing> {
        let line = self.read(self.line)?;
        self.mark_from(line.start);
        self.fixed = Some(line.end.max(self.text_end(&line)?));
        Ok(())
    }

    /// The block taken out, the cursor where it began.
    pub(super) fn take_out(&mut self, (from, to): (u64, u64)) -> Result<(), Missing> {
        let line = self.line_holding(from)?;
        let col = Self::col_of(&self.read(line)?, from);
        self.delete_bytes(from, to - from);
        self.changed_at(from);
        self.anchor = None;
        (self.line, self.col) = (line, col);
        Ok(())
    }

    /// Where the cursor's text goes in: its byte, after spaces up to a cursor past the line's end.
    pub(super) fn insert_point(&self, line: &Line) -> Result<(u64, String), Missing> {
        let pos = self.offset_at(line, self.col)?;
        let pad = self.col.saturating_sub(line.cells.len());
        Ok((pos, " ".repeat(pad)))
    }

    /// Text typed or pasted at the cursor, in place of the block; its breaks the file's.
    pub(super) fn typed(&mut self, text: &str, block: Option<(u64, u64)>) -> Result<(), Missing> {
        let normal = (text.replace("\r\n", "\n").replace('\r', "\n")).replace('\n', &self.eol);
        self.put_in(&normal, block)
    }

    /// `text` put in at the cursor as it is, in place of the block.
    pub(super) fn put_in(&mut self, text: &str, block: Option<(u64, u64)>) -> Result<(), Missing> {
        // Typed over a block: the block's going and the text one undo step (review A2).
        if let Some(block) = block {
            self.doc.begin_group();
            let put = self.take_out(block).and_then(|()| self.put_in(text, None));
            self.doc.end_group();
            return put;
        }
        let line = self.read(self.line)?;
        let (pos, pad) = self.insert_point(&line)?;
        let bytes = self.encoding.encode(&format!("{pad}{text}"));
        self.insert_bytes(pos, &bytes);
        self.changed_at(pos);
        let end = pos + bytes.len() as u64;
        self.line = self.line_holding(end)?;
        self.col = Self::col_of(&self.read(self.line)?, end);
        if self.options.expand_tabs == 2 {
            self.expand_line_tabs()?;
        }
        Ok(())
    }

    /// FAR's `EXPAND_ALLTABS`: the cursor's line's tabs as the spaces they show as (the cursor where
    /// it was on the screen).
    fn expand_line_tabs(&mut self) -> Result<(), Missing> {
        let line = self.read(self.line)?;
        let tabs: Vec<usize> = (0..line.chars.len())
            .filter(|&at| line.chars[at] == '\t')
            .collect();
        for &at in tabs.iter().rev() {
            let cells = line.cell_of(at + 1) - line.cell_of(at);
            let from = line.offsets[line.cell_of(at)];
            let unit = self.encoding.encode("\t").len() as u64;
            self.delete_bytes(from, unit);
            // As every edit: the lines counted and the places after it moved (review A8: the raw
            // insert left them where the tab's byte was).
            let spaces = self.encoding.encode(&" ".repeat(cells));
            self.insert_bytes(from, &spaces);
            self.changed_at(from);
        }
        Ok(())
    }

    /// Enter: the line broken at the cursor (past its end: at its end), the cursor at the next line's
    /// start.
    pub(super) fn enter(&mut self, block: Option<(u64, u64)>) -> Result<(), Missing> {
        if let Some(block) = block {
            self.take_out(block)?;
        }
        let line = self.read(self.line)?;
        let pos = self.offset_at(&line, self.col.min(line.cells.len()))?;
        // FAR's "Auto indent": the new line starts with the current one's leading blanks.
        let indent: String = match self.options.auto_indent {
            true => (line.chars.iter())
                .take_while(|c| matches!(c, ' ' | '\t'))
                .collect(),
            false => String::new(),
        };
        let eol = self.encoding.encode(&format!("{}{indent}", self.eol));
        self.insert_bytes(pos, &eol);
        self.changed_at(pos);
        let end = pos + eol.len() as u64;
        let broken = self.line;
        self.line = self.line_holding(end)?;
        self.column_line_added(broken, self.line);
        self.col = Self::col_of(&self.read(self.line)?, end);
        Ok(())
    }

    /// FAR's "Expand tabs": a Tab typed as spaces up to the next tab stop.
    fn tab_as_spaces(&mut self, block: Option<(u64, u64)>) -> Result<(), Missing> {
        // The stop from where the block went (it was counted from the cursor at the block's end).
        if let Some(block) = block {
            self.doc.begin_group();
            let done = self.take_out(block).and_then(|()| self.tab_as_spaces(None));
            self.doc.end_group();
            return done;
        }
        let tab = self.tab_size();
        let spaces = tab - self.col % tab;
        self.typed(&" ".repeat(spaces), block)
    }

    /// BS: the character before the cursor out, or the line joined to the one before at its start; past
    /// the line's end only the cursor goes left.
    pub(super) fn backspace(&mut self, block: Option<(u64, u64)>) -> Result<(), Missing> {
        if let Some(block) = block {
            return self.take_out(block);
        }
        let line = self.read(self.line)?;
        if self.col > line.cells.len() {
            self.col -= 1;
            return Ok(());
        }
        if self.col > 0 {
            let from = line.offsets[self.col - 1];
            let to = self.offset_at(&line, self.col)?;
            self.delete_bytes(from, to - from);
            self.changed_at(from);
            self.col = Self::col_of(&self.read(self.line)?, from);
            return Ok(());
        }
        if self.line <= self.floor {
            return Ok(());
        }
        let previous = self.line_holding(self.line - 1)?;
        let before = self.read(previous)?;
        let end = self.text_end(&before)?;
        self.delete_bytes(end, self.line - end);
        self.changed_at(end);
        self.line = previous;
        self.col = before.cells.len();
        Ok(())
    }

    /// Del: the block out, else the character at the cursor, else (at or past the line's end) the
    /// next line joined to this one after spaces up to the cursor.
    pub(super) fn delete(&mut self, block: Option<(u64, u64)>) -> Result<(), Missing> {
        if let Some(block) = block {
            return self.take_out(block);
        }
        let line = self.read(self.line)?;
        if self.col < line.cells.len() {
            let from = line.offsets[self.col];
            let next = (self.col..line.cells.len()).find(|&at| line.offsets[at] != from);
            let to = match next {
                Some(at) => line.offsets[at],
                None => self.text_end(&line)?,
            };
            self.delete_bytes(from, to - from);
            self.changed_at(from);
            return Ok(());
        }
        let breaks = self.break_len(&line)?;
        if breaks == 0 {
            return Ok(());
        }
        let end = self.text_end(&line)?;
        self.delete_bytes(end, breaks);
        let pad = self.col - line.cells.len();
        if pad > 0 {
            let spaces = self.encoding.encode(&" ".repeat(pad));
            self.insert_bytes(end, &spaces);
        }
        self.changed_at(end);
        Ok(())
    }

    /// Ctrl+Y: the cursor's line out with its break, the cursor's column kept.
    fn delete_line(&mut self) -> Result<(), Missing> {
        let line = self.read(self.line)?;
        self.column_line_taken(line.start)?;
        let end = line.end.max(self.text_end(&line)?);
        if end > line.start {
            self.delete_bytes(line.start, end - line.start);
            self.changed_at(line.start);
        }
        self.anchor = None;
        Ok(())
    }

    /// Undo or redo: the cursor where the edit was made.
    fn undone(&mut self, redo: bool) -> Result<(), Missing> {
        let at = match redo {
            true => self.doc.redo(),
            false => self.doc.undo(),
        };
        let Some(at) = at else {
            return Ok(());
        };
        self.counter_reset();
        self.unmark();
        self.line = self.line_holding(at)?;
        self.col = Self::col_of(&self.read(self.line)?, at);
        Ok(())
    }

    /// Undo and redo change what is anywhere: the counts are made again.
    pub(super) fn counter_reset(&mut self) {
        self.counter.forget_from(self.floor);
        self.changed_at(self.floor);
    }

    /// Alt+F8: line `line` (from 0; past the last: the last) at column `col`, found by the line
    /// count's checkpoints (R7 review F4).
    fn go_to_line(&mut self, line: u64, col: usize) -> Result<(), Missing> {
        let floor = self.floor;
        let start = match (self.counter).start_of(&self.doc, &self.encoding, (floor, line))? {
            Some(start) => start,
            None => {
                let end = self.doc.len();
                let last = (self.counter).line_of(&self.doc, &self.encoding, (floor, end))?;
                (self.counter)
                    .start_of(&self.doc, &self.encoding, (floor, last))?
                    .unwrap_or(floor)
            }
        };
        self.line = start;
        self.col = col;
        self.unmark();
        Ok(())
    }

    /// A place kept put back; the lines are found by the bytes, so a file changed since puts the
    /// cursor on a line's start still.
    fn restore(&mut self, line: u64, col: usize, (top, left): (u64, usize)) -> Result<(), Missing> {
        self.line = self.line_holding(line)?;
        self.top = self.line_holding(top.min(line))?;
        (self.col, self.left) = (col, left);
        self.unmark();
        Ok(())
    }

    /// A match replaced: its bytes out, the text in, the cursor past it.
    fn replace(&mut self, at: u64, size: u64, text: &str) -> Result<(), Missing> {
        // One undo step, as FAR's (review A2: undone, the match stood deleted, nothing put in).
        self.doc.begin_group();
        self.delete_bytes(at, size);
        let bytes = self.encoding.encode(text);
        self.insert_bytes(at, &bytes);
        self.doc.end_group();
        self.changed_at(at);
        let end = at + bytes.len() as u64;
        self.line = self.line_holding(end)?;
        self.col = Self::col_of(&self.read(self.line)?, end);
        self.unmark();
        Ok(())
    }

    /// "All"'s matches (the last first, none overlapping) replaced by `text` as one undo step: each
    /// replaced before those ahead of it move, so every offset holds; the cursor past the last one, or
    /// at the first going back.
    fn replace_all(&mut self, found: &[(u64, u64)], text: &str, back: bool) -> Result<(), Missing> {
        let (Some(&(last, _)), Some(&(first, _))) = (found.first(), found.last()) else {
            return Ok(());
        };
        let bytes = self.encoding.encode(text);
        self.doc.begin_group();
        for &(at, size) in found {
            self.delete_bytes(at, size);
            self.insert_bytes(at, &bytes);
            self.changed_at(at);
        }
        self.doc.end_group();
        // The last one moved by what the ones before it changed.
        let before: i64 = (found[1..].iter())
            .map(|&(_, size)| bytes.len() as i64 - size as i64)
            .sum();
        let end = (last as i64 + before + bytes.len() as i64).max(0) as u64;
        let place = if back { first } else { end };
        self.line = self.line_holding(place)?;
        self.col = Self::col_of(&self.read(self.line)?, place);
        self.unmark();
        Ok(())
    }

    /// The block's text (Ctrl+C / Ctrl+Ins; a vertical block's as FAR's `VBlock2Text`); none without
    /// a block.
    pub fn block_text(&self) -> Result<Option<String>, Missing> {
        if let Some(column) = self.column_span()? {
            return self.column_text(&column).map(Some);
        }
        let Some((from, to)) = self.block()? else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        let mut at = from;
        while at < to {
            let piece = self.doc.at(at)?;
            if piece.is_empty() {
                break;
            }
            let take = usize::try_from(to - at)
                .unwrap_or(usize::MAX)
                .min(piece.len());
            bytes.extend_from_slice(&piece[..take]);
            at += take as u64;
        }
        let mut text = String::new();
        let mut rest = &bytes[..];
        while !rest.is_empty() {
            let (ch, len) = self.encoding.decode(rest, false).unwrap_or(('\u{FFFD}', 1));
            text.push(ch);
            rest = &rest[len.max(1).min(rest.len())..];
        }
        Ok(Some(text))
    }
}
