//! FAR's `EditorControl` on the view (S2d.7, API 10): what a plugin reads of the editor by line numbers
//! (from 0) and character positions (a tab one character), and the steps its changes are. The lines are
//! found by walking from the cursor's or the document's first one (ADR-0015: the view knows lines by
//! their bytes; their numbers are counted).

use super::{EditView, Line, Missing, Step};

/// The lines from one known a plugin's line is walked to line by line; further, it is found by the
/// line count's checkpoints.
const NEAR: u64 = 64;

/// What a plugin reads of a line (FAR's `EditorGetString`): its text, its break, the block's part of
/// it (its first character, -1: none; the one past its last, -1: on through the break).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLine {
    pub text: String,
    pub eol: String,
    pub block: (i64, i64),
}

/// The editor's state as FAR's `EditorInfo` tells it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PluginInfo {
    pub lines: u64,
    pub line: u64,
    pub pos: usize,
    pub tab_pos: usize,
    pub top: u64,
    pub left: usize,
    pub overtype: bool,
    /// FAR's `BTYPE_*`: 0 none, 1 stream, 2 column; and the block's first line.
    pub block: i64,
    pub block_line: u64,
}

/// Where a plugin puts the cursor (FAR's `EditorSetPosition`; none: kept); lines by their first bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Place {
    pub line: Option<u64>,
    pub pos: Option<usize>,
    pub tab_pos: Option<usize>,
    pub top: Option<u64>,
    pub left: Option<usize>,
    pub overtype: Option<bool>,
}

/// A block a plugin marks (FAR's `EditorSelect`): its kind (1 stream, 2 column; else none), its first
/// line by its first byte, the character (a column block: the column) it starts at, its width and its
/// lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub kind: i64,
    pub line: u64,
    pub pos: i64,
    pub width: i64,
    pub height: i64,
}

impl EditView {
    /// The number (from 0) of the line starting at `start`.
    pub fn number_of(&mut self, start: u64) -> Result<u64, Missing> {
        let floor = self.floor;
        (self.counter).line_of(&self.doc, &self.encoding, (floor, start))
    }

    /// The first byte of the line numbered `number` (none: the cursor's); none past the last line.
    pub fn line_start_of(&mut self, number: Option<u64>) -> Result<Option<u64>, Missing> {
        let Some(number) = number else {
            return Ok(Some(self.line));
        };
        let cursor = self.number_of(self.line)?;
        // From the nearest line known: the file's first, the cursor's, the one found last (FAR's
        // `GetStringByNumber` from `m_it_LastGetLine`), forwards or backwards.
        let known = [
            Some((0, self.floor)),
            Some((cursor, self.line)),
            self.counter.last_line,
        ];
        let Some((from, mut start)) =
            (known.into_iter().flatten()).min_by_key(|(known, _)| known.abs_diff(number))
        else {
            return Ok(None);
        };
        // Far from them: by the line count's checkpoints (R7 review F4: a walk a block stopped was
        // started over, a big file's first blocks gone again before its last came).
        if from.abs_diff(number) > NEAR {
            let floor = self.floor;
            let found = (self.counter).start_of(&self.doc, &self.encoding, (floor, number))?;
            if let Some(start) = found {
                self.counter.last_line = Some((number, start));
            }
            return Ok(found);
        }
        for _ in number..from {
            match self.previous_line(start)? {
                Some(above) => start = above,
                None => return Ok(None),
            }
        }
        for _ in from..number {
            match self.next_line(&self.read(start)?)? {
                Some(next) => start = next,
                None => return Ok(None),
            }
        }
        self.counter.last_line = Some((number, start));
        Ok(Some(start))
    }

    /// The line starting at `start` as a plugin reads it.
    pub fn plugin_line(&self, start: u64) -> Result<PluginLine, Missing> {
        let line = self.read(start)?;
        let end = self.text_end(&line)?;
        let eol = match line.end - end {
            0 => "",
            len if len > self.encoding.unit() => "\r\n",
            _ if self.unit_before(line.end)? == u32::from(b'\r') => "\r",
            _ => "\n",
        };
        Ok(PluginLine {
            text: line.chars.iter().collect(),
            eol: eol.to_string(),
            block: self.block_on(&line, end)?,
        })
    }

