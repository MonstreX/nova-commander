//! What a plugin asks of the editor (API 10, FAR's `EditorControl`) as the program reads it, and what
//! it answers. Lines by their numbers from 0 (none: the cursor's), positions in characters.

/// [`EditorInfo::options`] and [`EditorInfo::state`]: FAR's `EOPT_*` and `ECSTATE_*` as the ABI has them.
pub use nc_plugin_api::{
    OPTION_AUTO_INDENT, OPTION_CURSOR_BEYOND_EOL, OPTION_DEL_REMOVES_BLOCKS,
    OPTION_EXPAND_ALL_TABS, OPTION_EXPAND_NEW_TABS, OPTION_PERSISTENT_BLOCKS,
    OPTION_SHOW_LINE_NUMBERS, OPTION_SHOW_SCROLLBAR, OPTION_SHOW_WHITE_SPACE, STATE_LOCKED,
    STATE_SAVED,
};

/// A request to the editor up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorRequest {
    Info,
    GetLine(Option<u64>),
    SetLine {
        line: Option<u64>,
        text: String,
        /// The break a last line without one gets (none: the file's).
        eol: Option<String>,
    },
    /// Enter at the cursor, with the auto indent when `true`.
    InsertLine(bool),
    DeleteLine,
    InsertText(String),
    DeleteChar,
    SetPosition {
        line: Option<u64>,
        pos: Option<usize>,
        tab_pos: Option<usize>,
        top: Option<u64>,
        left: Option<usize>,
        overtype: Option<bool>,
    },
    /// The block: its kind (FAR's `BTYPE_*`), first line, start, width and lines.
    Select {
        kind: i64,
        /// None: the cursor's.
        line: Option<u64>,
        pos: i64,
        width: i64,
        height: i64,
    },
    /// FAR's `EUR_*`: 0 begin, 1 end, 2 undo, 3 redo.
    UndoRedo(i64),
    Redraw,
    ExpandTabs(Option<u64>),
    RealToTab(Option<u64>, usize),
    TabToReal(Option<u64>, usize),
    FileName,
    /// The next key pressed in the editor, answered with its name.
    ReadInput,
    /// The key of this name done by the editor.
    ProcessInput(String),
    /// The status line's title; none: the file's name.
    SetTitle(Option<String>),
    /// The key bar's labels of these keys (their names); none: the editor's own again.
    SetKeyBar(Option<Vec<(String, String)>>),
    /// The system's default sound.
    Beep,
}

/// The editor's state as a plugin reads it (FAR's `EditorInfo`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditorInfo {
    pub width: u64,
    pub height: u64,
    pub lines: u64,
    pub line: u64,
    pub pos: u64,
    pub tab_pos: u64,
    pub top: u64,
    pub left: u64,
    pub overtype: bool,
    pub block: i64,
    pub block_line: u64,
    /// FAR's `EOPT_*`, `ECSTATE_*`.
    pub options: u32,
    pub state: u32,
    pub tab_size: u64,
    pub code_page: u32,
}

/// The editor's answer; `Done(false)` when it could not do it (no editor, no such line, lock mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAnswer {
    Done(bool),
    Info(EditorInfo),
    Line {
        text: String,
        eol: String,
        block: (i64, i64),
    },
    Number(u64),
    Text(String),
}

impl super::PluginHost {
    /// The key `key` (its name) the editor has done sent to plugin `plugin`, which takes the editor's
    /// keys; its end comes as [`super::Event::Ran`] for `serial`. False when it did not reach the thread.
    pub fn send_editor_key(&self, plugin: usize, key: &str, serial: u64) -> bool {
        self.plugins.get(plugin).is_some_and(|plugin| {
            let job = super::worker::Job::EditorKey {
                key: key.to_string(),
                serial,
            };
            plugin.send_waited(serial, job)
        })
    }

    /// Command `index` of the editor's plugin commands menu of plugin `plugin` run on its thread with
    /// the editor up (API 10): what it asks of the editor comes as [`super::Event::EditorAsk`].
    pub fn run_editor_command(&self, plugin: usize, index: usize, serial: u64) -> bool {
        self.run(plugin, super::ffi::PLACE_EDITOR, index, (0, serial))
    }
}
