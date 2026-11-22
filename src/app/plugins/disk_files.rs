//! Shift+F2 and Shift+F3 on a disk's panel (FAR's `PluginHostGetFiles` and `ProcessHostFile` with
//! `ProcessOneHostFile`): the selected files, or the one under the cursor, one after another, each
//! offered to the plugins that open files in their order (`OpenPluginForFile` with `OFP_EXTRACT` /
//! `OFP_COMMANDS`; folders passed over); the first that opens it gets its panel's root into the
//! destination with `OPM_TOPLEVEL` (silent after that plugin's first file) or hands it to its commands,
//! its panel never shown. A file done is unselected (`ClearLastGetSelection`, `Select(false)`); one no
//! plugin takes is passed over; one not done ends the run. Both panels are read again at its end.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::super::dialog::Dialog;
use super::super::super::{App, Side};
use super::super::getting::target_names;
use super::super::keys::{KeyWait, WaitKind};
use crate::plugin_host::disk_files::{DiskEnd, DiskUse};

/// A run of Shift+F2 or Shift+F3 over a disk's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct DiskRun {
    /// Shift+F2's destination (FAR's `strDestPath`); none for Shift+F3.
    pub destination: Option<String>,
    /// The plugins that got a file of this run (FAR's `UsedPlugins`: the next ones `OPM_SILENT`).
    pub used: Vec<String>,
    /// The panel's folder the files are in.
    pub folder: PathBuf,
    /// The files left by name, the one at hand first.
    pub files: Vec<String>,
}

impl DiskRun {
    fn key(&self) -> KeyEvent {
        let number = if self.destination.is_some() { 2 } else { 3 };
        KeyEvent::new(KeyCode::F(number), KeyModifiers::SHIFT)
    }
}

impl App {
    /// Shift+F2 (`extracting`) or Shift+F3 on a disk's panel: the run started over its files (FAR saves
    /// the selection first).
    pub(in crate::app) fn start_disk_run(&mut self, extracting: bool) {
        let side = self.active;
        let panel = &self.panels[side.index()];
        let Some(folder) = panel.path.as_local().map(Path::to_path_buf) else {
            return;
        };
        let names = target_names(panel, false);
        let Some(first) = names.first() else {
            return;
        };
        let destination = extracting.then(|| self.disk_destination(first));
        // FAR's `OpenPluginForFile`: a folder is offered to none (told by the panel's entry).
        let folders: std::collections::HashSet<&str> = (panel.entries.iter())
            .filter(|entry| entry.is_dir)
            .map(|entry| entry.name.as_str())
            .collect();
        let files: Vec<String> = (names.iter())
            .filter(|name| !folders.contains(name.as_str()))
            .cloned()
            .collect();
        self.panels[side.index()].save_selection();
        let run = DiskRun {
            destination,
            used: Vec::new(),
            folder,
            files,
        };
        self.disk_step(run, side);
    }

    /// Shift+F2's destination (FAR's `PluginHostGetFiles`): the other panel's folder, unless it shows
    /// no files (hidden, or the info, tree or quick view panel) and nothing is selected, or it is no
    /// disk's; then the first file's name without its last extension (made whole by the plugin from
    /// the panel's folder).
    fn disk_destination(&self, first: &str) -> String {
        let passive = self.passive_panel();
        let shown = passive.visible && self.shows_files(self.active.other());
        let selected = !self.active_panel().selected.is_empty();
        let folder = passive.path.as_local().and_then(Path::to_str);
        match folder {
            Some(folder) if shown || selected => folder.to_string(),
            _ => match first.rsplit_once('.') {
                Some((stem, _)) => stem.to_string(),
                None => first.to_string(),
            },
        }
    }

    /// The run's next file offered to its first plugin; a file no plugin opens (or whose path no plugin
    /// can be handed exactly: not Unicode) passed over; none left: the run's end.
    pub(super) fn disk_step(&mut self, mut run: DiskRun, side: Side) {
        while let Some(name) = run.files.first() {
            let exact = run.folder.join(name).to_str().is_some();
            let plugins = if exact {
                self.plugins_for(name)
            } else {
                Vec::new()
            };
            if !plugins.is_empty() {
                return self.ask_disk_plugin(run, plugins, side);
            }
            run.files.remove(0);
        }
        self.disk_run_ended();
    }

