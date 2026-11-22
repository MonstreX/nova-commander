//! The editor's file (S2d.4b; ADR-0015 §4): the editor's view over the piece table, its blocks read on
//! a thread of their own as the screen and the steps ask for them (the viewer's reader), the steps
//! done as soon as theirs come, in order. The code page and the line break are told from the first
//! block (FAR's `GetFileCodepage`, the file's first break; `\r\n` when it has none).

use crate::editor::view::edit_view::Mark;
use std::cell::Cell;
use std::collections::VecDeque;

use crate::editor::syntax::highlight::Wait;
use crate::editor::view::edit_view::{EditScreen, EditView, Step};
use crate::editor::view::pieces::PieceTable;
use crate::editor::view::{BLOCK, Encoding, Missing, detect};
use crate::jobs::file_reader::FileReader;
use crate::vfs::VfsPath;

/// Steps waiting for their blocks; more keys than this before the disk answers are dropped.
const WAITING: usize = 256;

#[derive(Debug)]
pub struct EditorText {
    /// None for a file made new (nothing to read).
    reader: Option<FileReader>,
    /// The file's length until its first block tells the code page; then the view.
    len: Option<u64>,
    first: Option<Vec<u8>>,
    view: Option<EditView>,
    waiting: VecDeque<Step>,
    pub failed: Option<String>,
    /// The rows' room as last drawn (the drawing writes it).
    pub room: Cell<(u16, u16)>,
    /// The rows' first cell on the screen as last drawn (the drawing writes it), for the mouse.
    pub origin: Cell<(u16, u16)>,
    /// The status line's counts as last read.
    place: Option<(u64, u64, usize, usize)>,
    /// The syntax highlighting (S2d.5d).
    syntax: super::editor_syntax::TextSyntax,
    /// FAR's `AutoDetectCodePage`, `DefaultCodePage` and `TabSize` (the editor's settings).
    detection: (bool, u32),
    tab: usize,
    /// How it edits (the editor's settings).
    options: crate::editor::view::edit_view::EditOptions,
    /// FAR's "Show white space", "Show scrollbar" and "Show line numbers" (Ctrl+F3).
    pub white: bool,
    pub scrollbar: bool,
    pub numbers: bool,
    /// The first row's line number and the lines as last read (0: not counted).
    lines: (u64, u64),
    /// A kept place's code page, read in when the file opens (FAR's cached `CodePage`).
    pub(super) kept_page: Option<u32>,
    /// Kept bookmarks, the view's once the file opens.
    kept_marks: Option<[Option<Mark>; 10]>,
    /// FAR's lock mode (Ctrl+L, a read-only file with "Lock editing of read-only files").
    locked: bool,
    /// The document moved onto the file just saved, its undo kept (S2d.6e): the view's once the file
    /// opens.
    prepared: Option<PieceTable>,
    /// When it was opened: past [`QUICK`](super::held::QUICK) unread, the screen says it reads.
    opened: std::time::Instant,
}

/// The block's text as [`EditorText::block_text_now`] reads it.
#[derive(Debug, PartialEq, Eq)]
pub enum BlockText {
    /// Its text; none without a block.
    Text(Option<String>),
    Reading,
    TooLarge,
}

/// The first break `first` has in `encoding` (the system's when none), as text.
fn first_break(first: &[u8], encoding: &Encoding) -> &'static str {
    let unit = usize::try_from(encoding.unit()).unwrap_or(1);
    let values: Vec<u32> = (first.chunks_exact(unit))
        .map(|bytes| encoding.unit_value(bytes))
        .collect();
    let Some(at) = values.iter().position(|value| matches!(value, 0x0A | 0x0D)) else {
        return crate::platform::LINE_BREAK;
    };
    match (values[at], values.get(at + 1)) {
        (0x0D, Some(0x0A)) => "\r\n",
        (0x0D, _) => "\r",
        _ => "\n",
    }
}

impl EditorText {
    /// The file just saved, its document `doc` (the steps before the save kept, FAR's undo over a
    /// save).
    pub fn open_saved(path: VfsPath, doc: PieceTable) -> Self {
        let mut text = Self::open(path);
        text.prepared = Some(doc);
        text
    }

