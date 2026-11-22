//! arclite's settings (`options.cpp`): under its names with its defaults, and its configuration dialog
//! (`SettingsDialog`, ui.cpp) in its order: the masks with their "Edit" (a list to check), "Generate
//! masks" (every format's extensions) and "Default masks"; the formats enabled or disabled (by name, with
//! "Edit" offering 7-Zip's), whether they count for Ctrl+PgDn; "7z library info". The code pages are
//! given to 7-Zip's library too (arclite's `Patch7zCP`).

use nc_plugin_sdk::sevenzip::{Format, Module};
use nc_plugin_sdk::{Closed, Controls, Dialog, DialogEvent, DialogItem, Host};

use crate::options_dialogs::{library_info, masks_of, multi_select};

/// The plugin's title: the Plugins menu's item, the configuration's, the panel title's start (arclite's
/// "ArcLite", the program's own name).
pub const TITLE: &str = "Archive";

/// arclite's `include_masks` default.
pub const DEFAULT_MASKS: &str = "*.zip,*.rar,*.[7bgxl]z,*.[bg]zip,*.tar,*.t[agbxl]z,*.z,*.ar[cj],*.r[0-9][0-9],*.a[0-9][0-9],*.bz2,*.cab,*.jar,*.lha,*.lzh,*.ha,*.ac[bei],*.pa[ck],*.rk,*.cpio,*.rpm,*.zoo,*.hqx,*.sit,*.ice,*.uc2,*.ain,*.imp,*.777,*.ufa,*.boa,*.bs[2a],*.sea,*.[ah]pk,*.ddi,*.x2,*.rkv,*.[lw]sz,*.h[ay]p,*.lim,*.sqz,*.chz,*.aa[br],*.zst";

/// arclite's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub handle_create: bool,
    /// Enter and Ctrl+PgDn open archives (with Shift+F2: S6.2).
    pub handle_commands: bool,
    /// The command line prefix, without its `:`.
    pub plugin_prefix: String,
    /// The panel opens in `panel_view_mode`, sorted by name.
    pub own_panel_view_mode: bool,
    pub panel_view_mode: usize,
    pub use_include_masks: bool,
    pub include_masks: String,
    pub use_exclude_masks: bool,
    pub exclude_masks: String,
    /// The masks count for Ctrl+PgDn too (else Ctrl+PgDn opens any file the plugin reads).
    pub pgdn_masks: bool,
    /// The code page of names not marked UTF-8 from FAT, NTFS and Unix (0: the system's OEM one).
    pub oem_cp: u32,
    /// The code page of names from other hosts and of the archive's comment (0: the system's ANSI one).
    pub ansi_cp: u32,
    pub save_cp: bool,
    /// Only these formats (names joined by commas, whatever their case) are tried.
    pub use_enabled_formats: bool,
    pub enabled_formats: String,
    /// These formats are not tried.
    pub use_disabled_formats: bool,
    pub disabled_formats: String,
    /// The formats' lists count for Ctrl+PgDn too.
    pub pgdn_formats: bool,
}

impl Default for Options {
    /// arclite's defaults.
    fn default() -> Self {
        Self {
            handle_create: true,
            handle_commands: true,
            plugin_prefix: "arc".to_string(),
            own_panel_view_mode: true,
            panel_view_mode: 2,
            use_include_masks: true,
            include_masks: DEFAULT_MASKS.to_string(),
            use_exclude_masks: false,
            exclude_masks: String::new(),
            pgdn_masks: false,
            oem_cp: 0,
            ansi_cp: 0,
            save_cp: false,
            use_enabled_formats: false,
            enabled_formats: String::new(),
            use_disabled_formats: false,
            disabled_formats: String::new(),
            pgdn_formats: false,
        }
    }
}

/// The dialog's items by index.
const HANDLE_CREATE: usize = 0;
const HANDLE_COMMANDS: usize = 1;
const OWN_VIEW_MODE: usize = 2;
const OEM_CP: usize = 5;
const ANSI_CP: usize = 7;
const SAVE_CP: usize = 8;
const USE_INCLUDE: usize = 11;
const EDIT_INCLUDE: usize = 12;
const INCLUDE: usize = 13;
const USE_EXCLUDE: usize = 15;
const EDIT_EXCLUDE: usize = 16;
const EXCLUDE: usize = 17;
const PGDN_MASKS: usize = 18;
pub const GENERATE_MASKS: usize = 19;
pub const DEFAULT_MASKS_BUTTON: usize = 20;
const USE_ENABLED: usize = 23;
const EDIT_ENABLED: usize = 24;
const ENABLED: usize = 25;
const USE_DISABLED: usize = 27;
const EDIT_DISABLED: usize = 28;
const DISABLED: usize = 29;
const PGDN_FORMATS: usize = 30;
pub const LIBRARY_INFO: usize = 32;
pub const OK: usize = 34;

