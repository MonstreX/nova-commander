//! The plugins' settings and configurations (ADR-0016 §14), as FAR's `PluginSettings`,
//! `PluginManager::Configure` and `ConfigureCurrent` (`plugins.cpp`): a value a plugin sets is kept in
//! the program's settings as `plugins.<name>.settings.<key>` and saved with them; Options > Plugins
//! configuration lists every plugin's configurations (`plugins::hotkeys`), Enter runs one on its plugin's
//! thread and the menu comes back when it ends, closing only on Esc (FAR's loop); F11's Shift+F9
//! configures the item's plugin and F11 comes back, its Alt+Shift+F9 opens Plugins configuration with F11
//! back after it. A configuration that changed the settings has the plugin's items read again and the
//! plugins' panels shown read again (`Update(UPDATE_KEEP_SELECTION)`).

use super::super::App;
use super::{HotkeyMenu, Pending};
use crate::plugin_host::{Event, PluginItems};

impl App {
    /// Options > Plugins configuration.
    pub(in crate::app) fn open_plugin_configs(&mut self) {
        self.plugins.commands_back = None;
        self.open_program_plugin_menu(HotkeyMenu::Config, 0);
    }

    /// The configuration `index` of plugin `plugin` runs on its thread; `back` (the menu and its line)
    /// comes back when it ends. A plugin without it: the menu at once (FAR's `has(iConfigure)`).
    pub(in crate::app) fn configure_plugin(
        &mut self,
        plugin: usize,
        index: usize,
        back: (HotkeyMenu, usize),
    ) {
        let Some(loaded) = self.plugins.host.plugins.get(plugin) else {
            return;
        };
        if index >= loaded.configs.len() {
            return self.open_program_plugin_menu(back.0, back.1);
        }
        self.plugins.configuring = Some((loaded.name.clone(), back.0, back.1));
        let side = self.command_side();
        self.run_plugin_item(plugin, side, |host, serial| {
            host.configure(plugin, index, side.index(), serial)
        });
    }

    /// A value plugin `plugin` set, kept to be saved with the settings.
    pub(super) fn keep_plugin_setting(&mut self, plugin: &str, key: &str, value: String) {
        let key = format!("plugins.{plugin}.settings.{key}");
        self.plugins.kept_settings.insert(key, value.into());
    }

    /// A configuration of plugin `plugin` ended. With items (the settings changed) they replace its items
    /// and every plugin's panel shown is read again (FAR's `ConfigureCurrent`: each `PLUGIN_PANEL`); the
    /// menu it was run from comes back in its turn, after the windows it showed. Its own failure is
    /// shown instead, and no menu after it.
    /// `serial`: the configuration's call; its failure ends only that call's wait.
    pub(super) fn plugin_configured(
        &mut self,
        plugin: &str,
        result: Result<Option<PluginItems>, String>,
        serial: Option<u64>,
    ) {
        // Another plugin's configuration waited for stays waited for.
        let back =
            (self.plugins.configuring).take_if(|(name, ..)| name.eq_ignore_ascii_case(plugin));
        let items = match result {
            Ok(items) => items,
            Err(error) => {
                let failed = Event::Failed {
                    plugin: plugin.to_string(),
                    error,
                    serial,
                };
                self.plugins.waiting.push_back(Pending::Event(failed));
                return;
            }
        };
        if let Some(items) = items {
            if let Some(loaded) = (self.plugins.host.plugins.iter_mut())
                .find(|loaded| loaded.name.eq_ignore_ascii_case(plugin))
            {
                loaded.set_items(items);
            }
            self.reload_all_plugin_panels();
        }
        if let Some((_, menu, selected)) = back {
            self.plugins
                .waiting
                .push_back(Pending::MenuBack(menu, selected));
        }
    }

    /// A configuration's menu back: over its window without buttons, never over anything else the user
    /// opened meanwhile. False when it did not come.
    pub(super) fn plugin_menu_back(&mut self, menu: HotkeyMenu, selected: usize) -> bool {
        if self.overlay.is_some() && !self.plugin_busy_shown() {
            return false;
        }
        self.plugins.busy = None;
        self.open_program_plugin_menu(menu, selected);
        true
    }

    /// F11's Alt+Shift+F9 (FAR's `CommandsMenu`): "Plugins configuration", F11 back at `selected` when it
    /// closes.
    pub(in crate::app) fn open_plugin_configs_from_commands(&mut self, selected: usize) {
        self.plugins.commands_back = Some(selected);
        self.open_program_plugin_menu(HotkeyMenu::Config, 0);
    }

    /// Esc in "Plugins configuration": F11 back when it was opened from there.
    pub(in crate::app) fn plugin_configs_closed(&mut self) {
        if let Some(selected) = self.plugins.commands_back.take() {
            self.open_program_plugin_menu(HotkeyMenu::Commands, selected);
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
