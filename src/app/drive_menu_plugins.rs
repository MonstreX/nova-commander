//! The plugins' items in the drive menu (FAR's `DiskMenuStrings`, `diskmenu.cpp` `AddPluginItems`): under
//! the drives after a separator, each with the hotkey the user gave it (F4: `plugins::hotkeys`), else one
//! of its own (owner 2026-11-22, over FAR's none: FAR's first-start key when it is free, else the first
//! free digit, then letter no drive has), by name or, with "Sort plugins by hotkey", by hotkey; Ctrl+7
//! shows or hides them. Enter or the hotkey runs the item on its plugin's thread (until a plugin has panels, 5c). The
//! menu's lines are the drives, the separator, then the items: `selected` counts all of them.

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::DriveMenu;
use crate::app::console_input::Mouse;
use crate::app::plugins::{DRIVE_MENU_HOTKEYS, HotkeyEdit, HotkeyMenu};
use crate::app::{App, Side, vmenu_keys};
use crate::vfs::local::Drive;

/// A plugin's item in the drive menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskItem {
    /// The plugin's place in the host's list.
    pub plugin: usize,
    /// The item's index among the plugin's.
    pub index: usize,
    pub label: String,
    /// The hotkey the user gave it.
    pub hotkey: Option<char>,
}

impl DriveMenu {
    /// The drives, and the separator and the items when there are items.
    pub fn line_count(&self) -> usize {
        let items = self.plugins.len();
        self.drives.len() + if items == 0 { 0 } else { items + 1 }
    }

    /// The plugin's item on line `line`.
    pub fn plugin_at(&self, line: usize) -> Option<&DiskItem> {
        self.plugins.get(line.checked_sub(self.drives.len() + 1)?)
    }

    /// Whether line `line` can be selected: a drive or an item, not the separator.
    pub fn choosable(&self, line: usize) -> bool {
        line < self.drives.len() || self.plugin_at(line).is_some()
    }

    /// The separator's line marked, as the menus' keys take them.
    fn skipped(&self) -> Vec<bool> {
        (0..self.line_count())
            .map(|line| !self.choosable(line))
            .collect()
    }

    /// The line next to `line` up or down (`forward`) past the separator, round from the ends.
    pub fn step(&self, line: usize, forward: bool) -> usize {
        let key = if forward { KeyCode::Down } else { KeyCode::Up };
        vmenu_keys::step(&self.skipped(), line, key, 1)
    }

    /// `line` if it can be selected, else the nearest one above (the drive over the separator when a
    /// drive went), else below; past the end the last line.
    pub fn fit(&self, line: usize) -> usize {
        vmenu_keys::last_from(&self.skipped(), line)
    }

    /// The line of the item with hotkey `key`, in either case (FAR's `VMenu` compares them upper case).
    pub fn hotkey_line(&self, key: char) -> Option<usize> {
        let same = |hotkey: char| hotkey.to_lowercase().eq(key.to_lowercase());
        let at = (self.plugins.iter()).position(|item| item.hotkey.is_some_and(same))?;
        Some(self.drives.len() + 1 + at)
    }
}

impl App {
    /// The plugins' items as the menu over `drives` shows them: none when Ctrl+7 hides them.
    pub(super) fn disk_items(&self, drives: &[Drive]) -> Vec<DiskItem> {
        if !self.config.drive_columns.plugins {
            return Vec::new();
        }
        let plugins = &self.plugins.host.plugins;
        let mut items: Vec<DiskItem> = (plugins.iter())
            .enumerate()
            .flat_map(|(plugin, loaded)| {
                (loaded.disks.iter().enumerate()).map(move |(index, label)| DiskItem {
                    plugin,
                    index,
                    label: label.clone(),
                    hotkey: self.plugin_hotkey(plugin, HotkeyMenu::Disks, label),
                })
            })
            .collect();
        // FAR's `diskmenu.cpp:198-210`: `string_sort::less` of the texts as the plugins give them, `&`
        // and all (review R42); the keys of their own given in that order.
        items.sort_by(|a, b| crate::platform::files::compare_names(&a.label, &b.label));
        let far_key = |item: &DiskItem| {
            let name = &plugins[item.plugin].name;
            (DRIVE_MENU_HOTKEYS.iter())
                .find(|(plugin, label, _)| plugin == name && *label == item.label)
                .and_then(|(_, _, key)| key.chars().next())
        };
        give_free_hotkeys(&mut items, drives, far_key);
        if self.config.drive_columns.sort_plugins_by_hotkey {
            // FAR's: by hotkey, equal ones by name.
            items.sort_by(|a, b| {
                (a.hotkey.map(|key| key.to_ascii_uppercase()))
                    .cmp(&b.hotkey.map(|key| key.to_ascii_uppercase()))
                    .then_with(|| crate::platform::files::compare_names(&a.label, &b.label))
            });
        }
        items
    }