/// What the dialog's buttons need: the host for their windows, 7-Zip's formats and libraries.
pub struct Context<'a> {
    pub host: Host,
    pub formats: &'a [Format],
    pub modules: &'a [Module],
}

/// The dialog's width inside its frame (arclite's `c_client_xs`).
const CLIENT: i32 = 60;

impl Options {
    pub fn load(host: &Host) -> Self {
        let defaults = Self::default();
        let flag = |name: &str, default: bool| host.bool_setting(name, default);
        let text = |name: &str, default: &str| host.setting(name).unwrap_or(default.to_string());
        let number = |name: &str, default: i64| host.int_setting(name, default);
        let page = |name: &str| u32::try_from(number(name, 0)).unwrap_or(0);
        Self {
            handle_create: flag("handle_create", defaults.handle_create),
            handle_commands: flag("handle_commands", defaults.handle_commands),
            plugin_prefix: text("plugin_prefix", &defaults.plugin_prefix),
            own_panel_view_mode: flag("own_panel_view_mode", defaults.own_panel_view_mode),
            panel_view_mode: usize::try_from(number("panel_view_mode", 2))
                .ok()
                .filter(|mode| *mode < 10)
                .unwrap_or(defaults.panel_view_mode),
            use_include_masks: flag("use_include_masks", defaults.use_include_masks),
            include_masks: text("include_masks", &defaults.include_masks),
            use_exclude_masks: flag("use_exclude_masks", defaults.use_exclude_masks),
            exclude_masks: text("exclude_masks", &defaults.exclude_masks),
            pgdn_masks: flag("pgdn_masks", defaults.pgdn_masks),
            oem_cp: page("oemCP"),
            ansi_cp: page("ansiCP"),
            save_cp: flag("saveCP", defaults.save_cp),
            use_enabled_formats: flag("use_enabled_formats", defaults.use_enabled_formats),
            enabled_formats: text("enabled_formats", &defaults.enabled_formats),
            use_disabled_formats: flag("use_disabled_formats", defaults.use_disabled_formats),
            disabled_formats: text("disabled_formats", &defaults.disabled_formats),
            pgdn_formats: flag("pgdn_formats", defaults.pgdn_formats),
        }
    }

    /// arclite's `Options::save` of what the dialog sets: the code pages only with "Save Codepages" (else
    /// they last while the plugin runs).
    pub fn save(&self, host: &Host) {
        host.set_bool("handle_create", self.handle_create);
        host.set_bool("handle_commands", self.handle_commands);
        host.set_bool("own_panel_view_mode", self.own_panel_view_mode);
        host.set_bool("use_include_masks", self.use_include_masks);
        host.set_setting("include_masks", &self.include_masks);
        host.set_bool("use_exclude_masks", self.use_exclude_masks);
        host.set_setting("exclude_masks", &self.exclude_masks);
        host.set_bool("pgdn_masks", self.pgdn_masks);
        host.set_bool("use_enabled_formats", self.use_enabled_formats);
        host.set_setting("enabled_formats", &self.enabled_formats);
        host.set_bool("use_disabled_formats", self.use_disabled_formats);
        host.set_setting("disabled_formats", &self.disabled_formats);
        host.set_bool("pgdn_formats", self.pgdn_formats);
        host.set_bool("saveCP", self.save_cp);
        if self.save_cp {
            host.set_int("oemCP", i64::from(self.oem_cp));
            host.set_int("ansiCP", i64::from(self.ansi_cp));
        }
    }

