//! FAR's way up from a share's root (`FileList::ChangeDir("..")` at a root, its call of the Network
//! plugin by `KnownIDs.Network` with `OPEN_FILEPANEL`): ".." at the root of `\\server\share`, and
//! Ctrl+PgUp there, open the computer in the plugin that has the prefix `net_up` (the shipped network)
//! with the share under the cursor; with "Use Ctrl+PgUp to change drive" in its third state a network
//! drive's root goes to its share so too. A share the plugin does not go up from (not Microsoft's), or no
//! such plugin: the drive menu (FAR's `ChangeDisk`). The listing shows ".." at a share's root only while
//! that plugin is there (FAR's `NetRoot` with the Network plugin found).

use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::keys::WaitKind;
use crate::app::{App, Side};
use crate::platform;
use crate::vfs::local::share_of;

/// The prefix of the plugin the program goes up from a share to.
pub(in crate::app) const SHARE_UP: &str = "net_up";

impl App {
    /// FAR's `ChangeDir("..")` at the root of the panel `side`'s disk: to the network plugin from a
    /// share (from a network drive with `PgUpChangeDisk` 2), else the drive menu.
    pub(in crate::app) fn up_from_root(&mut self, side: Side) {
        let Some(root) = self.panels[side.index()]
            .path
            .as_local()
            .map(Path::to_path_buf)
        else {
            return;
        };
        let mapped = || {
            (self.prefs.ctrl_pg_up_changes_drive == 2)
                .then(|| platform::volumes::network_name(&root.to_string_lossy()))
                .flatten()
        };
        let share = share_of(&root).or_else(mapped);
        match (self.plugins.host.prefix_plugin(SHARE_UP), share) {
            (Some(plugin), Some(share)) => self.ask_share_up(plugin, &share, side),
            _ => self.open_drive_menu(side),
        }
    }

    /// The plugin asked to open at `share`'s computer (not echoed); its "not mine" opens the drive menu.
    fn ask_share_up(&mut self, plugin: usize, share: &str, side: Side) {
        let name = self.plugins.host.plugins[plugin].name.clone();
        let folder = self.far_folder(side);
        let key = KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL);
        let serial = self.wait_for_plugin(WaitKind::ShareUp, &name, side, key);
        let to = (side.index(), serial);
        (self.plugins.host).command(plugin, (SHARE_UP, share), folder.as_deref(), to);
    }

    /// Whether a share's root shows "..": while a plugin goes up from it; the panels told, one shown at a
    /// share's root read again at once (the panels are read before the plugins load, FAR loads them
    /// first; a plugin loaded or unloaded later too).
    pub(super) fn sync_share_parent(&mut self) {
        let shown = self.plugins.host.prefix_plugin(SHARE_UP).is_some();
        for panel in &mut self.panels {
            let at_share = panel.path.as_local().and_then(share_of).is_some();
            if panel.set_share_parent(shown) && at_share {
                // Never waited for: a dead share would hold the screen once more.
                panel.reload_quietly();
            }
        }
    }
}

#[cfg(test)]
#[path = "share_up_tests.rs"]
mod tests;
