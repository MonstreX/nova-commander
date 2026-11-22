//! The program's editor as a plugin reads and changes it (API 10, FAR's `EditorControl`):
//! [`NcHost::editor`](crate::NcHost::editor), asked from a call of the plugin's made while the editor is
//! up (its item of the editor's F11 menu, [`PLACE_EDITOR`](crate::PLACE_EDITOR)). Lines are numbered from
//! 0, -1 being the cursor's; positions in a line are characters from 0 (a tab one) unless named "tab"
//! positions, the screen's columns (FAR's `CurTabPos`).

use crate::NcStr;

/// FAR's `ECTL_GETINFO`: the editor's state into the answer's [`NcEditorAnswer::info`].
pub const EDITOR_INFO: u32 = 0;
/// `ECTL_GETSTRING`: line `line`'s text into the answer's `text`, its break into `text2` (empty for the
/// last line without one), the block's part of it into `number` (its first character, -1: none) and
/// `number2` (the one past its last, -1: on through the break).
pub const EDITOR_GET_LINE: u32 = 1;
/// `ECTL_SETSTRING`: line `line`'s text replaced by `text` (its break kept; a last line without one
/// gets `text2`'s, or the file's), the cursor's character position kept.
pub const EDITOR_SET_LINE: u32 = 2;
/// `ECTL_INSERTSTRING`: the cursor's line broken at the cursor as Enter does; `number` 1: with the
/// editor's auto indent.
pub const EDITOR_INSERT_LINE: u32 = 3;
/// `ECTL_DELETESTRING`: the cursor's line taken out as Ctrl+Y does.
pub const EDITOR_DELETE_LINE: u32 = 4;
/// `ECTL_INSERTTEXT`: `text` typed at the cursor (`\n` as Enter).
pub const EDITOR_INSERT_TEXT: u32 = 5;
/// `ECTL_DELETECHAR`: the character at the cursor taken out as Del does (a block kept).
pub const EDITOR_DELETE_CHAR: u32 = 6;
/// `ECTL_SETPOSITION`: each of `line`, `pos`, `tab_pos`, `top` (the first line shown, at most the
/// cursor's), `left` (the first column shown) and `number` (overtype: 1 on, 0 off) that is not -1.
pub const EDITOR_SET_POSITION: u32 = 7;
/// `ECTL_SELECT`: the block (`number`: [`BLOCK_NONE`], [`BLOCK_STREAM`], [`BLOCK_COLUMN`]) from line
/// `line` at `pos` (a column block: at the tab position `pos`), `number2` lines high; a stream block's
/// last line ends at `pos` + `width` (its lines before: through their breaks), a column block is
/// `width` columns wide (less than 0: to the left of `pos`). [`BLOCK_NONE`] or `pos` -1: no block.
pub const EDITOR_SELECT: u32 = 8;
/// `ECTL_UNDOREDO`: `number` [`UNDO_BEGIN`] / [`UNDO_END`] (the edits between them one undo step),
/// [`UNDO_UNDO`], [`UNDO_REDO`].
pub const EDITOR_UNDO_REDO: u32 = 9;
/// `ECTL_REDRAW`: the editor drawn again with what the plugin did.
pub const EDITOR_REDRAW: u32 = 10;
/// `ECTL_EXPANDTABS`: line `line`'s tabs as the spaces they show as.
pub const EDITOR_EXPAND_TABS: u32 = 11;
/// `ECTL_REALTOTAB`: line `line`'s character position `pos` as its tab position, into the answer's
/// `number`.
pub const EDITOR_REAL_TO_TAB: u32 = 12;
/// `ECTL_TABTOREAL`: line `line`'s tab position `tab_pos` as its character position, into `number`.
pub const EDITOR_TAB_TO_REAL: u32 = 13;
/// `ECTL_GETFILENAME`: the edited file's full name into the answer's `text`.
pub const EDITOR_FILE_NAME: u32 = 14;
/// `ECTL_READINPUT` (S2d.7d): the next key pressed in the editor, by its name as the plugins name keys
/// ("Shift+Up", "F2", "a"), into the answer's `text`; the call waits for it. The editor does not do it:
/// the plugin passes on what it does not take with [`EDITOR_PROCESS_INPUT`].
pub const EDITOR_READ_INPUT: u32 = 15;
/// `ECTL_PROCESSINPUT`: the key named `text` done by the editor as if pressed (the plugins that take the
/// editor's keys do not hear it).
pub const EDITOR_PROCESS_INPUT: u32 = 16;
/// `ECTL_SETTITLE`: `text` shown in the editor's status line instead of the file's name; empty: the
/// name again.
pub const EDITOR_SET_TITLE: u32 = 17;
/// `ECTL_SETKEYBAR`: the key bar's labels of the function keys `text` names, a line each: the key's
/// name, a tab, its label ("F2\tDouble", "Ctrl+F5\t"); the others kept. Empty: the editor's own again.
pub const EDITOR_SET_KEY_BAR: u32 = 18;
/// The system's default sound, as FAR's editor plugins sound it with `MessageBeep(0)` (Brackets'
/// "Beep if matched"): a plugin reaches the system only through the program.
pub const EDITOR_BEEP: u32 = 19;