    /// The file the document was read from, kept open (its save reads the bytes not edited from it).
    pub fn held_file(&self) -> Option<crate::jobs::file_reader::HeldFile> {
        self.reader.as_ref()?.held()
    }

    pub fn open(path: VfsPath) -> Self {
        let name = path.file_name().unwrap_or_default();
        Self {
            syntax: super::editor_syntax::TextSyntax::new(&name),
            detection: (true, 0),
            tab: crate::editor::view::Layout::default().tab,
            options: crate::editor::view::edit_view::EditOptions::default(),
            white: false,
            scrollbar: false,
            numbers: false,
            lines: (0, 0),
            kept_page: None,
            kept_marks: None,
            locked: false,
            prepared: None,
            // Its file kept open for its life: another editor's save of it is never read (review C2).
            reader: Some(FileReader::open_kept(path, BLOCK)),
            len: None,
            first: None,
            view: None,
            waiting: VecDeque::new(),
            failed: None,
            room: Cell::new((0, 0)),
            origin: Cell::new((0, 0)),
            place: None,
            opened: std::time::Instant::now(),
        }
    }

    /// A file not there yet: empty, in the system's ANSI code page (FAR's default for a new file).
    pub fn new_file() -> Self {
        let mut text = Self {
            syntax: super::editor_syntax::TextSyntax::new(""),
            detection: (true, 0),
            tab: crate::editor::view::Layout::default().tab,
            options: crate::editor::view::edit_view::EditOptions::default(),
            white: false,
            scrollbar: false,
            numbers: false,
            lines: (0, 0),
            kept_page: None,
            kept_marks: None,
            locked: false,
            prepared: None,
            reader: None,
            len: Some(0),
            first: Some(Vec::new()),
            view: None,
            waiting: VecDeque::new(),
            failed: None,
            room: Cell::new((0, 0)),
            origin: Cell::new((0, 0)),
            place: None,
            opened: std::time::Instant::now(),
        };
        text.start_view();
        text
    }

    /// The document as it is now, for a save on a worker.
    pub fn document(&self) -> Option<PieceTable> {
        self.view.as_ref().map(|view| view.doc().clone())
    }

    /// The document for a search on a worker: its bytes, not its undo steps (review A3: each search
    /// copied every step's pieces).
    pub fn document_to_read(&self) -> Option<PieceTable> {
        self.view.as_ref().map(|view| view.doc().reading_copy())
    }

    pub fn view(&self) -> Option<&EditView> {
        self.view.as_ref()
    }

    /// A key's step; done as soon as its blocks are read.
    pub fn press(&mut self, step: Step) {
        if self.waiting.len() < WAITING {
            self.waiting.push_back(step);
        }
        self.pump();
    }

    /// The code page shown by its number.
    pub fn page_number(&self) -> Option<u32> {
        self.view
            .as_ref()
            .map(|view| super::quick_view_text::page_number_of(view.encoding()))
    }

    /// Another code page (none: told again from the first block), the file read in it as it is; false
    /// for an edited document (its edits are in the page it has).
    pub fn set_page(&mut self, page: Option<u32>) -> bool {
        if self.is_changed() {
            return false;
        }
        let encoding = match (page, self.first.as_ref()) {
            (Some(page), _) => super::quick_view_text::encoding_of_page(page),
            (None, Some(first)) => super::quick_view_text::encoding_of(detect(first).0),
            (None, None) => return true,
        };
        if let Some(view) = self.view.as_mut() {
            view.set_encoding(encoding);
            self.place = None;
            self.pump();
        }
        true
    }

