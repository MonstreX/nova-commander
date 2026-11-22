//! The editor (F4, stage 2d step 4b; FAR's `FileEditor` holding an `Editor`): a file of any size opened
//! at once on the piece table (ADR-0015 §4), FAR's keys of the basic set (`Editor::ProcessKey`): the
//! moves, with Shift the stream block, typing, Enter, BS, Del, Ctrl+Y, Ctrl+A / Ctrl+U, the clipboard,
//! Ctrl+Z / Alt+BS and Ctrl+Shift+Z, F2 / Shift+F2 / Shift+F10, Esc / F10, F6, Alt+F8.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

use super::editor_text::EditorText;
use crate::editor::view::edit_view::Step;
use crate::vfs::VfsPath;

/// The full-screen editor of a file (F4).
#[derive(Debug)]
pub struct EditorScreen {
    pub path: PathBuf,
    pub text: EditorText,
    /// An action asked before the steps typed ahead were done: done once they are.
    pub pending: Option<EditorAction>,
    /// The code page menu stands behind its rename dialog, its cursor on this page.
    pub menu_behind: Option<u32>,
    /// This editor's own settings (FAR's `EdOpt` of the editor: Ctrl+F3, Alt+Shift+F9).
    pub settings: crate::settings::EditorSettings,
    /// Ctrl+Q was pressed: the next key's character goes in as it is (FAR's `FEDITOR_PROCESSCTRLQ`).
    pub quoting: bool,
    /// What a plugin shows in the status line and the key bar (API 10).
    pub plugin_bars: PluginBars,
    /// What saving asks about (`editor_save`).
    pub saving: super::editor_save::Saving,
    /// The file's stamp and identity being looked at on a thread of their own (R7 review F5).
    pub saving_told: Option<std::sync::mpsc::Receiver<super::editor_save::Saving>>,
}

/// A plugin's title and key bar labels in the editor (FAR's `ECTL_SETTITLE`, `ECTL_SETKEYBAR`).
#[derive(Debug, Default)]
pub struct PluginBars {
    /// Shown instead of the file's name.
    pub title: Option<String>,
    /// Labels of function keys for the modifiers held, over the editor's own.
    pub labels: Vec<(u8, crate::platform::input::Modifiers, String)>,
}

/// What a key asks of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    None,
    /// F2.
    Save,
    /// Shift+F2: FAR's "Save file as".
    SaveAs,
    /// Shift+F4: another file to open or make (FAR hands it to the active panel).
    EditNew,
    /// Esc / F10: closed, FAR's question first when it was edited.
    Close,
    /// Shift+F10: saved and closed.
    SaveAndClose,
    /// F6: the file in the viewer.
    View,
    /// Alt+F8: FAR's "Go to".
    GoTo,
    /// Ctrl+C / Ctrl+Ins, Ctrl+X / Shift+Del: the block's text to the clipboard (and out).
    Copy {
        cut: bool,
    },
    /// Ctrl+V / Shift+Ins: the clipboard's text in.
    Paste,
    /// Ctrl+Gray +: the block's text added to the clipboard's (FAR's `Copy(TRUE)`).
    AppendCopy,
    /// F7: FAR's "Search".
    Search,
    /// Ctrl+Space in the code mode: the words that complete the one before the cursor.
    Complete,
    /// A FarColorer item that reads the parse (`app::colorer_actions`): done once the blocks are read.
    Colorer(super::colorer_menu::Action),
    /// Shift+F7 (forward), Alt+F7 (`true`: back): the search goes on.
    SearchOn(bool),
    /// Ctrl+F7: FAR's "Replace".
    Replace,
    /// The replace going on from `pos` after a Replace, Skip or All answered (`all`: the rest in one
    /// pass), once the steps before it are done.
    ReplaceFrom {
        pos: u64,
        backward: bool,
        all: bool,
    },
    /// F8: OEM after ANSI, ANSI after any other (FAR's `F8CP`).
    NextPage,
    /// Shift+F8: FAR's code page menu.
    PageMenu,
    /// Ctrl+F3: FAR's line numbers shown or not (this editor's).
    LineNumbers,
    /// Ctrl+L: FAR's lock mode set or let go.
    Lock,
    /// Alt+Shift+F9: FAR's "Editor" settings for this editor alone.
    Settings,
    /// Ctrl+F10: the panel onto the file (FAR's `GoToFile`), the editor left open.
    GoToFile,
    /// Ctrl+[ / Ctrl+], Ctrl+Shift+[ / ], Ctrl+Alt+[ / ], Alt+Shift+[ / ], Shift+Enter and
    /// Ctrl+Shift+Enter: a panel's folder or name put in (FAR's `ProcessInsPath`).
    InsertPath(KeyEvent),
    /// Ctrl+F: the file's full name put in.
    InsertFileName,
    /// F11: FAR's "Plugin commands" of the editor (the plugins' editor items, API 10).
    Plugins,
}

