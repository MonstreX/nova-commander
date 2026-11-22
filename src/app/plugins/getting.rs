//! A plugin's panel whose plugin writes its files out itself (FAR's `GetFilesW`, API 5, ADR-0021; arclite):
//! F5 / F6 to a disk hand it the items and the other panel's folder, its own dialog instead of the
//! program's (FAR's `PluginGetFiles`), the selection going once it is done and both panels read again;
//! F3 / F4 have it write the file into a folder of the program's first (FAR's `GetFile` with `OPM_VIEW` /
//! `OPM_EDIT` and `OPM_SILENT`), then the viewer or the editor opens that copy (`extracted`). Each is a
//! press the program waits for, as a panel key is: the keys after it held, Esc asking it to stop.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::dialog::Dialog;
use super::super::transfer_dialog::Operation;
use super::super::{App, Side};
use super::keys::WaitKind;
use crate::plugin_host::Event;
use crate::plugin_host::getting::{GET_EDIT, GET_SILENT, GET_VIEW, GetFiles};
use crate::vfs::VfsPath;

/// A file written out for the viewer or the editor: the program's folder it goes into, where it came
/// from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct GetView {
    pub folder: PathBuf,
    pub origin: VfsPath,
    pub edit: bool,
}

/// What files written out by the plugin are for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) enum Getting {
    /// The viewer or the editor (F3 / F4).
    View(GetView),
    /// F5 / F6; Shift+F5 / Shift+F6 keep the selection (FAR's `ReturnCurrentFile`).
    Copy { keep_selection: bool },
    /// Enter or (`alternative`) Ctrl+PgDn on the file `name`, its copy into `folder` (FAR's
    /// `ProcessEnter` with `PluginMode`): then offered to the plugins.
    Open {
        folder: PathBuf,
        name: String,
        alternative: bool,
    },
    /// F8 by the plugin itself (FAR's `DeleteFilesW`): the panels read again as it ends.
    Delete,
    /// F7 by the plugin itself (FAR's `MakeDirectoryW`): the cursor to the folder made.
    MakeFolder,
    /// Shift+F3 (FAR's `ProcessHostFileW`): the entries `names` handed to the plugin's commands on the
    /// panel's file, unselected or the selection gone as it says.
    HostFile { names: Vec<String> },
}

/// The items F5 / F6 take, by name, in the panel's order: the selected ones, else the one under the
/// cursor (not ".."); with `current_only` (Shift+F5) the one under the cursor whatever is selected.
pub(super) fn target_names(panel: &super::super::Panel, current_only: bool) -> Vec<String> {
    if panel.selected.is_empty() || current_only {
        return (panel.current())
            .filter(|entry| !entry.is_parent)
            .map(|entry| entry.name.clone())
            .into_iter()
            .collect();
    }
    (panel.entries.iter())
        .filter(|entry| panel.selected.contains(&entry.name))
        .map(|entry| entry.name.clone())
        .collect()
}

/// A path as the plugin gets it: its text exactly (one that is not UTF-8 is none).
fn exact(path: &Path) -> Option<String> {
    path.to_str().map(str::to_string)
}

impl App {
    /// F5 / F6 on a panel whose plugin writes its files out (FAR's `GetFiles` with no
    /// `OPIF_EXTERNALGET`), not into another plugin's panel: sent to the plugin (true), the program's
    /// dialog not shown.
    pub(in crate::app) fn get_from_plugin(
        &mut self,
        operation: Operation,
        to_plugin: bool,
    ) -> bool {
        self.get_out(operation, to_plugin, false)
    }

    /// Shift+F5 / Shift+F6 on such a panel (FAR's `KEY_SHIFTF5` without real names: `ProcessCopyKeys`
    /// with `ReturnCurrentFile`): the entry under the cursor alone, the selection kept.
    pub(in crate::app) fn get_current_from_plugin(
        &mut self,
        operation: Operation,
        to_plugin: bool,
    ) -> bool {
        self.get_out(operation, to_plugin, true)
    }