    /// FAR's lock mode set or let go.
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
        if let Some(view) = self.view.as_mut() {
            view.locked = locked;
        }
    }

    /// Whether the editor is in FAR's lock mode.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// A kept place put back as the file opens (FAR's `FileEditor::LoadFile` with `SaveEditorPos`).
    pub fn restore(&mut self, place: &crate::settings::positions::EditorPosition) {
        if place.page != 0 && self.view.is_none() {
            self.kept_page = Some(place.page);
        }
        self.press(Step::Restore(place.line, place.col, place.top, place.left));
        self.restore_marks(&place.bookmarks);
    }

    /// Kept bookmarks put back (FAR's `m_SavePos` from `bm`).
    pub fn restore_marks(&mut self, kept: &[Option<(u64, usize, usize, usize)>]) {
        let mut marks = [None; 10];
        for (mark, kept) in marks.iter_mut().zip(kept) {
            *mark = kept.map(|(line, col, left, row)| Mark {
                line,
                col,
                left,
                row,
            });
        }
        match self.view.as_mut() {
            Some(view) => view.marks = marks,
            None => self.kept_marks = Some(marks),
        }
    }

    /// The place to keep: the cursor, the screen and the code page (none until the file is open).
    pub fn position(&self) -> Option<crate::settings::positions::EditorPosition> {
        let view = self.view.as_ref()?;
        let ((line, col), (top, left, _)) = (view.cursor(), view.window());
        Some(crate::settings::positions::EditorPosition {
            line,
            col,
            top,
            left,
            page: self.page_number().unwrap_or(0),
            bookmarks: (view.marks.iter())
                .map(|mark| mark.map(|mark| (mark.line, mark.col, mark.left, mark.row)))
                .collect(),
        })
    }

    /// What a search found shown.
    pub fn show_found(&mut self, at: u64, size: u64) {
        self.press(Step::Show(at, size));
    }

    /// `read` of the view once every step asked for is done and the blocks it needs are read; none
    /// until then (the block it wants asked for).
    pub fn query<T>(
        &mut self,
        read: impl FnOnce(&mut EditView) -> Result<T, Missing>,
    ) -> Option<T> {
        if !self.settled() {
            return None;
        }
        let answer = read(self.view.as_mut()?);
        if let Err(Missing(index)) = answer {
            self.want(index);
        }
        answer.ok()
    }

    /// The block's text for the clipboard: read now, being read (its blocks asked for: asked again
    /// once they come), or too large to be read so (a document not kept whole: its first blocks would
    /// go before its last came) - R7 review F10: it did nothing, unsaid.
    pub fn block_text_now(&mut self) -> BlockText {
        // Half the blocks a document not kept whole keeps.
        const LARGEST: u64 = 32 * BLOCK;
        let block = (self.view.as_ref()).and_then(|view| view.block().ok().flatten());
        let large = block.is_some_and(|(from, to)| to - from > LARGEST)
            && self.len.unwrap_or(0) > crate::editor::view::KEPT_WHOLE;
        match self.query(|view| view.block_text()) {
            Some(text) => BlockText::Text(text),
            None if large => BlockText::TooLarge,
            None => BlockText::Reading,
        }
    }

    /// `read` of the view with the highlighting (FarColorer's editor features), as [`Self::query`].
    pub fn colorer<T, E: Into<Wait>>(
        &mut self,
        read: impl FnOnce(&super::editor_syntax::TextSyntax, &mut EditView) -> Result<T, E>,
    ) -> Option<T> {
        if !self.settled() {
            return None;
        }
        // An edit not told to the highlighting yet is told first (review B11: its states past it were
        // stale, the pair found from them wrong).
        let view = self.view.as_mut()?;
        if let Some(changed) = view.take_changed() {
            let floor = view.window().2;
            self.syntax.tick(view, floor, Some(changed));
        }
        // The schemes being read (`Wait::Library`): done again at the next tick.
        let answer = read(&self.syntax, self.view.as_mut()?).map_err(Into::into);
        if let Err(Wait::Block(index)) = answer {
            self.want(index);
        }
        answer.ok()
    }

    /// FAR's `TabSize` of this editor.
    pub fn tab_size(&self) -> usize {
        self.tab
    }

    /// The editor's options as its settings made them.
    pub fn options(&self) -> &crate::editor::view::edit_view::EditOptions {
        &self.options
    }

    /// How long it has read its file with nothing to show yet, past [`QUICK`](super::held::QUICK):
    /// the screen says it reads (R7 review F9: a share that stopped answering left it blank).
    pub fn reading_for(&self) -> Option<std::time::Duration> {
        let elapsed = self.opened.elapsed();
        let unread = self.view.is_none() && self.failed.is_none();
        (unread && elapsed >= super::held::QUICK).then_some(elapsed)
    }

    /// Whether every step asked for is done (an action on the document waits for it).
    pub fn settled(&self) -> bool {
        self.view.is_some() && self.waiting.is_empty()
    }

    /// Whether edits asked for wait for blocks not read yet (the moves do not count).
    pub fn has_steps_waiting(&self) -> bool {
        self.waiting.iter().any(Step::edits)
    }

    /// Whether the document was edited since it was opened or saved.
    pub fn is_changed(&self) -> bool {
        self.view
            .as_ref()
            .is_some_and(|view| view.doc().is_changed())
    }

    /// The status line's counts: the line and the lines, the column and the character (from 1).
    pub fn place(&self) -> Option<(u64, u64, usize, usize)> {
        self.place
    }

    /// The colours of the rows of a `width` by `height` room (none without highlighting).
    pub fn paints(
        &self,
        width: u16,
        height: u16,
    ) -> Vec<Vec<crate::editor::syntax::highlight::Paint>> {
        let Some(view) = self
            .view
            .as_ref()
            .filter(|view| view.hex_cursor().is_none())
        else {
            return Vec::new();
        };
        let rows = view.rows(usize::from(height));
        (self.syntax).paints(view, &rows, usize::from(width))
    }

    /// Whether the document is highlighted.
    #[cfg(test)]
    pub fn highlighted(&self) -> bool {
        self.syntax.is_on()
    }

    /// The document's highlighting (FarColorer's menu).
    pub fn syntax_mut(&mut self) -> &mut super::editor_syntax::TextSyntax {
        &mut self.syntax
    }

    pub fn syntax(&self) -> &super::editor_syntax::TextSyntax {
        &self.syntax
    }

    /// The first row's line number and the lines (0: not counted).
    pub fn lines(&self) -> (u64, u64) {
        self.lines
    }

    /// The columns FAR's line numbers take (`LineNumbersWidth`: the digits of the lines, three at
    /// least, and a space); none without them.
    pub fn number_width(&self) -> u16 {
        if !self.numbers || self.hex_extent(0).is_some() {
            return 0;
        }
        let total = self.lines.1.max(1);
        let digits = total.ilog10() as u16 + 1;
        digits.max(3) + 1
    }

    /// In the hex mode (ADR-0027): the bytes from the first row's to past the last row's of `height`
    /// rows, and the document's bytes (the scrollbar's span); none in the text mode.
    pub fn hex_extent(&self, height: u16) -> Option<(u64, u64, u64)> {
        let view = self.view.as_ref()?;
        let top = view.hex_top()?;
        let size = view.doc().size();
        Some((top, (top + 16 * u64::from(height)).min(size), size.max(1)))
    }

    /// The rows to draw in a `width` by `height` room; none until the file is open.
    pub fn screen(&self, width: u16, height: u16) -> Option<EditScreen> {
        let view = self.view.as_ref()?;
        Some(view.screen(usize::from(width), usize::from(height)))
    }

    /// The view made once the first block tells the code page.
    fn start_view(&mut self) {
        let (Some(len), Some(first)) = (self.len, self.first.as_ref()) else {
            return;
        };
        let (encoding, floor) = super::quick_view_text::told(self.detection, first);
        let from = usize::try_from(floor).unwrap_or(0).min(first.len());
        let eol = first_break(&first[from..], &encoding);
        let mut doc = (self.prepared.take())
            .filter(|doc| doc.size() == len)
            .unwrap_or_else(|| PieceTable::new(len));
        doc.insert_block(0, first.clone());
        // Kept for "Automatic detection" in the code page menu.
        let mut view = EditView::new(doc, (encoding, floor), eol);
        view.set_tab(self.tab);
        view.options = self.options;
        view.set_white(self.white);
        view.locked = self.locked;
        if let Some(marks) = self.kept_marks.take() {
            view.marks = marks;
        }
        if let Some(page) = self.kept_page.take() {
            view.set_encoding(super::quick_view_text::encoding_of_page(page));
        }
        self.view = Some(view);
    }

    /// The editor's settings: its tab stops, its code page's detection, its highlighting.
    pub fn apply_settings(&mut self, settings: &crate::settings::EditorSettings) {
        self.detection = (settings.auto_detect, settings.default_page);
        self.tab = settings.tab();
        self.options = edit_options(settings);
        (self.white, self.scrollbar, self.numbers) = (
            settings.show_white_space,
            settings.show_scrollbar,
            settings.show_line_numbers,
        );
        if let Some(view) = self.view.as_mut() {
            view.set_tab(self.tab);
            view.options = self.options;
            view.set_white(self.white);
        }
        self.syntax.set_enabled(settings.highlighting);
    }

    /// What the reader brought, the steps it lets through, the blocks the screen asks for; true when
    /// the rows are to be drawn again.
    pub fn pump(&mut self) -> bool {
        let taken = self
            .reader
            .as_ref()
            .map(FileReader::take)
            .unwrap_or_default();
        let mut changed = false;
        if let Some(error) = taken.failed {
            self.failed = Some(error);
            changed = true;
        }
        if self.len.is_none() {
            self.len = taken.len;
        }
        for (index, bytes) in taken.blocks {
            match self.view.as_mut() {
                Some(view) => view.doc_mut().insert_block(index, bytes),
                None if index == 0 => self.first = Some(bytes),
                None => {}
            }
            changed = true;
        }
        if self.view.is_none() {
            match self.len {
                Some(0) => self.first = Some(Vec::new()),
                Some(_) if self.first.is_none() => self.want(0),
                _ => {}
            }
            self.start_view();
            if self.view.is_none() {
                return changed;
            }
            changed = true;
        }
        let stepped = self.step_on();
        let painted = match self.view.as_mut() {
            Some(view) => {
                let changed = view.take_changed();
                let floor = view.window().2;
                self.syntax.tick(view, floor, changed)
            }
            None => false,
        };
        changed | stepped | painted
    }

    /// The waiting steps done in order, the screen kept on the cursor, the counts read.
    fn step_on(&mut self) -> bool {
        let Some(view) = self.view.as_mut() else {
            return false;
        };
        let (width, height) = self.room.get();
        let (width, height) = (usize::from(width).max(1), usize::from(height).max(1));
        let mut changed = false;
        let mut wanted: Option<Missing> = None;
        while let Some(step) = self.waiting.front() {
            match view.apply(step, height) {
                Ok(()) => {
                    self.waiting.pop_front();
                    changed = true;
                }
                Err(missing) => {
                    wanted = Some(missing);
                    break;
                }
            }
        }
        // Not drawn yet: no room to keep the cursor in.
        if wanted.is_none() && self.room.get().0 > 0 {
            wanted = view.scroll(width, height).err();
        }
        if wanted.is_none() {
            wanted = view.screen(width, height).missing;
        }
        if wanted.is_none() {
            match view
                .place()
                .and_then(|place| Ok((place, view.top_number()?)))
            {
                Ok((place, top)) => {
                    changed |= self.place != Some(place) || self.lines.0 != top;
                    self.place = Some(place);
                    self.lines = (top, place.1);
                }
                Err(missing) => wanted = Some(missing),
            }
        }
        if let Some(Missing(index)) = wanted {
            self.want(index);
        }
        changed
    }

    /// Block `index` asked for, and the blocks around it: the moves back over long lines and the
    /// screen after a jump take them next (fewer trips to the reader).
    fn want(&self, index: u64) {
        if let Some(reader) = &self.reader {
            reader.want(index);
            for near in [index.checked_sub(1), Some(index + 1), index.checked_sub(2)] {
                if let Some(near) = near.filter(|near| near * BLOCK < self.len.unwrap_or(0)) {
                    reader.want(near);
                }
            }
        }
    }
}

/// How the editor edits, as its settings say (FAR's `EditorOptions`).
fn edit_options(
    settings: &crate::settings::EditorSettings,
) -> crate::editor::view::edit_view::EditOptions {
    crate::editor::view::edit_view::EditOptions {
        persistent_blocks: settings.persistent_blocks,
        del_removes_blocks: settings.del_removes_blocks,
        auto_indent: settings.auto_indent,
        expand_tabs: settings.expand_tabs.min(2),
        cursor_beyond_eol: settings.cursor_beyond_eol,
        select_found: settings.select_found,
        cursor_at_end: settings.cursor_at_end,
        code_editing: settings.code_editing,
    }
}

#[cfg(test)]
#[path = "editor_text_tests.rs"]
mod tests;
