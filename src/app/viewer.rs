//! The viewer (F3, stage 2d step 1a; FAR's `FileViewer` holding a `Viewer`): a file of any size shown
//! on the view core (ADR-0015) - its blocks read on a thread of their own as the screen asks, the keys'
//! moves done as soon as theirs come, a file that grows followed while its end is shown. FAR's status
//! line at the top (`FileViewer::ShowStatus`), its key bar (`MViewF1`...) with F2's, F4's and F8's
//! labels as the mode is, its keys (`Viewer::ProcessKey`) of text, hex and dump (S2d.1b: F4, F2 in hex,
//! Shift+F4's "View mode"), the wheel.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

use super::quick_view_text::{Facts, FileText};
use crate::app::text_cut::{cut_right, truncate_path};
use crate::editor::view::{Mode, Move};
use crate::jobs::line_index::{LineIndex, Units};
use crate::vfs::VfsPath;

/// FAR's Ctrl+Left / Ctrl+Right in text mode: twenty columns.
const COLUMNS: usize = 20;
/// Moves a wheel's notches may queue at most (more wait for the next).
const MOST_ROWS: usize = 64;

/// The full-screen view of a file (F3).
#[derive(Debug)]
pub struct Viewer {
    pub path: PathBuf,
    text: FileText,
    /// FAR's `NamesList`: the files Gray + / Gray - go through (Find file's found files).
    pub list: Vec<PathBuf>,
    /// FAR's `m_DumpTextMode`: F4 from hex goes to dump, F2 there to text (set by Shift+F4's menu).
    dump_text: bool,
    /// The code page menu stands behind a dialog of its (F4's rename), its cursor on this code page.
    pub menu_behind: Option<u32>,
    /// The line index for Alt+F8's lines, started at the first and kept for the code page it counts.
    index: Option<(std::sync::Arc<LineIndex>, (Units, u64))>,
    /// "Go to"'s "Hex value" as last left (FAR's `m_GotoHex`; none: hex mode's at first).
    goto_hex: Option<bool>,
    /// What the last search found and its way.
    searched: search::Searched,
    /// The text mode's syntax highlighting (S2d.5e, ADR-0024; FAR's viewer has none).
    syntax: super::editor_syntax::TextSyntax,
    /// FAR's `ShowArrows` and `ShowScrollbar` (S2d.6b).
    pub arrows: bool,
    pub scrollbar: bool,
    /// The settings it was given, Alt+Shift+F9's local copy.
    pub settings: crate::settings::ViewerSettings,
    /// Whether the file was looked at for FAR's `DetectDumpMode` (once, when it opens).
    dump_checked: bool,
    /// A plugin's file's copy: no byte editing, which would stay in the copy (R7 review C6).
    pub copy: bool,
    /// Its place kept from before, put back once it is open (FAR's `ViewerPosCache`).
    restore: Option<crate::settings::positions::ViewerPosition>,
    /// The place came from the editor (F6): put back whatever the settings (FAR's `SetFilePos`).
    from_editor: bool,
    /// The mode or the wrapping changed by the user (FAR's `m_mode_changed`).
    touched: bool,
    /// FAR's `BMSavePos`: Ctrl+Shift+0..9's places, the first byte shown and the first column.
    bookmarks: [Option<(u64, usize)>; 10],
    /// FAR's `UndoData`: the places keys left, for Alt+BS and Ctrl+Z.
    back: Vec<(u64, usize)>,
}

