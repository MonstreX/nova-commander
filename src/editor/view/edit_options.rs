//! How the editor edits: FAR's `EditorOptions` the editor's view takes (S2d.6d).

/// How the editor edits (FAR's `EditorOptions`, "Editor settings"; S2d.6d), FAR's defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditOptions {
    /// `PersistentBlocks`: a move or typing keeps the block.
    pub persistent_blocks: bool,
    /// `DelRemovesBlocks`: Del takes the block out.
    pub del_removes_blocks: bool,
    /// `AutoIndent`: Enter starts the new line under the current one's first character.
    pub auto_indent: bool,
    /// `ExpandTabs`: 0 tabs kept, 1 a typed Tab as spaces to the next stop (`EXPAND_NEWTABS`), 2 also
    /// the tabs of a line edited (`EXPAND_ALLTABS`).
    pub expand_tabs: u8,
    /// `CursorBeyondEOL`: the cursor may stand past a line's end.
    pub cursor_beyond_eol: bool,
    /// `SearchSelFound`: what a search found made the block.
    pub select_found: bool,
    /// `SearchCursorAtEnd`: the cursor after what a search found.
    pub cursor_at_end: bool,
    /// The code editing mode (ours, owner 2026-11-22; `edit_code`): today's code editors' Tab,
    /// Shift+Tab, Enter, Home, BS, closing brackets.
    pub code_editing: bool,
}

impl Default for EditOptions {
    fn default() -> Self {
        Self {
            persistent_blocks: false,
            del_removes_blocks: true,
            auto_indent: false,
            expand_tabs: 0,
            cursor_beyond_eol: true,
            select_found: false,
            cursor_at_end: false,
            code_editing: false,
        }
    }
}
