//! `App`'s smaller states (R53): each holds the fields of one concern, so `App` keeps the screen's own.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use super::{
    InfoSections, MenuState, Side, delete, drive_actions, drive_menu, mask_groups, persist,
};
use crate::jobs::tasks::TaskId;
use crate::settings::Settings;

/// What the settings give the app's work, as Options last set it.
pub(super) struct Config {
    /// FAR's file associations.
    pub(super) associations: Vec<crate::settings::Association>,
    /// FAR's groups of file masks, in the order of their names.
    pub(super) mask_groups: Vec<crate::settings::MaskGroup>,
    /// FAR's "Disable automatic panel update if object count exceeds" (0: no limit).
    pub(super) auto_update_limit: usize,
    /// FAR's "Network drives autorefresh".
    pub(super) auto_update_remote_drives: bool,
    /// FAR's settings for F8 / Shift+Del.
    pub(super) delete_settings: delete::DeleteSettings,
    /// What the drive menu shows (FAR's `DriveMenuMode`).
    pub(super) drive_columns: crate::vfs::local::DriveColumns,
    /// The info panel's names' formats and CD parameters (Options > InfoPanel settings).
    pub(super) info_options: crate::vfs::local::InfoOptions,
    /// FAR's "Folder description files".
    pub(super) description_files: String,
}

impl Config {
    pub(super) fn saved(settings: &Settings) -> Self {
        Self {
            associations: settings.associations.clone(),
            mask_groups: mask_groups::restored(&settings.mask_groups),
            auto_update_limit: settings.auto_update_limit,
            auto_update_remote_drives: settings.auto_update_remote_drives,
            delete_settings: delete::DeleteSettings::saved(settings),
            drive_columns: drive_menu::columns_from_ids(&settings.drive_menu),
            info_options: persist::info_options(settings),
            description_files: settings.info_panel.description_files.clone(),
        }
    }
}

/// The tasks' side of the screen (the tasks themselves are `App::tasks`).
pub(super) struct Work {
    /// The panel whose marks a task takes off as it finishes selected objects; of several
    /// destinations only the last one does (FAR's `FCOPY_COPYLASTTIME`).
    pub(super) task_marks: HashMap<TaskId, Side>,
    /// The count of files and folders in the open question before deleting.
    pub(super) delete_count: Option<delete::QuestionCount>,
    /// The "Set" the drive menu's F4 started: the menu comes back when it ends in front.
    pub(super) attributes_menu: Option<(TaskId, drive_actions::MenuPlace)>,
    /// F10 > Wait: quit once no task runs.
    pub(super) quit_when_idle: bool,
    /// A task ended since the event loop last looked.
    pub(super) task_ended_signal: bool,
    /// When a running task's progress was last drawn (`pumps`).
    pub(super) progress_drawn: Instant,
}

impl Default for Work {
    fn default() -> Self {
        Self {
            task_marks: HashMap::new(),
            delete_count: None,
            attributes_menu: None,
            quit_when_idle: false,
            task_ended_signal: false,
            progress_drawn: Instant::now(),
        }
    }
}

/// What the session keeps for the next time it is asked.
pub(super) struct Remembered {
    /// The folder the panels were last in on each drive (FAR's per-drive current folder), by
    /// [`drive_menu::root_key`].
    pub(super) drive_folders: HashMap<String, PathBuf>,
    /// The last mask of the Select/Deselect dialogs (FAR keeps it for the session).
    pub(super) last_mask: String,
    /// The item of the menu bar chosen last (FAR's `LastHItem`, `LastVItem`), for Shift+F10.
    pub(super) last_menu_item: Option<MenuState>,
    /// The info panel's unfolded sections, kept while it is off.
    pub(super) info_sections: InfoSections,
    /// Which panels Ctrl+O shows again (FAR's `m_StateBeforeHide`).
    pub(super) panels_before_hide: [bool; 2],
}

impl Remembered {
    pub(super) fn saved(settings: &Settings) -> Self {
        Self {
            drive_folders: HashMap::new(),
            last_mask: "*.*".to_string(),
            last_menu_item: None,
            info_sections: persist::info_sections(settings),
            panels_before_hide: [true, true],
        }
    }
}