pub enum ViewerAction {
    None,
    Close,
    /// Gray + / Gray -: the next or previous file of the list.
    Open(PathBuf),
    /// F6: the file in the editor (FAR's `FileViewer` F6).
    Edit(PathBuf),
    /// Shift+F4: FAR's "View mode" menu, its cursor on the mode shown.
    ModeMenu(Mode),
    /// Shift+F8: FAR's code page menu, its cursor on the code page shown.
    CodePageMenu(Option<u32>),
    /// Alt+F8: FAR's "Go to".
    GoTo,
    /// F7: FAR's "Search".
    Search,
    /// Shift+F7 / Space (forward), Alt+F7 (`true`: back): the search goes on.
    SearchOn(bool),
    /// Ctrl+S: the scroll bar shown or not (FAR's `ShowScrollbar`, the setting too).
    Scrollbar,
    /// Alt+Shift+F9: FAR's "Viewer" settings for this viewer alone (`LocalViewerConfig`).
    Settings,
    /// Ctrl+F10: the panel onto the file (FAR's `GoToFile`), the viewer left open.
    GoToFile,
    /// F2 / Shift+F2 changed the wrapping: FAR's `ViewerIsWrap` and `ViewerWrap` follow it.
    Wrapped {
        wrap: bool,
        words: bool,
    },
    /// F2 in the byte editing: the bytes typed written; Esc with bytes typed: the question.
    SaveBytes,
    AskBytes,
}

/// The modes in the order of Shift+F4's menu (FAR's `VIEWER_MODE_TYPE`).
pub const MODES: [Mode; 3] = [Mode::Text, Mode::Hex, Mode::Dump];

#[path = "viewer_bytes.rs"]
mod bytes;
#[path = "viewer_places.rs"]
mod places;

impl Viewer {
    /// `path` opened: shown once its first block is read.
    pub fn open(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            text: FileText::open(VfsPath::local(path)),
            list: Vec::new(),
            dump_text: false,
            menu_behind: None,
            index: None,
            goto_hex: None,
            searched: search::Searched::default(),
            arrows: true,
            scrollbar: false,
            settings: crate::settings::ViewerSettings::default(),
            dump_checked: false,
            copy: false,
            restore: None,
            from_editor: false,
            touched: false,
            bookmarks: [None; 10],
            back: Vec::new(),
            syntax: super::editor_syntax::TextSyntax::new(
                &path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
        }
    }

    /// The viewer's settings: its tab stops, its longest line, its highlighting (kept as its own copy
    /// for Alt+Shift+F9).
    pub fn apply_settings(&mut self, settings: &crate::settings::ViewerSettings) {
        self.settings = settings.clone();
        let layout = crate::editor::view::Layout {
            tab: settings.tab(),
            max_line: settings.max_line(),
            zero: settings.visible_nul.then_some('·'),
            white: false,
        };
        self.text.set_layout(layout);
        self.text
            .set_detection(settings.auto_detect, settings.default_page);
        self.syntax.set_enabled(settings.highlighting);
        self.arrows = settings.show_arrows;
        self.scrollbar = settings.show_scrollbar;
        if self.text.words() != settings.word_wrap {
            self.text.set_words(settings.word_wrap);
        }
        if self.text.wrapped() != settings.wrap {
            self.text.set_wrap(settings.wrap);
        }
    }

    /// What FAR's `«` and `»` stand for on each row of a `width` by `height` room (text mode).
    pub fn overflow(&self, width: u16, height: u16) -> Vec<(bool, bool)> {
        let Some(view) = self.text.view().filter(|view| view.mode() == Mode::Text) else {
            return Vec::new();
        };
        let screen = view.screen(usize::from(width), usize::from(height));
        view.overflow(&screen, usize::from(width))
    }

    /// The scroll bar's part: the first byte shown, the first past the screen, the file's bytes.
    pub fn extent(&self, width: u16, height: u16) -> Option<(u64, u64, u64)> {
        let view = self.text.view()?;
        let screen = view.screen(usize::from(width), usize::from(height));
        Some((screen.position, screen.end, view.len()))
    }

    /// The width the text takes in a room `width` wide: one less with the scroll bar.
    pub fn text_width(&self, width: u16) -> u16 {
        if self.scrollbar {
            width.saturating_sub(1)
        } else {
            width
        }
    }

    /// The file's text as the screen draws it.
    pub fn text(&self) -> &FileText {
        &self.text
    }

