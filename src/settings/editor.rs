//! The editor's settings (FAR's `EditorOptions`, "Editor settings"; S2d.6c): flat keys under `editor.`
//! in the settings file, FAR's defaults.

use serde::{Deserialize, Serialize};

use super::viewer::DEFAULT_TAB;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    /// FAR's `UseExternalEditor`: F4 runs the external editor, Alt+F4 the program's.
    #[serde(rename = "editor.useExternal")]
    pub use_external: bool,
    /// FAR's `ExternalEditorName`: the external editor's command (its `!.!` the file).
    #[serde(rename = "editor.externalCommand")]
    pub external_command: String,
    /// FAR's `TabSize`.
    #[serde(rename = "editor.tabSize")]
    pub tab_size: usize,
    /// FAR's `AutoDetectCodePage`.
    #[serde(rename = "editor.autoDetectCodePage")]
    pub auto_detect: bool,
    /// FAR's `DefaultCodePage` (0: ANSI, FAR's `CP_DEFAULT`).
    #[serde(rename = "editor.defaultCodePage")]
    pub default_page: u32,
    /// The syntax highlighting (ADR-0024; FAR's Colorer's switch).
    #[serde(rename = "editor.syntaxHighlighting")]
    pub highlighting: bool,
    /// FAR's `ExpandTabs`: 0 do not, 1 newly entered tabs, 2 all tabs.
    #[serde(rename = "editor.expandTabs")]
    pub expand_tabs: u8,
    /// FAR's `PersistentBlocks`.
    #[serde(rename = "editor.persistentBlocks")]
    pub persistent_blocks: bool,
    /// FAR's `DelRemovesBlocks`.
    #[serde(rename = "editor.delRemovesBlocks")]
    pub del_removes_blocks: bool,
    /// FAR's `AutoIndent`.
    #[serde(rename = "editor.autoIndent")]
    pub auto_indent: bool,
    /// FAR's `CursorBeyondEOL`.
    #[serde(rename = "editor.cursorBeyondEndOfLine")]
    pub cursor_beyond_eol: bool,
    /// FAR's `SearchSelFound`.
    #[serde(rename = "editor.selectFound")]
    pub select_found: bool,
    /// FAR's `SearchCursorAtEnd`.
    #[serde(rename = "editor.cursorAtEnd")]
    pub cursor_at_end: bool,
    /// FAR's `ShowWhiteSpace`: spaces, tabs and line breaks shown.
    #[serde(rename = "editor.showWhiteSpace")]
    pub show_white_space: bool,
    /// FAR's `ShowScrollBar`.
    #[serde(rename = "editor.showScrollbar")]
    pub show_scrollbar: bool,
    /// FAR's `ShowLineNumbers` (Ctrl+F3 in the editor).
    #[serde(rename = "editor.showLineNumbers")]
    pub show_line_numbers: bool,
    /// FAR's `SaveEditorPos`: the cursor, the screen and the code page kept for the file.
    #[serde(rename = "editor.saveFilePosition")]
    pub save_position: bool,
    /// FAR's `SaveEditorShortPos`: Ctrl+Shift+0..9's bookmarks kept for the file.
    #[serde(rename = "editor.saveBookmarks")]
    pub save_bookmarks: bool,
    /// FAR's `ReadOnlyLock` 0b01: a read-only file opens in lock mode.
    #[serde(rename = "editor.lockReadOnlyFiles")]
    pub read_only_lock: bool,
    /// FAR's `ReadOnlyLock` 0b10: "Do you wish to edit it?" before a read-only file opens.
    #[serde(rename = "editor.warnReadOnlyFiles")]
    pub read_only_warn: bool,
    /// FAR's `ShowKeyBar` (Ctrl+B in the editor).
    #[serde(rename = "editor.showKeyBar")]
    pub show_key_bar: bool,
    /// FAR's `ShowTitleBar`: the status line (Ctrl+Shift+B in the editor).
    #[serde(rename = "editor.showTitleBar")]
    pub show_title_bar: bool,
    /// The code editing mode (ours, owner 2026-11-22): today's code editors' Tab / Shift+Tab, Enter,
    /// Home, BS, brackets, Ctrl+/ and Alt+Up / Alt+Down; off: FAR's keys.
    #[serde(rename = "editor.codeEditing")]
    pub code_editing: bool,
    /// The code mode's ruler: a column (from 1) marked down the screen; 0 none.
    #[serde(rename = "editor.rulerColumn")]
    pub ruler_column: usize,
    /// The code mode's save: the blanks at the lines' ends taken out and a last line given its
    /// break.
    #[serde(rename = "editor.trimOnSave")]
    pub trim_on_save: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            use_external: false,
            external_command: String::new(),
            tab_size: DEFAULT_TAB,
            auto_detect: true,
            default_page: 0,
            highlighting: true,
            expand_tabs: 0,
            persistent_blocks: false,
            del_removes_blocks: true,
            auto_indent: false,
            cursor_beyond_eol: true,
            select_found: false,
            cursor_at_end: false,
            show_white_space: false,
            show_scrollbar: false,
            show_line_numbers: false,
            save_position: true,
            save_bookmarks: true,
            read_only_lock: false,
            read_only_warn: false,
            show_key_bar: true,
            show_title_bar: true,
            code_editing: false,
            ruler_column: 0,
            trim_on_save: true,
        }
    }
}

impl EditorSettings {
    /// The tab size as FAR's validator takes it: 1 to 512, else the default.
    pub fn tab(&self) -> usize {
        if (1..=512).contains(&self.tab_size) {
            self.tab_size
        } else {
            DEFAULT_TAB
        }
    }
}
