//! The plugin's settings (FAR's Network `Options::Read`, `Options::Write`, `Config`): under FAR's names
//! with FAR's defaults; the configuration dialog in FAR's order. FAR's box around "Hidden Shares" is a
//! titled separator here (the plugins' dialogs have no box of their own).

use nc_plugin_sdk::{Closed, Dialog, DialogItem, Host};

/// The plugin's title: the Plugins menu's item, the configuration's (FAR's `MNetMenu`).
pub const TITLE: &str = "Network browser";
/// The Disks menu's item (FAR's `MDiskMenuString`).
pub const DISK_ITEM: &str = "network";
/// The panel's kind and its title's start (FAR's `MNetwork`).
pub const FORMAT: &str = "Net";

/// Which hidden shares (`$`) a server's folder lists (FAR's `HiddenShares`, `HiddenSharesAsHidden`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HiddenShares {
    Never,
    /// Listed with the hidden attribute: shown as the panel shows hidden files.
    AsHidden,
    Always,
}

/// FAR's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub add_to_disks: bool,
    pub add_to_plugins: bool,
    /// "Show '..' in the providers list" (FAR's `NoRootDoublePoint`, whose name says the opposite).
    pub root_double_point: bool,
    /// The panel opens first in this computer's domain or workgroup.
    pub local_network: bool,
    pub show_printers: bool,
    /// Domains and networks checked by opening them before they are entered.
    pub scan_network: bool,
    /// A share named by its whole path (`\\SERVER\share`), else by its name.
    pub full_path_shares: bool,
    pub hidden_shares: HiddenShares,
    /// FAR's `FavoritesFlags` (P6.6d).
    pub favorites_flags: i64,
    /// `net:name` goes to the domain `name` rather than to the computer (FAR's `NavigateToDomains`,
    /// not in the dialog).
    pub navigate_to_domains: bool,
}

/// FAR's `FAVORITES_UPBROWSE_TO_FAVORITES`, its `FAVORITES_DEFAULTS`.
pub const UPBROWSE_TO_FAVORITES: i64 = 1;
const FAVORITES_DEFAULTS: i64 = UPBROWSE_TO_FAVORITES;

impl Default for Options {
    /// FAR's defaults.
    fn default() -> Self {
        Self {
            add_to_disks: true,
            add_to_plugins: true,
            root_double_point: true,
            local_network: true,
            show_printers: false,
            scan_network: true,
            full_path_shares: true,
            hidden_shares: HiddenShares::AsHidden,
            favorites_flags: FAVORITES_DEFAULTS,
            navigate_to_domains: false,
        }
    }
}

/// The dialog's items by index.
const ADD_TO_DISKS: usize = 0;
const ADD_TO_PLUGINS: usize = 1;
const ROOT_DOUBLE_POINT: usize = 2;
const LOCAL_NETWORK: usize = 3;
const SHOW_PRINTERS: usize = 4;
const SCAN_NETWORK: usize = 5;
const FULL_PATH: usize = 7;
const NEVER: usize = 9;
const AS_HIDDEN: usize = 10;
const UPBROWSE: usize = 13;
/// "Ok".
pub const OK: usize = 15;

impl Options {
    pub fn load(host: &Host) -> Self {
        let defaults = Self::default();
        let flag = |name: &str, default: bool| host.bool_setting(name, default);
        let shown = flag("HiddenShares", true);
        let as_hidden = flag("HiddenSharesAsHidden", true);
        Self {
            add_to_disks: flag("AddToDisksMenu", defaults.add_to_disks),
            add_to_plugins: flag("AddToPluginsMenu", defaults.add_to_plugins),
            root_double_point: flag("NoRootDoublePoint", defaults.root_double_point),
            local_network: flag("LocalNetwork", defaults.local_network),
            show_printers: flag("ShowPrinters", defaults.show_printers),
            scan_network: flag("ScanNetwork", defaults.scan_network),
            full_path_shares: flag("FullPathShares", defaults.full_path_shares),
            hidden_shares: match (shown, as_hidden) {
                (false, _) => HiddenShares::Never,
                (true, true) => HiddenShares::AsHidden,
                (true, false) => HiddenShares::Always,
            },
            favorites_flags: host.int_setting("FavoritesFlags", defaults.favorites_flags),
            navigate_to_domains: flag("NavigateToDomains", defaults.navigate_to_domains),
        }
    }

