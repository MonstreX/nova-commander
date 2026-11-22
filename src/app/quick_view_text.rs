//! Quick view's file contents (2c step 3b, ADR-0015; FAR's `QuickView` holds a `Viewer` without its
//! status line): the view core lays out the rows, a reader thread brings the blocks it asks for, and
//! the keys' moves wait for theirs. Wrapped (FAR's `IsWrap` default); the code page told from the first
//! block (FAR's default page: ANSI); a file that grows while its end is shown shows what was added.

use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use crate::editor::view::{
    BLOCK, ByteDecoder, Detected, Encoding, Missing, Mode, Move, Screen, TextView, detect,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::jobs::file_reader::FileReader;
use crate::jobs::line_index::Units;
use crate::platform;
use crate::vfs::VfsPath;

/// UTF-8's and UTF-16's numbers as code pages (Windows' `CP_UTF8`, FAR's `CP_UTF16LE` / `CP_UTF16BE`).
const UTF8_PAGE: u32 = 65001;
const UTF16_LE_PAGE: u32 = 1200;
const UTF16_BE_PAGE: u32 = 1201;

/// Moves waiting for their blocks; more keys than this before the disk answers are dropped.
const WAITING: usize = 64;

#[derive(Debug)]
pub struct FileText {
    reader: FileReader,
    /// None until the file's length is known.
    view: Option<TextView>,
    waiting: VecDeque<Move>,
    /// Why the file cannot be shown.
    pub failed: Option<String>,
    /// The rows' room as last drawn: width and height (the drawing writes it).
    pub room: Cell<(u16, u16)>,
    /// The room the screen was last laid out in, and whether the file's end was on it.
    laid: Option<(u16, u16)>,
    at_end: bool,
    /// The first row's first byte as last laid out, and the first past its last row.
    position: u64,
    end: u64,
    /// The code page was told from a short first block of ASCII only: told again as the file grows (a
    /// log started empty may turn out UTF-8).
    tentative: bool,
    /// The file grew since: the code page is told again once its first block is read anew.
    retell: bool,
    /// The tab stops and the longest line (the viewer's settings).
    layout: crate::editor::view::Layout,
    /// FAR's `AutoDetectCodePage` and `DefaultCodePage` (0: ANSI).
    detection: (bool, u32),
    /// FAR's `m_Wrap` and `m_WordWrap`, kept for the view to come.
    wrap: (bool, bool),
    /// When it was opened: past [`QUICK`](super::held::QUICK) unread, the screen says it reads.
    opened: std::time::Instant,
}

impl FileText {
    pub fn open(path: VfsPath) -> Self {
        Self {
            reader: FileReader::open(path, BLOCK),
            view: None,
            waiting: VecDeque::new(),
            failed: None,
            room: Cell::new((0, 0)),
            laid: None,
            at_end: false,
            position: 0,
            end: 0,
            tentative: false,
            retell: false,
            layout: crate::editor::view::Layout::default(),
            detection: (true, 0),
            wrap: (true, false),
            opened: std::time::Instant::now(),
        }
    }

    /// How long it has read its file with nothing to show yet, past [`QUICK`](super::held::QUICK):
    /// the screen says it reads (R7 review F9: a share that stopped answering left it blank).
    pub fn reading_for(&self) -> Option<std::time::Duration> {
        let elapsed = self.opened.elapsed();
        let unread = self.view.is_none() && self.failed.is_none();
        (unread && elapsed >= super::held::QUICK).then_some(elapsed)
    }

    /// FAR's code page detection: by the bytes or by the mark alone, and the page when nothing tells.
    pub fn set_detection(&mut self, heuristics: bool, default_page: u32) {
        self.detection = (heuristics, default_page);
    }

    /// A view of `len` bytes laid out as the settings say.
    fn new_view(&self, len: u64) -> TextView {
        let mut view = TextView::new(len);
        view.set_layout(self.layout);
        (view.wrap, view.words) = self.wrap;
        view
    }

    /// The viewer's tab stops and longest line.
    pub fn set_layout(&mut self, layout: crate::editor::view::Layout) {
        self.layout = layout;
        if let Some(view) = self.view.as_mut() {
            view.set_layout(layout);
        }
    }

    /// The view, once the code page is told.
    pub fn view(&self) -> Option<&TextView> {
        self.view.as_ref().filter(|view| view.encoding().is_some())
    }

    /// The view to change in place (the byte editing on and off), once the code page is told.
    pub fn view_mut(&mut self) -> Option<&mut TextView> {
        self.view.as_mut().filter(|view| view.encoding().is_some())
    }

    /// The rows to draw in a `width` by `height` room; none until the file is open.
    pub fn screen(&self, width: u16, height: u16) -> Option<Screen> {
        let view = self
            .view
            .as_ref()
            .filter(|view| view.encoding().is_some())?;
        Some(view.screen(usize::from(width), usize::from(height)))
    }

    /// The label of F8: the code page it would switch to (FAR's `F8CP::NextCPname`).
    pub fn next_code_page(&self) -> &'static str {
        match self.page() {
            Some(page) if page == platform::locale::ansi_code_page() => "OEM",
            _ => "ANSI",
        }
    }

    /// The system code page shown, when it is one.
    fn page(&self) -> Option<u32> {
        match self.view.as_ref().and_then(TextView::encoding) {
            Some(Encoding::Page(page, _)) => Some(*page),
            _ => None,
        }
    }

    /// FAR's F8 (`F8CP::NextCP` with its default order: ANSI, OEM): OEM after ANSI, ANSI after any
    /// other; the screen from its line's start.
    pub fn switch_code_page(&mut self) {
        let ansi = platform::locale::ansi_code_page();
        let next = match self.page() {
            Some(page) if page == ansi => platform::locale::oem_code_page(),
            _ => ansi,
        };
        let Some(view) = self.view.as_mut().filter(|view| view.encoding().is_some()) else {
            return;
        };
        view.change_encoding(system_page(next));
        // Told by hand: never told again as the file grows.
        self.tentative = false;
        self.laid = None;
        self.pump();
    }

    /// The code page shown by its number (UTF-8 65001, UTF-16 1200 and 1201); none until it is told.
    pub fn page_number(&self) -> Option<u32> {
        self.view
            .as_ref()
            .and_then(TextView::encoding)
            .map(page_number_of)
    }

    /// The code page chosen in Shift+F8's menu by its number (FAR's `SelectCodePage`); the screen from
    /// its line's start.
    pub fn set_code_page(&mut self, page: u32) {
        let encoding = encoding_of_page(page);
        let Some(view) = self.view.as_mut().filter(|view| view.encoding().is_some()) else {
            return;
        };
        view.change_encoding(encoding);
        self.tentative = false;
        self.laid = None;
        self.pump();
    }

    /// Shift+F8's "Automatic detection" (FAR's `CP_DEFAULT`): the code page told again from the first
    /// block.
    pub fn detect_again(&mut self) {
        if self.view.is_some() {
            self.retell = true;
            self.laid = None;
            self.pump();
        }
    }

    /// The code page's name as FAR's status line shows it (`ShortReadableCodepageName`).
    pub fn code_page_name(&self) -> String {
        (self.view.as_ref().and_then(TextView::encoding)).map_or_else(String::new, page_name)
    }

    /// The code page shown, for a search; none until it is told.
    pub fn encoding(&self) -> Option<Encoding> {
        self.view.as_ref()?.encoding().cloned()
    }

    /// What a search found selected (none: nothing).
    pub fn select(&mut self, selection: Option<(u64, u64)>) {
        if let Some(view) = self.view.as_mut() {
            view.set_selection(selection);
            self.laid = None;
            self.pump();
        }
    }

    /// The first byte of the screen as last laid out and the first past it (FAR's `BegOfScreen`,
    /// `EndOfScreen`).
    pub fn span(&self) -> (u64, u64) {
        (self.position, self.end)
    }

    /// How the line index finds the breaks, and where line 1 starts; none until the code page is told.
    pub fn line_units(&self) -> Option<(Units, u64)> {
        let view = self.view.as_ref()?;
        let units = match view.encoding()? {
            Encoding::Utf16Le => Units::Utf16Le,
            Encoding::Utf16Be => Units::Utf16Be,
            _ => Units::Bytes,
        };
        Some((units, view.floor()))
    }

    /// How the file is shown (text until it is open).
    pub fn mode(&self) -> Mode {
        self.view.as_ref().map_or(Mode::Text, TextView::mode)
    }

    /// Whether long lines are wrapped (FAR's `m_Wrap`).
    pub fn wrapped(&self) -> bool {
        self.wrap.0
    }

    /// Whether long lines are wrapped at the words (FAR's `m_WordWrap`).
    pub fn words(&self) -> bool {
        self.wrap.1
    }

    /// FAR's Shift+F2: wrapped at the words or by the width, wrapped either way.
    pub fn set_words(&mut self, words: bool) {
        self.wrap = (true, words);
        if let Some(view) = self.view.as_mut() {
            view.set_words(words);
            self.laid = None;
            self.pump();
        }
    }

    /// FAR's F2 in text mode.
    pub fn set_wrap(&mut self, wrap: bool) {
        self.wrap.0 = wrap;
        if let Some(view) = self.view.as_mut() {
            view.set_wrap(wrap);
            self.laid = None;
            self.pump();
        }
    }

    /// What the status line tells: the file's length, the first row's byte, the first column, and
    /// whether its end is on screen; none until the file is open.
    pub fn facts(&self) -> Option<Facts> {
        let view = self.view.as_ref()?;
        Some(Facts {
            len: view.len(),
            position: self.position,
            left: view.left(),
            at_end: self.at_end,
            mode: view.mode(),
        })
    }

    /// A key's move; done as soon as its blocks are read.
    pub fn press(&mut self, step: Move) {
        if self.waiting.len() < WAITING {
            self.waiting.push_back(step);
        }
        self.pump();
    }

    /// What the reader brought, the moves it lets through and the blocks the screen asks for next;
    /// true when the rows are to be drawn again.
    pub fn pump(&mut self) -> bool {
        let taken = self.reader.take();
        let mut changed = false;
        if let Some(error) = taken.failed {
            self.failed = Some(error);
            changed = true;
        }
        if taken.reopened {
            // Another file at the path: read anew from its length on, its end followed if it was.
            self.view = None;
            self.waiting.clear();
            self.tentative = false;
            self.retell = false;
            if self.at_end {
                self.waiting.push_back(Move::End);
            }
            changed = true;
        }
        if let Some(len) = taken.len {
            match &mut self.view {
                None => self.view = Some(self.new_view(len)),
                Some(view) => {
                    view.set_len(len);
                    self.retell = self.tentative;
                    if self.at_end && self.waiting.is_empty() {
                        self.waiting.push_back(Move::End);
                    }
                }
            }
            changed = true;
        }
        let Some(view) = &mut self.view else {
            return changed;
        };
        for (index, bytes) in taken.blocks {
            view.insert(index, bytes);
            changed = true;
        }
        if self.retell || view.encoding().is_none() {
            // The whole first block (all of a shorter file), not what was read before it grew.
            let whole = view.len().min(BLOCK);
            match view.first_block() {
                Ok(first) if first.len() as u64 == whole => {
                    self.tentative = whole < BLOCK && first.is_ascii();
                    self.retell = false;
                    let (encoding, floor) = told(self.detection, first);
                    view.set_encoding(encoding, floor);
                    changed = true;
                }
                Ok(_) => self.reader.want(0),
                Err(Missing(index)) => self.reader.want(index),
            }
            if view.encoding().is_none() {
                return changed;
            }
        }
        let (width, height) = self.room.get();
        let (width, height) = (usize::from(width), usize::from(height));
        while let Some(&step) = self.waiting.front() {
            match view.apply(step, width, height) {
                Ok(()) => {
                    self.waiting.pop_front();
                    changed = true;
                }
                Err(Missing(index)) => {
                    self.reader.want(index);
                    break;
                }
            }
        }
        if changed || self.laid != Some(self.room.get()) {
            let screen = view.screen(width, height);
            if let Some(Missing(index)) = screen.missing {
                self.reader.want(index);
            }
            self.at_end = screen.at_end;
            self.position = screen.position;
            self.end = screen.end;
            self.laid = Some(self.room.get());
        }
        changed
    }
}

