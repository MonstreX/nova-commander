//! The plugin's settings (FAR's proclist `options::Read`, `options::Write`, `Config`): under FAR's names
//! with FAR's defaults; the configuration dialog at FAR's DialogBuilder places (48 x 16). Its saving
//! writes the panel modes too, as FAR's `SavePanelModes` does. The panel's start mode and sort
//! (`StartPanelMode`, `SortMode`) are saved as a panel closes.

use nc_plugin_sdk::{Closed, Dialog, DialogItem, Host};

/// The plugin's title: its menus' item and its panel's title (FAR's `MPlistPanel`).
pub const TITLE: &str = "Process list";

/// FAR's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub add_to_disks: bool,
    pub add_to_plugins: bool,
    pub export_environment: bool,
    pub export_module_info: bool,
    pub export_module_version: bool,
    pub export_performance: bool,
    pub export_handles: bool,
    pub export_handles_unnamed: bool,
}

impl Default for Options {
    /// FAR's defaults.
    fn default() -> Self {
        Self {
            add_to_disks: true,
            add_to_plugins: true,
            export_environment: true,
            export_module_info: true,
            export_module_version: false,
            export_performance: true,
            export_handles: false,
            export_handles_unnamed: false,
        }
    }
}

/// Each setting's name in the dialog's order.
const NAMES: [&str; 8] = [
    "AddToDisksMenu",
    "AddToPluginsMenu",
    "ExportEnvironment",
    "ExportModuleInfo",
    "ExportModuleVersion",
    "ExportPerformance",
    "ExportHandles",
    "ExportHandlesUnnamed",
];

/// The dialog's check boxes by index, in the order of [`NAMES`].
const CHECKS: [usize; 8] = [0, 1, 4, 5, 6, 7, 8, 9];
/// "Ok".
pub const OK: usize = 11;

/// The saved start mode (FAR's `StartPanelMode`, its default 1) and sort (`SortMode`, unsorted).
pub const START_MODE: &str = "StartPanelMode";
pub const SORT_MODE: &str = "SortMode";

impl Options {
    fn flags(&self) -> [bool; 8] {
        [
            self.add_to_disks,
            self.add_to_plugins,
            self.export_environment,
            self.export_module_info,
            self.export_module_version,
            self.export_performance,
            self.export_handles,
            self.export_handles_unnamed,
        ]
    }

    fn of_flags(flags: [bool; 8]) -> Self {
        let [
            add_to_disks,
            add_to_plugins,
            export_environment,
            export_module_info,
            export_module_version,
            export_performance,
            export_handles,
            export_handles_unnamed,
        ] = flags;
        Self {
            add_to_disks,
            add_to_plugins,
            export_environment,
            export_module_info,
            export_module_version,
            export_performance,
            export_handles,
            export_handles_unnamed,
        }
    }

    pub fn load(host: &Host) -> Self {
        let defaults = Self::default().flags();
        let mut flags = defaults;
        for (flag, (name, default)) in flags.iter_mut().zip(NAMES.iter().zip(defaults)) {
            *flag = host.bool_setting(name, default);
        }
        Self::of_flags(flags)
    }

    pub fn save(&self, host: &Host) {
        for (name, flag) in NAMES.iter().zip(self.flags()) {
            host.set_bool(name, flag);
        }
    }

    /// FAR's `Config` dialog: the check boxes from column 5, the unnamed handles' indented by 4.
    pub fn dialog(&self) -> Dialog {
        let check = |label: &str, on: bool, x: i32, y: i32| {
            let x2 = x + i32::try_from(label.len()).unwrap_or(0) + 4;
            DialogItem::check(label, on).at(x, y, x2, y)
        };
        let text = |label: &str, y: i32| {
            let x2 = 5 + i32::try_from(label.len()).unwrap_or(0) - 1;
            DialogItem::text(label).at(5, y, x2, y)
        };
        let items = vec![
            check("&Add to Disks menu", self.add_to_disks, 5, 2),
            check("Add to &Plugins menu", self.add_to_plugins, 5, 3),
            DialogItem::separator().at(-1, 4, -1, 4),
            text("Include in the additional information:", 5),
            check("&Environment (local only)", self.export_environment, 5, 6),
            check("&Module info", self.export_module_info, 5, 7),
            check(
                "Module &versions and descriptions",
                self.export_module_version,
                5,
                8,
            ),
            check("Pe&rformance data", self.export_performance, 5, 9),
            check("&Handles (local only)", self.export_handles, 5, 10),
            check(
                "Include unnamed handles",
                self.export_handles_unnamed,
                9,
                11,
            ),
            DialogItem::separator().at(-1, 12, -1, 12),
            DialogItem::button("Ok").default_button().at(15, 13, 20, 13),
            DialogItem::button("Cancel").at(22, 13, 31, 13),
        ];
        Dialog::new("Process list - config", items).size(48, 16)
    }

    /// The settings the dialog closed by "Ok" gives.
    pub fn from_dialog(closed: &Closed) -> Option<Self> {
        if closed.button != Some(OK) {
            return None;
        }
        let mut flags = [false; 8];
        for (flag, item) in flags.iter_mut().zip(CHECKS) {
            *flag = closed.values.get(item).is_some_and(|value| value.checked());
        }
        Some(Self::of_flags(flags))
    }

    /// Shift+F3's dialog (FAR's `MViewWithOptions`): the text's parts for this view only, at the places
    /// of the configuration's (48 x 13).
    pub fn view_dialog(&self) -> Dialog {
        let config = self.dialog();
        let mut items: Vec<DialogItem> = config.items[3..10].to_vec();
        for item in &mut items {
            item.place.1 -= 3;
            item.place.3 -= 3;
        }
        items.push(DialogItem::separator().at(-1, 9, -1, 9));
        items.push(DialogItem::button("Ok").default_button().at(15, 10, 20, 10));
        items.push(DialogItem::button("Cancel").at(22, 10, 31, 10));
        Dialog::new("View with Options...", items).size(48, 13)
    }

    /// The options Shift+F3's dialog closed by "Ok" gives: the menus' as they are.
    pub fn from_view_dialog(&self, closed: &Closed) -> Option<Self> {
        if closed.button != Some(VIEW_OK) {
            return None;
        }
        let checked = |item: usize| closed.values.get(item).is_some_and(|value| value.checked());
        Some(Self {
            export_environment: checked(1),
            export_module_info: checked(2),
            export_module_version: checked(3),
            export_performance: checked(4),
            export_handles: checked(5),
            export_handles_unnamed: checked(6),
            ..*self
        })
    }
}

/// Shift+F3's "Ok".
pub const VIEW_OK: usize = 8;

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
