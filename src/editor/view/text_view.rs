//! What a view shows and how its keys move it (FAR's `Viewer::ProcessKey` in text mode): the screen
//! starts at a row of a line, found again from bytes whatever the width; every move is a few lines'
//! work. A move that needs a block not read yet changes nothing and names it: the caller reads it and
//! asks again.

use super::blocks::{Blocks, Missing};
use super::code_page::Encoding;
use super::lines::{Layout, Line, line_before, read_line};

/// A key's move (FAR's viewer keys of text mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    /// FAR's PgUp: up a screen less one row.
    PageUp,
    /// FAR's PgDn: the last row shown becomes the first.
    PageDown,
    Home,
    /// The last screen: the file's end on its last row.
    End,
    /// FAR's Ctrl+PgUp / Ctrl+PgDn: the file's start or its last screen, the columns kept.
    Start,
    Finish,
    /// Columns left or right, without wrapping (FAR's Left / Right, Ctrl+Left / Ctrl+Right).
    Left(usize),
    Right(usize),
    /// Back to the lines' first column (FAR's Ctrl+Shift+Left).
    LineStarts,
    /// Another way of showing the file (FAR's F4, F2 in hex, Shift+F4's menu): the place kept.
    Mode(Mode),
    /// Hex mode's bytes per line by this much, to a multiple of it (FAR's Alt+Left / Alt+Right by 1,
    /// Ctrl+Alt+Left / Ctrl+Alt+Right by 16).
    Bytes(i32),
    /// Hex and dump: the place by characters (FAR's Ctrl+Left / Ctrl+Right, `RollContents`).
    Roll(i64),
    /// Alt+F8's place (FAR's `GoTo`): text from the row that shows the byte, hex and dump from it.
    To(u64),
    /// Alt+F8's column of lines not wrapped (FAR's `LeftPos`).
    Column(usize),
    /// What a search found shown (FAR's `SelectText`): text from the row that shows the byte, lines not
    /// wrapped scrolled to it past ten columns from the right; hex and dump from its row.
    Show(u64),
    /// The byte editing's moves of its cursor, Tab, a character typed (ADR-0027, hex mode).
    Byte(super::hex_cursor::HexMove),
    ByteColumn,
    TypeByte(char),
}

/// How the file is shown (FAR's `VIEWER_MODE_TYPE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Text,
    /// Offsets, bytes and their characters (FAR's `VMT_HEX`).
    Hex,
    /// The characters of every byte, lines not broken (FAR's `VMT_DUMP`).
    Dump,
}

/// The rows laid out from the top.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Screen {
    pub rows: Vec<String>,
    /// The block the next row needs; the rows before it are shown.
    pub missing: Option<Missing>,
    /// The file's end is on screen (FAR's `LastPage`): Down and PgDn do nothing.
    pub at_end: bool,
    /// The first row's first byte (FAR's `FilePos`).
    pub position: u64,
    /// The first byte past the last row (FAR's `EndOfScreen`).
    pub end: u64,
    /// Each row's cells a selection covers, as a range.
    pub marks: Vec<Option<(usize, usize)>>,
    /// More cells marked on each row (the bytes typed in hex) and the byte editing's cursor.
    pub spans: Vec<Vec<(usize, usize)>>,
    pub cursor: Option<(usize, usize)>,
    /// Where each row starts: its line and the row in it.
    tops: Vec<Top>,
}

/// The first row on screen: the line's first byte and the row in it at `width`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Top {
    line: u64,
    row: usize,
}

