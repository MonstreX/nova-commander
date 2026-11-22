//! The viewer's settings (FAR's `ViewerOptions`, "Viewer settings"; S2d.6a): flat keys under `viewer.`
//! in the settings file. Values out of FAR's ranges fall back as FAR's validators make them.

use serde::{Deserialize, Serialize};

/// FAR's `DefaultTabSize`.
pub const DEFAULT_TAB: usize = 8;
/// FAR's `ViewerOptions::eDefLineSize`, `eMinLineSize` and `eMaxLineSize`.
pub const DEFAULT_LINE: usize = 10_000;
pub const MIN_LINE: usize = 1_000;
pub const MAX_LINE: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewerSettings {
    /// FAR's `PersistentBlocks`: what a search found stays marked over other keys.
    #[serde(rename = "viewer.persistentSelection")]
    pub persistent_selection: bool,
    /// FAR's `TabSize`.
    #[serde(rename = "viewer.tabSize")]
    pub tab_size: usize,
    /// FAR's `MaxLineSize`: "Maximum line width".
    #[serde(rename = "viewer.maxLineWidth")]
    pub max_line_width: usize,
    /// The text mode's syntax highlighting (ADR-0024; FAR's viewer has none).
    #[serde(rename = "viewer.syntaxHighlighting")]
    pub highlighting: bool,
    /// FAR's `ShowArrows`: `«` and `»` where a line goes on past the screen.
    #[serde(rename = "viewer.showArrows")]
    pub show_arrows: bool,
    /// FAR's `ShowScrollbar` (Ctrl+S in the viewer).
    #[serde(rename = "viewer.showScrollbar")]
    pub show_scrollbar: bool,
    /// FAR's `Visible0x00`: a NUL shown as FAR's `ZeroChar` `·`.
    #[serde(rename = "viewer.visibleNul")]
    pub visible_nul: bool,
    /// FAR's `UseExternalViewer`: F3 runs the external viewer, Alt+F3 the program's.
    #[serde(rename = "viewer.useExternal")]
    pub use_external: bool,
    /// FAR's `ExternalViewerName`: the external viewer's command (its `!.!` the file).
    #[serde(rename = "viewer.externalCommand")]
    pub external_command: String,
    /// FAR's `AutoDetectCodePage`: UTF-8 and UTF-16 told by their bytes; off, by their mark alone.
    #[serde(rename = "viewer.autoDetectCodePage")]
    pub auto_detect: bool,
    /// FAR's `DefaultCodePage`: a file's code page when nothing tells it (0: ANSI, FAR's `CP_DEFAULT`).
    #[serde(rename = "viewer.defaultCodePage")]
    pub default_page: u32,
    /// FAR's `DetectDumpMode`: a file with a NUL in its first 2 KB opened in dump mode.
    #[serde(rename = "viewer.detectDumpMode")]
    pub detect_dump: bool,
    /// FAR's `SavePos`: a file opened again where it was left (its code page too).
    #[serde(rename = "viewer.savePosition")]
    pub save_position: bool,
    /// FAR's `SaveShortPos`: Ctrl+Shift+0..9's bookmarks kept with the file.
    #[serde(rename = "viewer.saveBookmarks")]
    pub save_bookmarks: bool,
    /// FAR's `SaveCodepage`.
    #[serde(rename = "viewer.saveCodePage")]
    pub save_code_page: bool,
    /// FAR's `SaveViewMode`: the mode the user left it in.
    #[serde(rename = "viewer.saveViewMode")]
    pub save_view_mode: bool,
    /// FAR's `SaveWrapMode`: the wrapping the user left it with.
    #[serde(rename = "viewer.saveWrapMode")]
    pub save_wrap_mode: bool,
    /// FAR's `IsWrap`: long lines wrapped (F2 in text mode changes it, as FAR's).
    #[serde(rename = "viewer.wrap")]
    pub wrap: bool,
    /// FAR's `Wrap`: wrapped at the words (Shift+F2 changes it, as FAR's).
    #[serde(rename = "viewer.wordWrap")]
    pub word_wrap: bool,
    /// FAR's `ShowKeyBar` (Ctrl+B in the viewer).
    #[serde(rename = "viewer.showKeyBar")]
    pub show_key_bar: bool,
    /// FAR's `ShowTitleBar`: the status line (Ctrl+Shift+B in the viewer).
    #[serde(rename = "viewer.showTitleBar")]
    pub show_title_bar: bool,
}

impl ViewerSettings {
    /// Whether anything of a file's place is kept (FAR's `SavePosition`'s test).
    pub fn keeps_positions(&self) -> bool {
        self.save_position
            || self.save_bookmarks
            || self.save_code_page
            || self.save_view_mode
            || self.save_wrap_mode
    }
}

impl Default for ViewerSettings {
    fn default() -> Self {
        Self {
            persistent_selection: true,
            tab_size: DEFAULT_TAB,
            max_line_width: DEFAULT_LINE,
            highlighting: true,
            show_arrows: true,
            show_scrollbar: false,
            visible_nul: false,
            use_external: false,
            external_command: String::new(),
            auto_detect: true,
            default_page: 0,
            detect_dump: true,
            save_position: true,
            save_code_page: true,
            save_bookmarks: true,
            save_view_mode: true,
            save_wrap_mode: false,
            wrap: true,
            word_wrap: false,
            show_key_bar: true,
            show_title_bar: true,
        }
    }
}

impl ViewerSettings {
    /// The tab size as FAR's validator takes it: 1 to 512, else the default.
    pub fn tab(&self) -> usize {
        if (1..=512).contains(&self.tab_size) {
            self.tab_size
        } else {
            DEFAULT_TAB
        }
    }

    /// The longest line as FAR's validator takes it: within 1 000 and 100 000 (0: the default).
    pub fn max_line(&self) -> usize {
        match self.max_line_width {
            0 => DEFAULT_LINE,
            width => width.clamp(MIN_LINE, MAX_LINE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_out_of_far_s_ranges_fall_back_as_its_validators_make_them() {
        let mut settings = ViewerSettings::default();
        assert_eq!((settings.tab(), settings.max_line()), (8, 10_000));
        settings.tab_size = 0;
        settings.max_line_width = 50;
        assert_eq!((settings.tab(), settings.max_line()), (8, 1_000));
        settings.tab_size = 4;
        settings.max_line_width = 500_000;
        assert_eq!((settings.tab(), settings.max_line()), (4, 100_000));
    }
}
