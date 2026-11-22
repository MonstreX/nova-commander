//! The program's editor as a plugin reads and changes it (API 10, FAR's `EditorControl`), from a call
//! made while the editor is up (its [`Plugin::editor_items`](crate::Plugin::editor_items)). Lines are
//! numbered from 0 (`None`: the cursor's); positions are characters (a tab one), "tab" positions the
//! screen's columns.

use crate::Host;
use crate::api::{self, NcEditorAction, NcEditorAnswer, NcStr};

/// The editor's state (FAR's `EditorInfo`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditorInfo {
    /// The text's columns and rows on the screen.
    pub width: usize,
    pub height: usize,
    /// Its lines (FAR's `TotalLines`); a big document's are counted first, the call waits for them.
    pub lines: usize,
    /// The cursor's line, character position and tab position.
    pub line: usize,
    pub pos: usize,
    pub tab_pos: usize,
    /// The first line and column shown.
    pub top: usize,
    pub left: usize,
    pub overtype: bool,
    /// The block (`Block`), and its first line.
    pub block: Block,
    pub block_line: usize,
    /// `api::OPTION_*`.
    pub options: u32,
    pub tab_size: usize,
    /// `api::STATE_*`.
    pub state: u32,
    pub code_page: u32,
}

/// A block's kind (FAR's `BTYPE_*`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Block {
    #[default]
    None,
    Stream,
    Column,
}

impl Block {
    fn code(self) -> i64 {
        match self {
            Self::None => api::BLOCK_NONE,
            Self::Stream => api::BLOCK_STREAM,
            Self::Column => api::BLOCK_COLUMN,
        }
    }

    fn from_code(code: i64) -> Self {
        match code {
            api::BLOCK_STREAM => Self::Stream,
            api::BLOCK_COLUMN => Self::Column,
            _ => Self::None,
        }
    }
}

/// A line as the editor has it (FAR's `EditorGetString`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorLine {
    pub text: String,
    /// Its break (`""` on the last line without one).
    pub eol: String,
    /// The block's part of it: its first character and the one past its last (`None` past it: on
    /// through the break); `None` when the block is not on it.
    pub block: Option<(usize, Option<usize>)>,
}

/// Where the cursor and the screen go (FAR's `EditorSetPosition`); `None` keeps it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Position {
    pub line: Option<usize>,
    pub pos: Option<usize>,
    pub tab_pos: Option<usize>,
    pub top: Option<usize>,
    pub left: Option<usize>,
    pub overtype: Option<bool>,
}

/// A block to mark (FAR's `EditorSelect`): from `line` at `pos` (a column block: at the tab position),
/// `height` lines; a stream block's last line ends at `pos + width`, a column block is `width` columns
/// wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub block: Block,
    pub line: usize,
    pub pos: usize,
    pub width: i64,
    pub height: usize,
}

fn signed(number: Option<usize>) -> i64 {
    number.map_or(-1, |number| i64::try_from(number).unwrap_or(i64::MAX))
}

fn unsigned(number: i64) -> usize {
    usize::try_from(number).unwrap_or(0)
}

impl Host {
    /// `action` done to the editor; its answer, none when it could not be.
    fn editor_do(&self, action: &NcEditorAction) -> Option<NcEditorAnswer> {
        let table = self.table();
        let mut answer = NcEditorAnswer::default();
        // SAFETY: as every call; the action lives through it, the answer's texts are copied before the
        // next call.
        let done = unsafe { (table.editor)(table.context, action, &raw mut answer) };
        (done == 0).then_some(answer)
    }

    fn editor_line_action(command: u32, line: Option<usize>) -> NcEditorAction {
        let mut action = NcEditorAction::new(command);
        action.line = signed(line);
        action
    }

    /// The editor's state (FAR's `ECTL_GETINFO`); none without an editor.
    pub fn editor_info(&self) -> Option<EditorInfo> {
        let info = self.editor_do(&NcEditorAction::new(api::EDITOR_INFO))?.info;
        Some(EditorInfo {
            width: unsigned(info.width),
            height: unsigned(info.height),
            lines: unsigned(info.lines),
            line: unsigned(info.line),
            pos: unsigned(info.pos),
            tab_pos: unsigned(info.tab_pos),
            top: unsigned(info.top),
            left: unsigned(info.left),
            overtype: info.overtype != 0,
            block: Block::from_code(info.block),
            block_line: unsigned(info.block_line),
            options: info.options,
            tab_size: unsigned(info.tab_size),
            state: info.state,
            code_page: info.code_page,
        })
    }