impl EditorScreen {
    pub fn open(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            text: EditorText::open(VfsPath::local(path)),
            pending: None,
            menu_behind: None,
            settings: crate::settings::EditorSettings::default(),
            quoting: false,
            plugin_bars: PluginBars::default(),
            saving: super::editor_save::Saving::default(),
            saving_told: super::editor_save::Saving::later(path),
        }
    }

    /// FAR's status line (`FileEditor::ShowStatus`) `width` wide: the name in the room left of
    /// `│*│cp│Ln n/total│Col n│Ch n│code`, `clock` columns kept free on its right.
    pub fn status(&self, width: usize, clock: usize) -> String {
        let changed = if self.text.is_changed() { '*' } else { ' ' };
        // FAR's `-` for the lock mode, `"` after Ctrl+Q.
        let locked = match (self.text.is_locked(), self.quoting) {
            (true, _) => '-',
            (false, true) => '"',
            (false, false) => ' ',
        };
        let page = (self.text.view()).map_or_else(String::new, |view| {
            super::quick_view_text::page_name(view.encoding())
        });
        let page: String = page.chars().take(5).collect();
        let place = self.hex_place().unwrap_or_else(|| self.text_place());
        let status = format!("│{changed}{locked}│{page:5}│{place}");
        let room = width.saturating_sub(clock + usize::from(clock > 0));
        let status = super::text_cut::cut_right(&status, room);
        let name_width = room.saturating_sub(status.chars().count());
        let title =
            (self.plugin_bars.title.clone()).unwrap_or_else(|| self.path.display().to_string());
        let name = super::text_cut::truncate_path(&title, name_width);
        let mut line = format!("{name:name_width$}{status}");
        if clock > 0 {
            line.push('│');
        }
        line
    }

    /// The status line's place in the text: `Ln n/total│Col n│Ch n│code`.
    fn text_place(&self) -> String {
        let (line, total, col, char) = self.text.place().unwrap_or((1, 1, 1, 1));
        // Not counted (a big file): `?`.
        let number = |count: u64| match count {
            0 => "?".to_string(),
            count => count.to_string(),
        };
        let lines = format!("{}/{}", number(line), number(total));
        let room = format!("{0}/{0}", number(total)).len();
        let code = (self.text.view())
            .and_then(|view| view.char_at_cursor().ok().flatten())
            .map_or_else(String::new, |ch| u32::from(ch).to_string());
        format!("Ln {lines:>room$}│Col {col:<3}│Ch {char:<3}│{code:<5}")
    }

    /// The status line's place in the hex mode (ADR-0027): the cursor's byte's offset and the
    /// document's bytes in hex, the byte's value.
    fn hex_place(&self) -> Option<String> {
        let view = self.text.view()?;
        let cursor = view.hex_cursor()?;
        let size = view.doc().size();
        let byte = view
            .hex_byte()
            .map_or_else(String::new, |byte| byte.to_string());
        Some(format!("Off {:X}/{size:X}│{byte:<5}", cursor.pos))
    }

    /// F8's label: the code page it would switch to (FAR's `F8CP::NextCPname`).
    pub fn next_page_label(&self) -> &'static str {
        match self.text.page_number() {
            Some(page) if page == crate::platform::locale::ansi_code_page() => "OEM",
            _ => "ANSI",
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> EditorAction {
        if std::mem::take(&mut self.quoting) {
            if let Some(ch) = quoted(key) {
                self.text.press(Step::Quoted(ch));
            }
            return EditorAction::None;
        }
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        if ctrl && key.code == KeyCode::Char('q') && !self.text.is_locked() {
            self.quoting = true;
            return EditorAction::None;
        }
        if let Some(action) = self.code_action(&key).or_else(|| command_key(key)) {
            // The hex mode replaces nothing (ADR-0027: it only overwrites).
            let hex = self.text.hex_extent(0).is_some();
            if !hex {
                self.before_save(&action);
            }
            return match action {
                EditorAction::Replace if hex => EditorAction::None,
                action => action,
            };
        }
        if let Some(step) = self.code_key(&key).or_else(|| step_of(key)) {
            self.text.press(step);
        }
        EditorAction::None
    }
}

