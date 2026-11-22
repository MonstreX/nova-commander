//! The temporary panel's options, as FAR's TmpPanel's (`TmpCfg.cpp`): read from its settings at the start,
//! at every open and before its configuration (`GetOptions`), written only on the dialog's OK, under FAR's
//! keys and with FAR's defaults. This step uses the menus' items, its own mode and the list files' mask
//! (SavLst's extension); the others are kept for the steps that use them.

use nc_plugin_sdk::{Dialog, DialogItem, Host, PanelMode, Value};

/// TmpPanel's `Opt` (`TmpCfg.hpp`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub add_to_disks_menu: bool,
    pub add_to_plugins_menu: bool,
    pub common_panel: bool,
    pub safe_mode_panel: bool,
    pub any_in_panel: bool,
    /// 0 off, 1 on, 2 ask ("Copy folder contents?").
    pub copy_contents: i64,
    /// The settings' "Mode": a list file replaces the panel's items.
    pub replace: bool,
    pub menu_for_filelist: bool,
    pub new_panel_for_search_results: bool,
    pub full_screen_panel: bool,
    /// Not in the dialog (FAR's far:config), written back on OK.
    pub list_utf8: bool,
    pub column_types: String,
    pub column_widths: String,
    pub status_column_types: String,
    pub status_column_widths: String,
    pub mask: String,
    pub prefix: String,
}

impl Default for Options {
    /// FAR's defaults (`TmpCfg.cpp:18-34`).
    fn default() -> Self {
        Self {
            add_to_disks_menu: true,
            add_to_plugins_menu: true,
            common_panel: true,
            safe_mode_panel: false,
            any_in_panel: false,
            copy_contents: 0,
            replace: true,
            menu_for_filelist: false,
            new_panel_for_search_results: false,
            full_screen_panel: false,
            list_utf8: true,
            column_types: "N,S".to_string(),
            column_widths: "0,8".to_string(),
            status_column_types: "NR,SC,D,T".to_string(),
            status_column_widths: "0,8,0,5".to_string(),
            mask: "*.temp".to_string(),
            prefix: "tmp".to_string(),
        }
    }
}

/// The flags' settings keys in FAR's order, each with its field.
const FLAGS: [&str; 10] = [
    "AddToDisksMenu",
    "AddToPluginsMenu",
    "CommonPanel",
    "SafeModePanel",
    "AnyInPanel",
    "Mode",
    "MenuForFilelist",
    "NewPanelForSearchResults",
    "FullScreenPanel",
    "ListUTF8",
];

/// The texts' settings keys in FAR's order.
const TEXTS: [&str; 6] = [
    "ColumnTypes",
    "ColumnWidths",
    "StatusColumnTypes",
    "StatusColumnWidths",
    "Mask",
    "Prefix",
];

impl Options {
    fn flag(&mut self, key: &str) -> Option<&mut bool> {
        Some(match key {
            "AddToDisksMenu" => &mut self.add_to_disks_menu,
            "AddToPluginsMenu" => &mut self.add_to_plugins_menu,
            "CommonPanel" => &mut self.common_panel,
            "SafeModePanel" => &mut self.safe_mode_panel,
            "AnyInPanel" => &mut self.any_in_panel,
            "Mode" => &mut self.replace,
            "MenuForFilelist" => &mut self.menu_for_filelist,
            "NewPanelForSearchResults" => &mut self.new_panel_for_search_results,
            "FullScreenPanel" => &mut self.full_screen_panel,
            "ListUTF8" => &mut self.list_utf8,
            _ => return None,
        })
    }

    fn text(&mut self, key: &str) -> Option<&mut String> {
        Some(match key {
            "ColumnTypes" => &mut self.column_types,
            "ColumnWidths" => &mut self.column_widths,
            "StatusColumnTypes" => &mut self.status_column_types,
            "StatusColumnWidths" => &mut self.status_column_widths,
            "Mask" => &mut self.mask,
            "Prefix" => &mut self.prefix,
            _ => return None,
        })
    }

    /// FAR's `GetOptions`: each value from the settings, its default where there is none.
    pub fn read(host: &Host) -> Self {
        let mut options = Self::default();
        for key in FLAGS {
            if let Some(flag) = options.flag(key) {
                *flag = host.bool_setting(key, *flag);
            }
        }
        for key in TEXTS {
            if let Some(text) = options.text(key)
                && let Some(value) = host.setting(key)
            {
                *text = value;
            }
        }
        options.copy_contents = host.int_setting("CopyContents", options.copy_contents);
        options
    }

    /// The settings as `Config` writes them on OK (`TmpCfg.cpp:89-107`).
    pub fn write(&self, host: &Host) {
        let mut options = self.clone();
        for key in FLAGS {
            if let Some(flag) = options.flag(key) {
                host.set_bool(key, *flag);
            }
        }
        host.set_int("CopyContents", self.copy_contents);
        for key in TEXTS {
            if let Some(text) = options.text(key) {
                host.set_setting(key, text);
            }
        }
    }

