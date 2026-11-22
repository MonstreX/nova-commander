//! Portable settings: `settings\settings.json` next to the executable.
//! No installer, no registry, no user profile: copying the folder copies the configuration.
//! Keys are flat and dotted, as in VS Code's settings.json. No UI dependencies.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::vfs::local::ops;

mod code_pages;
mod defaults;
mod dialogs;
mod editor;
mod find;
mod highlight;
pub mod history;
mod info_panel;
mod panel_modes;
mod panels;
mod plugins;
pub mod positions;
mod syntax_types;
mod tree;
pub mod user_menu;
mod viewer;

pub use code_pages::CodePageSettings;
pub use dialogs::DialogSwitches;
pub use editor::EditorSettings;
pub use find::FindSettings;
pub use highlight::{HighlightColors, HighlightGroup, HighlightGroups};
pub use info_panel::InfoPanelSettings;
pub use panel_modes::{PanelMode, PanelModes};
pub use panels::PanelSwitches;
pub use plugins::{plugin_enabled, plugin_logs_dir};
pub use syntax_types::SyntaxTypeSettings;
pub use tree::{TreeOptions, tree_cache_dir};
pub use viewer::ViewerSettings;

pub const SETTINGS_DIR: &str = "settings";
pub const SETTINGS_FILE: &str = "settings.json";

/// A command of an association, and whether it is used (FAR's check box beside it).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AssociationCommand {
    pub command: String,
    pub on: bool,
}

/// FAR's file association: masks, a description, a command for each way a file is opened.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Association {
    pub masks: String,
    pub description: String,
    /// Enter.
    pub execute: AssociationCommand,
    /// Ctrl+PgDn.
    pub alt_execute: AssociationCommand,
    /// F3.
    pub view: AssociationCommand,
    /// Alt+F3.
    pub alt_view: AssociationCommand,
    /// F4.
    pub edit: AssociationCommand,
    /// Alt+F4.
    pub alt_edit: AssociationCommand,
}

/// A folder of a folder shortcut (FAR's `Shortcuts` key: `Shortcut<n>` and `Name<n>`). A shortcut with
/// no folder is a line of its menu: `--` a separator, another name its title.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderShortcut {
    /// Shown instead of the folder; `&` marks its hotkey, `%NAME%` is the variable's value.
    pub name: String,
    pub folder: String,
}

/// A group of file masks (FAR's `Masks` key): `<name>` in a mask stands for its masks.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MaskGroup {
    pub name: String,
    pub masks: String,
}

/// How a filter is marked in each area (FAR's `Flags`): empty, `+`, `-`, `I` or `X`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FilterMarks {
    pub left_panel: String,
    pub right_panel: String,
    pub find_file: String,
    pub copy: String,
    pub select: String,
    pub custom: String,
}

/// A filter of the user's (FAR's `Filters\Filter<n>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Filter {
    pub title: String,
    pub use_mask: bool,
    pub mask: String,
    pub use_size: bool,
    /// FAR's size strings: digits and K, M, G... (`vfs::filter::parse_size`).
    pub size_above: String,
    pub size_below: String,
    pub use_date: bool,
    /// modified | created | accessed | changed.
    pub date_type: String,
    /// The bounds as seconds before now (relative), else as seconds since 1970; 0 is no bound.
    pub date_relative: bool,
    pub date_after: u64,
    pub date_before: u64,
    pub use_attributes: bool,
    /// Windows' `FILE_ATTRIBUTE_*` bits that must be set and that must be clear.
    pub attributes_set: u32,
    pub attributes_clear: u32,
    pub use_hard_links: bool,
    pub marks: FilterMarks,
}

impl Default for Filter {
    /// FAR's `FileFilterParams`: the mask `*`, used.
    fn default() -> Self {
        Self {
            title: String::new(),
            use_mask: true,
            mask: "*".to_string(),
            use_size: false,
            size_above: String::new(),
            size_below: String::new(),
            use_date: false,
            date_type: "modified".to_string(),
            date_relative: false,
            date_after: 0,
            date_before: 0,
            use_attributes: false,
            attributes_set: 0,
            attributes_clear: 0,
            use_hard_links: false,
            marks: FilterMarks::default(),
        }
    }
}

/// A file type's filter that stays marked (FAR's `Filters\PanelMask<n>`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TypeFilter {
    pub mask: String,
    pub marks: FilterMarks,
}

/// One of the user's own shells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserShell {
    pub name: String,
    pub program: String,
    #[serde(default)]
    pub arguments: String,
    /// How it speaks: cmd | powershell | msys | wsl.
    #[serde(rename = "type", default)]
    pub kind: String,
}

/// FAR's default columns of Find file's results (`SearchOutFormat`, `SearchOutFormatWidth`).
pub const FIND_COLUMN_TYPES: &str = "D,S,A";
pub const FIND_COLUMN_WIDTHS: &str = "0,0,0";