/// FAR's editor keys that ask the app.
fn command_key(key: KeyEvent) -> Option<EditorAction> {
    let none = key.modifiers.is_empty();
    let shift = key.modifiers == KeyModifiers::SHIFT;
    let alt = key.modifiers == KeyModifiers::ALT;
    let action = match key.code {
        KeyCode::F(2) if none => EditorAction::Save,
        KeyCode::F(2) if shift => EditorAction::SaveAs,
        KeyCode::F(4) if shift => EditorAction::EditNew,
        // FAR's F4 too (`FileEditor::ReProcessKey`; its key bar says "Quit").
        KeyCode::Esc | KeyCode::F(4 | 10) if none => EditorAction::Close,
        KeyCode::F(10) if shift => EditorAction::SaveAndClose,
        KeyCode::F(6) if none => EditorAction::View,
        KeyCode::F(7) if none => EditorAction::Search,
        KeyCode::F(7) if shift => EditorAction::SearchOn(false),
        KeyCode::F(7) if alt => EditorAction::SearchOn(true),
        KeyCode::F(8) if alt => EditorAction::GoTo,
        KeyCode::F(8) if none => EditorAction::NextPage,
        KeyCode::F(8) if shift => EditorAction::PageMenu,
        _ => return own_key(key).or_else(|| clipboard_key(key)),
    };
    Some(action)
}

/// FAR's Ctrl+F7, and the keys of the editor's own options: Ctrl+F3, Ctrl+L, Alt+Shift+F9.
fn own_key(key: KeyEvent) -> Option<EditorAction> {
    let ctrl = key.modifiers == KeyModifiers::CONTROL;
    let action = match key.code {
        KeyCode::F(7) if ctrl => EditorAction::Replace,
        KeyCode::F(3) if ctrl => EditorAction::LineNumbers,
        KeyCode::Char('l') if ctrl => EditorAction::Lock,
        KeyCode::F(10) if ctrl => EditorAction::GoToFile,
        KeyCode::F(9) if key.modifiers == KeyModifiers::ALT | KeyModifiers::SHIFT => {
            EditorAction::Settings
        }
        KeyCode::Char('f') if ctrl => EditorAction::InsertFileName,
        KeyCode::F(11) if key.modifiers.is_empty() => EditorAction::Plugins,
        _ if is_path_key(&key) => EditorAction::InsertPath(key),
        _ => return None,
    };
    Some(action)
}

/// FAR's keys that put a panel's folder or name into the line (`Edit::ProcessInsPath`).
fn is_path_key(key: &KeyEvent) -> bool {
    let (ctrl, alt, shift) = (
        KeyModifiers::CONTROL,
        KeyModifiers::ALT,
        KeyModifiers::SHIFT,
    );
    match key.code {
        KeyCode::Char('[' | ']') => {
            [ctrl, ctrl | shift, ctrl | alt, alt | shift].contains(&key.modifiers)
        }
        KeyCode::Enter => key.modifiers == shift || key.modifiers == ctrl | shift,
        _ => false,
    }
}

