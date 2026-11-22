//! A plugin's items asked again as a menu of them opens (FAR's `GetPluginInfo`, called each time F11,
//! the drive menu or "Plugins configuration" is built): each plugin is sent a quiet call for them, and
//! the menu waits for the answers a short while only (ADR-0019: the screen never waits for a plugin).
//! An answer that comes later is taken as it comes, for the next menu; a plugin still asked is not
//! asked again.

use std::sync::mpsc::{RecvTimeoutError, TryRecvError, channel};
use std::time::{Duration, Instant};

use super::worker::Job;
use super::{Plugin, PluginHost};

/// A plugin's items in the program's menus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginItems {
    /// In the plugin commands menu (F11).
    pub commands: Vec<String>,
    /// In the drive menu (Alt+F1 / Alt+F2).
    pub disks: Vec<String>,
    /// In Options > Plugins configuration (FAR's `PluginConfig`).
    pub configs: Vec<String>,
    /// Its command line prefixes (FAR's `CommandPrefix`), without the `:`.
    pub prefixes: Vec<String>,
    /// The masks of the files it opens (Enter, Ctrl+PgDn).
    pub file_masks: Vec<String>,
    /// It takes Alt+F7's found files (FAR's `SetFindListW`).
    pub find_list: bool,
    /// In the editor's plugin commands menu (F11 in the editor, API 10).
    pub editor: Vec<String>,
    /// It takes the editor's keys (FAR's `ProcessEditorInputW`, API 10).
    pub editor_keys: bool,
}

impl PluginHost {
    /// Every plugin asked for its items anew, the answers waited for until `wait` has passed: those in
    /// time taken now, a late one by [`PluginHost::pump_items`]. Whether any plugin's items changed.
    pub fn refresh_items(&mut self, wait: Duration) -> bool {
        // Each idle plugin asked first, all of them waited for together: a busy one never takes the
        // others' time. One asked before and not answered yet is busy: looked for, not waited for.
        let asked: Vec<bool> = (self.plugins.iter_mut())
            .map(|plugin| plugin.items_asked.is_none() && plugin.ask_items())
            .collect();
        let deadline = Instant::now() + wait;
        let mut changed = false;
        for (plugin, asked) in self.plugins.iter_mut().zip(asked) {
            changed |= plugin.take_items(asked.then_some(deadline));
        }
        changed
    }

    /// The late answers taken (each tick). Whether any plugin's items changed.
    pub fn pump_items(&mut self) -> bool {
        let mut changed = false;
        for plugin in &mut self.plugins {
            changed |= plugin.take_items(None);
        }
        changed
    }
}

impl Plugin {
    /// Its items asked on its thread; false when the plugin is gone.
    fn ask_items(&mut self) -> bool {
        let (reply, answer) = channel();
        let sent = self.worker.send(Job::Items { reply }).is_some();
        self.items_asked = sent.then_some(answer);
        sent
    }

    /// The answer to the items asked, waited for `until` then (none: only looked for).
    fn take_items(&mut self, until: Option<Instant>) -> bool {
        let Some(answer) = &self.items_asked else {
            return false;
        };
        let got = match until {
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                (answer.recv_timeout(left)).map_err(|error| error == RecvTimeoutError::Disconnected)
            }
            None => answer
                .try_recv()
                .map_err(|error| error == TryRecvError::Disconnected),
        };
        match got {
            Ok(items) => {
                self.items_asked = None;
                self.set_items(items)
            }
            Err(gone) => {
                if gone {
                    self.items_asked = None;
                }
                false
            }
        }
    }

    /// Its items as it tells them now; whether they changed.
    pub fn set_items(&mut self, items: PluginItems) -> bool {
        if self.items() == items {
            return false;
        }
        self.commands = items.commands;
        self.disks = items.disks;
        self.configs = items.configs;
        self.prefixes = items.prefixes;
        self.file_masks = items.file_masks;
        self.find_list = items.find_list;
        self.editor = items.editor;
        self.editor_keys = items.editor_keys;
        true
    }

    fn items(&self) -> PluginItems {
        PluginItems {
            commands: self.commands.clone(),
            disks: self.disks.clone(),
            configs: self.configs.clone(),
            prefixes: self.prefixes.clone(),
            file_masks: self.file_masks.clone(),
            find_list: self.find_list,
            editor: self.editor.clone(),
            editor_keys: self.editor_keys,
        }
    }
}

#[cfg(test)]
#[path = "items_tests.rs"]
mod tests;