    /// The block's part of `line` (its text ending at `end`).
    fn block_on(&self, line: &Line, end: u64) -> Result<(i64, i64), Missing> {
        let index = |pos: u64| Self::char_of(line, pos) as i64;
        if let Some(column) = self.column_span()? {
            if !(column.first..=column.last).contains(&line.start) {
                return Ok((-1, 0));
            }
            let from = Self::char_at(line, column.x);
            let to = Self::char_at(line, column.x + column.width);
            return Ok((from as i64, to as i64));
        }
        match self.block()? {
            Some((from, to)) if from <= line.end.max(end) && to > line.start => {
                let first = if from > line.start { index(from) } else { 0 };
                let past = if to > end { -1 } else { index(to) };
                Ok((first, past))
            }
            _ => Ok((-1, 0)),
        }
    }

    /// The character of `line` byte `pos` falls on (its count past the text's end).
    fn char_of(line: &Line, pos: u64) -> usize {
        (line.char_cells.iter())
            .take_while(|&&cell| line.offsets.get(cell).is_some_and(|&at| at < pos))
            .count()
    }

    /// The editor's state for a plugin.
    pub fn plugin_info(&mut self) -> Result<PluginInfo, Missing> {
        let last = self.last_line()?;
        let line = self.read(self.line)?;
        let block_start = match (self.column_span()?, self.block()?) {
            (Some(column), _) => Some((2, column.first)),
            (None, Some((from, _))) => Some((1, self.line_holding(from)?)),
            (None, None) => None,
        };
        let block_line = match block_start {
            Some((_, start)) => self.number_of(start)?,
            None => 0,
        };
        Ok(PluginInfo {
            lines: self.number_of(last)? + 1,
            line: self.number_of(self.line)?,
            pos: Self::char_at(&line, self.col),
            tab_pos: self.col,
            top: self.number_of(self.top)?,
            left: self.left,
            overtype: self.overtype,
            block: block_start.map_or(0, |(kind, _)| kind),
            block_line,
        })
    }

    /// The cursor's line (its first byte) and its character position (FAR's `CurLine`, `CurPos`).
    pub fn cursor_char(&self) -> Result<(u64, usize), Missing> {
        Ok((self.line, Self::char_at(&self.read(self.line)?, self.col)))
    }

    /// Where the cursor and the screen are: the cursor's line, its column, the first line and column
    /// shown (to put them back).
    pub fn screen_place(&self) -> Place {
        Place {
            line: Some(self.line),
            pos: None,
            tab_pos: Some(self.col),
            top: Some(self.top),
            left: Some(self.left),
            overtype: None,
        }
    }

    /// FAR's `RealPosToVisual` on the line starting at `start`.
    pub fn real_to_tab(&self, start: u64, pos: usize) -> Result<usize, Missing> {
        let line = self.read(start)?;
        Ok(match line.char_cells.get(pos) {
            Some(&cell) => cell,
            None => line.cells.len() + (pos - line.chars.len()),
        })
    }

    /// FAR's `VisualPosToReal` on the line starting at `start`.
    pub fn tab_to_real(&self, start: u64, tab_pos: usize) -> Result<usize, Missing> {
        Ok(Self::char_at(&self.read(start)?, tab_pos))
    }

    /// A plugin's change of the editor; none for another step.
    pub(super) fn plugin_step(&mut self, step: &Step) -> Option<Result<(), Missing>> {
        Some(match step {
            Step::SetLineText(start, text, eol) => {
                let eol = eol.clone().unwrap_or_else(|| self.eol.clone());
                self.set_line_text(*start, text, Some(&eol))
            }
            Step::PluginText(text) => self.plugin_text(text),
            Step::DeleteChar => self.delete(None),
            Step::Break(indent) => self.plugin_break(*indent),
            Step::Place(place) => self.place_at(place),
            Step::SelectRange(range) => self.select_range(range),
            Step::Group(begin) => {
                match begin {
                    true => self.doc.begin_group(),
                    false => self.doc.end_group(),
                }
                Ok(())
            }
            Step::ExpandTabs(start) => self.expand_tabs_at(*start),
            _ => return None,
        })
    }

