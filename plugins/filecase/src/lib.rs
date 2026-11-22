//! Case conversion (FAR's filecase, `far-source/plugins/filecase`): the selected files' names on the active
//! panel (or the file under the cursor) in lower case, upper case, a first capital or title case, the name
//! and the extension each its own; mixed case left when asked, what is in folders too, folders' names too.
//! FAR's dialog (`filecvt.cpp`, the labels of `CaseEng.lng`) at FAR's places, its "Reset" putting the
//! word delimiters back, "Current run only" keeping the settings as they were. A window without buttons
//! says it works (FAR's `SaveScreen` with a message); then the panel is read again, its selection gone
//! (`FCTL_UPDATEPANEL` with 0). On a plugin's panel only its items that are files on a disk are taken
//! (FAR's `MoveFile` fails on the others).

use std::path::PathBuf;

use nc_plugin_sdk::{Dialog, DialogEvent, DialogItem, Host, Info, Plugin, Which, export_plugin};

pub mod case;
pub mod walk;

use case::{DELIMITERS, Mode, Rules};

/// Its item in the plugin commands and its dialog's title.
const ITEM: &str = "Case conversion";
/// The dialog's items that matter.
pub const NAME_MODES: usize = 1;
pub const EXTENSION_MODES: usize = 7;
pub const SKIP_MIXED: usize = 13;
pub const SUBFOLDERS: usize = 14;
pub const FOLDER_NAMES: usize = 15;
pub const THIS_RUN: usize = 17;
pub const DELIMITERS_LINE: usize = 20;
pub const RESET: usize = 21;
pub const OK: usize = 23;

/// What the dialog sets and the settings keep (FAR's names).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options {
    pub rules: Rules,
    /// What is in the folders too (`ProcessSubDir`).
    pub subfolders: bool,
}

impl Options {
    pub fn read(host: &Host) -> Self {
        let rules = Rules {
            name: Mode::of(host.int_setting("ConvertMode", 0)),
            extension: Mode::of(host.int_setting("ConvertModeExt", 0)),
            skip_mixed: host.bool_setting("SkipMixedCase", true),
            folder_names: host.bool_setting("ProcessDir", false),
            delimiters: host
                .setting("WordDiv")
                .unwrap_or_else(|| DELIMITERS.to_string()),
        };
        Self {
            rules,
            subfolders: host.bool_setting("ProcessSubDir", false),
        }
    }

    pub fn write(&self, host: &Host) {
        host.set_setting("WordDiv", &self.rules.delimiters);
        host.set_int("ConvertMode", self.rules.name.index());
        host.set_int("ConvertModeExt", self.rules.extension.index());
        host.set_bool("SkipMixedCase", self.rules.skip_mixed);
        host.set_bool("ProcessSubDir", self.subfolders);
        host.set_bool("ProcessDir", self.rules.folder_names);
    }
}

/// The five radio buttons of a part from row 3 at column `x`, `chosen` on; the first of the names'
/// group has the focus where it is chosen (FAR's `FocusOnSelected`).
fn modes(labels: [&str; 5], x: i32, chosen: Mode, focus: bool) -> Vec<DialogItem> {
    (labels.into_iter().zip(0..))
        .map(|(label, row)| {
            let on = i64::from(row) == chosen.index();
            let radio = DialogItem::radio(label, on).at(x, 3 + row, x + 18, 3 + row);
            let radio = if row == 0 { radio.group() } else { radio };
            if on && focus { radio.focus() } else { radio }
        })
        .collect()
}

