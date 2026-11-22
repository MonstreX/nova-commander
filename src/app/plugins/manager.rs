//! Plugins loaded and unloaded while the program runs (P6.0e, the owner's: FAR has only `PluginsControl`,
//! farcmds' `load:` / `unload:`). Options > Plugins lists the plugins' folders by name, each state at the
//! right in brackets in one column - loaded, starting, disabled (turned off), not loaded (a new one too) -,
//! a menu as F11's with its keys in the bottom frame (owner, 2026-11-15): Enter or Space loads or unloads one (kept for the next start), Ctrl+R starts
//! it anew (one hanging in a call is left where it hangs, as Ctrl+R in its wait's window does). A plugin let
//! go takes its panels back to the folders they came from and its windows, waits, requests and keys with
//! it; a start's failure is told as it comes.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::Pending;
use crate::app::dialog::{Dialog, ListAction, PluginRow, PluginState};
use crate::app::{App, Overlay, Side};
use crate::plugin_host::Started;
use crate::plugin_host::panel_requests::PluginsAction;

/// The list's title.
const TITLE: &str = "Plugins";
/// The list's keys, in its bottom frame.
pub const PLUGINS_KEYS: &str = "Enter-Load/Unload, Ctrl+R-Restart";
/// The columns between a name and its state.
const GAP: usize = 3;

/// The line of a call's wait window once it does not answer Esc.
pub(super) const RESTART: &str = "Ctrl+R: start the plugin anew";

impl App {
    /// Options > Plugins, the cursor on line `cursor`.
    pub(in crate::app) fn open_plugin_manager(&mut self, cursor: usize) {
        let rows = self.plugin_rows();
        let items = plugin_lines(&rows);
        let cursor = cursor.min(items.len().saturating_sub(1));
        self.show_dialog(Dialog::List {
            title: TITLE.to_string(),
            items,
            cursor,
            top: std::cell::Cell::default(),
            action: ListAction::Plugins(rows),
        });
    }

    /// Each plugin's folder by its name: loaded, starting, turned off in the list, or not loaded.
    fn plugin_rows(&self) -> Vec<PluginRow> {
        let host = &self.plugins.host;
        let row = |name: &str, state| PluginRow {
            name: name.to_string(),
            state,
        };
        let off = |name: &str| {
            let key = format!("plugins.{name}.enabled");
            self.plugins.kept_settings.get(&key) == Some(&serde_json::Value::Bool(false))
        };
        let mut rows: Vec<PluginRow> = (host.plugins.iter())
            .map(|plugin| row(&plugin.name, PluginState::Loaded))
            .chain((host.starting_names().iter()).map(|name| row(name, PluginState::Starting)))
            .chain((host.not_loaded().iter()).map(|(name, _)| match off(name) {
                true => row(name, PluginState::Disabled),
                false => row(name, PluginState::NotLoaded),
            }))
            .collect();
        rows.sort_by_key(|row| row.name.to_lowercase());
        rows
    }

    /// Enter or Space on the list's line `index`: its plugin unloaded, or loaded; the list again.
    pub(in crate::app) fn toggle_listed_plugin(&mut self, rows: &[PluginRow], index: usize) {
        if let Some(name) = rows.get(index).map(|row| row.name.as_str()) {
            if self.let_plugin_go(name).is_some() {
                self.set_plugin_enabled(name, false);
            } else if let Some((_, place)) = (self.plugins.host.not_loaded().into_iter())
                .find(|(found, _)| found.eq_ignore_ascii_case(name))
            {
                self.load_plugin_from(&place);
            }
        }
        self.open_plugin_manager(index);
    }

    /// Ctrl+R on the list's line `index`: its plugin started anew; the list again.
    pub(in crate::app) fn restart_listed_plugin(&mut self, rows: &[PluginRow], index: usize) {
        if let Some(name) = rows.get(index).map(|row| row.name.as_str()) {
            self.restart_plugin(name);
        }
        self.open_plugin_manager(index);
    }

    /// Ctrl+R in the window of a call that does not answer Esc: its plugin started anew, the call left
    /// where it hangs. Whether it was.
    pub(super) fn restart_hanging(&mut self, key: KeyEvent) -> bool {
        let restart = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
        let Some(wait) =
            (self.plugins.key_wait.as_ref()).filter(|wait| key == restart && wait.held.asking())
        else {
            return false;
        };
        let plugin = wait.plugin.clone();
        self.restart_plugin(&plugin);
        true
    }