/// FAR's viewer keys of a view in a panel (`Viewer::ProcessKey`; Ctrl+Left / Right and Ctrl+Up / Down
/// stay the panels', as FAR's `FilePanels` takes them first).
pub(super) fn viewer_move(key: KeyEvent) -> Option<Move> {
    let ctrl = key.modifiers == KeyModifiers::CONTROL;
    let plain = key.modifiers.is_empty();
    let step = match key.code {
        KeyCode::Up if plain => Move::Up,
        KeyCode::Down if plain => Move::Down,
        KeyCode::PageUp if plain => Move::PageUp,
        KeyCode::PageDown if plain => Move::PageDown,
        KeyCode::Home | KeyCode::PageUp if plain || ctrl => Move::Home,
        KeyCode::End | KeyCode::PageDown if plain || ctrl => Move::End,
        KeyCode::Left if plain => Move::Left(1),
        KeyCode::Right if plain => Move::Right(1),
        KeyCode::Left if key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT => {
            Move::LineStarts
        }
        _ => return None,
    };
    Some(step)
}

/// The code page of a first block (FAR's `GetFileCodepage`): a mark always; UTF-8 and UTF-16 by
/// their bytes with the heuristics; else the default page.
pub(super) fn told(detection: (bool, u32), first: &[u8]) -> (Encoding, u64) {
    let (detected, floor) = detect(first);
    let (heuristics, default_page) = detection;
    if floor > 0 || heuristics && detected != Detected::System {
        return (encoding_of(detected), floor);
    }
    let encoding = match default_page {
        0 => encoding_of(Detected::System),
        page => encoding_of_page(page),
    };
    (encoding, 0)
}