/// FAR's dialog as its `PluginDialogBuilder` lays it out: 53 x 21, the extension's column at 28.
pub fn dialog(options: &Options) -> Dialog {
    let rules = &options.rules;
    let line = |y: i32| DialogItem::separator().at(-1, y, -1, y);
    let check = |label: &str, on: bool, y: i32| DialogItem::check(label, on).at(5, y, 5, y);
    let names = [
        "&1. lower case",
        "&2. UPPER CASE",
        "&3. First capital",
        "&4. Title Case",
        "&5. None",
    ];
    let extensions = [
        "&6. lower case",
        "&7. UPPER CASE",
        "&8. First capital",
        "&9. Title Case",
        "&0. None",
    ];
    let mut items = vec![DialogItem::text("Name").at(5, 2, 8, 2)];
    items.extend(modes(names, 5, rules.name, true));
    items.push(DialogItem::text("Extension").at(28, 2, 36, 2));
    items.extend(modes(extensions, 28, rules.extension, false));
    items.extend([
        line(8),
        check("Do not process mi&xed case files", rules.skip_mixed, 9),
        check("Process &subfolders", options.subfolders, 10),
        check("Process &folder names", rules.folder_names, 11),
        line(12),
        check("C&urrent run only", false, 13),
        line(14),
        DialogItem::text("&Word delimiters:").at(5, 15, 21, 15),
        DialogItem::edit(&rules.delimiters)
            .history("FileCase_WordDiv", false)
            .at(5, 16, 24, 16),
        DialogItem::button("&Reset").no_close().at(26, 16, 35, 16),
        line(17),
        DialogItem::button("Ok").default_button().at(18, 18, 23, 18),
        DialogItem::button("Cancel").at(25, 18, 34, 18),
    ]);
    Dialog::new(ITEM, items).size(53, 21)
}

/// The options the dialog's OK left (`values` one per item) and whether they are for this run only.
pub fn from_dialog(values: &[nc_plugin_sdk::Value]) -> (Options, bool) {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let mode = |first: usize| {
        let on = (0..5).find(|row| value(first + row).checked()).unwrap_or(0);
        Mode::of(i64::try_from(on).unwrap_or(0))
    };
    let rules = Rules {
        name: mode(NAME_MODES),
        extension: mode(EXTENSION_MODES),
        skip_mixed: value(SKIP_MIXED).checked(),
        folder_names: value(FOLDER_NAMES).checked(),
        delimiters: value(DELIMITERS_LINE).text,
    };
    let options = Options {
        rules,
        subfolders: value(SUBFOLDERS).checked(),
    };
    (options, value(THIS_RUN).checked())
}

pub struct FileCase {
    host: Host,
    options: Options,
}

impl FileCase {
    /// The dialog; its options and whether for this run only, none when it was not OK.
    fn ask(&self) -> Option<(Options, bool)> {
        let closed = self
            .host
            .dialog_with(&dialog(&self.options), |event, controls| {
                if event == DialogEvent::Button(RESET) {
                    controls.set_text(DELIMITERS_LINE, DELIMITERS);
                }
                true
            });
        (closed.button == Some(OK)).then(|| from_dialog(&closed.values))
    }

    /// The files the active panel gives: the selected ones, or the one under the cursor.
    fn files(&self) -> Vec<(PathBuf, bool)> {
        let Some(panel) = self.host.panel(Which::Active) else {
            return Vec::new();
        };
        (self.host.selected_items(Which::Active).into_iter())
            .filter(|item| !item.parent)
            .filter_map(|item| {
                let path = match (&item.path, &panel.plugin) {
                    (Some(path), _) => PathBuf::from(path),
                    (None, None) => PathBuf::from(&panel.folder).join(&item.name),
                    (None, Some(_)) => return None,
                };
                Some((path, item.folder))
            })
            .collect()
    }

    fn convert(&mut self) {
        let Some((options, this_run)) = self.ask() else {
            return;
        };
        let rules = &options.rules;
        if rules.name != Mode::None || rules.extension != Mode::None {
            self.host.busy(ITEM, &["Converting files case"]);
            let stop = || self.host.cancelled();
            for (path, folder) in self.files() {
                walk::convert(&path, folder, rules, options.subfolders, &stop);
            }
            if !this_run {
                options.write(&self.host);
            }
            let _ = self.host.reread(Which::Active, false);
        }
        if !this_run {
            self.options = options;
        }
    }
}

impl Plugin for FileCase {
    fn new(host: Host) -> Self {
        let options = Options::read(&host);
        Self { host, options }
    }

    fn info(&self) -> Info {
        Info {
            title: ITEM.to_string(),
            description: "File names case conversion".to_string(),
            author: "nova-commander".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<String> {
        vec![ITEM.to_string()]
    }

    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        self.convert();
        Ok(None)
    }
}

export_plugin!(FileCase);

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