/// FAR's clipboard keys: Ctrl+C / Ctrl+Ins, Ctrl+X / Shift+Del, Ctrl+V / Shift+Ins.
fn clipboard_key(key: KeyEvent) -> Option<EditorAction> {
    let ctrl = key.modifiers == KeyModifiers::CONTROL;
    let shift = key.modifiers == KeyModifiers::SHIFT;
    let action = match key.code {
        KeyCode::Char('c') | KeyCode::Insert if ctrl => EditorAction::Copy { cut: false },
        KeyCode::Char('x') if ctrl => EditorAction::Copy { cut: true },
        KeyCode::Delete if shift => EditorAction::Copy { cut: true },
        KeyCode::Char('v') if ctrl => EditorAction::Paste,
        KeyCode::Char('+') if ctrl && key.state.contains(KeyEventState::KEYPAD) => {
            EditorAction::AppendCopy
        }
        KeyCode::Insert if shift => EditorAction::Paste,
        _ => return None,
    };
    Some(action)
}

/// A move's step by its key, FAR's (none: not a move).
fn move_of(code: KeyCode, ctrl: bool) -> Option<Step> {
    let step = match (code, ctrl) {
        (KeyCode::Up, false) => Step::Up,
        (KeyCode::Down, false) => Step::Down,
        (KeyCode::Up, true) => Step::ScrollUp,
        (KeyCode::Down, true) => Step::ScrollDown,
        (KeyCode::Char('n'), true) => Step::ScreenTop,
        (KeyCode::Char('e'), true) => Step::ScreenBottom,
        (KeyCode::PageUp, true) => Step::FirstLine,
        (KeyCode::PageDown, true) => Step::LastLine,
        (KeyCode::Left, false) => Step::Left,
        (KeyCode::Right, false) => Step::Right,
        (KeyCode::Left, true) => Step::WordLeft,
        (KeyCode::Right, true) => Step::WordRight,
        (KeyCode::Home, false) => Step::Home,
        (KeyCode::End, false) => Step::End,
        (KeyCode::PageUp, false) => Step::PageUp,
        (KeyCode::PageDown, false) => Step::PageDown,
        (KeyCode::Home, true) => Step::Start,
        (KeyCode::End, true) => Step::Finish,
        _ => return None,
    };
    Some(step)
}

/// The character a key types after Ctrl+Q (FAR's `ProcessCtrlQ`: the console's character, a control
/// code with Ctrl).
fn quoted(key: KeyEvent) -> Option<char> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char(ch) if ctrl && ch.is_ascii() => {
            let code = u32::from(ch.to_ascii_uppercase()) & 0x1F;
            char::from_u32(code)
        }
        KeyCode::Char(ch) => Some(ch),
        KeyCode::Enter => Some('\r'),
        KeyCode::Tab => Some('\t'),
        KeyCode::Esc => Some('\u{1b}'),
        KeyCode::Backspace => Some('\u{8}'),
        _ => None,
    }
}

/// FAR's line keys: Ctrl+BS, Ctrl+Shift+BS, Ctrl+Del / Ctrl+T, Ctrl+K, Ins.
fn line_step(key: &KeyEvent) -> Option<Step> {
    let ctrl = key.modifiers == KeyModifiers::CONTROL;
    let ctrl_shift = key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    let step = match key.code {
        KeyCode::Backspace if ctrl => Step::DeleteWordLeft,
        KeyCode::Backspace if ctrl_shift => Step::DeleteToStart,
        KeyCode::Delete | KeyCode::Char('t') if ctrl => Step::DeleteWordRight,
        KeyCode::Char('k') if ctrl => Step::DeleteToEnd,
        KeyCode::Insert if key.modifiers.is_empty() => Step::Overtype,
        KeyCode::Char('d') if ctrl => Step::DeleteBlock,
        KeyCode::Char('p' | 'm') if ctrl => Step::CopyBlock(key.code == KeyCode::Char('m')),
        KeyCode::Char('u' | 'i') if key.modifiers == KeyModifiers::ALT => {
            Step::ShiftBlock(key.code == KeyCode::Char('i'))
        }
        // ADR-0027 addendum: Ctrl+F4 (Alt+F4 closes the console window in Windows Terminal).
        KeyCode::F(4) if ctrl => Step::Hex,
        _ => return None,
    };
    Some(step)
}

