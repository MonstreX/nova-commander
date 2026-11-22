//! Shift+F2 and Shift+F3 on a plugin's panel opened from a file (FAR's `KEY_SHIFTF2` with the plugin's
//! `HostFile`: F5's copy out; `ProcessHostFile` with the panel's `m_HostFile`): Shift+F3 hands the
//! panel's entries, the selected ones or the one under the cursor, to the plugin's commands on that file
//! (FAR's `ProcessHostFileW`; arclite's menu "Test archive files"), waited for as a panel key is. Once
//! it ends the selection goes and both panels are read again when the plugin changed something (FAR's
//! `ClearSelection`, `Update`), the entries handed are unselected when it says so (FAR's
//! `PluginClearSelection`). A disk's files come with S6.2e.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::super::App;
use super::super::super::dialog::Dialog;
use super::super::getting::{Getting, target_names};
use super::super::keys::WaitKind;
use crate::plugin_host::Event;
use crate::plugin_host::getting::{HOST_FILE_CHANGED, HOST_FILE_UNSELECT};

impl App {
    /// Shift+F2: on a plugin's panel opened from a file, or whose plugin writes its files out (FAR: the
    /// plugin names its `HostFile`), F5's copy out of it.
    pub(in crate::app) fn extract_from_host_file(&mut self) {
        let panel = self.active_panel();
        if !self.plugins.host.is_plugin_panel(&panel.path) {
            // A disk's files: each to the plugin that opens it (FAR's `PluginHostGetFiles`).
            return self.start_disk_run(true);
        }
        if panel.host_file().is_some() || panel.look.gives_files {
            self.ask_copy();
        }
    }

    /// Shift+F3 ("Archive commands") on a plugin's panel opened from a file: its entries handed to the
    /// plugin's commands on that file.
    pub(in crate::app) fn host_file_commands(&mut self) {
        let side = self.active;
        let panel = &self.panels[side.index()];
        if panel.path.as_local().is_some() {
            // A disk's files: each to the plugin that opens it (FAR's `ProcessOneHostFile`).
            return self.start_disk_run(false);
        }
        if panel.host_file().is_none() || !self.plugins.host.is_plugin_panel(&panel.path) {
            return;
        }
        let names = target_names(panel, false);
        if names.is_empty() {
            return;
        }
        let path = panel.path.clone();
        let plugin = path.provider().unwrap_or_default().to_string();
        let serial = self.plugins.key_serial + 1;
        if (self.plugins.host).process_host_file(&path, names.clone(), serial) {
            let key = KeyEvent::new(KeyCode::F(3), KeyModifiers::SHIFT);
            let kind = WaitKind::Get(Getting::HostFile { names });
            self.wait_for_plugin(kind, &plugin, side, key);
        }
    }

    /// The plugin's commands on its panel's file ended for the press `serial`: the selection gone and
    /// both panels read again when it changed something, the entries unselected when it says so; a
    /// failure said; the keys held go on.
    pub(in crate::app::plugins) fn plugin_processed(&mut self, event: Event) {
        let Event::Processed {
            plugin,
            serial,
            result,
        } = event
        else {
            return;
        };
        let bits = result.as_ref().map_or(0, |bits| *bits);
        let changed = bits & HOST_FILE_CHANGED != 0;
        let held = match self.take_key_wait(serial) {
            Some(wait) => {
                let panel = &mut self.panels[wait.side.index()];
                if let WaitKind::Get(Getting::HostFile { names }) = &wait.kind
                    && panel.path == wait.path
                {
                    if changed {
                        panel.clear_selection();
                    } else if bits & HOST_FILE_UNSELECT != 0 {
                        // FAR's `PluginClearSelection` saves the selection first (Ctrl+M back).
                        panel.save_selection();
                        for name in names {
                            panel.selected.remove(name);
                        }
                        panel.selection_changed();
                    }
                }
                if changed {
                    for panel in &mut self.panels {
                        panel.reload();
                    }
                }
                wait.held.keys
            }
            // Not waited for any more: its panels read again when it changed something.
            None => {
                for panel in &mut self.panels {
                    if changed && panel.path.provider() == Some(plugin.as_str()) {
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