/// Missing keys take their default value, so a partial or older file keeps working.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// cmd | powershell | pwsh | wsl | gitbash | `user:` and the name of one of `terminal.userShells`.
    #[serde(rename = "terminal.shell")]
    pub shell: String,
    /// The user's own shells for the guest (Options > Command line settings > Shells).
    #[serde(rename = "terminal.userShells")]
    pub user_shells: Vec<UserShell>,
    /// FAR's file associations (Commands > File associations), in their order.
    #[serde(rename = "files.associations")]
    pub associations: Vec<Association>,
    /// FAR's folder shortcuts: the folders of RCtrl+0, RCtrl+1, ... in their order.
    #[serde(rename = "panels.folderShortcuts")]
    pub folder_shortcuts: Vec<Vec<FolderShortcut>>,
    /// FAR's groups of file masks (Options > Groups of file masks); none: FAR's default ones.
    #[serde(rename = "files.maskGroups")]
    pub mask_groups: Vec<MaskGroup>,
    /// FAR's filters (Ctrl+I): the user's, the "Folders" one's marks, the file types' kept marked.
    #[serde(rename = "filters.list")]
    pub filters: Vec<Filter>,
    #[serde(rename = "filters.folders")]
    pub folders_filter: FilterMarks,
    #[serde(rename = "filters.types")]
    pub type_filters: Vec<TypeFilter>,
    /// Alt+F7's dialog as it was left (`settings::find`).
    #[serde(flatten)]
    pub find: FindSettings,
    /// The code page menu's favourites, names and mode (`settings::code_pages`).
    #[serde(flatten)]
    pub code_pages: CodePageSettings,
    /// The syntax type menu's favourites and hotkeys (`settings::syntax_types`).
    #[serde(flatten)]
    pub syntax_types: SyntaxTypeSettings,
    /// The viewer's settings (`settings::viewer`).
    #[serde(flatten)]
    pub viewer: ViewerSettings,
    /// The editor's settings (`settings::editor`).
    #[serde(flatten)]
    pub editor: EditorSettings,
    /// Rows the panels are made shorter by to show terminal lines (FAR's Ctrl+Up/Ctrl+Down).
    #[serde(rename = "panels.heightDecrement")]
    pub height_decrement: u16,
    /// Columns the left panel is made narrower by (FAR's Ctrl+Left / Ctrl+Right); negative: wider.
    #[serde(rename = "panels.widthDecrement")]
    pub width_decrement: i16,
    /// The key bar at the bottom (Ctrl+B).
    #[serde(rename = "screen.keyBar")]
    pub show_key_bar: bool,
    /// The theme (Options > Themes, ADR-0008): a built-in one or a file in the themes folder.
    #[serde(rename = "screen.theme")]
    pub theme: String,
    /// FAR's Options > Colors > "Classic palette": the console's 16 colours are set to the theme's.
    #[serde(rename = "screen.consolePalette")]
    pub console_palette: bool,
    /// FAR's Interface > "Mouse": the program takes the mouse; the terminal's selection is Shift+drag.
    #[serde(rename = "interface.mouse")]
    pub mouse: bool,
    /// FAR's Interface > "Clock".
    #[serde(rename = "interface.clock")]
    pub clock: bool,
    /// FAR's System > "Save commands history".
    #[serde(rename = "system.saveHistory")]
    pub save_history: bool,
    /// FAR's System > "Save folders history", "Save view and edit history".
    #[serde(rename = "system.saveFoldersHistory")]
    pub save_folders_history: bool,
    #[serde(rename = "system.saveViewHistory")]
    pub save_view_history: bool,
    /// FAR's System > "Automatic update of environment variables" (`UpdateEnvironment`, off).
    #[serde(rename = "system.updateEnvironment")]
    pub update_environment: bool,
    /// FAR's Panel settings.
    #[serde(rename = "panels.highlight")]
    pub highlight: bool,
    /// FAR's files highlighting and sort groups; none: FAR's default ones.
    #[serde(rename = "files.highlighting")]
    pub highlighting: Option<HighlightGroups>,
    /// FAR's file panel modes; none: FAR's own.
    #[serde(rename = "panels.modes")]
    pub panel_modes: Option<PanelModes>,
    #[serde(rename = "panels.tree")]
    pub tree: TreeOptions,
    #[serde(rename = "panels.selectFolders")]
    pub select_folders: bool,
    #[serde(rename = "panels.allowReverseSort")]
    pub allow_reverse_sort: bool,
    #[serde(rename = "panels.rightClickSelect")]
    pub right_click_select: bool,
    /// FAR's Dialog settings > "Mouse click outside a dialog closes it".
    #[serde(rename = "dialogs.clickOutside")]
    pub click_outside_dialog: bool,
    /// FAR's Menu settings: cancel | apply | ignore.
    #[serde(rename = "menus.leftClick")]
    pub menu_left_click: String,
    #[serde(rename = "menus.rightClick")]
    pub menu_right_click: String,
    #[serde(rename = "menus.middleClick")]
    pub menu_middle_click: String,
    /// FAR's AutoComplete > "Show a list".
    #[serde(rename = "commandLine.completionList")]
    pub completion_list: bool,
    /// FAR's Confirmations.
    #[serde(rename = "confirmations.interrupt")]
    pub confirm_interrupt: bool,
    /// FAR's Confirmations > "Drag and drop" (`Confirm.Drag`): a drop shows the copy or move dialog.
    #[serde(rename = "confirmations.drag")]
    pub confirm_drag: bool,
    #[serde(rename = "confirmations.exit")]
    pub confirm_exit: bool,
    #[serde(rename = "confirmations.removeConnection")]
    pub confirm_connection: bool,
    #[serde(rename = "confirmations.removeSubst")]
    pub confirm_subst: bool,
    #[serde(rename = "confirmations.detachVirtual")]
    pub confirm_virtual: bool,
    #[serde(rename = "confirmations.removeHotPlug")]
    pub confirm_hotplug: bool,
    #[serde(rename = "confirmations.historyClear")]
    pub confirm_history_clear: bool,
    /// FAR's `Confirm.AllowReedit`: a file already in an editor asks how to open it again.
    #[serde(rename = "confirmations.allowReedit")]
    pub confirm_reedit: bool,
    /// Ctrl+F's cases, Ctrl+PgUp, the scroll bar, what each side shows (`settings::panels`).
    #[serde(flatten)]
    pub panel_switches: PanelSwitches,
    /// The dialogs' edit histories, the menus' scroll bar (`settings::dialogs`).
    #[serde(flatten)]
    pub dialog_switches: DialogSwitches,
    /// The info panel's sections and description files (`infoPanel.*`).
    #[serde(flatten)]
    pub info_panel: InfoPanelSettings,
    /// Keys this program does not name itself: the plugins' (`plugins.<name>.enabled`), and any other the
    /// file has, kept as they are when it is written again.
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
    #[serde(rename = "panels.left.visible")]
    pub left_visible: bool,
    #[serde(rename = "panels.right.visible")]
    pub right_visible: bool,
    /// Hidden and system files are listed (Ctrl+H).
    #[serde(rename = "panels.showHidden")]
    pub show_hidden: bool,
    /// FAR's "Disable automatic panel update if object count exceeds": 0 updates panels of any size.
    #[serde(rename = "panels.autoUpdateLimit")]
    pub auto_update_limit: usize,
    /// FAR's "Network drives autorefresh": folders on network drives are watched for changes too.
    #[serde(rename = "panels.autoUpdateRemoteDrives")]
    pub auto_update_remote_drives: bool,
    /// The seconds a task that ended cleanly stays in the Screens list and the status row (ADR-0010).
    #[serde(rename = "tasks.keepFinishedSeconds")]
    pub keep_finished_tasks: u64,
    /// FAR's System > "Delete to Recycle Bin".
    #[serde(rename = "system.deleteToRecycleBin")]
    pub delete_to_recycle_bin: bool,
    /// FAR's `System.MultiMakeDir`: F7's "Process multiple names" as last left.
    #[serde(rename = "system.multiMakeDir")]
    pub multi_make_dir: bool,
    /// FAR's Confirmations > "Delete".
    #[serde(rename = "confirmations.delete")]
    pub confirm_delete: bool,
    /// FAR's Confirmations > "Delete non-empty folders".
    #[serde(rename = "confirmations.deleteFolder")]
    pub confirm_delete_folder: bool,
    /// FAR's Confirmations > "Delete read-only files".
    #[serde(rename = "confirmations.readOnly")]
    pub confirm_read_only: bool,
    /// FAR's `DelShowTotal`: everything to delete is counted, beside the deletion, for the total bar.
    /// On by default (owner; FAR's is off).
    #[serde(rename = "delete.showTotal")]
    pub delete_show_total: bool,
    /// FAR's `DelShowSelected`: how many names the question before deleting lists.
    #[serde(rename = "delete.showSelected")]
    pub delete_show_selected: usize,
    /// What the drive menu (Alt+F1 / Alt+F2) shows: type, path, label, shellName, fileSystem, size,
    /// sizeFloat, removable, cd, network, plugins, sortPluginsByHotkey, virtual, unmounted.
    #[serde(rename = "driveMenu.show")]
    pub drive_menu: Vec<String>,
    /// left | right
    #[serde(rename = "panels.active")]
    pub active_panel: String,
    /// Empty means "the directory the program was started in".
    #[serde(rename = "panels.left.path")]
    pub left_path: String,
    /// name | nameonly | extension | time | size | unsorted | creation | access | change | allocated
    #[serde(rename = "panels.left.sort")]
    pub left_sort: String,
    /// Missing (older files): the mode's default direction.
    #[serde(rename = "panels.left.sortReverse")]
    pub left_sort_reverse: Option<bool>,
    #[serde(rename = "panels.left.directoriesFirst")]
    pub left_directories_first: bool,
    #[serde(rename = "panels.left.selectedFirst")]
    pub left_selected_first: bool,
    #[serde(rename = "panels.left.sortGroups")]
    pub left_sort_groups: bool,
    /// brief | medium | full | wide | detailed | alternative
    #[serde(rename = "panels.left.view")]
    pub left_view: String,
    #[serde(rename = "panels.right.path")]
    pub right_path: String,
    #[serde(rename = "panels.right.sort")]
    pub right_sort: String,
    #[serde(rename = "panels.right.sortReverse")]
    pub right_sort_reverse: Option<bool>,
    #[serde(rename = "panels.right.directoriesFirst")]
    pub right_directories_first: bool,
    #[serde(rename = "panels.right.selectedFirst")]
    pub right_selected_first: bool,
    #[serde(rename = "panels.right.sortGroups")]
    pub right_sort_groups: bool,
    #[serde(rename = "panels.right.view")]
    pub right_view: String,
}

