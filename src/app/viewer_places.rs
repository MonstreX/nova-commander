//! The viewer's file as it opens (S2d.6b): its place kept from before put back (FAR's
//! `ViewerPosCache`), its place to keep as it is left, FAR's `DetectDumpMode`.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{MODES, Mode, Move, Viewer};

/// The places Alt+BS goes back through (FAR's `VIEWER_UNDO_COUNT`).
const BACK: usize = 65536;

/// The keys that leave what a search found marked without FAR's `PersistentBlocks` (`Viewer::
/// ProcessKey`): copying it (Ctrl+Ins, Ctrl+C, Ctrl+Num0) and going on with the search (Shift+F7,
/// Space, Alt+F7).
pub(super) fn keeps_selection(key: KeyEvent) -> bool {
    let (ctrl, shift, alt) = (
        KeyModifiers::CONTROL,
        KeyModifiers::SHIFT,
        KeyModifiers::ALT,
    );
    match key.code {
        KeyCode::Insert | KeyCode::Char('c' | '0') => key.modifiers == ctrl,
        KeyCode::F(7) => key.modifiers == shift || key.modifiers == alt,
        KeyCode::Char(' ') => key.modifiers.is_empty(),
        _ => false,
    }
}

impl Viewer {
    /// Whether the file just opened waits for its text or its colours before it is shown
    /// (`TextSyntax::holds_paint`; the text mode only, the others have none).
    pub fn holds_paint(&self) -> bool {
        self.text.mode() == Mode::Text && self.syntax.holds_paint()
    }

    /// The type chosen again (the schemes read anew).
    pub fn syntax_again(&mut self) {
        self.syntax.again();
    }

    /// The place kept from before, to be put back once the file is open.
    pub fn restore_position(&mut self, place: crate::settings::positions::ViewerPosition) {
        self.restore = Some(place);
    }

    /// F6 from the editor (FAR's `FileViewer::create` with the editor's `GetCurPos` and code page): the
    /// viewer shows from the editor's first row, in its code page, whatever the settings keep.
    pub fn start_at(&mut self, position: u64, page: u32) {
        self.restore = Some(crate::settings::positions::ViewerPosition {
            position,
            page,
            ..Default::default()
        });
        self.from_editor = true;
    }

    /// The place kept put back as the settings say (FAR's `Viewer::OpenFile`); true when it was.
    pub(super) fn put_back(&mut self) -> bool {
        if self.restore.is_none() || self.text.view().is_none() {
            return false;
        }
        let Some(place) = self.restore.take() else {
            return false;
        };
        let settings = self.settings.clone();
        let forced = std::mem::take(&mut self.from_editor);
        if (forced || settings.save_code_page || settings.save_position) && place.page != 0 {
            self.text.set_code_page(place.page);
        }
        let mode = place.mode.as_deref().and_then(mode_named);
        if let (true, Some(mode)) = (settings.save_view_mode, mode) {
            self.set_mode(mode);
            self.dump_checked = true;
        }
        if let (true, Some(words)) = (settings.save_wrap_mode, place.words) {
            self.text.set_words(words);
        }
        if let (true, Some(wrap)) = (settings.save_wrap_mode, place.wrap) {
            self.text.set_wrap(wrap);
        }
        if forced || settings.save_position {
            self.text.press(Move::To(place.position));
            self.text.press(Move::Column(place.left));
        }
        if settings.save_bookmarks {
            for (at, mark) in self.bookmarks.iter_mut().zip(place.bookmarks) {
                *at = mark;
            }
        }
        true
    }

    /// FAR's bookmark keys: Ctrl+Shift+0..9 or RCtrl+0..9 keeps the place shown, Ctrl+0..9 goes back to
    /// it (`BMSavePos`); true when `key` was one.
    pub(super) fn bookmark(&mut self, key: ratatui::crossterm::event::KeyEvent) -> bool {
        let Some((digit, set)) = crate::app::console_input::bookmark_key(&key) else {
            return false;
        };
        if set {
            if let Some(facts) = self.text.facts() {
                self.bookmarks[digit] = Some((facts.position, facts.left));
            }
        } else if let Some((position, left)) = self.bookmarks[digit] {
            self.text.press(Move::To(position));
            self.text.press(Move::Column(left));
        }
        true
    }

    /// Before a key: the place kept when it is not the last kept (FAR's `UndoData`, R7 review D7).
    pub fn remember_place(&mut self) {
        let Some(facts) = self.text.facts() else {
            return;
        };
        let place = (facts.position, facts.left);
        if self.back.last() != Some(&place) {
            if self.back.len() == BACK {
                self.back.remove(0);
            }
            self.back.push(place);
        }
    }

    /// Alt+BS, Ctrl+Z: the place before the last kept (FAR's `KEY_ALTBS`).
    pub fn go_back(&mut self) {
        self.back.pop();
        if let Some(&(position, left)) = self.back.last() {
            self.text.press(Move::To(position));
            self.text.press(Move::Column(left));
        }
    }

    /// Ctrl+U: what a search found no longer marked.
    pub fn drop_found(&mut self) {
        self.text.select(None);
    }

    /// What a search found, as text (Ctrl+C, Ctrl+Ins, Ctrl+Num0).
    pub fn found_text(&self) -> Option<String> {
        self.text.view()?.selected_text()
    }

    /// The place to keep as the file is left (FAR's `Viewer::SavePosition`).
    pub fn position(&self) -> Option<crate::settings::positions::ViewerPosition> {
        let facts = self.text.facts()?;
        Some(crate::settings::positions::ViewerPosition {
            position: facts.position,
            left: facts.left,
            page: self.text.page_number().unwrap_or(0),
            mode: self.touched.then(|| mode_name(facts.mode).to_string()),
            wrap: self.touched.then(|| self.text.wrapped()),
            words: self.touched.then(|| self.text.words()),
            bookmarks: self.bookmarks.to_vec(),
        })
    }

    /// FAR's `DetectDumpMode` once the first block is read (`isBinaryFile`: a NUL in the first 2 KB, a
    /// zero character in UTF-16): dump mode; true when it switched.
    pub(super) fn detect_dump(&mut self) -> bool {
        if self.dump_checked {
            return false;
        }
        let Some(view) = self.text.view() else {
            return false;
        };
        let Ok(first) = view.first_block() else {
            return false;
        };
        self.dump_checked = true;
        let head = &first[..first.len().min(2048)];
        let utf16 = matches!(
            view.encoding(),
            Some(crate::editor::view::Encoding::Utf16Le | crate::editor::view::Encoding::Utf16Be)
        );
        let binary = if utf16 {
            head.chunks_exact(2).any(|unit| unit == [0, 0])
        } else {
            head.contains(&0)
        };
        if !(self.settings.detect_dump && binary && self.text.mode() == Mode::Text) {
            return false;
        }
        self.set_mode(Mode::Dump);
        true
    }
}

/// A mode's name in the positions' file.
fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Text => "text",
        Mode::Hex => "hex",
        Mode::Dump => "dump",
    }
}

fn mode_named(name: &str) -> Option<Mode> {
    MODES.into_iter().find(|mode| mode_name(*mode) == name)
}
