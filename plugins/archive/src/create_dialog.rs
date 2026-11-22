//! arclite's "Create archive" dialog (`UpdateDialog` with `new_arc`, ui.cpp) in its order and words: the
//! profile with Save and Delete, the archive's path with Evaluate and "Append extension" (its history
//! `arclite.arc_path`), the archiver (7z, zip, Other with the other formats 7-Zip's library writes), the
//! level and method, Solid, the advanced parameters, encryption, a self-extracting archive (7z's), volumes,
//! moving, errors, files open for writing, the filter; "Save settings" keeps the answers. Its handler
//! keeps the items enabled and shown as arclite's `set_control_state`, the path's extension the format's
//! (`change_extension`), each format's level (`arc_levels`), chooses the profile the answers match, and
//! OK checks them as its `read_controls` does; the path is made whole as its `eval_arc_path` makes it.

use std::path::PathBuf;

use nc_plugin_sdk::{ButtonSet, Controls, Dialog, DialogEvent, DialogItem, Host, Message};

use crate::create_dialog_state::{
    change_extension, control_state, level_at, method_line, method_lines, wrapped, write_controls,
};
use crate::create_formats::Archivers;
use crate::create_options::{NewArchive, Profiles, level_in, with_level};
use crate::layout::{Layout, label_len};
use crate::options::TITLE as PLUGIN;
use crate::update_dialog::TITLE as ADD_TITLE;
use crate::update_options::LEVELS;
use crate::update_props::{fit_level, methods_of};

pub const TITLE: &str = "Create archive";

/// The dialog's items by index (as `dialog` lays them out).
pub(crate) const PROFILE: usize = 1;
pub(crate) const SAVE_PROFILE: usize = 2;
pub(crate) const DELETE_PROFILE: usize = 3;
pub(crate) const EVALUATE: usize = 5;
pub(crate) const APPEND_EXT: usize = 6;
pub(crate) const ARC_PATH: usize = 8;
pub(crate) const SEVEN: usize = 11;
pub(crate) const ZIP: usize = 12;
pub(crate) const OTHER: usize = 13;
pub(crate) const OTHER_LIST: usize = 14;
pub(crate) const LEVEL: usize = 16;
pub(crate) const METHOD_LABEL: usize = 17;
pub(crate) const METHOD: usize = 18;
pub(crate) const SOLID: usize = 19;
pub(crate) const ADVANCED: usize = 21;
pub(crate) const ENCRYPT: usize = 23;
pub(crate) const ENCRYPT_HEADER: usize = 24;
pub(crate) const SHOW_PASSWORD: usize = 25;
pub(crate) const PASSWORD_LABEL: usize = 26;
pub(crate) const PASSWORD: usize = 27;
pub(crate) const PASSWORD_VERIFY: usize = 29;
pub(crate) const PASSWORD_VISIBLE_LABEL: usize = 30;
pub(crate) const PASSWORD_VISIBLE: usize = 31;
pub(crate) const CREATE_SFX: usize = 33;
pub(crate) const SFX_OPTIONS: usize = 34;
pub(crate) const ENABLE_VOLUMES: usize = 36;
pub(crate) const VOLUME_SIZE_LABEL: usize = 37;
pub(crate) const VOLUME_SIZE: usize = 38;
pub(crate) const MOVE_FILES: usize = 40;
pub(crate) const IGNORE_ERRORS: usize = 41;
pub(crate) const OPEN_SHARED: usize = 42;
pub const OK: usize = 45;
pub(crate) const CANCEL: usize = 46;
pub(crate) const SAVE: usize = 47;

/// The dialog of one new archive: where it starts, the profiles, the folder a typed path is in.
pub struct CreateDialog {
    pub start: NewArchive,
    pub profiles: Profiles,
    /// arclite's `default_arc_name`: the name a path ending in a folder gets.
    pub default_name: String,
    pub folder: PathBuf,
    /// Each format's level as the dialog keeps it (arclite's `m_options.levels`).
    pub levels: String,
    /// The formats offered.
    pub archivers: Archivers,
    /// The extension the path was last given (arclite's `old_ext`).
    pub old_ext: String,
    /// The SFX modules found with the library.
    pub modules: crate::sfx::Modules,
    /// The SFX options as the dialog keeps them (arclite's `m_options.sfx_options`).
    pub sfx: crate::sfx::SfxOptions,
}