    fn get_out(&mut self, operation: Operation, to_plugin: bool, current_only: bool) -> bool {
        let side = self.active;
        let panel = &self.panels[side.index()];
        if operation == Operation::Link
            || to_plugin
            || !panel.look.gives_files
            || !self.plugins.host.is_plugin_panel(&panel.path)
        {
            return false;
        }
        let names = target_names(panel, current_only);
        let (Some(destination), false) = (self.get_destination(), names.is_empty()) else {
            return true;
        };
        let files = GetFiles {
            names,
            destination,
            moving: operation == Operation::Move,
            mode: 0,
            folder: self.get_folder(side),
        };
        let key = match operation {
            Operation::Move => KeyCode::F(6),
            _ => KeyCode::F(5),
        };
        let getting = Getting::Copy {
            keep_selection: current_only,
        };
        self.send_get(side, files, getting, KeyEvent::from(key));
        true
    }

    /// The disk's folder F5 / F6 hand the plugin: the other panel's (FAR's `GetCurDir`), the active
    /// panel's disk folder when an info panel or a quick view stands there, the archive's name without
    /// its extension when the other panel is hidden (FAR's `HostFile`, then made whole by the plugin).
    fn get_destination(&self) -> Option<String> {
        let active = self.active_panel();
        let passive = self.passive_panel();
        if !passive.visible {
            let host = (active.host_file()).map(|name| match name.rsplit_once('.') {
                Some((stem, _)) if !stem.is_empty() => stem.to_string(),
                _ => name.to_string(),
            });
            return host.or_else(|| exact(active.local_folder()?));
        }
        let target = VfsPath::parse(&self.transfer_target_dir());
        let folder = target.as_local().or_else(|| active.local_folder())?;
        exact(folder)
    }

    /// The program's folder for names given without one: the disk's folder the panel's plugin was
    /// opened from (FAR's current folder).
    fn get_folder(&self, side: Side) -> String {
        (self.panels[side.index()].before_plugins())
            .and_then(|entered| entered.from.as_local())
            .and_then(exact)
            .unwrap_or_default()
    }

    /// F3 / F4 on the file `origin` (named `name`) of such a panel: written into a new folder of the
    /// program's, then viewed or edited (true); false when it is no such panel's.
    pub(in crate::app) fn get_to_view(
        &mut self,
        origin: VfsPath,
        name: String,
        edit: bool,
    ) -> bool {
        let side = self.active;
        let panel = &self.panels[side.index()];
        if !panel.look.gives_files || !self.plugins.host.is_plugin_panel(&panel.path) {
            return false;
        }
        let title = if edit { "Edit" } else { "View" };
        let made = super::super::extracted::new_folder();
        let folder = match made {
            Ok(folder) => folder,
            Err(error) => {
                self.show_error(title, &error);
                return true;
            }
        };
        let Some(destination) = exact(&folder) else {
            let _ = std::fs::remove_dir_all(&folder);
            return true;
        };
        let files = GetFiles {
            names: vec![name],
            destination,
            moving: false,
            mode: GET_SILENT | if edit { GET_EDIT } else { GET_VIEW },
            folder: self.get_folder(side),
        };
        let view = GetView {
            folder,
            origin,
            edit,
        };
        let key = KeyEvent::from(KeyCode::F(if edit { 4 } else { 3 }));
        self.send_get(side, files, Getting::View(view), key);
        true
    }