/// [`EDITOR_SELECT`]'s and [`NcEditorInfo::block`]'s kinds (FAR's `BTYPE_*`).
pub const BLOCK_NONE: i64 = 0;
pub const BLOCK_STREAM: i64 = 1;
pub const BLOCK_COLUMN: i64 = 2;

/// [`EDITOR_UNDO_REDO`]'s commands (FAR's `EUR_*`).
pub const UNDO_BEGIN: i64 = 0;
pub const UNDO_END: i64 = 1;
pub const UNDO_UNDO: i64 = 2;
pub const UNDO_REDO: i64 = 3;

/// [`NcEditorInfo::options`] (FAR's `EOPT_*`).
pub const OPTION_EXPAND_ALL_TABS: u32 = 0x1;
pub const OPTION_PERSISTENT_BLOCKS: u32 = 0x2;
pub const OPTION_DEL_REMOVES_BLOCKS: u32 = 0x4;
pub const OPTION_AUTO_INDENT: u32 = 0x8;
pub const OPTION_SAVE_POSITION: u32 = 0x10;
pub const OPTION_AUTO_DETECT_CODE_PAGE: u32 = 0x20;
pub const OPTION_CURSOR_BEYOND_EOL: u32 = 0x40;
pub const OPTION_EXPAND_NEW_TABS: u32 = 0x80;
pub const OPTION_SHOW_WHITE_SPACE: u32 = 0x100;
pub const OPTION_SHOW_SCROLLBAR: u32 = 0x400;
pub const OPTION_SHOW_LINE_NUMBERS: u32 = 0x1000;

/// [`NcEditorInfo::state`] (FAR's `ECSTATE_*`): no changes since the last save.
pub const STATE_SAVED: u32 = 0x2;
/// Lock mode (Ctrl+L).
pub const STATE_LOCKED: u32 = 0x4;

/// A request to the editor: the command and what it takes (-1 where a number is not given).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcEditorAction {
    /// `EDITOR_*`.
    pub command: u32,
    pub line: i64,
    pub pos: i64,
    pub tab_pos: i64,
    pub top: i64,
    pub left: i64,
    pub width: i64,
    pub number: i64,
    pub number2: i64,
    pub text: NcStr,
    pub text2: NcStr,
}

impl NcEditorAction {
    /// `command` with nothing given.
    pub const fn new(command: u32) -> Self {
        Self {
            command,
            line: -1,
            pos: -1,
            tab_pos: -1,
            top: -1,
            left: -1,
            width: -1,
            number: -1,
            number2: -1,
            text: NcStr::EMPTY,
            text2: NcStr::EMPTY,
        }
    }
}

/// The editor's state (FAR's `EditorInfo`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NcEditorInfo {
    /// The text's columns and rows on the screen.
    pub width: i64,
    pub height: i64,
    /// The lines (FAR's `TotalLines`); a big document's are counted first, the answer waits for them.
    pub lines: i64,
    /// The cursor's line, its character position and its tab position.
    pub line: i64,
    pub pos: i64,
    pub tab_pos: i64,
    /// The first line and the first column shown.
    pub top: i64,
    pub left: i64,
    pub overtype: i64,
    /// `BLOCK_*`, and the block's first line.
    pub block: i64,
    pub block_line: i64,
    /// `OPTION_*`.
    pub options: u32,
    pub tab_size: i64,
    /// `STATE_*`.
    pub state: u32,
    /// The code page's number.
    pub code_page: u32,
}

/// What the editor answers; texts the host's, valid until the plugin's next call to the host.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcEditorAnswer {
    pub number: i64,
    pub number2: i64,
    pub text: NcStr,
    pub text2: NcStr,
    pub info: NcEditorInfo,
}

impl Default for NcEditorAnswer {
    fn default() -> Self {
        Self {
            number: -1,
            number2: -1,
            text: NcStr::EMPTY,
            text2: NcStr::EMPTY,
            info: NcEditorInfo::default(),
        }
    }
}