    /// What the reader brought, the highlighting's tick; true when the screen is to be drawn again.
    pub fn pump(&mut self) -> bool {
        let changed = self.text.pump() | self.put_back() | self.detect_dump();
        let painted = match self.text.view().filter(|view| view.mode() == Mode::Text) {
            Some(view) => self.syntax.tick(view, view.floor(), None),
            None => false,
        };
        changed | painted
    }

    /// The text mode's colours of the rows of a `width` by `height` room (none in hex and dump).
    pub fn paints(
        &self,
        width: u16,
        height: u16,
    ) -> Vec<Vec<crate::editor::syntax::highlight::Paint>> {
        let Some(view) = self.text.view().filter(|view| view.mode() == Mode::Text) else {
            return Vec::new();
        };
        let screen = view.screen(usize::from(width), usize::from(height));
        let rows = view.rows(&screen, usize::from(width));
        let mut paints = self.syntax.paints(view, &rows, usize::from(width));
        // A row wrapped at the words shows less than the width: no colour past its text.
        for (row, shown) in paints.iter_mut().zip(&screen.rows) {
            let len = shown.chars().count();
            row.retain_mut(|paint| {
                paint.to = paint.to.min(len);
                paint.from < paint.to
            });
        }
        paints
    }

    /// The file `step` places from this one in the list (FAR's `NamesList::GetNextName`).
    fn neighbour(&self, step: isize) -> Option<PathBuf> {
        let at = (self.list.iter()).position(|path| *path == self.path)?;
        let next = at.checked_add_signed(step)?;
        self.list.get(next).cloned()
    }

    /// What comes before a key's own work: the found text dropped without FAR's `PersistentBlocks`,
    /// Gray + / Gray -'s file of the list.
    fn before_key(&mut self, key: KeyEvent) -> Option<ViewerAction> {
        if !self.settings.persistent_selection && !places::keeps_selection(key) {
            self.text.select(None);
        }
        if self.bookmark(key) {
            return Some(ViewerAction::None);
        }
        if let KeyCode::Char(ch @ ('+' | '-')) = key.code
            && key.state.contains(KeyEventState::KEYPAD)
        {
            let step = if ch == '+' { 1 } else { -1 };
            return Some((self.neighbour(step)).map_or(ViewerAction::None, ViewerAction::Open));
        }
        None
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ViewerAction {
        if let Some(action) = (self.byte_key(key)).or_else(|| self.before_key(key)) {
            return action;
        }
        let plain = key.modifiers.is_empty();
        let mode = self.text.mode();
        match key.code {
            KeyCode::Esc | KeyCode::F(3) | KeyCode::F(10) if plain => return ViewerAction::Close,
            KeyCode::F(6) if plain => return ViewerAction::Edit(self.path.clone()),
            KeyCode::F(10) if key.modifiers == KeyModifiers::CONTROL => {
                return ViewerAction::GoToFile;
            }
            KeyCode::F(4) if key.modifiers == KeyModifiers::SHIFT => {
                return ViewerAction::ModeMenu(mode);
            }
            KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
                return ViewerAction::Scrollbar;
            }
            KeyCode::F(9) if key.modifiers == KeyModifiers::ALT | KeyModifiers::SHIFT => {
                return ViewerAction::Settings;
            }
            KeyCode::F(8) if key.modifiers == KeyModifiers::SHIFT => {
                return ViewerAction::CodePageMenu(self.text.page_number());
            }
            KeyCode::F(8) if key.modifiers == KeyModifiers::ALT => return ViewerAction::GoTo,
            KeyCode::F(7) if plain => return ViewerAction::Search,
            KeyCode::F(7) if key.modifiers == KeyModifiers::SHIFT => {
                return ViewerAction::SearchOn(false);
            }
            KeyCode::Char(' ') if plain => return ViewerAction::SearchOn(false),
            KeyCode::F(7) if key.modifiers == KeyModifiers::ALT => {
                return ViewerAction::SearchOn(true);
            }
            KeyCode::F(2) if plain && mode == Mode::Text => {
                self.touched = true;
                self.text.set_wrap(!self.text.wrapped());
                return self.wrapped();
            }
            KeyCode::F(2) if key.modifiers == KeyModifiers::SHIFT => {
                self.touched = true;
                self.text.set_words(!self.text.words());
                return self.wrapped();
            }
            KeyCode::F(2) if plain => {
                self.touched = true;
                self.text.press(Move::Mode(self.f2_mode()));
            }
            KeyCode::F(4) if plain => {
                self.touched = true;
                self.text.press(Move::Mode(self.f4_mode()));
            }
            KeyCode::F(8) if plain => self.text.switch_code_page(),
            _ => {
                let step = match mode {
                    Mode::Text => text_move(key),
                    _ => bytes_move(key),
                };
                if let Some(step) = step {
                    self.text.press(step);
                }
            }
        }
        ViewerAction::None
    }