    /// FAR's `Options::Write` (`NavigateToDomains` is only read).
    pub fn save(&self, host: &Host) {
        host.set_bool("AddToDisksMenu", self.add_to_disks);
        host.set_bool("AddToPluginsMenu", self.add_to_plugins);
        host.set_bool("LocalNetwork", self.local_network);
        host.set_bool("HiddenShares", self.hidden_shares != HiddenShares::Never);
        host.set_bool("ShowPrinters", self.show_printers);
        host.set_bool("FullPathShares", self.full_path_shares);
        host.set_int("FavoritesFlags", self.favorites_flags);
        host.set_bool("NoRootDoublePoint", self.root_double_point);
        host.set_bool(
            "HiddenSharesAsHidden",
            self.hidden_shares == HiddenShares::AsHidden,
        );
        host.set_bool("ScanNetwork", self.scan_network);
    }

    /// FAR's `Config`: its check boxes, "Shares" and "Favorites" parts, the hidden shares' radio buttons.
    pub fn dialog(&self) -> Dialog {
        let check = |label: &str, on: bool, y: i32| {
            let x2 = 5 + i32::try_from(label.len()).unwrap_or(0) + 4;
            DialogItem::check(label, on).at(5, y, x2, y)
        };
        let radio = |label: &str, on: bool, y: i32| {
            let x2 = 5 + i32::try_from(label.len()).unwrap_or(0) + 4;
            DialogItem::radio(label, on).at(5, y, x2, y)
        };
        let titled = |title: &str, y: i32| {
            let mut item = DialogItem::separator().at(-1, y, -1, y);
            item.text = title.to_string();
            item
        };
        let hidden = self.hidden_shares;
        let items = vec![
            check("Add to &Disks menu", self.add_to_disks, 2),
            check("Add to &Plugins menu", self.add_to_plugins, 3),
            check(
                "Show \"&..\" in the providers list",
                self.root_double_point,
                4,
            ),
            check("Show &local network", self.local_network, 5),
            check("Show p&rinters", self.show_printers, 6),
            check("Scan network", self.scan_network, 7),
            titled("Shares", 8),
            check("Show &full share name", self.full_path_shares, 9),
            titled("Hidden Shares", 10),
            radio("&Never show", hidden == HiddenShares::Never, 11).group(),
            radio("&Show as hidden", hidden == HiddenShares::AsHidden, 12),
            radio("&Always show", hidden == HiddenShares::Always, 13),
            titled("Favorites", 14),
            check(
                "&Browse up to favorites if possible",
                self.favorites_flags & UPBROWSE_TO_FAVORITES != 0,
                15,
            ),
            DialogItem::separator().at(-1, 16, -1, 16),
            DialogItem::button("&Ok")
                .default_button()
                .at(13, 17, 20, 17),
            DialogItem::button("&Cancel").at(22, 17, 33, 17),
        ];
        Dialog::new("Network browser", items).size(48, 20)
    }

    /// The settings the dialog closed by "Ok" gives.
    pub fn from_dialog(&self, closed: &Closed) -> Option<Self> {
        if closed.button != Some(OK) {
            return None;
        }
        let on = |item: usize| closed.values.get(item).is_some_and(|value| value.checked());
        let hidden_shares = if on(NEVER) {
            HiddenShares::Never
        } else if on(AS_HIDDEN) {
            HiddenShares::AsHidden
        } else {
            HiddenShares::Always
        };
        let upbrowse = if on(UPBROWSE) {
            self.favorites_flags | UPBROWSE_TO_FAVORITES
        } else {
            self.favorites_flags & !UPBROWSE_TO_FAVORITES
        };
        Some(Self {
            add_to_disks: on(ADD_TO_DISKS),
            add_to_plugins: on(ADD_TO_PLUGINS),
            root_double_point: on(ROOT_DOUBLE_POINT),
            local_network: on(LOCAL_NETWORK),
            show_printers: on(SHOW_PRINTERS),
            scan_network: on(SCAN_NETWORK),
            full_path_shares: on(FULL_PATH),
            hidden_shares,
            favorites_flags: upbrowse,
            navigate_to_domains: self.navigate_to_domains,
        })
    }
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