    /// Line `line` (FAR's `ECTL_GETSTRING`); none past the last.
    pub fn editor_line(&self, line: Option<usize>) -> Option<EditorLine> {
        let answer = self.editor_do(&Self::editor_line_action(api::EDITOR_GET_LINE, line))?;
        // SAFETY: the host's texts, valid until the next call; copied here.
        let (text, eol) = unsafe { (answer.text.as_str(), answer.text2.as_str()) };
        let block = (answer.number >= 0).then(|| {
            let past = usize::try_from(answer.number2).ok();
            (unsigned(answer.number), past)
        });
        Some(EditorLine {
            text: text.to_string(),
            eol: eol.to_string(),
            block,
        })
    }

    /// Line `line`'s text set (FAR's `ECTL_SETSTRING`), its break kept.
    pub fn set_editor_line(&self, line: Option<usize>, text: &str) -> bool {
        let mut action = Self::editor_line_action(api::EDITOR_SET_LINE, line);
        action.text = NcStr::new(text);
        self.editor_do(&action).is_some()
    }

    /// Enter at the cursor (FAR's `ECTL_INSERTSTRING`), with the auto indent when `indent`.
    pub fn editor_insert_line(&self, indent: bool) -> bool {
        let mut action = NcEditorAction::new(api::EDITOR_INSERT_LINE);
        action.number = i64::from(indent);
        self.editor_do(&action).is_some()
    }

    /// The cursor's line taken out (FAR's `ECTL_DELETESTRING`).
    pub fn editor_delete_line(&self) -> bool {
        self.editor_do(&NcEditorAction::new(api::EDITOR_DELETE_LINE))
            .is_some()
    }

    /// `text` typed at the cursor, `\n` as Enter (FAR's `ECTL_INSERTTEXT`).
    pub fn editor_insert_text(&self, text: &str) -> bool {
        let mut action = NcEditorAction::new(api::EDITOR_INSERT_TEXT);
        action.text = NcStr::new(text);
        self.editor_do(&action).is_some()
    }

    /// The character at the cursor taken out (FAR's `ECTL_DELETECHAR`).
    pub fn editor_delete_char(&self) -> bool {
        self.editor_do(&NcEditorAction::new(api::EDITOR_DELETE_CHAR))
            .is_some()
    }

    /// The cursor and the screen put at `position` (FAR's `ECTL_SETPOSITION`).
    pub fn set_editor_position(&self, position: &Position) -> bool {
        let mut action = Self::editor_line_action(api::EDITOR_SET_POSITION, position.line);
        action.pos = signed(position.pos);
        action.tab_pos = signed(position.tab_pos);
        action.top = signed(position.top);
        action.left = signed(position.left);
        action.number = position.overtype.map_or(-1, i64::from);
        self.editor_do(&action).is_some()
    }

    /// The block marked (FAR's `ECTL_SELECT`); `None`: no block.
    pub fn editor_select(&self, selection: Option<&Selection>) -> bool {
        let mut action = NcEditorAction::new(api::EDITOR_SELECT);
        action.number = api::BLOCK_NONE;
        if let Some(selection) = selection {
            action.number = selection.block.code();
            action.line = signed(Some(selection.line));
            action.pos = signed(Some(selection.pos));
            action.width = selection.width;
            action.number2 = signed(Some(selection.height));
        }
        self.editor_do(&action).is_some()
    }

    /// FAR's `ECTL_UNDOREDO`: `api::UNDO_*`.
    pub fn editor_undo(&self, command: i64) -> bool {
        let mut action = NcEditorAction::new(api::EDITOR_UNDO_REDO);
        action.number = command;
        self.editor_do(&action).is_some()
    }

    /// The editor drawn again (FAR's `ECTL_REDRAW`).
    pub fn editor_redraw(&self) -> bool {
        self.editor_do(&NcEditorAction::new(api::EDITOR_REDRAW))
            .is_some()
    }