    /// FAR's F2 in hex and dump: text from dump (or from hex after Shift+F4's dump), dump from hex.
    fn f2_mode(&self) -> Mode {
        match self.text.mode() == Mode::Dump || self.dump_text {
            true => Mode::Text,
            false => Mode::Dump,
        }
    }

    /// FAR's F4: hex, or from hex back to text (dump after Shift+F4's dump).
    fn f4_mode(&self) -> Mode {
        match (self.text.mode(), self.dump_text) {
            (Mode::Hex, true) => Mode::Dump,
            (Mode::Hex, false) => Mode::Text,
            _ => Mode::Hex,
        }
    }

    /// Shift+F8's code page (FAR's `SelectCodePage` chosen).
    pub fn set_code_page(&mut self, page: u32) {
        self.text.set_code_page(page);
        self.syntax.restart();
    }

    /// Shift+F8's "Automatic detection": the code page told again (FAR's `CP_DEFAULT`).
    pub fn detect_again(&mut self) {
        self.text.detect_again();
        self.syntax.restart();
    }

    /// Shift+F4's choice (FAR: text or dump chosen is what F4 goes back to from hex).
    pub fn choose_mode(&mut self, mode: Mode) {
        self.touched = true;
        self.set_mode(mode);
    }

    /// The mode shown (text or dump is what F4 goes back to from hex).
    fn set_mode(&mut self, mode: Mode) {
        if mode == self.text.mode() {
            return;
        }
        if mode != Mode::Hex {
            self.dump_text = mode == Mode::Dump;
        }
        self.text.press(Move::Mode(mode));
    }

    /// The wheel's notches (away from the user positive) by `lines` each (FAR's `KEY_MSWHEEL_UP`
    /// repeating Up); the tilt wheel's to the right by `chars` each.
    pub fn wheel(&mut self, notches: i32, lines: u32, horizontal: bool) {
        let count = usize::try_from(notches.unsigned_abs().saturating_mul(lines)).unwrap_or(0);
        let step = match (horizontal, notches > 0) {
            (false, true) => Move::Up,
            (false, false) => Move::Down,
            (true, true) => Move::Right(1),
            (true, false) => Move::Left(1),
        };
        for _ in 0..count.min(MOST_ROWS) {
            self.text.press(step);
        }
    }

    /// FAR's status line (`FileViewer::ShowStatus`) `width` wide: the file's name cut to the room left
    /// of `│t│cp│size│Col left│pct%`; `clock` columns kept free on its right.
    pub fn status(&self, width: usize, clock: usize) -> String {
        let facts = self.text.facts().unwrap_or(Facts {
            len: 0,
            position: 0,
            left: 0,
            at_end: false,
            mode: Mode::Text,
        });
        status_line(
            &self.path.display().to_string(),
            (&self.text.code_page_name(), facts),
            width,
            clock,
        )
    }

    /// The wrapping as it is now, for the settings (this viewer's and the program's).
    fn wrapped(&mut self) -> ViewerAction {
        let (wrap, words) = (self.text.wrapped(), self.text.words());
        (self.settings.wrap, self.settings.word_wrap) = (wrap, words);
        ViewerAction::Wrapped { wrap, words }
    }