/// FAR's bookmark keys: Ctrl+Shift+0..9 / RCtrl+0..9 sets one, Ctrl+0..9 goes to it.
fn mark_step(key: &KeyEvent) -> Option<Step> {
    let (digit, set) = super::console_input::bookmark_key(key)?;
    Some(if set {
        Step::SetMark(digit)
    } else {
        Step::GoToMark(digit)
    })
}

/// FAR's vertical block keys: Alt+Shift (or Alt) with a move; Ctrl+Alt with Left / Right (a word),
/// PgUp / Home and PgDn / End (the file's ends), Up / Down.
fn column_step(key: &KeyEvent) -> Option<Step> {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    if !alt || (ctrl && shift) {
        return None;
    }
    let step = match (key.code, ctrl) {
        (KeyCode::Left, true) => Step::WordLeft,
        (KeyCode::Right, true) => Step::WordRight,
        (KeyCode::PageUp | KeyCode::Home, true) => Step::Start,
        (KeyCode::PageDown | KeyCode::End, true) => Step::Finish,
        (KeyCode::Up, _) => Step::Up,
        (KeyCode::Down, _) => Step::Down,
        (KeyCode::Left, false) => Step::Left,
        (KeyCode::Right, false) => Step::Right,
        (KeyCode::Home, false) => Step::Home,
        (KeyCode::End, false) => Step::End,
        (KeyCode::PageUp, false) => Step::PageUp,
        (KeyCode::PageDown, false) => Step::PageDown,
        _ => return None,
    };
    Some(Step::Column(Box::new(step)))
}

/// FAR's editor keys of the basic set as the view's steps.
fn step_of(key: KeyEvent) -> Option<Step> {
    let own = mark_step(&key).or_else(|| line_step(&key));
    if let Some(step) = own.or_else(|| column_step(&key)) {
        return Some(step);
    }
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    if !alt && let Some(step) = move_of(key.code, ctrl) {
        return Some(if shift {
            Step::Select(Box::new(step))
        } else {
            step
        });
    }
    let step = match key.code {
        KeyCode::Char('z') if ctrl && shift => Step::Redo,
        KeyCode::Char('z' | 'Z') if ctrl => Step::Undo,
        KeyCode::Backspace if alt => Step::Undo,
        KeyCode::Char('y') if ctrl => Step::DeleteLine,
        KeyCode::Char('a') if ctrl => Step::SelectAll,
        KeyCode::Char('u') if ctrl => Step::Unselect,
        KeyCode::Char(ch) if is_text_input(key.modifiers) => Step::Type(ch),
        KeyCode::Tab if !ctrl && !alt => Step::Type('\t'),
        KeyCode::Enter if !ctrl && !alt => Step::Enter,
        KeyCode::Backspace if !ctrl => Step::Backspace,
        KeyCode::Delete if !ctrl && !shift => Step::Delete,
        _ => return None,
    };
    Some(step)
}