    /// FAR's `ECTL_SETSTRING`: the line's text replaced (its break kept), the cursor's character
    /// position kept on it.
    fn set_line_text(
        &mut self,
        start: u64,
        text: &str,
        last_break: Option<&str>,
    ) -> Result<(), Missing> {
        let line = self.read(start)?;
        let end = self.text_end(&line)?;
        let pos = (self.line == start).then(|| Self::char_at(&line, self.col));
        // FAR's `ECTL_SETSTRING`: a line without a break (the last) gets `last_break` (review D4).
        let text = match (end == line.end, last_break) {
            (true, Some(eol)) => format!("{text}{eol}"),
            _ => text.to_string(),
        };
        self.delete_bytes(start, end - start);
        self.insert_bytes(start, &self.encoding.encode(&text));
        self.changed_at(start);
        if let Some(pos) = pos {
            let line = self.read(start)?;
            self.col = self.real_to_tab(line.start, pos)?;
        }
        Ok(())
    }

    /// FAR's `ECTL_INSERTTEXT`: typed at the cursor (`\n` as Enter), the block kept.
    fn plugin_text(&mut self, text: &str) -> Result<(), Missing> {
        for (index, part) in text.split('\n').enumerate() {
            if index > 0 {
                self.plugin_break(false)?;
            }
            if !part.is_empty() {
                self.put_in(part, None)?;
            }
        }
        Ok(())
    }

    /// FAR's `ECTL_INSERTSTRING`: Enter at the cursor, with the auto indent only when asked.
    fn plugin_break(&mut self, indent: bool) -> Result<(), Missing> {
        let auto = self.options.auto_indent;
        self.options.auto_indent = auto && indent;
        let done = self.enter(None);
        self.options.auto_indent = auto;
        done
    }

    /// FAR's `ECTL_SETPOSITION`.
    fn place_at(&mut self, place: &Place) -> Result<(), Missing> {
        if let Some(line) = place.line {
            self.line = self.line_holding(line)?;
        }
        if let Some(top) = place.top.filter(|top| *top <= self.line) {
            self.top = self.line_holding(top)?;
        }
        if let Some(pos) = place.pos {
            self.col = self.real_to_tab(self.line, pos)?;
        }
        if let Some(tab_pos) = place.tab_pos {
            self.col = tab_pos;
        }
        if let Some(left) = place.left {
            self.left = left;
        }
        if let Some(overtype) = place.overtype {
            self.overtype = overtype;
        }
        Ok(())
    }

    /// FAR's `ECTL_SELECT`.
    fn select_range(&mut self, range: &Range) -> Result<(), Missing> {
        self.unmark();
        if !matches!(range.kind, 1 | 2) || range.pos < 0 || range.height < 1 {
            return Ok(());
        }
        let first = range.line;
        let mut last = first;
        for _ in 1..range.height {
            match self.next_line(&self.read(last)?)? {
                Some(next) => last = next,
                None => break,
            }
        }
        if range.kind == 2 {
            let (x, width) = match range.width < 0 {
                true => ((range.pos + range.width).max(0), -range.width),
                false => (range.pos, range.width),
            };
            self.column = Some(super::Column {
                first,
                last,
                x: x as usize,
                width: width as usize,
                marking: false,
            });
            return Ok(());
        }
        let from = self.char_byte(&self.read(first)?, range.pos as usize)?;
        let end_line = self.read(last)?;
        // FAR: the last line ends at the start position and the width, whatever the lines.
        let to = self.char_byte(&end_line, (range.pos + range.width).max(0) as usize)?;
        self.mark_from(from);
        self.fixed = Some(to);
        Ok(())
    }

    /// FAR's `ECTL_EXPANDTABS`: the line's tabs as the spaces they show as.
    fn expand_tabs_at(&mut self, start: u64) -> Result<(), Missing> {
        let line = self.read(start)?;
        if !line.chars.contains(&'\t') {
            return Ok(());
        }
        let (chars, _) = Self::tabs_as_spaces(&line);
        self.set_line_text(start, &chars.iter().collect::<String>(), None)
    }
}