    /// Shift+F2's label: the wrapping it would switch to (FAR's `shiftf2_label`).
    pub fn shift_f2_label(&self) -> &'static str {
        if self.text.words() { "Wrap" } else { "WWrap" }
    }

    /// F2's, F4's and F8's labels (FAR's `UpdateViewKeyBar`: what each would do; F8 the code page it
    /// would switch to).
    pub fn labels(&self) -> [&'static str; 3] {
        let name = |mode| match mode {
            Mode::Text => "Text",
            Mode::Hex => "Hex",
            Mode::Dump => "Dump",
        };
        let f2 = match self.text.mode() {
            Mode::Text if self.text.wrapped() => "Unwrap",
            Mode::Text if self.text.words() => "WWrap",
            Mode::Text => "Wrap",
            // ADR-0027: F2 in hex edits the bytes (FAR's goes to dump).
            Mode::Hex => self.f2_hex_label(),
            _ => name(self.f2_mode()),
        };
        [f2, name(self.f4_mode()), self.text.next_code_page()]
    }
}

impl super::App {
    /// Shift+F4: FAR's "View mode" menu (`MViewMode`: Text, Hex, Dump; `VMENU_WRAPMODE`,
    /// `VMENU_AUTOHIGHLIGHT`), its cursor on the mode shown, at most the screen's rows less four.
    pub(super) fn show_viewer_mode_menu(&mut self, mode: Mode) {
        use crate::plugin_host::windows::{MenuLine, MenuRequest};
        let lines = ["Text", "Hex", "Dump"].map(|text| MenuLine {
            text: text.to_string(),
            ..MenuLine::default()
        });
        let request = MenuRequest {
            title: "View mode".to_string(),
            lines: lines.to_vec(),
            selected: MODES.iter().position(|it| *it == mode).unwrap_or(0),
            auto_hotkeys: true,
            wrap: true,
            max_height: Some(usize::from(self.height).saturating_sub(4)),
            ..MenuRequest::default()
        };
        let mut menu = super::PluginMenu::new(request);
        menu.viewer_mode = true;
        self.overlay = Some(super::Overlay::PluginMenu(Box::new(menu)));
    }

    /// A key in "View mode": the mode chosen goes to the viewer.
    pub(super) fn viewer_mode_key(&mut self, mut menu: Box<super::PluginMenu>, key: KeyEvent) {
        use super::plugins::MenuOutcome;
        match menu.handle_key(key) {
            MenuOutcome::Stay => self.overlay = Some(super::Overlay::PluginMenu(menu)),
            MenuOutcome::Esc => {}
            MenuOutcome::Chosen { item, .. } => {
                if let (Some(mode), super::Screen::Viewer(viewer)) =
                    (item.and_then(|item| MODES.get(item)), &mut self.screen)
                {
                    viewer.choose_mode(*mode);
                }
            }
        }
    }

    /// The mouse in the viewer: the wheel (FAR's `KEY_MSWHEEL_*`: the system's lines or characters a
    /// notch, one with Alt); the rest of it does nothing there yet. True when the screen changed.
    pub(super) fn viewer_mouse(&mut self, mouse: &super::console_input::Mouse) -> bool {
        use super::console_input::MouseAction;
        if !self.mouse.enabled || self.overlay.is_some() || self.shown_task.is_some() {
            return false;
        }
        let super::Screen::Viewer(viewer) = &mut self.screen else {
            return false;
        };
        let (notches, horizontal) = match mouse.action {
            MouseAction::Wheel(notches) => (notches, false),
            MouseAction::HorizontalWheel(notches) => (notches, true),
            _ => return false,
        };
        let each = match (mouse.modifiers, horizontal) {
            (KeyModifiers::NONE, false) => crate::platform::input::wheel_scroll_lines(),
            (KeyModifiers::NONE, true) => crate::platform::input::wheel_scroll_chars(),
            (KeyModifiers::ALT, _) => 1,
            _ => return false,
        };
        viewer.wheel(notches, each, horizontal);
        true
    }
}

