//! What an extraction does (arclite's `ExtractOptions`) and what of it is kept (its `extract_*` options
//! under their names: Ignore errors, the overwrite answer, a separate folder, going to the folder), as
//! `Plugin::extract` starts them for F5 / F6 / Alt+F6 and for the viewer or the editor.

use std::path::PathBuf;

use nc_plugin_sdk::{GetMode, Host};

/// arclite's `OverwriteAction` (its numbers as kept).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overwrite {
    Ask = 0,
    Overwrite = 1,
    Skip = 2,
    Rename = 3,
    Append = 4,
    /// Overwritten under the archive's case of the name (arclite's `strict_case`).
    OverwriteCase = 5,
}

impl Overwrite {
    pub(crate) fn of(number: i64) -> Self {
        match number {
            1 => Self::Overwrite,
            2 => Self::Skip,
            3 => Self::Rename,
            4 => Self::Append,
            5 => Self::OverwriteCase,
            _ => Self::Ask,
        }
    }
}

/// arclite's `TriState` (its numbers as kept): a three-state check box's `[x]`, `[ ]`, `[?]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tri {
    True = 0,
    False = 1,
    Undef = 2,
}

impl Tri {
    fn of(number: i64) -> Self {
        match number {
            0 => Self::True,
            1 => Self::False,
            _ => Self::Undef,
        }
    }

    /// A three-state check box's state (`DialogItem::check3`).
    pub fn check(self) -> Option<bool> {
        match self {
            Self::True => Some(true),
            Self::False => Some(false),
            Self::Undef => None,
        }
    }

    /// The state a three-state check box closed with (0 `[ ]`, 1 `[x]`, 2 `[?]`).
    pub fn of_state(state: i32) -> Self {
        match state {
            0 => Self::False,
            1 => Self::True,
            _ => Self::Undef,
        }
    }
}

/// arclite's `extract_*` options, kept by "Save settings".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractSettings {
    pub ignore_errors: bool,
    pub overwrite: Overwrite,
    pub separate_dir: Tri,
    pub open_dir: bool,
}

impl Default for ExtractSettings {
    /// arclite's defaults.
    fn default() -> Self {
        Self {
            ignore_errors: false,
            overwrite: Overwrite::Ask,
            separate_dir: Tri::Undef,
            open_dir: false,
        }
    }
}

impl ExtractSettings {
    pub fn load(host: &Host) -> Self {
        let defaults = Self::default();
        Self {
            ignore_errors: host.bool_setting("extract_ignore_errors", defaults.ignore_errors),
            overwrite: Overwrite::of(host.int_setting("extract_overwrite", 0)),
            separate_dir: Tri::of(host.int_setting("extract_separate_dir", 2)),
            open_dir: host.bool_setting("extract_open_dir", defaults.open_dir),
        }
    }

    pub fn save(&self, host: &Host) {
        host.set_bool("extract_ignore_errors", self.ignore_errors);
        host.set_int("extract_overwrite", self.overwrite as i64);
        host.set_int("extract_separate_dir", self.separate_dir as i64);
        host.set_bool("extract_open_dir", self.open_dir);
    }
}

/// One extraction (arclite's `ExtractOptions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractOptions {
    pub dst_dir: PathBuf,
    pub ignore_errors: bool,
    pub overwrite: Overwrite,
    /// The files deleted from the archive after; `Undef` where the archive cannot be changed.
    pub move_files: Tri,
    pub password: String,
    pub separate_dir: Tri,
    pub delete_archive: bool,
    /// No file on a disk to delete (an archive inside another).
    pub disable_delete_archive: bool,
    /// `Undef`: not offered (only for archives of a disk's panel at once).
    pub open_dir: Tri,
}

impl ExtractOptions {
    /// arclite's `Plugin::extract` before its dialog: the kept answers for its dialog; without one (the
    /// viewer, the editor, Find file, the quick view) every file overwritten, no folder of its own;
    /// errors let be for Find file and the quick view. Moving is not offered: no archive can be changed
    /// before S6.2 writes zip (arclite's `updatable()`), so F6 extracts as F5 does.
    pub fn starting(destination: PathBuf, mode: GetMode, settings: &ExtractSettings) -> Self {
        let dialog = mode.with_dialog();
        let (overwrite, separate_dir) = if dialog {
            (settings.overwrite, settings.separate_dir)
        } else {
            (Overwrite::Overwrite, Tri::False)
        };
        let open_dir = match (dialog, mode.top_level) {
            (true, true) if settings.open_dir => Tri::True,
            (true, true) => Tri::False,
            _ => Tri::Undef,
        };
        Self {
            dst_dir: destination,
            ignore_errors: mode.find || mode.quick_view || settings.ignore_errors,
            overwrite,
            move_files: Tri::Undef,
            password: String::new(),
            separate_dir,
            delete_archive: false,
            disable_delete_archive: false,
            open_dir,
        }
    }
}

#[cfg(test)]
#[path = "extract_options_tests.rs"]
mod tests;