    /// The plugin `name` let go and started anew from its folder.
    pub(in crate::app) fn restart_plugin(&mut self, name: &str) {
        if let Some(place) = self.let_plugin_go(name) {
            self.load_plugin_from(&place);
        }
    }

    /// The plugin in `place` started; a refusal shown.
    pub(in crate::app) fn load_plugin_from(&mut self, place: &std::path::Path) {
        let settings = self.plugins.kept_settings.clone();
        if let Err(reason) = self.plugins.host.start_plugin(place, &settings) {
            self.show_dialog(Dialog::error("Plugins", vec![reason]));
        }
    }

    /// The plugin `name` let go: its panels back on the folders they came from, its windows, waits,
    /// requests and keys forgotten, its plugin unloaded, then the keys held after its call done. Its
    /// folder; none when it is not loaded.
    pub(in crate::app) fn let_plugin_go(&mut self, name: &str) -> Option<PathBuf> {
        if !self.plugins.host.is_loaded(name) {
            return None;
        }
        // The program's menus of the plugins' items name them by their places in the host's list.
        let by_places = match &self.overlay {
            Some(Overlay::DriveMenu(_)) => true,
            Some(Overlay::PluginMenu(menu)) => menu.commands.is_some(),
            _ => false,
        };
        if by_places {
            self.overlay = None;
            self.plugins.commands_back = None;
        }
        let ours = |path: &crate::vfs::VfsPath| {
            path.provider()
                .is_some_and(|id| id.eq_ignore_ascii_case(name))
        };
        for side in [Side::Left, Side::Right] {
            let panel = &mut self.panels[side.index()];
            panel.plugin_stack_mut().retain(|entered| {
                let kept = !ours(&entered.from);
                // A copy one of its panels was opened from goes with it, once it has let it go.
                let copy = (entered.host.as_ref()).and_then(|host| host.temp.clone());
                if let Some(copy) = copy.filter(|_| !kept) {
                    super::super::panel::forget_copy(copy);
                }
                kept
            });
            if ours(&panel.path) {
                self.close_plugin_panel(side, None);
            }
        }
        let plugins = &mut self.plugins;
        plugins.panels.retain(|path| !ours(path));
        plugins.opening.retain(|(path, _)| !ours(path));
        // Its copies and moves stop: a new instance would take their names (R7 review B7).
        self.tasks.cancel_on_provider(name);
        self.forget_plugin(name);
        let place = self.plugins.host.unload(name);
        // Done once it is gone: a key held may open a menu of the plugins' items or call one.
        self.plugin_key_failed(name, None);
        place
    }

    /// What the program keeps of the plugin `name` forgotten: its window up (Esc), its dialogs, its
    /// requests, posted keys and events not taken, its configuration, put and window without buttons.
    fn forget_plugin(&mut self, name: &str) {
        let ours = |plugin: &str| plugin.eq_ignore_ascii_case(name);
        if self.plugins.reply_of.as_deref().is_some_and(ours) {
            if self.plugin_window_shown() {
                self.overlay = None;
            }
            self.answer_plugin(crate::plugin_host::windows::Answer::Esc);
        }
        self.drop_dialogs_of(name);
        let plugins = &mut self.plugins;
        plugins.panel_asks.retain(|ask| !ours(&ask.plugin));
        plugins.gone.push(name.to_string());
        plugins.posted_keys.retain(|(plugin, _)| !ours(plugin));
        plugins.unload_after.retain(|plugin| !ours(plugin));
        (plugins.drive_menu_under).take_if(|(plugin, _)| ours(plugin));
        plugins
            .waiting
            .retain(|pending| !matches!(pending, Pending::Event(event) if ours(event.plugin())));
        (plugins.configuring).take_if(|(plugin, ..)| ours(plugin));
        (plugins.putting).take_if(|putting| ours(&putting.plugin));
        if plugins.busy.as_deref().is_some_and(ours) {
            plugins.busy = None;
            if self.plugin_busy_shown() {
                self.overlay = None;
            }
        }
    }

