//! Shift+F1 on a disk's panel (FAR's `PluginPutFilesToNew`, `filelist.cpp`): the selected files, or the
//! one under the cursor, offered to the plugins that open files, in their order (FAR's `OpenFilePlugin`
//! with no file, `OFP_CREATE`; with FAR's default confirmations no menu asks): the first that makes a new
//! panel of its own takes them into it (its `PutFilesW`; arclite asks for the new archive), the panel
//! closed after. Waited for as a panel key is; once done the selection goes and the panel is read again
//! (FAR's `ClearSelection`, `Update(UPDATE_KEEP_SELECTION)`).

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::{App, Side};
use super::getting::target_names;
use super::keys::WaitKind;

impl App {
    /// Shift+F1: the active panel's files on a disk to the first plugin that makes a new panel for them.
    pub(in crate::app) fn add_to_new_by_plugin(&mut self) {
        let panel = self.active_panel();
        // FAR: real files only (a disk's, or a plugin's panel with `OPIF_REALNAMES`).
        let Some(folder) = panel.path.as_local().map(std::path::Path::to_path_buf) else {
            return;
        };
        let names = target_names(panel, false);
        if names.is_empty() {
            return;
        }
        let sources: Vec<PathBuf> = names.iter().map(|name| folder.join(name)).collect();
        // FAR asks every plugin that opens files (`AnalyseW`, `OpenFilePlugin`): here those with masks.
        let plugins: Vec<usize> = (self.plugins.host.plugins.iter().enumerate())
            .filter(|(_, plugin)| !plugin.file_masks.is_empty())
            .map(|(index, _)| index)
            .collect();
        let side = self.active;
        self.ask_to_new(plugins, (sources, folder), side);
    }

    /// The first of `plugins` asked to make a new panel for `sources` of `folder` from the panel
    /// `side`, the others after it if it makes none.
    pub(super) fn ask_to_new(
        &mut self,
        mut plugins: Vec<usize>,
        (sources, folder): (Vec<PathBuf>, PathBuf),
        side: Side,
    ) {
        if plugins.is_empty() {
            return;
        }
        let plugin = plugins.remove(0);
        let name = self.plugins.host.plugins[plugin].name.clone();
        let kind = WaitKind::ToNew {
            sources: sources.clone(),
            folder: folder.clone(),
            next: plugins,
        };
        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::SHIFT);
        let serial = self.wait_for_plugin(kind, &name, side, key);
        if !(self.plugins.host).create(plugin, (&sources, &folder), (side.index(), serial)) {
            // A path the plugin cannot be handed exactly, or the plugin gone: nothing sent.
            self.plugin_key_failed(&name, Some(serial));
        }
    }

    /// Shift+F1's files put into a plugin's new panel for the press `serial` (`done` false: the user
    /// stopped it): the selection gone once done, the panel read again, the keys held go on.
    pub(super) fn plugin_created(&mut self, serial: u64, done: bool) {
        let Some(wait) = self.take_key_wait(serial) else {
            self.active_panel_mut().reload();
            return;
        };
        let panel = &mut self.panels[wait.side.index()];
        if done && panel.path == wait.path {
            panel.clear_selection();
        }
        panel.reload();
        self.release_plugin_keys(wait.held.keys);
    }
}