#[derive(Debug)]
pub struct TextView {
    blocks: Blocks,
    /// None until the first block tells it.
    encoding: Option<Encoding>,
    /// The first byte past a byte order mark.
    floor: u64,
    top: Top,
    /// The width `top.row` was counted at.
    width: usize,
    pub wrap: bool,
    /// FAR's word wrap (`m_WordWrap`, Shift+F2): the rows wrapped at the words.
    pub words: bool,
    left: usize,
    mode: Mode,
    /// Hex and dump: the first row's first byte (FAR's `FilePos`).
    pos: u64,
    /// Hex: the bytes of a line (FAR's `m_HexModeBytesPerLine`).
    hex_bytes: usize,
    /// What a search found: its first byte and its bytes.
    selection: Option<(u64, u64)>,
    /// The tab stops and the longest line (the viewer's settings).
    pub(super) layout: Layout,
    /// The byte editing in hex (ADR-0027), when on.
    edit: Option<dump_edit::ByteEdit>,
}

impl TextView {
    /// A view of a file `len` bytes long, wrapped (FAR's default `IsWrap`).
    pub fn new(len: u64) -> Self {
        Self {
            blocks: Blocks::new(len),
            encoding: None,
            floor: 0,
            top: Top { line: 0, row: 0 },
            width: 1,
            wrap: true,
            words: false,
            left: 0,
            mode: Mode::Text,
            pos: 0,
            hex_bytes: dump::HEX_BYTES,
            selection: None,
            layout: Layout::default(),
            edit: None,
        }
    }