/// Plain and shifted keys type text; so do Ctrl+Alt (AltGr on European layouts).
pub fn is_text_input(modifiers: KeyModifiers) -> bool {
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let alt = modifiers.contains(KeyModifiers::ALT);
    ctrl == alt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn far_s_keys_are_steps_and_actions() {
        let none = KeyModifiers::NONE;
        let shift = KeyModifiers::SHIFT;
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(
            step_of(key(KeyCode::Char('a'), none)),
            Some(Step::Type('a'))
        );
        assert_eq!(
            step_of(key(KeyCode::Char('@'), ctrl | KeyModifiers::ALT)),
            Some(Step::Type('@'))
        );
        assert_eq!(
            step_of(key(KeyCode::Char('q'), ctrl)),
            None,
            "Ctrl+letters are not typed"
        );
        assert_eq!(
            step_of(key(KeyCode::Right, shift)),
            Some(Step::Select(Box::new(Step::Right)))
        );
        assert_eq!(step_of(key(KeyCode::Home, ctrl)), Some(Step::Start));
    }

    #[test]
    fn far_s_path_keys_and_ctrl_f_put_text_in() {
        let ctrl = KeyModifiers::CONTROL;
        let shift = KeyModifiers::SHIFT;
        for (code, modifiers) in [
            (KeyCode::Char('['), ctrl),
            (KeyCode::Char(']'), ctrl | shift),
            (KeyCode::Char(']'), ctrl | KeyModifiers::ALT),
            (KeyCode::Char('['), KeyModifiers::ALT | shift),
            (KeyCode::Enter, shift),
            (KeyCode::Enter, ctrl | shift),
        ] {
            let pressed = key(code, modifiers);
            assert_eq!(
                command_key(pressed),
                Some(EditorAction::InsertPath(pressed))
            );
        }
        assert_eq!(command_key(key(KeyCode::Enter, KeyModifiers::NONE)), None);
        assert_eq!(
            command_key(key(KeyCode::Char('f'), ctrl)),
            Some(EditorAction::InsertFileName)
        );
    }

    #[test]
    fn far_s_vertical_block_keys_are_column_steps() {
        let alt = KeyModifiers::ALT;
        let column = |step: Step| Some(Step::Column(Box::new(step)));
        let alt_shift = alt | KeyModifiers::SHIFT;
        let ctrl_alt = alt | KeyModifiers::CONTROL;
        assert_eq!(step_of(key(KeyCode::Left, alt_shift)), column(Step::Left));
        assert_eq!(step_of(key(KeyCode::Down, alt)), column(Step::Down));
        assert_eq!(
            step_of(key(KeyCode::PageDown, alt_shift)),
            column(Step::PageDown)
        );
        assert_eq!(
            step_of(key(KeyCode::Right, ctrl_alt)),
            column(Step::WordRight)
        );
        assert_eq!(step_of(key(KeyCode::Home, ctrl_alt)), column(Step::Start));
        assert_eq!(
            step_of(key(KeyCode::PageDown, ctrl_alt)),
            column(Step::Finish)
        );
        assert_eq!(step_of(key(KeyCode::Up, ctrl_alt)), column(Step::Up));
        assert_eq!(
            step_of(key(KeyCode::Left, ctrl_alt | KeyModifiers::SHIFT)),
            None,
            "not FAR's"
        );
    }

    #[test]
    fn far_s_editing_keys_are_steps_and_actions() {
        let none = KeyModifiers::NONE;
        let shift = KeyModifiers::SHIFT;
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(step_of(key(KeyCode::Char('z'), ctrl)), Some(Step::Undo));
        assert_eq!(
            step_of(key(KeyCode::Char('z'), ctrl | shift)),
            Some(Step::Redo)
        );
        assert_eq!(
            step_of(key(KeyCode::Backspace, KeyModifiers::ALT)),
            Some(Step::Undo)
        );
        assert_eq!(
            step_of(key(KeyCode::F(4), ctrl)),
            Some(Step::Hex),
            "ADR-0027's hex mode"
        );
        assert_eq!(
            step_of(key(KeyCode::F(4), KeyModifiers::ALT)),
            None,
            "Alt+F4 is the console window's"
        );
        assert_eq!(
            step_of(key(KeyCode::Char('y'), ctrl)),
            Some(Step::DeleteLine)
        );
        assert_eq!(
            command_key(key(KeyCode::F(2), none)),
            Some(EditorAction::Save)
        );
        assert_eq!(
            command_key(key(KeyCode::F(2), shift)),
            Some(EditorAction::SaveAs)
        );
        assert_eq!(
            command_key(key(KeyCode::F(10), shift)),
            Some(EditorAction::SaveAndClose)
        );
        assert_eq!(
            command_key(key(KeyCode::Esc, none)),
            Some(EditorAction::Close)
        );
        assert_eq!(
            command_key(key(KeyCode::Delete, shift)),
            Some(EditorAction::Copy { cut: true })
        );
        assert_eq!(
            command_key(key(KeyCode::Insert, shift)),
            Some(EditorAction::Paste)
        );
    }
}