    /// arclite's `SettingsDialog`.
    pub fn dialog(&self) -> Dialog {
        let right = 4 + CLIENT;
        let check = |label: &str, on: bool, x: i32, y: i32| {
            let x2 = x + i32::try_from(label.len()).unwrap_or(0) + 3;
            DialogItem::check(label, on).at(x, y, x2, y)
        };
        let text = |label: &str, x: i32, y: i32| {
            let x2 = x + i32::try_from(label.len()).unwrap_or(0) - 1;
            DialogItem::text(label).at(x, y, x2, y)
        };
        let page = |cp: u32| {
            if cp == 0 {
                String::new()
            } else {
                cp.to_string()
            }
        };
        let line = |y: i32| DialogItem::separator().at(-1, y, -1, y);
        let items = vec![
            check(
                "Handle archive creation (Shift-F&1)",
                self.handle_create,
                5,
                2,
            ),
            check(
                "Handle archive commands (Ctrl-PgDn, Shift-F&2)",
                self.handle_commands,
                5,
                3,
            ),
            check("Use o&wn panel view mode", self.own_panel_view_mode, 5, 4),
            line(5),
            text("OEM &Codepage:", 5, 6),
            DialogItem::edit(&page(self.oem_cp)).at(20, 6, 25, 6),
            text("&ANSI Codepage:", 28, 6),
            DialogItem::edit(&page(self.ansi_cp)).at(44, 6, 49, 6),
            check("Sa&ve Codepages", self.save_cp, right - 18, 6),
            line(7),
            text("&Open files matching masks:", 5, 8),
            check("Active", self.use_include_masks, 33, 8),
            edit_button(44, 8),
            DialogItem::edit(&self.include_masks).at(5, 9, right, 9),
            text("&Exclude files matching masks:", 5, 10),
            check("Active", self.use_exclude_masks, 36, 10),
            edit_button(47, 10),
            DialogItem::edit(&self.exclude_masks).at(5, 11, right, 11),
            check("Ma&sk settings apply to Ctrl-PgDn", self.pgdn_masks, 5, 12),
            DialogItem::button("&Generate masks")
                .no_close()
                .at(5, 13, 23, 13),
            DialogItem::button("Default &masks")
                .no_close()
                .at(25, 13, 42, 13),
            line(14),
            text("E&nable archive formats:", 5, 15),
            check("Active", self.use_enabled_formats, 30, 15),
            edit_button(41, 15),
            DialogItem::edit(&self.enabled_formats).at(5, 16, right, 16),
            text("&Disable archive formats:", 5, 17),
            check("Active", self.use_disabled_formats, 31, 17),
            edit_button(42, 17),
            DialogItem::edit(&self.disabled_formats).at(5, 18, right, 18),
            check(
                "&Format settings apply to Ctrl-PgDn",
                self.pgdn_formats,
                5,
                19,
            ),
            line(20),
            DialogItem::button("&7z library info")
                .no_close()
                .at(5, 21, 24, 21),
            line(22),
            DialogItem::button("OK").default_button().at(24, 23, 29, 23),
            DialogItem::button("Cancel").at(31, 23, 40, 23),
        ];
        Dialog::new(TITLE, items).size(CLIENT.unsigned_abs() + 10, 26)
    }

    /// The dialog's events (arclite's `dialog_proc`): a list's line and its "Edit" are enabled while its
    /// "Active" is on (a masks' "Edit" only with masks in its line); "Edit" offers the list's items (the
    /// masks there, or 7-Zip's formats) to check; "Generate masks" puts every format's extensions in,
    /// "Default masks" arclite's; "7z library info" shows the libraries.
    pub fn handle(event: &DialogEvent, controls: &mut Controls, context: &Context) -> bool {
        match *event {
            DialogEvent::Init => {
                for active in [USE_INCLUDE, USE_EXCLUDE, USE_ENABLED, USE_DISABLED] {
                    enable_list(controls, active, false);
                }
            }
            DialogEvent::Changed(
                item @ (USE_INCLUDE | USE_EXCLUDE | USE_ENABLED | USE_DISABLED),
            ) => {
                enable_list(controls, item, true);
            }
            DialogEvent::Changed(item @ (INCLUDE | EXCLUDE)) => {
                enable_list(controls, item - 2, false);
            }
            DialogEvent::Button(GENERATE_MASKS) => {
                controls.set_text(INCLUDE, &masks_of(context.formats, None));
            }
            DialogEvent::Button(DEFAULT_MASKS_BUTTON) => {
                controls.set_text(INCLUDE, DEFAULT_MASKS);
            }
            DialogEvent::Button(LIBRARY_INFO) => {
                library_info(&context.host, context.modules, context.formats);
            }
            DialogEvent::Button(
                edit @ (EDIT_INCLUDE | EDIT_EXCLUDE | EDIT_ENABLED | EDIT_DISABLED),
            ) => {
                edit_list(controls, edit, context);
            }
            _ => {}
        }
        true
    }

