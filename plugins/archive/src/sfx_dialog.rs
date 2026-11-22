//! arclite's "SFX Options" dialog (sfx.cpp `SfxOptionsDialog`) in its order and words: the profile to
//! apply (the profiles that make a self-extracting archive), the module, the icon, the version
//! information, the install configuration; the items enabled as its `set_control_state` keeps them, the
//! answers checked as its `read_controls` checks them (the module, the icon's path), the profile the
//! answers match chosen.

use std::path::Path;

use nc_plugin_sdk::{Closed, Controls, Dialog, DialogEvent, DialogItem, Host};

use crate::create_options::Profiles;
use crate::layout::{Layout, label_len};
use crate::sfx::{InstallConfig, Modules, SfxOptions, VersionInfo, WRONG_MODULE};

pub const TITLE: &str = "SFX Options";

/// The dialog's items by index (as `dialog` lays them out).
pub(crate) const PROFILE: usize = 1;
pub(crate) const MODULE: usize = 4;
pub(crate) const REPLACE_ICON: usize = 5;
pub(crate) const ICON_PATH: usize = 7;
pub(crate) const REPLACE_VERSION: usize = 8;
/// The version's edit lines: product name, version, company, description, comments, copyright.
pub(crate) const VERSION_EDITS: [usize; 6] = [10, 12, 14, 16, 18, 20];
pub(crate) const APPEND_CONFIG: usize = 21;
/// The configuration's items: title, begin prompt, progress (a check box), run program, directory,
/// execute file, execute parameters.
pub(crate) const CONFIG_ITEMS: [usize; 7] = [23, 25, 27, 29, 31, 33, 35];
pub const OK: usize = 37;
pub(crate) const CANCEL: usize = 38;

const VERSION_LABELS: [&str; 6] = [
    "Product &name:",
    "V&ersion:",
    "C&ompany name:",
    "File &description:",
    "&Comments:",
    "&Legal copyright:",
];

const CONFIG_LABELS: [&str; 7] = [
    "&Title:",
    "&Begin prompt:",
    "Pro&gress meter:",
    "&Run program:",
    "Director&y:",
    "E&xecute file:",
    "Execute p&arameters:",
];

/// The version's values in the labels' order.
fn version_values(info: &VersionInfo) -> [&String; 6] {
    [
        &info.product_name,
        &info.version,
        &info.company_name,
        &info.file_description,
        &info.comments,
        &info.legal_copyright,
    ]
}

/// The configuration's texts in the labels' order but the progress.
fn config_values(config: &InstallConfig) -> [&String; 6] {
    [
        &config.title,
        &config.begin_prompt,
        &config.run_program,
        &config.directory,
        &config.execute_file,
        &config.execute_parameters,
    ]
}