/// `<folder of the executable>\settings\settings.json`.
pub fn default_path() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "the executable has no parent folder",
        )
    })?;
    Ok(dir.join(SETTINGS_DIR).join(SETTINGS_FILE))
}

/// The user's theme files (ADR-0008): `themes` next to the settings file.
pub fn themes_dir(settings_path: &Path) -> Option<PathBuf> {
    Some(settings_path.parent()?.join("themes"))
}

/// Reads settings; a missing file gives the defaults, a broken file is an `InvalidData` error.
pub fn load(path: &Path) -> io::Result<Settings> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(error) => return Err(error),
    };
    // Notepad and PowerShell 5 may save UTF-8 with a BOM, which JSON parsers reject.
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    serde_json::from_str(text).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {error}", path.display()),
        )
    })
}

/// Writes settings as pretty JSON, creating the settings folder; the old file survives a failed write.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    text.push('\n');
    ops::write_atomic(path, text.as_bytes())
}

/// "Save setup": writes the settings unless the file failed to load at start, so a broken file the
/// user may still want to repair is never replaced. Returns whether the file was written.
pub fn save_unless_broken(
    path: &Path,
    settings: &Settings,
    loaded_cleanly: bool,
) -> io::Result<bool> {
    if !loaded_cleanly {
        return Ok(false);
    }
    save(path, settings)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_setup_writes_a_cleanly_loaded_file_and_keeps_a_broken_one() {
        let temp = tempfile::tempdir().unwrap();
        let good = temp.path().join("good.json");
        let settings = Settings {
            shell: "pwsh".to_string(),
            ..Settings::default()
        };
        assert!(save_unless_broken(&good, &settings, true).unwrap());
        assert_eq!(load(&good).unwrap(), settings);

        let broken = temp.path().join("broken.json");
        fs::write(&broken, "{ not json").unwrap();
        assert!(load(&broken).is_err());
        assert!(!save_unless_broken(&broken, &settings, false).unwrap());
        assert_eq!(fs::read(&broken).unwrap(), b"{ not json");
    }

    #[test]
    fn missing_file_gives_defaults() {
        let temp = tempfile::tempdir().unwrap();
        let settings = load(&temp.path().join("none.json")).unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn save_creates_folder_and_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(SETTINGS_DIR).join(SETTINGS_FILE);
        let settings = Settings {
            shell: "pwsh".to_string(),
            height_decrement: 9,
            left_path: "C:\\Users".to_string(),
            ..Settings::default()
        };

        save(&path, &settings).unwrap();

        assert_eq!(load(&path).unwrap(), settings);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"terminal.shell\": \"pwsh\""));
    }

    #[test]
    fn partial_file_fills_in_defaults() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("s.json");
        fs::write(
            &path,
            r#"{ "panels.heightDecrement": 3, "unknown.key": true }"#,
        )
        .unwrap();

        let settings = load(&path).unwrap();

        assert_eq!(settings.height_decrement, 3);
        assert_eq!(settings.shell, "cmd");
    }

    #[test]
    fn broken_file_is_reported_as_invalid_data() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("s.json");
        fs::write(&path, "{ not json").unwrap();

        let error = load(&path).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn file_with_utf8_bom_is_accepted() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("s.json");
        fs::write(&path, "\u{feff}{ \"panels.heightDecrement\": 2 }").unwrap();

        assert_eq!(load(&path).unwrap().height_decrement, 2);
    }
}