/// FAR's status line: `name│t│cp:5.5│size:<10│Col left:<3│pct:4`, the name filling the room left
/// (truncated as a path), `clock` columns and their separator kept on the right.
pub(super) fn status_line(
    name: &str,
    (page, facts): (&str, Facts),
    width: usize,
    clock: usize,
) -> String {
    let percent = if facts.at_end {
        100
    } else {
        to_percent(facts.position, facts.len)
    };
    let page: String = page.chars().take(5).collect();
    let mode = match facts.mode {
        Mode::Text => 't',
        Mode::Hex => 'h',
        Mode::Dump => 'd',
    };
    let line = format!(
        "│{mode}│{page:5}│{:<10}│Col {:<3}│{:<4}",
        facts.len,
        facts.left,
        format!("{percent}%")
    );
    let room = width.saturating_sub(clock + usize::from(clock > 0));
    let line = cut_right(&line, room);
    let name_width = room.saturating_sub(line.chars().count());
    let name = truncate_path(name, name_width);
    let mut status = format!("{name:name_width$}{line}");
    if clock > 0 {
        status.push('│');
    }
    status
}

/// FAR's `ToPercent`: `value` of `base` in whole percents, 0 of nothing.
fn to_percent(value: u64, base: u64) -> u64 {
    if value == 0 || base == 0 {
        return 0;
    }
    if value >= base {
        return 100;
    }
    u64::try_from(u128::from(value) * 100 / u128::from(base)).unwrap_or(100)
}

/// FAR's viewer keys of text mode (`Viewer::ProcessKey`).
fn text_move(key: KeyEvent) -> Option<Move> {
    let ctrl = key.modifiers == KeyModifiers::CONTROL;
    let plain = key.modifiers.is_empty();
    let ctrl_shift = key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    let step = match key.code {
        KeyCode::Up if plain => Move::Up,
        KeyCode::Down if plain => Move::Down,
        KeyCode::PageUp if ctrl => Move::Start,
        KeyCode::PageDown if ctrl => Move::Finish,
        KeyCode::PageUp | KeyCode::Up if plain || ctrl => Move::PageUp,
        KeyCode::PageDown | KeyCode::Down if plain || ctrl => Move::PageDown,
        KeyCode::Home if plain || ctrl => Move::Home,
        KeyCode::End if plain || ctrl => Move::End,
        KeyCode::Left if plain => Move::Left(1),
        KeyCode::Right if plain => Move::Right(1),
        KeyCode::Left if ctrl => Move::Left(COLUMNS),
        KeyCode::Right if ctrl => Move::Right(COLUMNS),
        KeyCode::Left if ctrl_shift => Move::LineStarts,
        _ => return None,
    };
    Some(step)
}

/// FAR's viewer keys of hex and dump (`Viewer::ProcessKey`): as text's up and down; Left / Right a
/// column of hex (none in dump), Ctrl+Left / Ctrl+Right the place by a character, Alt+Left /
/// Alt+Right hex's bytes per line by one, Ctrl+Alt+Left / Ctrl+Alt+Right by sixteen.
fn bytes_move(key: KeyEvent) -> Option<Move> {
    let ctrl = key.modifiers == KeyModifiers::CONTROL;
    let alt = key.modifiers == KeyModifiers::ALT;
    let ctrl_alt = key.modifiers == KeyModifiers::CONTROL | KeyModifiers::ALT;
    let step = match key.code {
        KeyCode::Left if ctrl => Move::Roll(-1),
        KeyCode::Right if ctrl => Move::Roll(1),
        KeyCode::Left if alt => Move::Bytes(-1),
        KeyCode::Right if alt => Move::Bytes(1),
        KeyCode::Left if ctrl_alt => Move::Bytes(-16),
        KeyCode::Right if ctrl_alt => Move::Bytes(16),
        _ => return text_move(key),
    };
    Some(step)
}

#[path = "viewer_goto.rs"]
mod goto;
pub(super) use goto::{answer as goto_answer, form as goto_form};
#[path = "viewer_search.rs"]
pub(super) mod search;

#[cfg(test)]
#[path = "viewer_tests.rs"]
mod tests;