/// The progress meter's check box: `yes`, `no`, else its third state.
fn progress_state(progress: &str) -> Option<bool> {
    match progress {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// The dialog: the profiles offered with their SFX options, the modules.
pub struct SfxDialog {
    pub profiles: Vec<(String, SfxOptions)>,
    pub modules: Modules,
}

impl SfxDialog {
    /// arclite's `SfxOptionsDialog`: the profiles that make a self-extracting archive.
    pub fn new(profiles: &Profiles, modules: &Modules) -> Self {
        Self {
            profiles: (profiles.0.iter())
                .filter(|profile| profile.options.create_sfx)
                .map(|profile| (profile.name.clone(), profile.options.sfx.clone()))
                .collect(),
            modules: modules.clone(),
        }
    }

    /// arclite's `SfxOptionsDialog::show`.
    pub fn dialog(&self, options: &SfxOptions) -> Dialog {
        let mut layout = Layout::new(TITLE, 69);
        layout.label("Apply &profile:");
        let mut names: Vec<&str> = self
            .profiles
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        names.push("");
        layout.combo_sized(&names, self.profiles.len(), 30);
        layout.new_line();
        layout.separator();
        layout.new_line();
        layout.label("&Module name:");
        let mut modules: Vec<String> = (0..self.modules.0.len())
            .map(|at| self.modules.description(at))
            .collect();
        let width = modules.iter().map(|name| name.chars().count()).max();
        modules.push(String::new());
        let modules: Vec<&str> = modules.iter().map(String::as_str).collect();
        let chosen = (self.modules.by_name(&options.name)).unwrap_or(self.modules.0.len());
        let width = i32::try_from(width.unwrap_or(0) + 6).unwrap_or(6);
        layout.combo_sized(&modules, chosen, width);
        layout.new_line();
        layout.boxed(DialogItem::check("Replace &icon", options.replace_icon));
        layout.new_line();
        layout.spacer(2);
        layout.label("Icon pat&h:");
        let icon = DialogItem::edit(&options.icon_path).history("arclite.icon_path", false);
        layout.edit(icon.edit_path(), 0);
        layout.new_line();
        let version = DialogItem::check("Replace &version", options.replace_version);
        labelled_rows(&mut layout, version, &VERSION_LABELS, |at, layout| {
            let value = version_values(&options.ver_info)[at];
            layout.edit(DialogItem::edit(value), 0);
        });
        let config = &options.install_config;
        let append = DialogItem::check("In&stall configuration", options.append_install_config);
        labelled_rows(&mut layout, append, &CONFIG_LABELS, |at, layout| {
            let edit = |index: usize| DialogItem::edit(config_values(config)[index]);
            match at {
                0 | 1 => layout.edit(edit(at), 0),
                2 => layout.boxed(DialogItem::check3("", progress_state(&config.progress))),
                _ => layout.edit(edit(at - 1), 0),
            };
        });
        layout.separator();
        layout.new_line();
        layout.buttons(vec![
            DialogItem::button("OK").default_button(),
            DialogItem::button("Cancel"),
        ]);
        layout.new_line();
        let mut dialog = layout.dialog();
        let states: Vec<i32> = dialog.items.iter().map(|item| item.state).collect();
        let state = |item: usize| states.get(item).copied().unwrap_or(0);
        for (item, enabled) in self.control_state(&state) {
            dialog.items[item].disabled = !enabled;
        }
        dialog
    }

    /// arclite's `set_control_state`: the icon's path with "Replace icon", the version's lines with
    /// "Replace version", the install configuration for a module that reads one, its items with it.
    pub fn control_state(&self, state: &dyn Fn(usize) -> i32) -> Vec<(usize, bool)> {
        let icon = state(REPLACE_ICON) == 1;
        let version = state(REPLACE_VERSION) == 1;
        let module = usize::try_from(state(MODULE)).unwrap_or(usize::MAX);
        let config = module < self.modules.0.len() && self.modules.install_config(module);
        let append = config && state(APPEND_CONFIG) == 1;
        let mut items = vec![
            (ICON_PATH - 1, icon),
            (ICON_PATH, icon),
            (APPEND_CONFIG, config),
        ];
        items.extend((REPLACE_VERSION + 1..APPEND_CONFIG).map(|item| (item, version)));
        items.extend((APPEND_CONFIG + 1..=CONFIG_ITEMS[6]).map(|item| (item, append)));
        items
    }

    /// arclite's `read_controls`: the options the items hold, or why they cannot be taken (no module
    /// chosen, an icon's path that is not there); what is not replaced or appended left empty.
    pub fn read(&self, value: &dyn Fn(usize) -> (i32, String)) -> Result<SfxOptions, &'static str> {
        let module = usize::try_from(value(MODULE).0).unwrap_or(usize::MAX);
        let path = self.modules.0.get(module).ok_or(WRONG_MODULE)?;
        let mut options = SfxOptions {
            name: (path.file_name()).map_or_else(String::new, |name| name.to_string_lossy().into()),
            replace_icon: value(REPLACE_ICON).0 == 1,
            replace_version: value(REPLACE_VERSION).0 == 1,
            ..SfxOptions::default()
        };
        if options.replace_icon {
            options.icon_path = value(ICON_PATH).1;
            if !Path::new(&options.icon_path).exists() {
                return Err("Icon path is invalid");
            }
        }
        if options.replace_version {
            let [product, version, company, description, comments, copyright] =
                VERSION_EDITS.map(|item| value(item).1);
            options.ver_info = VersionInfo {
                version,
                comments,
                company_name: company,
                file_description: description,
                legal_copyright: copyright,
                product_name: product,
            };
        }
        options.append_install_config =
            self.modules.install_config(module) && value(APPEND_CONFIG).0 == 1;
        if options.append_install_config {
            let [title, prompt, progress, run, directory, file, parameters] =
                CONFIG_ITEMS.map(value);
            options.install_config = InstallConfig {
                title: title.1,
                begin_prompt: prompt.1,
                progress: match progress.0 {
                    0 => "no".to_string(),
                    1 => "yes".to_string(),
                    _ => String::new(),
                },
                run_program: run.1,
                directory: directory.1,
                execute_file: file.1,
                execute_parameters: parameters.1,
            };
        }
        Ok(options)
    }

    /// arclite's `write_controls`: a profile's options in the items.
    fn write_controls(&self, controls: &mut Controls, options: &SfxOptions) {
        let module = (self.modules.by_name(&options.name)).unwrap_or(self.modules.0.len());
        controls.set_state(MODULE, i32::try_from(module).unwrap_or(0));
        controls.set_checked(REPLACE_ICON, options.replace_icon);
        controls.set_text(ICON_PATH, &options.icon_path);
        controls.set_checked(REPLACE_VERSION, options.replace_version);
        for (item, value) in VERSION_EDITS.iter().zip(version_values(&options.ver_info)) {
            controls.set_text(*item, value);
        }
        controls.set_checked(APPEND_CONFIG, options.append_install_config);
        let config = &options.install_config;
        let texts = (CONFIG_ITEMS.iter()).filter(|item| **item != CONFIG_ITEMS[2]);
        for (item, value) in texts.zip(config_values(config)) {
            controls.set_text(*item, value);
        }
        let progress = progress_state(&config.progress).map_or(2, i32::from);
        controls.set_state(CONFIG_ITEMS[2], progress);
    }

    /// The dialog's events (arclite's `dialog_proc`): the items' state kept, a profile chosen written
    /// in, OK with answers wrong says why and stays, any change choosing the profile the answers match.
    pub fn handle(&self, host: &Host, event: &DialogEvent, controls: &mut Controls) -> bool {
        match *event {
            DialogEvent::Changed(PROFILE) => {
                let chosen = usize::try_from(controls.value(PROFILE).state).unwrap_or(usize::MAX);
                if let Some((_, options)) = self.profiles.get(chosen) {
                    self.write_controls(controls, options);
                }
            }
            DialogEvent::Close(Some(button)) if button != CANCEL => {
                if let Err(why) = self.read(&|item| value_of(controls, item)) {
                    host.message(TITLE, &[why]);
                    return false;
                }
            }
            _ => {}
        }
        if matches!(event, DialogEvent::Changed(_) | DialogEvent::Button(_)) {
            let matching = (self.read(&|item| value_of(controls, item)).ok())
                .and_then(|read| {
                    self.profiles
                        .iter()
                        .position(|(_, options)| *options == read)
                })
                .unwrap_or(self.profiles.len());
            controls.set_state(PROFILE, i32::try_from(matching).unwrap_or(0));
            let states: Vec<i32> = (0..=CANCEL)
                .map(|item| controls.value(item).state)
                .collect();
            let state = |item: usize| states.get(item).copied().unwrap_or(0);
            for (item, enabled) in self.control_state(&state) {
                controls.enable(item, enabled);
            }
        }
        true
    }

    /// The options the dialog closed with by OK; none for Cancel or Esc.
    pub fn answers(&self, closed: &Closed) -> Option<SfxOptions> {
        if closed.button.is_none_or(|button| button == CANCEL) {
            return None;
        }
        let value = |item: usize| {
            (closed.values.get(item)).map_or((0, String::new()), |value| {
                (value.state, value.text.clone())
            })
        };
        self.read(&value).ok()
    }
}

/// A check box, then a row per label (two columns in, its item at the labels' common column).
fn labelled_rows(
    layout: &mut Layout,
    check: DialogItem,
    labels: &[&str],
    mut item: impl FnMut(usize, &mut Layout),
) {
    let column = labels
        .iter()
        .map(|label| label_len(label))
        .max()
        .unwrap_or(0)
        + 2;
    layout.boxed(check);
    layout.new_line();
    for (at, label) in labels.iter().enumerate() {
        layout.spacer(2);
        layout.label(label);
        layout.pad(column);
        item(at, layout);
        layout.new_line();
    }
}

/// An item's state and text as the dialog holds them now.
fn value_of(controls: &Controls, item: usize) -> (i32, String) {
    let value = controls.value(item);
    (value.state, value.text)
}

/// arclite's `sfx_options_dialog`: the options `options` start from, the ones OK closed with; none for
/// Cancel or Esc.
pub fn ask(
    host: &Host,
    options: &SfxOptions,
    profiles: &Profiles,
    modules: &Modules,
) -> Option<SfxOptions> {
    let dialog = SfxDialog::new(profiles, modules);
    let shown = dialog.dialog(options);
    let closed = host.dialog_with(&shown, |event, controls| {
        dialog.handle(host, &event, controls)
    });
    dialog.answers(&closed)
}

#[cfg(test)]
#[path = "sfx_dialog_tests.rs"]
mod tests;