    /// F4 on an item: FAR's "Assign plugin hot key", the menu back at `selected` afterwards.
    pub(super) fn ask_disk_hotkey(&mut self, item: &DiskItem, side: Side, selected: usize) {
        self.ask_plugin_hotkey(HotkeyEdit {
            plugin: item.plugin,
            menu: HotkeyMenu::Disks,
            label: item.label.clone(),
            selected,
            side,
        });
    }

    /// FAR's Apps and Shift+Apps on a drive (`held` the modifiers; others not taken: false): EMenu with
    /// the drive's `root`; the menu back once its call ends (FAR's stays under it), at once without the
    /// plugin.
    pub(super) fn drive_context_menu(
        &mut self,
        menu: &DriveMenu,
        root: &str,
        held: KeyModifiers,
    ) -> bool {
        if held.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return false;
        }
        match self.context_menu_key(Some(root), menu.side, None) {
            Some(plugin) => self.plugins.drive_menu_under = Some((plugin, menu.clone())),
            None => self.overlay = Some(super::Overlay::DriveMenu(menu.clone())),
        }
        true
    }

    /// FAR's right click (Shift or nothing held) on the drive at line `index`: EMenu for it at the mouse,
    /// the menu back once its call ends; false when no plugin gives the menu or it is no drive's line.
    pub(in crate::app) fn drive_right_click(&mut self, index: usize, mouse: &Mouse) -> bool {
        let plain = !(mouse.modifiers).intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let menu_plugin = (self.plugins.host).prefix_plugin(crate::app::plugins::CLICK_PREFIX);
        let Some(super::Overlay::DriveMenu(menu)) = &self.overlay else {
            return false;
        };
        let Some(drive) = menu
            .drives
            .get(index)
            .filter(|_| plain && menu_plugin.is_some())
        else {
            return false;
        };
        let root = drive.root.clone();
        let mut menu = menu.clone();
        menu.selected = index;
        self.overlay = None;
        self.mouse.menu = None;
        self.mouse.menu_down = None;
        let click = Some((mouse.column, mouse.row));
        match self.context_menu_key(Some(&root), menu.side, click) {
            Some(plugin) => self.plugins.drive_menu_under = Some((plugin, menu)),
            None => self.overlay = Some(super::Overlay::DriveMenu(menu)),
        }
        true
    }

    /// An item chosen in the menu of `side`: the menu goes and the plugin runs it on its thread (a panel
    /// it opens opens there).
    pub(super) fn open_plugin_disk(&mut self, item: &DiskItem, side: Side) {
        let (plugin, index) = (item.plugin, item.index);
        self.run_plugin_item(plugin, side, |host, serial| {
            host.open_disk(plugin, index, side.index(), serial)
        });
    }
}

/// The owner's rule over FAR (2026-11-22: FAR leaves an item without the user's key blank): each item
/// without a hotkey gets `far_key`'s when no other item or drive has it, else the first free of `0`-`9`,
/// then of the letters no drive in the menu has (a drive's letter is its key); none when all are taken.
fn give_free_hotkeys(
    items: &mut [DiskItem],
    drives: &[Drive],
    far_key: impl Fn(&DiskItem) -> Option<char>,
) {
    let mut taken: Vec<char> = (drives.iter().filter_map(|drive| drive.letter))
        .chain(items.iter().filter_map(|item| item.hotkey))
        .map(|key| key.to_ascii_uppercase())
        .collect();
    for item in items.iter_mut().filter(|item| item.hotkey.is_none()) {
        if let Some(key) = far_key(item).filter(|key| !taken.contains(key)) {
            item.hotkey = Some(key);
            taken.push(key);
        }
    }
    for item in items.iter_mut().filter(|item| item.hotkey.is_none()) {
        let Some(key) = ('0'..='9')
            .chain('A'..='Z')
            .find(|key| !taken.contains(key))
        else {
            return;
        };
        item.hotkey = Some(key);
        taken.push(key);
    }
}

#[cfg(test)]
#[path = "drive_menu_plugins_tests.rs"]
mod tests;