    /// Another tab size or longest line: the screen laid out anew from its first line.
    pub fn set_layout(&mut self, layout: Layout) {
        if layout != self.layout {
            self.layout = layout;
            self.top.row = 0;
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The first byte past a byte order mark: where line 1 starts.
    pub fn floor(&self) -> u64 {
        self.floor
    }

    /// The file's length as last told.
    pub fn len(&self) -> u64 {
        self.blocks.len()
    }

    /// Block `index` as read.
    pub fn insert(&mut self, index: u64, bytes: Vec<u8>) {
        self.blocks.insert(index, bytes);
    }

    /// What a search found, as text in the file's code page (FAR's Ctrl+C in the viewer); none when
    /// nothing is or its bytes are not all read.
    pub fn selected_text(&self) -> Option<String> {
        let (start, size) = self.selection?;
        let encoding = self.encoding.as_ref()?;
        let (mut bytes, mut at, end) = (Vec::new(), start, start + size);
        while at < end {
            let piece = self.blocks.at(at).ok()?;
            let take = usize::try_from(end - at)
                .unwrap_or(usize::MAX)
                .min(piece.len());
            if take == 0 {
                break;
            }
            bytes.extend_from_slice(&piece[..take]);
            at += take as u64;
        }
        let (mut text, mut rest) = (String::new(), &bytes[..]);
        while let Some((ch, len)) = (!rest.is_empty())
            .then(|| encoding.decode(rest, false))
            .flatten()
        {
            text.push(ch);
            rest = &rest[len.clamp(1, rest.len())..];
        }
        Some(text)
    }

    /// The file is now `len` long; one that shrank is shown again from its start.
    pub fn set_len(&mut self, len: u64) {
        let shrank = len < self.len();
        self.blocks.set_len(len);
        if shrank || self.top.line > len {
            self.top = Top {
                line: self.floor,
                row: 0,
            };
        }
        if shrank || self.pos > len {
            self.pos = 0;
        }
    }

    /// The first block (all of a shorter file) to tell the code page by.
    pub fn first_block(&self) -> Result<&[u8], Missing> {
        self.blocks.at(0)
    }

    pub fn encoding(&self) -> Option<&Encoding> {
        self.encoding.as_ref()
    }

    /// The code page and the byte order mark's length the first block told.
    pub fn set_encoding(&mut self, encoding: Encoding, floor: u64) {
        self.encoding = Some(encoding);
        self.floor = floor.min(self.len());
        if self.top.line < self.floor {
            self.top = Top {
                line: self.floor,
                row: 0,
            };
        }
    }

    /// Another code page chosen (FAR's F8 / Shift+F8): the screen from its line's start (FAR's
    /// `AdjustFilePos`).
    pub fn change_encoding(&mut self, encoding: Encoding) {
        self.encoding = Some(encoding);
        self.top.row = 0;
    }

    /// FAR's F2 (`ProcessWrapMode`): wrapped from the column 0, the screen from its line's start.
    pub fn set_wrap(&mut self, wrap: bool) {
        self.wrap = wrap;
        self.top.row = 0;
        if wrap {
            self.left = 0;
        }
    }

    /// FAR's Shift+F2 (`ProcessTypeWrapMode`): wrapped at the words or by the width, wrapped either way.
    pub fn set_words(&mut self, words: bool) {
        self.words = words;
        self.set_wrap(true);
    }

    /// The first column shown of lines not wrapped (FAR's `LeftPos`).
    pub fn left(&self) -> usize {
        self.left
    }

    fn line(&self, start: u64) -> Result<Line, Missing> {
        let encoding = self.encoding.as_ref().ok_or(Missing(0))?;
        read_line(&self.blocks, encoding, start, self.layout)
    }

    /// Whether each row of a laid out screen has more of its line to the left and to the right (lines
    /// not wrapped: FAR's `«` and `»`).
    pub fn overflow(&self, screen: &Screen, width: usize) -> Vec<(bool, bool)> {
        (screen.tops.iter())
            .map(|top| match self.line(top.line) {
                Ok(line) if !self.wrap => (
                    self.left > 0 && !line.cells.is_empty(),
                    line.cells.len() > self.left + width,
                ),
                _ => (false, false),
            })
            .collect()
    }

    /// The rows of a laid out screen: each one's line and its first cell shown (wrapped or scrolled).
    pub fn rows(&self, screen: &Screen, width: usize) -> Vec<(u64, usize)> {
        (screen.tops.iter())
            .map(|top| {
                let cell = match (self.wrap, self.words) {
                    (false, _) => self.left,
                    (true, false) => top.row * width,
                    (true, true) => (self.line(top.line).ok())
                        .and_then(|line| line.row_bounds(width, true).get(top.row).copied())
                        .map_or(0, |bounds| bounds.0),
                };
                (top.line, cell)
            })
            .collect()
    }

    /// The top's row at `width` (counted again after the width changed).
    fn top_at(&self, line: &Line, width: usize) -> usize {
        if !self.wrap {
            return 0;
        }
        let row = if width == self.width {
            self.top.row
        } else {
            let start = line.row_start(self.top.row, self.width, self.words);
            line.row_of(start, width, self.words)
        };
        row.min(line.rows(width, self.words) - 1)
    }

    /// The rows of a `width` by `height` screen.
    pub fn screen(&self, width: usize, height: usize) -> Screen {
        let mut screen = Screen::default();
        let laid = match self.mode {
            Mode::Text => self.lay_out(width.max(1), height, &mut screen),
            Mode::Hex | Mode::Dump => self.lay_out_bytes(width.max(1), height, &mut screen),
        };
        if let Err(missing) = laid {
            screen.missing = Some(missing);
        }
        screen
    }

    fn lay_out(&self, width: usize, height: usize, screen: &mut Screen) -> Result<(), Missing> {
        let mut start = self.top.line;
        let mut first = None;
        screen.position = start;
        while screen.rows.len() < height {
            if start >= self.len() {
                screen.at_end = true;
                return Ok(());
            }
            let line = self.line(start)?;
            let from = *first.get_or_insert_with(|| {
                let row = self.top_at(&line, width);
                screen.position = line.row_start(row, width, self.words);
                row
            });
            let bounds = match self.wrap {
                true => line.row_bounds(width, self.words),
                false => vec![(self.left, self.left + width)],
            };
            for (row, &(cell, past)) in bounds.iter().enumerate().skip(from) {
                if screen.rows.len() == height {
                    return Ok(());
                }
                screen.rows.push(line.text(cell, past - cell));
                screen
                    .marks
                    .push(self.marked_cells(&line, cell, past - cell));
                screen.tops.push(Top { line: start, row });
                screen.end = match bounds.get(row + 1) {
                    Some(next) => line.offsets.get(next.0).copied().unwrap_or(line.end),
                    None => line.end,
                };
            }
            first = Some(0);
            if line.end >= self.len() || line.end <= start {
                screen.at_end = true;
                screen.end = self.len();
                return Ok(());
            }
            start = line.end;
        }
        Ok(())
    }

    /// FAR's `SelectText` in text mode: the row that shows `pos` on top, column 0; lines not wrapped
    /// scrolled so the byte stands ten columns from the right when it would be past them.
    fn show_text(&mut self, pos: u64, width: usize) -> Result<(), Missing> {
        let top = self.top_of(pos.min(self.len()), width)?;
        self.top = top;
        self.width = width;
        self.left = 0;
        if self.wrap {
            return Ok(());
        }
        let line = self.line(top.line)?;
        let column = line.offsets.iter().take_while(|at| **at < pos).count();
        if column + 10 > width {
            self.left = if width <= 10 {
                column
            } else {
                column + 10 - width
            };
        }
        Ok(())
    }

    /// The cells of a row (`width` of them from `cell`) the selection covers, as a range.
    fn marked_cells(&self, line: &Line, cell: usize, width: usize) -> Option<(usize, usize)> {
        let (first, size) = self.selection?;
        let shown = line.offsets.get(cell..)?.iter().take(width);
        let inside: Vec<usize> = (shown.enumerate())
            .filter(|(_, at)| **at >= first && **at < first + size.max(1))
            .map(|(index, _)| index)
            .collect();
        Some((*inside.first()?, *inside.last()? + 1))
    }

    /// What a search found, selected (FAR's `SelectPos`, `SelectSize`); none: nothing.
    pub fn set_selection(&mut self, selection: Option<(u64, u64)>) {
        self.selection = selection;
    }

    /// A move to a place or a mode (Alt+F8's, a search's, F4's); none for the keys' moves.
    fn place(&mut self, step: Move, width: usize) -> Option<Result<(), Missing>> {
        let text = self.mode == Mode::Text;
        let done = match step {
            Move::Mode(mode) => self.switch(mode, width),
            Move::To(pos) if text => self.top_of(pos.min(self.len()), width).map(|top| {
                self.top = top;
                self.width = width;
            }),
            Move::To(pos) => {
                self.pos = pos.min(self.len());
                Ok(())
            }
            Move::Column(left) => {
                self.left = left.min(self.layout.max_line);
                Ok(())
            }
            Move::Show(pos) if text => self.show_text(pos, width),
            Move::Show(pos) => {
                // FAR's `SelectText` in hex and dump: the row of the byte, the row before when that
                // would start past it.
                let size = self.line_size(width);
                let row = self.pos % size + size * (pos / size);
                self.pos = if row < pos {
                    row
                } else {
                    row.saturating_sub(size)
                };
                Ok(())
            }
            _ => return None,
        };
        Some(done)
    }

    /// Moves as `step` says on a `width` by `height` screen; nothing moves when a block is missing.
    pub fn apply(&mut self, step: Move, width: usize, height: usize) -> Result<(), Missing> {
        let width = width.max(1);
        let height = height.max(1);
        if let Some(done) = (self.place(step, width)).or_else(|| self.byte_step(step, height)) {
            return done;
        }
        if self.mode != Mode::Text {
            return self.apply_bytes(step, width, height);
        }
        let current = self.line(self.top.line)?;
        let mut top = Top {
            line: self.top.line,
            row: self.top_at(&current, width),
        };
        match step {
            Move::Up => top = self.up(top, width)?.unwrap_or(top),
            Move::PageUp => top = self.up_by(top, width, height - 1)?,
            Move::Down | Move::PageDown => {
                let shown = self.screen(width, height);
                if let Some(missing) = shown.missing {
                    return Err(missing);
                }
                if shown.at_end {
                } else if step == Move::PageDown {
                    top = shown.tops.last().copied().unwrap_or(top);
                } else if self.wrap && top.row + 1 < current.rows(width, self.words) {
                    top.row += 1;
                } else {
                    top = Top {
                        line: current.end,
                        row: 0,
                    };
                }
            }
            Move::Home | Move::End => {
                self.left = 0;
                top = if step == Move::Home {
                    self.home()
                } else {
                    self.end(width, height)?
                };
            }
            Move::Start => top = self.home(),
            Move::Finish => top = self.end(width, height)?,
            Move::Left(by) if !self.wrap => self.left = self.left.saturating_sub(by),
            Move::Right(by) if !self.wrap => {
                self.left = (self.left + by).min(self.layout.max_line);
            }
            Move::LineStarts => self.left = 0,
            Move::Left(_)
            | Move::Right(_)
            | Move::Mode(_)
            | Move::Bytes(_)
            | Move::Roll(_)
            | Move::To(_)
            | Move::Column(_)
            | Move::Show(_)
            | Move::Byte(_)
            | Move::ByteColumn
            | Move::TypeByte(_) => {}
        }
        self.top = top;
        self.width = width;
        Ok(())
    }

    fn home(&self) -> Top {
        Top {
            line: self.floor,
            row: 0,
        }
    }

    /// FAR's End: up a screen from the end; a last line without a break takes the last row.
    fn end(&self, width: usize, height: usize) -> Result<Top, Missing> {
        let len = self.len();
        if len <= self.floor {
            return Ok(self.home());
        }
        let encoding = self.encoding.as_ref().ok_or(Missing(0))?;
        let unit = encoding.unit();
        // A last unit cut short (UTF-16 of an odd length) is no break.
        let whole = self.floor + (len - self.floor) / unit * unit;
        let broken = whole == len && {
            let last = self.blocks.before(len)?;
            let value = encoding.unit_value(&last[last.len().saturating_sub(unit as usize)..]);
            value == u32::from(b'\n') || value == u32::from(b'\r')
        };
        let rows = if broken { height - 1 } else { height };
        self.up_by(Top { line: len, row: 0 }, width, rows)
    }

    fn up_by(&self, mut top: Top, width: usize, rows: usize) -> Result<Top, Missing> {
        for _ in 0..rows {
            match self.up(top, width)? {
                Some(above) => top = above,
                None => break,
            }
        }
        Ok(top)
    }

    /// The row above `top`; none at the file's start.
    fn up(&self, top: Top, width: usize) -> Result<Option<Top>, Missing> {
        if top.row > 0 {
            return Ok(Some(Top {
                row: top.row - 1,
                ..top
            }));
        }
        if top.line <= self.floor {
            return Ok(None);
        }
        let encoding = self.encoding.as_ref().ok_or(Missing(0))?;
        let line = line_before(&self.blocks, encoding, top.line, self.floor, self.layout)?;
        let row = if self.wrap {
            self.line(line)?.rows(width, self.words) - 1
        } else {
            0
        };
        Ok(Some(Top { line, row }))
    }
}

#[path = "dump.rs"]
mod dump;
#[path = "dump_edit.rs"]
mod dump_edit;

impl super::LineSource for TextView {
    fn line_at(&self, start: u64) -> Result<Line, Missing> {
        self.line(start)
    }

    fn line_after(&self, line: &Line) -> Result<Option<u64>, Missing> {
        Ok((line.end < self.len() && line.end > line.start).then_some(line.end))
    }

    fn line_above(&self, start: u64) -> Result<Option<u64>, Missing> {
        let encoding = self.encoding.as_ref().ok_or(Missing(0))?;
        if start <= self.floor {
            return Ok(None);
        }
        line_before(&self.blocks, encoding, start, self.floor, self.layout).map(Some)
    }

    fn size(&self) -> u64 {
        self.len()
    }
}

#[cfg(test)]
#[path = "text_view_tests.rs"]
mod tests;