    /// Enter or (`alternative`) Ctrl+PgDn on a file of such a panel (FAR's `ProcessEnter` with
    /// `PluginMode`: `GetFile` with `OPM_SILENT | OPM_EDIT` into a folder of its own): written out by the
    /// plugin, then offered to the plugins of its masks (true); false when it is no such panel's file.
    pub(in crate::app) fn get_to_open(&mut self, alternative: bool) -> bool {
        let side = self.active;
        let panel = &self.panels[side.index()];
        let Some(entry) = (panel.current())
            .filter(|entry| !entry.is_dir && !entry.is_parent && entry.real_path.is_none())
        else {
            return false;
        };
        if !panel.look.gives_files || !self.plugins.host.is_plugin_panel(&panel.path) {
            return false;
        }
        let name = entry.name.clone();
        let title = if alternative { "Open" } else { "Run" };
        let folder = match super::super::extracted::new_folder() {
            Ok(folder) => folder,
            Err(error) => {
                self.show_error(title, &error);
                return true;
            }
        };
        let Some(destination) = exact(&folder) else {
            let _ = std::fs::remove_dir_all(&folder);
            return true;
        };
        let files = GetFiles {
            names: vec![name.clone()],
            destination,
            moving: false,
            mode: GET_SILENT | GET_EDIT,
            folder: self.get_folder(side),
        };
        let key = match alternative {
            true => KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL),
            false => KeyEvent::from(KeyCode::Enter),
        };
        let getting = Getting::Open {
            folder,
            name,
            alternative,
        };
        self.send_get(side, files, getting, key);
        true
    }

    /// The copy for Enter / Ctrl+PgDn written: offered to the plugins (its folder's first file).
    fn got_to_open(&mut self, folder: &Path, name: &str, alternative: bool) {
        let listed = std::fs::read_dir(folder).into_iter().flatten();
        let first = (listed.flatten())
            .find(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .map(|entry| entry.path());
        match first {
            Some(copy) => self.open_copy_by_plugins(
                (copy, name.to_string()),
                folder.to_path_buf(),
                alternative,
            ),
            None => {
                let _ = std::fs::remove_dir_all(folder);
            }
        }
    }

    /// `files` of the panel `side` sent to its plugin as the press `key`, waited for.
    fn send_get(&mut self, side: Side, files: GetFiles, getting: Getting, key: KeyEvent) {
        let path = self.panels[side.index()].path.clone();
        let plugin = path.provider().unwrap_or_default().to_string();
        let serial = self.plugins.key_serial + 1;
        if !self.plugins.host.get(&path, files, serial) {
            match getting {
                Getting::View(GetView { folder, .. }) | Getting::Open { folder, .. } => {
                    let _ = std::fs::remove_dir_all(&folder);
                }
                Getting::Copy { .. }
                | Getting::Delete
                | Getting::MakeFolder
                | Getting::HostFile { .. } => {}
            }
            return;
        }
        self.wait_for_plugin(WaitKind::Get(getting), &plugin, side, key);
    }

    /// The plugin's files written out for the press `serial` (FAR's `GetFilesW` returned): a copy done
    /// takes the selection of its panel, both panels read again; a file for the viewer or the editor
    /// opens; a stop or a failure (said so) leaves the program's folder gone. The keys held go on.
    pub(super) fn plugin_got(&mut self, event: Event) {
        let Event::Got {
            plugin,
            serial,
            result,
        } = event
        else {
            return;
        };
        let wait = self.take_key_wait(serial);
        let done = matches!(result, Ok(true));
        let held = match wait {
            Some(wait) => {
                match &wait.kind {
                    WaitKind::Get(Getting::View(view)) if done => self.open_got(view.clone()),
                    WaitKind::Get(Getting::View(view)) => {
                        let _ = std::fs::remove_dir_all(&view.folder);
                    }
                    WaitKind::Get(Getting::Copy { keep_selection }) => {
                        self.copied_out(wait.side, done && !keep_selection);
                    }
                    WaitKind::Get(Getting::Delete) => self.copied_out(wait.side, false),
                    WaitKind::Get(Getting::Open {
                        folder,
                        name,
                        alternative,
                    }) if done => self.got_to_open(folder, name, *alternative),
                    WaitKind::Get(Getting::Open { folder, .. }) => {
                        let _ = std::fs::remove_dir_all(folder);
                    }
                    _ => {}
                }
                wait.held.keys
            }
            // Not waited for any more (Esc let it go, the plugin failed or started anew): the panels
            // read again all the same, as FAR's `PluginGetFiles` ends.
            None => {
                self.copied_out(self.active, false);
                Vec::new()
            }
        };
        if let Err(error) = result {
            let title = self.plugin_title(&plugin);
            self.show_dialog(Dialog::error(&title, vec![error]));
        }
        self.release_plugin_keys(held);
    }

    /// F5 / F6 by the plugin ended: `clear`, the panel's selection goes (FAR's `ClearSelection` when
    /// done); both panels read again either way, the selection kept.
    pub(super) fn copied_out(&mut self, side: Side, clear: bool) {
        if clear {
            let panel = &mut self.panels[side.index()];
            panel.selected.clear();
            panel.selection_changed();
        }
        for panel in &mut self.panels {
            panel.reload();
        }
    }
}