    /// The settings the dialog closed by OK gives (a code page not a number: 0, the system's).
    pub fn from_dialog(&self, closed: &Closed) -> Option<Self> {
        if closed.button != Some(OK) {
            return None;
        }
        let on = |item: usize| closed.values.get(item).is_some_and(|value| value.checked());
        let text =
            |item: usize| (closed.values.get(item)).map_or(String::new(), |v| v.text.clone());
        let page = |item: usize| text(item).trim().parse().unwrap_or(0);
        Some(Self {
            handle_create: on(HANDLE_CREATE),
            handle_commands: on(HANDLE_COMMANDS),
            own_panel_view_mode: on(OWN_VIEW_MODE),
            oem_cp: page(OEM_CP),
            ansi_cp: page(ANSI_CP),
            save_cp: on(SAVE_CP),
            use_include_masks: on(USE_INCLUDE),
            include_masks: text(INCLUDE),
            use_exclude_masks: on(USE_EXCLUDE),
            exclude_masks: text(EXCLUDE),
            pgdn_masks: on(PGDN_MASKS),
            use_enabled_formats: on(USE_ENABLED),
            enabled_formats: text(ENABLED),
            use_disabled_formats: on(USE_DISABLED),
            disabled_formats: text(DISABLED),
            pgdn_formats: on(PGDN_FORMATS),
            ..self.clone()
        })
    }

    /// arclite's `analyse_open`: the formats to try a file as, of `formats` (their names), when the
    /// enabled or disabled lists count (for Ctrl+PgDn only with "Format settings apply to Ctrl-PgDn");
    /// none: every one.
    pub fn tried_formats(&self, formats: &[Format], pgdn: bool) -> Option<Vec<usize>> {
        let counts =
            (self.use_enabled_formats || self.use_disabled_formats) && (!pgdn || self.pgdn_formats);
        if !counts {
            return None;
        }
        let names =
            |list: &str| -> Vec<String> { list.split(',').map(str::to_uppercase).collect() };
        let enabled = names(&self.enabled_formats);
        let disabled = names(&self.disabled_formats);
        let tried = (formats.iter().enumerate())
            .filter(|(_, format)| {
                let name = format.name.to_uppercase();
                (!self.use_enabled_formats || enabled.contains(&name))
                    && !(self.use_disabled_formats && disabled.contains(&name))
            })
            .map(|(index, _)| index)
            .collect();
        Some(tried)
    }
}

/// An "Edit" button of the dialog (its `DIF_BTNNOCLOSE`).
fn edit_button(x: i32, y: i32) -> DialogItem {
    DialogItem::button("Edit").no_close().at(x, y, x + 7, y)
}

/// A list's line and "Edit" enabled as its "Active" (`active`) says; a masks' "Edit" only with masks,
/// but as "Active" is clicked (arclite's `DN_BTNCLICK` enables it whatever the line).
fn enable_list(controls: &mut Controls, active: usize, clicked: bool) {
    let on = controls.value(active).checked();
    let line = active + 2;
    let masks = matches!(active, USE_INCLUDE | USE_EXCLUDE);
    let edit = edit_enabled(on, clicked, masks, controls.value(line).text.is_empty());
    controls.enable(line, on);
    controls.enable(active + 1, edit);
}

/// Whether a list's "Edit" is enabled: with "Active" on; a masks list's only with masks in its line,
/// but as "Active" is clicked (arclite's `DN_INITDIALOG`, `DN_EDITCHANGE`, `DN_BTNCLICK`).
pub(crate) fn edit_enabled(on: bool, clicked: bool, masks: bool, line_empty: bool) -> bool {
    on && (clicked || !masks || !line_empty)
}

/// "Edit": the list's items offered in arclite's `MultiSelectDialog` (the masks in the line, or 7-Zip's
/// formats sorted whatever their case), the line set to the ones checked.
fn edit_list(controls: &mut Controls, edit: usize, context: &Context) {
    let line = edit + 1;
    let chosen = controls.value(line).text;
    let (title, offered) = match edit {
        EDIT_INCLUDE => ("&Open files matching masks:", chosen.clone()),
        EDIT_EXCLUDE => ("&Exclude files matching masks:", chosen.clone()),
        EDIT_ENABLED => ("E&nable archive formats:", format_names(context.formats)),
        _ => ("&Disable archive formats:", format_names(context.formats)),
    };
    if let Some(picked) = multi_select(&context.host, title, &offered, &chosen) {
        controls.set_text(line, &picked);
        controls.focus(line);
    }
}

/// The formats' names joined by commas (arclite's `get_available_formats`).
fn format_names(formats: &[Format]) -> String {
    (formats.iter())
        .map(|format| format.name.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
