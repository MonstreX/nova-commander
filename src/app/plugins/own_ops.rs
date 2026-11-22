//! F8 and F7 on a plugin's panel whose plugin deletes and makes folders itself (FAR's `PluginDelete`
//! with `DeleteFilesW` and F7's `MakeDirectoryW`, API 5; arclite): the program shows no question of its
//! own, the plugin asks; each is a press the program waits for as a panel key is. Deleting ended, both
//! panels are read again; a folder made, the panel is read again with the cursor on it (FAR's
//! `GoToFile`).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::App;
use super::super::dialog::Dialog;
use super::getting::{Getting, target_names};
use super::keys::{KeyWait, WaitKind};
use crate::plugin_host::Event;

/// Shift+F2 and Shift+F3 on a plugin's panel opened from a file.
#[path = "host_file.rs"]
mod host_file;

/// Shift+F2 and Shift+F3 on a disk's files.
#[path = "disk_files.rs"]
pub(super) mod disk_files;

impl App {
    /// The wait for the press `serial`, taken: the one waited for, or one a window of its call let go
    /// (its answer applied all the same); none when it is neither.
    pub(super) fn take_key_wait(&mut self, serial: u64) -> Option<KeyWait> {
        if let Some(wait) = self.plugins.key_wait.take_if(|wait| wait.serial == serial) {
            return Some(wait);
        }
        let late = &mut self.plugins.key_late;
        let at = late.iter().position(|wait| wait.serial == serial)?;
        Some(late.remove(at))
    }

    /// The plugin's key call failed: its wait ends, the keys after it go on (the failure is shown); a
    /// run of Shift+F2 / Shift+F3 it was in ends with the panels read again. `serial`: the command's
    /// number when the failure ends one (only its wait ends).
    pub(super) fn plugin_key_failed(&mut self, plugin: &str, serial: Option<u64>) {
        // Its wait, or every wait of the plugin: ended, a file's copy with it.
        let ends = |wait: &KeyWait| match serial {
            Some(serial) => wait.serial == serial,
            None => wait.plugin.eq_ignore_ascii_case(plugin),
        };
        let run = |wait: &KeyWait| matches!(wait.kind, WaitKind::DiskFile { .. });
        let mut runs = false;
        (self.plugins.key_late).retain(|wait| {
            !ends(wait) || {
                runs |= run(wait);
                super::opening::let_wait_go(wait)
            }
        });
        if let Some(wait) = self.plugins.key_wait.take_if(|wait| ends(wait)) {
            runs |= run(&wait);
            super::opening::let_wait_go(&wait);
            self.release_plugin_keys(wait.held.keys);
        }
        if runs {
            self.disk_run_ended();
        }
    }

    /// A folder made, a panel's file's commands, Shift+F1's new panel or a disk's file offered ended by
    /// the plugin itself.
    pub(super) fn own_ended(&mut self, event: Event) {
        match event {
            event @ Event::Processed { .. } => self.plugin_processed(event),
            Event::Created { serial, done, .. } => self.plugin_created(serial, done),
            Event::DiskFile {
                plugin,
                serial,
                end,
                error,
            } => self.disk_file_ended(&plugin, serial, (end, error)),
            event => self.plugin_made(event),
        }
    }

    /// F8 (Shift+F8: `current_only`) on a panel whose plugin deletes itself: its entries sent to it
    /// (true), the program's question not shown.
    pub(in crate::app) fn delete_by_plugin(&mut self, current_only: bool) -> bool {
        let side = self.active;
        let panel = &self.panels[side.index()];
        if !panel.look.deletes || !self.plugins.host.is_plugin_panel(&panel.path) {
            return false;
        }
        let names = target_names(panel, current_only);
        if names.is_empty() {
            return true;
        }
        let path = panel.path.clone();
        let plugin = path.provider().unwrap_or_default().to_string();
        let serial = self.plugins.key_serial + 1;
        if self.plugins.host.delete_files(&path, names, serial) {
            let key = KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE);
            self.wait_for_plugin(WaitKind::Get(Getting::Delete), &plugin, side, key);
        }
        true
    }

    /// F7 on a panel whose plugin makes folders itself: asked of it (true), the program's "Make
    /// directory" not shown.
    pub(in crate::app) fn make_folder_by_plugin(&mut self) -> bool {
        let side = self.active;
        let panel = &self.panels[side.index()];
        if !panel.look.makes_folders || !self.plugins.host.is_plugin_panel(&panel.path) {
            return false;
        }
        let path = panel.path.clone();
        let plugin = path.provider().unwrap_or_default().to_string();
        let serial = self.plugins.key_serial + 1;
        if self.plugins.host.make_folder(&path, serial) {
            let key = KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE);
            self.wait_for_plugin(WaitKind::Get(Getting::MakeFolder), &plugin, side, key);
        }
        true
    }

    /// A folder made by the plugin for the press `serial` (FAR's `MakeDirectoryW` returned): the panel it
    /// was asked of read again with the cursor on it; a failure said; the keys held go on.
    pub(super) fn plugin_made(&mut self, event: Event) {
        let Event::Made {
            plugin,
            serial,
            result,
        } = event
        else {
            return;
        };
        let wait = self.take_key_wait(serial);
        let held = match wait {
            Some(wait) => {
                let panel = &mut self.panels[wait.side.index()];
                match &result {
                    // FAR's `PointToName`: the cursor on the last part of the name made.
                    Ok(Some(name)) if panel.path == wait.path => {
                        panel.reload_onto(name.rsplit(['/', '\\']).next().unwrap_or(name));
                    }
                    _ => panel.reload(),
                }
                wait.held.keys
            }
            // Not waited for any more (Esc let it go, the plugin started anew): its panels read again
            // all the same, as FAR's `Update` after `MakeDirectory`.
            None => {
                for panel in &mut self.panels {
                    if panel.path.provider() == Some(plugin.as_str()) {
                        panel.reload();
                    }
                }
                Vec::new()
            }
        };
        if let Err(error) = result {
            let title = self.plugin_title(&plugin);
            self.show_dialog(Dialog::error(&title, vec![error]));
        }
        self.release_plugin_keys(held);
    }
}

#[cfg(test)]
#[path = "own_ops_tests.rs"]
mod tests;