    /// The run's file at hand offered to the first of `plugins`, the others after it if it does not
    /// open it.
    pub(super) fn ask_disk_plugin(&mut self, run: DiskRun, mut plugins: Vec<usize>, side: Side) {
        let plugin = plugins.remove(0);
        let name = self.plugins.host.plugins[plugin].name.clone();
        let purpose = match &run.destination {
            Some(destination) => DiskUse::Extract {
                destination: destination.clone(),
                silent: run.used.contains(&name),
            },
            None => DiskUse::Commands,
        };
        let path = run.folder.join(&run.files[0]);
        let folder = run.folder.clone();
        let key = run.key();
        let kind = WaitKind::DiskFile { run, next: plugins };
        let serial = self.wait_for_plugin(kind, &name, side, key);
        let to = (side.index(), serial);
        if !(self.plugins.host).disk_file(plugin, (&path, purpose), Some(&folder), to) {
            self.plugin_key_failed(&name, Some(serial));
        }
    }

    /// A plugin did not take the run's file at hand (FAR's `OpenFilePlugin` asks the next): the next
    /// plugin, then the next file, while the panels are as they were and nothing else is waited for.
    pub(in crate::app::plugins) fn disk_file_passed(&mut self, wait: &KeyWait) {
        let WaitKind::DiskFile { run, next } = &wait.kind else {
            return;
        };
        if !self.run_goes_on(wait) || self.plugins.key_wait.is_some() {
            return self.disk_run_ended();
        }
        let mut run = run.clone();
        if next.is_empty() {
            run.files.remove(0);
            self.disk_step(run, wait.side);
        } else {
            self.ask_disk_plugin(run, next.clone(), wait.side);
        }
    }

    /// The run's file at hand ended with `end` for the press `serial`: done, it is unselected and the
    /// next one goes; not opened, the next one goes; not done, the run ends. A failure is said.
    pub(in crate::app::plugins) fn disk_file_ended(
        &mut self,
        plugin: &str,
        serial: u64,
        (end, error): (DiskEnd, Option<String>),
    ) {
        if let Some(error) = error {
            let title = self.plugin_title(plugin);
            self.show_dialog(Dialog::error(&title, vec![error]));
        }
        let Some(wait) = self.take_key_wait(serial) else {
            return self.disk_run_ended();
        };
        let held = wait.held.keys.clone();
        // Another press waited for meanwhile (the keys let go under the plugin's dialog): the run ends
        // rather than take its place.
        let goes_on =
            end != DiskEnd::NotDone && self.run_goes_on(&wait) && self.plugins.key_wait.is_none();
        match wait.kind {
            WaitKind::DiskFile { mut run, .. } if goes_on => {
                let name = run.files.remove(0);
                if end == DiskEnd::Done {
                    let panel = &mut self.panels[wait.side.index()];
                    if panel.selected.remove(&name) {
                        panel.selection_changed();
                    }
                    if !run.used.iter().any(|used| used == plugin) {
                        run.used.push(plugin.to_string());
                    }
                }
                self.disk_step(run, wait.side);
            }
            _ => self.disk_run_ended(),
        }
        self.release_plugin_keys(held);
    }

    /// Whether the run goes on after its file at hand: its panel still on the folder it started in (FAR
    /// runs it all in one call; the keys pressed meanwhile are held, a window of the plugin's may still
    /// be up as its call ends).
    fn run_goes_on(&self, wait: &KeyWait) -> bool {
        self.panels[wait.side.index()].path == wait.path
    }

    /// The run's end (FAR's `Update(UPDATE_KEEP_SELECTION)` of both panels).
    pub(in crate::app::plugins) fn disk_run_ended(&mut self) {
        for panel in &mut self.panels {
            panel.reload();
        }
    }
}