fn names<const N: usize>(table: &[(&'static str, u32); N]) -> Vec<&'static str> {
    table.iter().map(|(name, _)| *name).collect()
}

/// The profiles' names and the empty line after them.
fn profile_lines(profiles: &Profiles) -> Vec<String> {
    let mut lines: Vec<String> = (profiles.0.iter())
        .map(|profile| profile.name.clone())
        .collect();
    lines.push(String::new());
    lines
}

/// arclite's `parse_size_string`: a number with an optional `K`, `M`, `G` or `T` (of 1024); 0 when
/// it is none.
pub fn parse_size(text: &str) -> u64 {
    let text = text.trim();
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let number: u64 = digits.parse().unwrap_or(0);
    let unit = text[digits.len()..].trim_start().chars().next();
    let shift = match unit.map(|unit| unit.to_ascii_uppercase()) {
        Some('K') => 10,
        Some('M') => 20,
        Some('G') => 30,
        Some('T') => 40,
        _ => 0,
    };
    number.checked_shl(shift).unwrap_or(u64::MAX)
}

impl CreateDialog {
    /// arclite's `UpdateDialog::show` for a new archive.
    pub fn dialog(&self) -> Dialog {
        let start = &self.start;
        let profile = &start.profile;
        let mut layout = Layout::new(TITLE, 68);
        layout.label("&Profile:");
        let lines = profile_lines(&self.profiles);
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let chosen = (self.profiles.by_options(profile)).unwrap_or(self.profiles.0.len());
        layout.combo_sized(&lines, chosen, 30);
        layout.spacer(1);
        layout.boxed(DialogItem::button("Save").no_close());
        layout.spacer(1);
        layout.boxed(DialogItem::button("Delete").no_close());
        layout.new_line();
        layout.separator();
        layout.new_line();
        let path_label = "Arc&hive path:";
        layout.spacer(label_len(path_label) + 1);
        layout.boxed(DialogItem::button("Eval&uate").no_close());
        layout.spacer(1);
        layout.boxed(DialogItem::check("&Append extension", start.append_ext));
        layout.reset_line();
        layout.label(path_label);
        layout.new_line();
        let path = DialogItem::edit(&format!("{}{}", start.path, self.old_ext))
            .history("arclite.arc_path", false);
        layout.edit(path, 68);
        layout.new_line();
        layout.separator();
        layout.new_line();
        self.archiver_items(&mut layout);
        self.lower_items(&mut layout);
        let mut dialog = layout.dialog();
        let states: Vec<i32> = dialog.items.iter().map(|item| item.state).collect();
        let state = |item: usize| states.get(item).copied().unwrap_or(0);
        let format = self.format_of(&state).unwrap_or_default();
        let chosen = chosen < self.profiles.0.len();
        for (item, (enabled, shown)) in control_state(&state, &format, chosen) {
            dialog.items[item].disabled = !enabled;
            dialog.items[item].hidden = !shown;
        }
        if self.archivers.seven.is_none() {
            dialog.items[SEVEN].disabled = true;
        }
        if self.archivers.others.is_empty() {
            dialog.items[OTHER].disabled = true;
        }
        dialog
    }

    /// "Select archiver:": 7z, zip, Other and its list.
    fn archiver_items(&self, layout: &mut Layout) {
        let format = self.start.profile.format.as_str();
        let is = |name: &str| format.eq_ignore_ascii_case(name);
        layout.label("Select archiver:");
        layout.spacer(1);
        layout.boxed(DialogItem::radio("&7z", is("7z") && self.archivers.seven.is_some()).group());
        layout.spacer(1);
        layout.boxed(DialogItem::radio("&zip", is("zip")));
        layout.spacer(1);
        let others = &self.archivers.others;
        let other = others.iter().position(|other| is(&other.name));
        layout.boxed(DialogItem::radio("O&ther:", other.is_some()));
        let mut lines: Vec<&str> = others.iter().map(|other| other.name.as_str()).collect();
        if lines.is_empty() {
            lines.push("");
        }
        layout.combo(&lines, other.unwrap_or(0));
        layout.new_line();
    }

    /// The items from the level on, as for an archive that is there but the volumes' and the
    /// self-extracting archive's rows in place of the existing file's answer.
    fn lower_items(&self, layout: &mut Layout) {
        let start = &self.start;
        let profile = &start.profile;
        layout.label("Compression &level:");
        layout.combo(&names(&LEVELS), level_at(profile.level));
        layout.spacer(2);
        layout.label("&Method:");
        let methods = methods_of(&profile.format);
        layout.combo(methods, method_line(methods, &profile.method));
        layout.spacer(2);
        layout.boxed(DialogItem::check("S&olid", profile.solid));
        layout.new_line();
        layout.label("Advan&ced parameters:");
        let advanced = DialogItem::edit(&profile.advanced).history("arclite.advanced", false);
        layout.edit(advanced, 0);
        layout.new_line();
        layout.separator();
        layout.new_line();
        layout.boxed(DialogItem::check("&Encrypt archive", profile.encrypt));
        layout.spacer(2);
        layout.boxed(DialogItem::check3("Encrypt header", profile.encrypt_header));
        layout.spacer(2);
        layout.boxed(DialogItem::check("Show password", start.show_password));
        layout.new_line();
        layout.label("Pass&word:");
        layout.edit(DialogItem::password(&profile.password), 20);
        layout.spacer(2);
        layout.label("Repeat:");
        layout.edit(DialogItem::password(&profile.password), 20);
        layout.reset_line();
        layout.label("Pass&word:");
        layout.edit(DialogItem::edit(&profile.password), 20);
        layout.new_line();
        layout.separator();
        layout.new_line();
        let sfx = DialogItem::check("Create self-e&xtracting archive", profile.create_sfx);
        layout.boxed(sfx);
        layout.spacer(2);
        layout.boxed(DialogItem::button("SFX options").no_close());
        layout.new_line();
        layout.separator();
        layout.new_line();
        let volumes = DialogItem::check("Split into &volumes", profile.enable_volumes);
        layout.boxed(volumes);
        layout.spacer(2);
        layout.label("Volume size:");
        let size = DialogItem::edit(&profile.volume_size).history("arclite.volume_size", false);
        layout.edit(size, 20);
        layout.new_line();
        layout.separator();
        layout.new_line();
        let moving = DialogItem::check("&Delete files after archiving", profile.move_files);
        layout.boxed(moving);
        layout.spacer(2);
        layout.boxed(DialogItem::check("&Ignore errors", profile.ignore_errors));
        layout.new_line();
        let shared = DialogItem::check("Archive files open for w&riting", start.open_shared);
        layout.boxed(shared);
        layout.spacer(2);
        // FAR's custom filter menu is not the plugins' yet (far-parity §14).
        layout.boxed(DialogItem::check("Use &filter", false).disabled(true));
        layout.new_line();
        layout.separator();
        layout.new_line();
        layout.buttons(vec![
            DialogItem::button("OK").default_button(),
            DialogItem::button("Cancel"),
            DialogItem::button("Save settings").no_close(),
        ]);
        layout.new_line();
    }

    /// The dialog's events (arclite's `dialog_proc` with `new_arc`): a profile chosen fills the items, a
    /// format chosen takes its level, its methods and its extension, the items follow the level,
    /// Encrypt, Show password and the volumes; Save and Delete keep the profiles, Evaluate shows the path
    /// made whole, "Save settings" keeps the answers; OK with answers wrong says why and stays; any
    /// change chooses the profile the answers match.
    pub fn handle(&mut self, host: &Host, event: &DialogEvent, controls: &mut Controls) -> bool {
        match *event {
            DialogEvent::Init => {
                self.apply_state(controls);
                controls.focus(ARC_PATH);
            }
            DialogEvent::Changed(PROFILE) => {
                let chosen = usize::try_from(controls.value(PROFILE).state).unwrap_or(usize::MAX);
                if let Some(profile) = self.profiles.0.get(chosen).cloned() {
                    write_controls(controls, &profile.options, &self.archivers);
                    self.sfx = profile.options.sfx.clone();
                    self.apply_state(controls);
                }
            }
            DialogEvent::Changed(SEVEN | ZIP | OTHER | OTHER_LIST) => self.format_chosen(controls),
            DialogEvent::Changed(LEVEL | ENCRYPT | CREATE_SFX | ENABLE_VOLUMES) => {
                self.apply_state(controls);
            }
            DialogEvent::Changed(SHOW_PASSWORD) => {
                if controls.value(SHOW_PASSWORD).checked() {
                    let typed = controls.value(PASSWORD).text;
                    controls.set_text(PASSWORD_VISIBLE, &typed);
                } else {
                    let typed = controls.value(PASSWORD_VISIBLE).text;
                    controls.set_text(PASSWORD, &typed);
                    controls.set_text(PASSWORD_VERIFY, &typed);
                }
                self.apply_state(controls);
            }
            DialogEvent::Button(SFX_OPTIONS) => {
                let modules = &self.modules;
                if let Some(sfx) = crate::sfx_dialog::ask(host, &self.sfx, &self.profiles, modules)
                {
                    self.sfx = sfx;
                }
                self.apply_state(controls);
            }
            DialogEvent::Button(SAVE_PROFILE) => self.save_profile(host, controls),
            DialogEvent::Button(DELETE_PROFILE) => self.delete_profile(host, controls),
            DialogEvent::Button(EVALUATE) => self.evaluate(host, controls),
            DialogEvent::Button(SAVE) => {
                if let Ok(read) = self.read(&|item| value_of(controls, item)) {
                    read.save(host);
                    host.message(ADD_TITLE, &["Settings saved"]);
                }
                controls.focus(OK);
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
            self.choose_profile(controls);
        }
        true
    }

    /// A format chosen (arclite's `DN_BTNCLICK` of an archiver and `DN_EDITCHANGE` of the other
    /// formats' list): its level kept in `levels`, its methods (the method kept when it has it), the
    /// items' state and the path's extension.
    fn format_chosen(&mut self, controls: &mut Controls) {
        let state = |item: usize| controls.value(item).state;
        let Some(format) = self.format_of(&state) else {
            return;
        };
        if let Some(level) = level_in(&self.levels, &format)
            && let Some(at) = LEVELS.iter().position(|(_, known)| *known == level)
        {
            controls.set_state(LEVEL, i32::try_from(at).unwrap_or(0));
        }
        method_lines(controls, &format);
        self.apply_state(controls);
    }

    /// Evaluate: the path made whole shown in lines of the message's width.
    fn evaluate(&self, host: &Host, controls: &mut Controls) {
        let state = |item: usize| controls.value(item).state;
        let format = self.format_of(&state).unwrap_or_default();
        let ext = self.default_ext(&format, ext_flags(controls));
        let typed = controls.value(ARC_PATH).text;
        let path = self.eval_path_with(&typed, controls.value(APPEND_EXT).checked(), &ext);
        // arclite's `word_wrap` to FAR's `get_optimal_msg_width` (the screen less 20).
        let columns = host.screen().map_or(80, |screen| screen.columns);
        let width = usize::try_from(columns.saturating_sub(20))
            .unwrap_or(60)
            .max(20);
        let lines = wrapped(&path.display().to_string(), width);
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        host.show(&Message::new("", &lines).buttons(&["OK"]));
        controls.focus(ARC_PATH);
    }

    /// arclite's Save: the profile named (the one the answers match offered) takes the answers.
    fn save_profile(&mut self, host: &Host, controls: &mut Controls) {
        let Ok(read) = self.read(&|item| value_of(controls, item)) else {
            return;
        };
        let offered = (self.profiles.by_options(&read.profile))
            .map(|at| self.profiles.0[at].name.clone())
            .unwrap_or_default();
        let Some(name) = host.input(PLUGIN, "Input profile name", &offered) else {
            return;
        };
        if !Profiles::fits(&name) {
            host.message(PLUGIN, &["A profile's name has text and no \\ in it"]);
            return;
        }
        self.profiles.update(&name, &read.profile);
        self.profiles.save(host);
        self.set_profiles(controls, self.profiles.by_name(&name));
        self.apply_state(controls);
    }

    /// arclite's Delete: the profile chosen gone once the user says Yes.
    fn delete_profile(&mut self, host: &Host, controls: &mut Controls) {
        let chosen = usize::try_from(controls.value(PROFILE).state).unwrap_or(usize::MAX);
        if chosen >= self.profiles.0.len() {
            return;
        }
        let question = Message::new(PLUGIN, &["Do you want to delete selected profile?"])
            .button_set(ButtonSet::YesNo);
        if host.show(&question) != Some(0) {
            return;
        }
        self.profiles.0.remove(chosen);
        self.profiles.save(host);
        self.set_profiles(controls, None);
        self.apply_state(controls);
    }

    /// The profiles' list anew, `chosen` (none: the empty line) chosen.
    fn set_profiles(&self, controls: &mut Controls, chosen: Option<usize>) {
        let lines = profile_lines(&self.profiles);
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        controls.set_lines(PROFILE, &lines);
        let at = chosen.unwrap_or(self.profiles.0.len());
        controls.set_state(PROFILE, i32::try_from(at).unwrap_or(0));
    }

    /// The profile the answers match chosen (the empty line when none does).
    fn choose_profile(&mut self, controls: &mut Controls) {
        let matching = (self.read(&|item| value_of(controls, item)).ok())
            .and_then(|read| self.profiles.by_options(&read.profile))
            .unwrap_or(self.profiles.0.len());
        let at = i32::try_from(matching).unwrap_or(0);
        if controls.value(PROFILE).state != at {
            controls.set_state(PROFILE, at);
            self.apply_state(controls);
        }
    }

    /// arclite's `set_control_state`: the level fitted to the format and kept in `levels` (its
    /// `update_level_list`), the path's extension the format's (`change_extension`), the items
    /// enabled and shown as the dialog's values say.
    fn apply_state(&mut self, controls: &mut Controls) {
        let format = self.format_of(&|item| controls.value(item).state);
        let format = format.unwrap_or_default();
        let at = usize::try_from(controls.value(LEVEL).state).unwrap_or(usize::MAX);
        if let Some((_, level)) = LEVELS.get(at) {
            let fitted = fit_level(&format, *level);
            if fitted != *level {
                controls.set_state(LEVEL, i32::try_from(level_at(fitted)).unwrap_or(0));
            }
            self.levels = with_level(&self.levels, &format, fitted);
        }
        let ext = self.default_ext(&format, ext_flags(controls));
        change_extension(controls, &mut self.old_ext, &ext);
        let states: Vec<i32> = (0..=SAVE).map(|item| controls.value(item).state).collect();
        let state = |item: usize| states.get(item).copied().unwrap_or(0);
        let chosen = usize::try_from(state(PROFILE)).unwrap_or(usize::MAX);
        for (item, (enabled, shown)) in
            control_state(&state, &format, chosen < self.profiles.0.len())
        {
            controls.enable(item, enabled);
            controls.show(item, shown);
        }
        controls.enable(SEVEN, self.archivers.seven.is_some());
        controls.enable(OTHER, !self.archivers.others.is_empty());
    }
}

/// The self-extracting archive's and the volumes' check boxes as the extension follows them.
fn ext_flags(controls: &Controls) -> (bool, bool) {
    (
        controls.value(CREATE_SFX).checked(),
        controls.value(ENABLE_VOLUMES).checked(),
    )
}

/// An item's state and text as the dialog holds them now.
fn value_of(controls: &Controls, item: usize) -> (i32, String) {
    let value = controls.value(item);
    (value.state, value.text)
}

#[cfg(test)]
#[path = "create_dialog_tests.rs"]
mod tests;