/// A code page's decoding by its number (UTF-8 65001, UTF-16 1200 and 1201, else the system's).
pub(super) fn encoding_of_page(page: u32) -> Encoding {
    match page {
        UTF8_PAGE => Encoding::Utf8,
        UTF16_LE_PAGE => Encoding::Utf16Le,
        UTF16_BE_PAGE => Encoding::Utf16Be,
        page => system_page(page),
    }
}

/// A code page's number (UTF-8 65001, UTF-16 1200 and 1201).
pub(super) fn page_number_of(encoding: &Encoding) -> u32 {
    match encoding {
        Encoding::Utf8 => UTF8_PAGE,
        Encoding::Utf16Le => UTF16_LE_PAGE,
        Encoding::Utf16Be => UTF16_BE_PAGE,
        Encoding::Page(page, _) => *page,
    }
}

/// A code page's name as FAR's status lines show it (`ShortReadableCodepageName`).
pub(super) fn page_name(encoding: &Encoding) -> String {
    match encoding {
        Encoding::Utf8 => "UTF-8".to_string(),
        Encoding::Utf16Le => "U16LE".to_string(),
        Encoding::Utf16Be => "U16BE".to_string(),
        Encoding::Page(page, _) if *page == platform::locale::ansi_code_page() => {
            "ANSI".to_string()
        }
        Encoding::Page(page, _) if *page == platform::locale::oem_code_page() => "OEM".to_string(),
        Encoding::Page(page, _) => page.to_string(),
    }
}