    /// Its own mode at Ctrl+4 (`TmpClass.cpp:101-111`): the columns from the options, case conversion,
    /// full screen with its option.
    pub fn mode(&self) -> PanelMode {
        PanelMode {
            columns: self.column_types.clone(),
            widths: self.column_widths.clone(),
            status_columns: self.status_column_types.clone(),
            status_widths: self.status_column_widths.clone(),
            full_screen: self.full_screen_panel,
            case_conversion: true,
            ..PanelMode::default()
        }
    }

    /// SavLst's extension (`TmpClass.cpp:682-684`): the first mask's part after its first dot, when there
    /// is one without wildcards.
    pub fn list_extension(&self) -> Option<&str> {
        let first = self.mask.split(',').next().unwrap_or_default();
        let (_, extension) = first.split_once('.')?;
        (!extension.is_empty() && !extension.contains(['*', '?'])).then_some(extension)
    }
}

/// The dialog's items whose values are read back (FAR's `TmpCfg.cpp:41-84` in its order).
const DISKS: usize = 0;
const PLUGINS: usize = 1;
const COMMON: usize = 3;
const SAFE: usize = 4;
const ANY: usize = 5;
const COPY: usize = 6;
const REPLACE: usize = 7;
const MENU: usize = 8;
const NEW_PANEL: usize = 9;
const COLUMN_TYPES: usize = 12;
const COLUMN_WIDTHS: usize = 14;
const STATUS_TYPES: usize = 16;
const STATUS_WIDTHS: usize = 18;
const FULL_SCREEN: usize = 19;
const MASK: usize = 22;
const PREFIX: usize = 24;
/// The OK button.
pub const OK: usize = 25;

/// FAR's configuration dialog ("Temporary panel", `TmpCfg.cpp:41-84`, the labels of `TmpEng.lng`): its
/// two columns one after another, the 3-state "Copy folder contents".
pub fn dialog(options: &Options) -> Dialog {
    let copy = match options.copy_contents {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    };
    let edit = |text: &str| DialogItem::edit(text).width(30);
    let items = vec![
        DialogItem::check("Add to &Disks menu", options.add_to_disks_menu),
        DialogItem::check("Add to &Plugins menu", options.add_to_plugins_menu).same_row(),
        DialogItem::separator(),
        DialogItem::check("Use &common panel", options.common_panel),
        DialogItem::check("&Safe panel mode", options.safe_mode_panel),
        DialogItem::check("&Any elements in panel", options.any_in_panel),
        DialogItem::check3("Copy folder c&ontents", copy),
        DialogItem::check("&Replace files with file list", options.replace),
        DialogItem::check("&Menu from file list", options.menu_for_filelist),
        DialogItem::check(
            "Ne&w panel for search results",
            options.new_panel_for_search_results,
        ),
        DialogItem::separator(),
        DialogItem::text("Column &types"),
        edit(&options.column_types),
        DialogItem::text("Column &widths"),
        edit(&options.column_widths),
        DialogItem::text("Status line column t&ypes"),
        edit(&options.status_column_types),
        DialogItem::text("Status l&ine column widths"),
        edit(&options.status_column_widths),
        DialogItem::check("F&ull screen mode", options.full_screen_panel),
        DialogItem::separator(),
        DialogItem::text("File masks for the file &lists:"),
        edit(&options.mask),
        DialogItem::text("Command line pre&fix:"),
        edit(&options.prefix),
        DialogItem::button("Ok").default_button(),
        DialogItem::button("Cancel"),
    ];
    Dialog::new("Temporary panel", items)
}

/// The options as the dialog's OK left them (`values`, one per item); those not in it as they were.
pub fn from_dialog(options: &Options, values: &[Value]) -> Options {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let checked = |item: usize| value(item).checked();
    Options {
        add_to_disks_menu: checked(DISKS),
        add_to_plugins_menu: checked(PLUGINS),
        common_panel: checked(COMMON),
        safe_mode_panel: checked(SAFE),
        any_in_panel: checked(ANY),
        copy_contents: i64::from(value(COPY).state.clamp(0, 2)),
        replace: checked(REPLACE),
        menu_for_filelist: checked(MENU),
        new_panel_for_search_results: checked(NEW_PANEL),
        full_screen_panel: checked(FULL_SCREEN),
        list_utf8: options.list_utf8,
        column_types: value(COLUMN_TYPES).text,
        column_widths: value(COLUMN_WIDTHS).text,
        status_column_types: value(STATUS_TYPES).text,
        status_column_widths: value(STATUS_WIDTHS).text,
        mask: value(MASK).text,
        prefix: value(PREFIX).text,
    }
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