    /// Line `line`'s tabs as spaces (FAR's `ECTL_EXPANDTABS`).
    pub fn editor_expand_tabs(&self, line: Option<usize>) -> bool {
        (self.editor_do(&Self::editor_line_action(api::EDITOR_EXPAND_TABS, line))).is_some()
    }

    /// Line `line`'s character position `pos` as its tab position (FAR's `ECTL_REALTOTAB`).
    pub fn editor_real_to_tab(&self, line: Option<usize>, pos: usize) -> Option<usize> {
        let mut action = Self::editor_line_action(api::EDITOR_REAL_TO_TAB, line);
        action.pos = signed(Some(pos));
        Some(unsigned(self.editor_do(&action)?.number))
    }

    /// Line `line`'s tab position as its character position (FAR's `ECTL_TABTOREAL`).
    pub fn editor_tab_to_real(&self, line: Option<usize>, tab_pos: usize) -> Option<usize> {
        let mut action = Self::editor_line_action(api::EDITOR_TAB_TO_REAL, line);
        action.tab_pos = signed(Some(tab_pos));
        Some(unsigned(self.editor_do(&action)?.number))
    }

    /// The edited file's full name (FAR's `ECTL_GETFILENAME`).
    pub fn editor_file_name(&self) -> Option<String> {
        self.editor_text(&NcEditorAction::new(api::EDITOR_FILE_NAME))
    }

    /// The answer's text to `action`.
    fn editor_text(&self, action: &NcEditorAction) -> Option<String> {
        let answer = self.editor_do(action)?;
        // SAFETY: the host's text, valid until the next call; copied here.
        Some(unsafe { answer.text.as_str() }.to_string())
    }

    /// The next key pressed in the editor by its name ("Shift+Up", "F10", "a"; FAR's
    /// `ECTL_READINPUT`), waited for; the editor does not do it. None once the editor is gone.
    pub fn editor_read_input(&self) -> Option<String> {
        self.editor_text(&NcEditorAction::new(api::EDITOR_READ_INPUT))
    }

    /// The key named `key` done by the editor as if pressed (FAR's `ECTL_PROCESSINPUT`).
    pub fn editor_process_input(&self, key: &str) -> bool {
        let mut action = NcEditorAction::new(api::EDITOR_PROCESS_INPUT);
        action.text = NcStr::new(key);
        self.editor_do(&action).is_some()
    }

    /// `title` in the editor's status line instead of the file's name, none: the name again (FAR's
    /// `ECTL_SETTITLE`).
    pub fn set_editor_title(&self, title: Option<&str>) -> bool {
        let mut action = NcEditorAction::new(api::EDITOR_SET_TITLE);
        action.text = NcStr::new(title.unwrap_or_default());
        self.editor_do(&action).is_some()
    }

    /// The key bar's labels of the function keys named (`("Shift+F2", "Draw")`), the others kept;
    /// none: the editor's own again (FAR's `ECTL_SETKEYBAR`).
    pub fn set_editor_key_bar(&self, labels: Option<&[(&str, &str)]>) -> bool {
        let text: String = (labels.unwrap_or_default().iter())
            .map(|(key, label)| format!("{key}\t{label}\n"))
            .collect();
        let mut action = NcEditorAction::new(api::EDITOR_SET_KEY_BAR);
        action.text = NcStr::new(&text);
        self.editor_do(&action).is_some()
    }

    /// The system's default sound (FAR's plugins' `MessageBeep(0)`).
    pub fn editor_beep(&self) -> bool {
        self.editor_do(&NcEditorAction::new(api::EDITOR_BEEP))
            .is_some()
    }
}

/// `NcPlugin::editor_key`: the plugin hears the key the editor has done.
pub(crate) unsafe extern "C" fn editor_key<P: crate::Plugin>(
    raw: *mut std::ffi::c_void,
    key: NcStr,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { crate::state::<P>(raw) };
    state.kept.clear();
    // SAFETY: the host's text, alive through the call; copied here.
    let key = unsafe { key.as_str() }.to_string();
    let plugin = &mut state.plugin;
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| plugin.editor_key(&key))) {
        Ok(()) => 0,
        Err(_) => {
            state.error = crate::panic_text();
            1
        }
    }
}

#[cfg(test)]
#[path = "editor_tests.rs"]
pub(crate) mod tests;