    /// A plugin's `PluginsControl` (`asker` the plugin asking): the plugin named `text` (or, given an
    /// absolute path, of that folder, manifest or library) found, loaded or unloaded; whether it was (a
    /// load: started). The plugin asking unloads itself as its call ends (FAR's `UnloadPluginExternal`).
    pub(super) fn plugins_control(
        &mut self,
        asker: &str,
        action: PluginsAction,
        text: &str,
    ) -> bool {
        let text = text.trim();
        let path = std::path::Path::new(text);
        let place = if !path.is_absolute() {
            None
        } else if path.is_file() {
            path.parent().map(std::path::Path::to_path_buf)
        } else {
            path.is_dir().then(|| path.to_path_buf())
        };
        let host = &self.plugins.host;
        let loaded = (host.plugins.iter())
            .find(|plugin| plugin.name.eq_ignore_ascii_case(text))
            .or_else(|| {
                let place = place.as_ref()?;
                (host.plugins.iter())
                    .find(|plugin| crate::platform::files::same_place(&plugin.place, place))
            })
            .map(|plugin| plugin.name.clone());
        match (action, loaded) {
            (PluginsAction::Find, loaded) => loaded.is_some(),
            (PluginsAction::Unload, Some(name)) if name.eq_ignore_ascii_case(asker) => {
                self.plugins.unload_after.push(name);
                true
            }
            (PluginsAction::Unload, loaded) => {
                loaded.is_some_and(|name| self.let_plugin_go(&name).is_some())
            }
            (PluginsAction::Load, Some(_)) => false,
            (PluginsAction::Load, None) => {
                let place = (host.not_loaded().into_iter())
                    .find(|(name, _)| name.eq_ignore_ascii_case(text))
                    .map(|(_, place)| place)
                    .or(place);
                let settings = self.plugins.kept_settings.clone();
                place.is_some_and(|place| self.plugins.host.start_plugin(&place, &settings).is_ok())
            }
        }
    }

    /// The plugins that asked to be unloaded themselves let go once their calls have ended; whether one
    /// was.
    fn unload_ended_calls(&mut self) -> bool {
        let host = &self.plugins.host;
        let (ended, still): (Vec<String>, Vec<String>) =
            (std::mem::take(&mut self.plugins.unload_after).into_iter())
                .partition(|name| !host.in_call(name));
        self.plugins.unload_after = still;
        let mut any = false;
        for name in ended {
            any |= self.let_plugin_go(&name).is_some();
        }
        any
    }

    /// The plugins asking to be unloaded let go as their calls end; the plugins started since told: a
    /// loaded one in the list when it is up, a failure shown.
    pub(super) fn pump_plugin_starts(&mut self) -> bool {
        let unloaded = self.unload_ended_calls();
        let told = self.plugins.host.pump_starts();
        if told.is_empty() {
            return unloaded;
        }
        for started in told {
            match started {
                Started::Loaded(name) => self.set_plugin_enabled(&name, true),
                Started::Failed(failure) => {
                    self.plugins.waiting.push_back(Pending::Failure(failure))
                }
            }
        }
        if let Some(Overlay::Dialog(Dialog::List {
            action: ListAction::Plugins(_),
            cursor,
            ..
        })) = &self.overlay
        {
            let cursor = *cursor;
            self.overlay = None;
            self.open_plugin_manager(cursor);
        }
        true
    }

    /// The plugin `name` loaded at the next start or not (`plugins.<name>.enabled`).
    fn set_plugin_enabled(&mut self, name: &str, enabled: bool) {
        let key = format!("plugins.{name}.enabled");
        if enabled {
            self.plugins.kept_settings.remove(&key);
        } else {
            (self.plugins.kept_settings).insert(key, serde_json::Value::Bool(false));
        }
    }
}

/// The list's lines: the names at the left, the states at the right in one column (the text sizes the box;
/// the drawing puts each state at its right edge).
pub fn plugin_lines(rows: &[PluginRow]) -> Vec<String> {
    let names = rows.iter().map(|row| row.name.chars().count()).max();
    let states = rows.iter().map(|row| row.state.shown().len()).max();
    let (names, states) = (names.unwrap_or(0), states.unwrap_or(0));
    (rows.iter())
        .map(|row| {
            let (name, state) = (&row.name, row.state.shown());
            format!("{name:<names$}{:GAP$}{state:>states$}", "")
        })
        .collect()
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