/// What a file's status line tells (FAR's `FileViewer::ShowStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    pub len: u64,
    pub position: u64,
    pub left: usize,
    pub at_end: bool,
    pub mode: Mode,
}

/// The code page the first block told; the system's ANSI page when it told nothing.
pub(super) fn encoding_of(detected: Detected) -> Encoding {
    match detected {
        Detected::Utf8 => Encoding::Utf8,
        Detected::Utf16Le => Encoding::Utf16Le,
        Detected::Utf16Be => Encoding::Utf16Be,
        Detected::System => system_page(platform::locale::ansi_code_page()),
    }
}

/// A system code page's decoder, its table made once for each page.
fn system_page(page: u32) -> Encoding {
    static PAGES: OnceLock<Mutex<Vec<Arc<SystemPage>>>> = OnceLock::new();
    let pages = PAGES.get_or_init(Mutex::default);
    let mut pages = pages.lock().unwrap_or_else(PoisonError::into_inner);
    let known = pages.iter().find(|known| known.page == page).cloned();
    let decoder = known.unwrap_or_else(|| {
        let made = Arc::new(SystemPage::new(page));
        pages.push(Arc::clone(&made));
        made
    });
    Encoding::Page(page, decoder as Arc<dyn ByteDecoder>)
}

/// A system code page: its single bytes in a table, a lead byte decoded with the byte after it.
#[derive(Debug)]
struct SystemPage {
    page: u32,
    singles: Vec<Option<char>>,
}

impl SystemPage {
    fn new(page: u32) -> Self {
        let singles = (0..=u8::MAX)
            .map(|byte| {
                let (text, took) = platform::locale::decode_page(page, &[byte], false);
                let mut chars = text.chars();
                match (took, chars.next(), chars.next()) {
                    (1, Some(ch), None) => Some(ch),
                    _ => None,
                }
            })
            .collect();
        Self { page, singles }
    }
}

impl ByteDecoder for SystemPage {
    fn encode(&self, text: &str) -> Vec<u8> {
        platform::locale::encode_page(self.page, text)
    }

    fn is_lead(&self, byte: u8) -> bool {
        self.singles[usize::from(byte)].is_none()
    }

    fn decode(&self, bytes: &[u8]) -> (char, usize) {
        if let Some(ch) = self.singles[usize::from(bytes[0])] {
            return (ch, 1);
        }
        if bytes.len() < 2 {
            return ('?', 2);
        }
        // The double-byte pages' trail bytes are 0x40 and up (932, 936, 949, 950): a line break or a
        // digit after a lead byte is its own character, the lead alone a fault.
        if bytes[1] < 0x40 {
            return ('?', 1);
        }
        // A pair that is not one character: the lead alone.
        let (text, _) = platform::locale::decode_page(self.page, &bytes[..2], true);
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(ch), None) => (ch, 2),
            (first, _) => (first.unwrap_or('?'), 1),
        }
    }
}

#[cfg(test)]
#[path = "quick_view_text_tests.rs"]
mod tests;
