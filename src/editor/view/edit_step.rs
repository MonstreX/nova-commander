//! The editor's steps: what a key asks of the view (moves, edits, FAR's bookmarks, places).

/// A step of the editor's keys (FAR's `Editor::ProcessKey`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    /// Ctrl+Home / Ctrl+End.
    Start,
    Finish,
    /// Ctrl+Left / Ctrl+Right: a word on.
    WordLeft,
    WordRight,
    /// A move holding Shift: the stream block from where the cursor was.
    Select(Box<Step>),
    /// A move holding Alt+Shift (or Alt; Ctrl+Alt: a word, the file's ends): FAR's vertical block.
    Column(Box<Step>),
    /// A vertical block's text put in at the cursor (Ctrl+V with one on the clipboard).
    PasteColumn(String),
    /// A panel's folder or name put in at the cursor (Ctrl+[ / Ctrl+], Shift+Enter...: FAR's
    /// `ProcessInsPath` in the line), in place of a stream block that is not persistent.
    Insert(String),
    /// A plugin's line text (the line by its first byte; FAR's `ECTL_SETSTRING`).
    /// `Some`: the break a last line without one gets (none given: the file's; FAR's StringEOL).
    SetLineText(u64, String, Option<String>),
    /// A plugin's text typed at the cursor, `\n` as Enter, the block kept (`ECTL_INSERTTEXT`).
    PluginText(String),
    /// The character at the cursor taken out, the block kept (`ECTL_DELETECHAR`).
    DeleteChar,
    /// Enter at the cursor, with the auto indent when `true` (`ECTL_INSERTSTRING`).
    Break(bool),
    /// The cursor and the screen put where a plugin says (`ECTL_SETPOSITION`).
    Place(super::plugin::Place),
    /// A block a plugin marks (`ECTL_SELECT`).
    SelectRange(super::plugin::Range),
    /// The edits from `true` to `false` one undo step (`ECTL_UNDOREDO`'s `EUR_BEGIN` / `EUR_END`).
    Group(bool),
    /// A line's tabs as spaces (the line by its first byte; `ECTL_EXPANDTABS`).
    ExpandTabs(u64),
    /// Ctrl+F: the file's full name put in (FAR's `KEY_CTRLF`: a block not persistent taken out
    /// first, any block let go after).
    InsertName(String),
    Type(char),
    /// Text put in at the cursor (Ctrl+V), its breaks the file's.
    Paste(String),
    Enter,
    Backspace,
    Delete,
    /// Ctrl+Y.
    DeleteLine,
    /// Ctrl+A.
    SelectAll,
    /// Ctrl+U: the block let go.
    Unselect,
    /// A click on the text (FAR's `Editor::ProcessMouse`): the cursor on the screen's row and the
    /// column from the first one shown (past the last line: on it).
    Click(usize, usize),
    /// The cursor's line's characters from the first to the one past the last marked as the stream
    /// block, the cursor where it is (FAR's double click on a word, the click after it on the line).
    MarkChars(usize, usize),
    /// Ctrl+C / Ctrl+Ins with no block: the cursor's line marked with its break (FAR's
    /// `AddSelect(0, -1)`), the cursor where it is.
    MarkLine,
    /// Ctrl+X / Shift+Del: the block taken out (its text is the caller's, read before).
    Cut,
    Undo,
    Redo,
    /// Alt+F8's line (from 0) and column.
    GoTo(u64, usize),
    /// What a search found (its first byte, its bytes): the cursor at it, it marked.
    Show(u64, u64),
    /// A match (its first byte, its bytes) replaced by the text, the cursor past it; `true`: a
    /// replacement of "All", one undo step with the "All" ones before it (FAR's `undo_block`).
    Replace(u64, u64, String, bool),
    /// "All"'s matches (first byte, bytes; the last first) found in one pass, all replaced by the text
    /// as one undo step, the cursor past the last (`false`) or at the first going back (`true`) (R7
    /// review F2, F3: replaced one at a time, each next one was found in a copy without those still
    /// waiting for their blocks).
    ReplaceAll(Vec<(u64, u64)>, String, bool),
    /// Ctrl+BS, Ctrl+Del / Ctrl+T: FAR's word before / after the cursor taken out.
    DeleteWordLeft,
    DeleteWordRight,
    /// Ctrl+Shift+BS, Ctrl+K: the line up to / from the cursor taken out.
    DeleteToStart,
    DeleteToEnd,
    /// Ctrl+D: the block taken out.
    DeleteBlock,
    /// Ctrl+P / Ctrl+M (`true`): the block copied / moved to the cursor.
    CopyBlock(bool),
    /// Alt+U / Alt+I (`true`): the block's lines a column left / right.
    ShiftBlock(bool),
    /// Shift+Tab in the code mode: the block's lines (or the cursor's) an indent left.
    Outdent,
    /// Ctrl+/ in the code mode: the lines commented out or in with the file type's token.
    ToggleComment(String),
    /// Alt+Up / Alt+Down (`true`: up) in the code mode: the lines moved over their neighbour.
    MoveLines(bool),
    /// Ctrl+Shift+D in the code mode: the lines put again after them.
    Duplicate,
    /// Before a save in the code mode: the lines' ending blanks out, a last line given its break.
    CleanUp,
    /// Ctrl+Space's word chosen: the characters typed before the cursor replaced by it.
    Complete(usize, String),
    /// Ins: FAR's overtype on or off.
    Overtype,
    /// The character after Ctrl+Q, put in as it is (FAR's `ProcessCtrlQ`: a CR stays a CR).
    Quoted(char),
    /// Ctrl+Up / Ctrl+Down: the screen and the cursor a line (FAR's `ScrollUp`, `ScrollDown`).
    ScrollUp,
    ScrollDown,
    /// Ctrl+N / Ctrl+E: the cursor on the screen's first / last row.
    ScreenTop,
    ScreenBottom,
    /// Ctrl+PgUp / Ctrl+PgDn: the first / last line, the column kept.
    FirstLine,
    LastLine,
    /// Ctrl+Shift+0..9 / RCtrl+0..9: FAR's bookmark set at the cursor.
    SetMark(usize),
    /// Ctrl+0..9: the cursor at FAR's bookmark.
    GoToMark(usize),
    /// A place kept put back (FAR's `SetCacheParams`): the cursor's line by a byte in it and its
    /// column, the first row's line by a byte in it, the first column shown.
    Restore(u64, usize, u64, usize),
    /// Ctrl+F4: the hex mode on or off (ADR-0027).
    Hex,
    /// The hex mode on at a byte, in the text column when `true` (the editor read again after a save).
    HexAt(u64, bool),
}

impl Step {
    /// Whether the step changes the document (FAR's lock mode leaves these undone).
    pub fn edits(&self) -> bool {
        matches!(
            self,
            Step::Type(_)
                | Step::Paste(_)
                | Step::PasteColumn(_)
                | Step::Insert(_)
                | Step::InsertName(_)
                | Step::SetLineText(..)
                | Step::PluginText(_)
                | Step::DeleteChar
                | Step::Break(_)
                | Step::ExpandTabs(_)
                | Step::Enter
                | Step::Backspace
                | Step::Delete
                | Step::DeleteLine
                | Step::Cut
                | Step::Undo
                | Step::Redo
                | Step::Replace(..)
                | Step::ReplaceAll(..)
                | Step::DeleteWordLeft
                | Step::DeleteWordRight
                | Step::DeleteToStart
                | Step::DeleteToEnd
                | Step::Quoted(_)
                | Step::DeleteBlock
                | Step::CopyBlock(_)
                | Step::ShiftBlock(_)
                | Step::Outdent
                | Step::ToggleComment(_)
                | Step::MoveLines(_)
                | Step::Duplicate
                | Step::CleanUp
                | Step::Complete(..)
        )
    }
}
