//! The panel's keys for connections and names (FAR's Network `ProcessKey`, `DeleteFiles`,
//! `ManualConnect`, `FileNames2Clipboard`): on a computer's shares F5 maps them to the first free
//! letters, F6 to the letters asked for, Shift with them not restored at logon; F8 (Del, Shift+Del,
//! Alt+Del; Shift+F8 the one under the cursor) ends a share's disk connection; F4 connects with a name
//! and password asked for and enters (FAR's "ManCon"); Ctrl+F puts the name into the command line,
//! Alt+Shift+Ins and Ctrl+Alt+Ins the names onto the clipboard (".." the folder's own).

use nc_plugin_sdk::system::network::{
    DISPLAY_DOMAIN, DISPLAY_SERVER, ERROR_CANCELLED, NetResource,
};
use nc_plugin_sdk::{KeyAnswer, KeyGo, KeyPress, PanelKey, Which};

use crate::NetworkPlugin;
use crate::asking::{SystemConnections, Windows};
use crate::browse::{is_share, join};
use crate::mapping::{self, Connections, Disconnecting, quoted};

/// FAR's `DisconnectMode` setting: "Reconnect at Logon" as last left.
const DISCONNECT_MODE: &str = "DisconnectMode";
/// The keys that end connections (FAR's `DeleteFiles` keys).
pub const UNMAP_KEYS: [&str; 5] = ["F8", "Shift+F8", "Del", "Shift+Del", "Alt+Del"];

/// The keys and FAR's labels on a folder of `display` (a computer's: its shares; the Favorites folder:
/// F8 "Remove", FAR's `MF8Fav`).
pub fn connection_keys(display: Option<u32>, favorites: bool) -> Vec<PanelKey> {
    let server = display == Some(DISPLAY_SERVER);
    let label = |text: &'static str, shown: bool| if shown { text } else { "" };
    let mut keys = vec![
        PanelKey::new(
            "F4",
            label("ManCon", server || display == Some(DISPLAY_DOMAIN)),
        ),
        PanelKey::new("F5", label("Map", server)),
        PanelKey::new("F6", label("MapTo", server)),
        PanelKey::new("Shift+F5", label("TMap", server)),
        PanelKey::new("Shift+F6", label("TMapTo", server)),
    ];
    keys.extend(UNMAP_KEYS.iter().map(|key| {
        let label = match *key {
            "F8" if favorites => "Remove",
            "F8" => label("Unmap", server),
            _ => "",
        };
        PanelKey::new(key, label)
    }));
    keys.extend(
        ["Ctrl+F", "Alt+Shift+Ins", "Ctrl+Alt+Ins"]
            .iter()
            .map(|key| PanelKey::new(key, "")),
    );
    keys
}

/// The entries a key acts on: Shift+F8 the one under the cursor only, the others the selection or
/// else that one; none on "..".
fn targets(press: &KeyPress) -> Vec<String> {
    if press.key == "Shift+F8" {
        press.current.iter().cloned().collect()
    } else {
        press.targets()
    }
}

/// FAR's Ctrl+F (`FileNames2Clipboard` to the command line): the name under the cursor (on "..", `own`:
/// the folder's), quoted when it has a space, and a space.
pub fn command_line_text(press: &KeyPress, own: &str) -> String {
    let name = press.current.as_deref().unwrap_or(own);
    format!("{} ", quoted(name))
}

/// FAR's Alt+Shift+Ins and Ctrl+Alt+Ins: the names (the selection, else the one under the cursor; on
/// "..", `own`), each quoted when it has a space, a line each.
pub fn clipboard_text(press: &KeyPress, own: &str) -> String {
    let mut names = press.targets();
    if names.is_empty() {
        names.push(own.to_string());
    }
    let names: Vec<String> = names.iter().map(|name| quoted(name)).collect();
    names.join("\r\n")
}

impl NetworkPlugin {
    /// The connection and name keys; none of them: `None`.
    pub(crate) fn connection_key(&mut self, path: &str, press: &KeyPress) -> Option<KeyAnswer> {
        let key = press.key.as_str();
        Some(match key {
            "F5" | "F6" | "Shift+F5" | "Shift+F6" => {
                self.map_shares(path, press, key.ends_with("F6"), !key.starts_with("Shift"))
            }
            "F4" => self.manual_connect(path, press),
            "Ctrl+F" => {
                let own = self.own_name(path);
                self.host
                    .insert_command_line(&command_line_text(press, &own));
                KeyAnswer::default()
            }
            "Alt+Shift+Ins" | "Ctrl+Alt+Ins" => {
                let own = self.own_name(path);
                self.host.set_clipboard(&clipboard_text(press, &own));
                KeyAnswer::default()
            }
            _ if UNMAP_KEYS.contains(&key) && crate::favorite_keys::in_folder(path) => {
                self.remove_favorites(&targets(press))
            }
            _ if UNMAP_KEYS.contains(&key) => self.unmap_shares(path, press),
            _ => return None,
        })
    }

    /// The folder's resource, as its listing found it.
    fn folder_resource(&mut self, path: &str) -> Option<NetResource> {
        (self.browser.resolve(
            &crate::favorites::favored(&self.net, &self.favorites.borrow()),
            &self.options,
            path,
        ))
        .ok()
        .flatten()
    }

    /// FAR's name for ".." (`FileNames2Clipboard`): the folder's remote name, `.\` at the top.
    fn own_name(&mut self, path: &str) -> String {
        self.folder_resource(path)
            .map_or_else(|| ".\\".to_string(), |folder| folder.remote_name)
    }

    /// The remote name of the entry `name` (a share's `\\SERVER\share` whatever the names shown).
    fn remote_of(&mut self, path: &str, name: &str) -> String {
        let found = self.browser.entry(
            &crate::favorites::favored(&self.net, &self.favorites.borrow()),
            &self.options,
            path,
            name,
        );
        found
            .ok()
            .flatten()
            .map_or_else(|| name.to_string(), |resource| resource.remote_name)
    }

    /// F5, F6 (FAR's `MapNetworkDrive` for each share, the first refused leaving the rest): only on a
    /// computer's shares; the panel read again.
    fn map_shares(
        &mut self,
        path: &str,
        press: &KeyPress,
        choose: bool,
        permanent: bool,
    ) -> KeyAnswer {
        let on_server = self
            .folder_resource(path)
            .is_some_and(|folder| folder.display == DISPLAY_SERVER);
        if !on_server {
            return KeyAnswer::default();
        }
        for name in press.targets() {
            let remote = self.remote_of(path, &name);
            let windows = Windows {
                host: self.host,
                last: &self.last_logon,
                favorites: &self.favorites,
            };
            if !mapping::map(&SystemConnections, &windows, &remote, choose, permanent) {
                break;
            }
        }
        // FAR's `FCTL_UPDATEPANEL` without keeping the selection.
        self.host.select_all(Which::Active, false);
        KeyAnswer::reread()
    }

    /// F8 (FAR's `DeleteFiles`): each share connected as a disk disconnected, the first refused leaving
    /// the rest; "Reconnect at Logon" saved as a remembered disk's question left it.
    fn unmap_shares(&mut self, path: &str, press: &KeyPress) -> KeyAnswer {
        let connected = SystemConnections.connections();
        let confirm = self.host.confirms("RemoveConnection").unwrap_or(true);
        for name in targets(press) {
            let remote = self.remote_of(path, &name);
            let mapped = (connected.iter()).any(|one| {
                !one.local_name.is_empty() && one.remote_name.eq_ignore_ascii_case(&remote)
            });
            if !mapped {
                continue;
            }
            let settings = Disconnecting {
                confirm,
                reconnect: self.host.bool_setting(DISCONNECT_MODE, false),
            };
            let windows = Windows {
                host: self.host,
                last: &self.last_logon,
                favorites: &self.favorites,
            };
            let (done, saved) = mapping::unmap(&SystemConnections, &windows, &remote, settings);
            if let Some(reconnect) = saved {
                self.host.set_bool(DISCONNECT_MODE, reconnect);
            }
            if !done {
                break;
            }
        }
        // FAR's `PluginDelete`: both panels read again (the other may show the disk just gone).
        KeyAnswer {
            reread: true,
            reread_passive: true,
            ..KeyAnswer::default()
        }
    }

    /// F4 (FAR's `ChangeToDirectory` explicitly): the name and password asked for and connected with;
    /// a share then left for, a domain or computer entered; a failure told (not a cancel).
    fn manual_connect(&mut self, path: &str, press: &KeyPress) -> KeyAnswer {
        let Some(name) = press.targets().into_iter().next() else {
            return KeyAnswer::default();
        };
        let found = self.browser.entry(
            &crate::favorites::favored(&self.net, &self.favorites.borrow()),
            &self.options,
            path,
            &name,
        );
        let Ok(Some(resource)) = found else {
            return KeyAnswer::default();
        };
        let windows = Windows {
            host: self.host,
            last: &self.last_logon,
            favorites: &self.favorites,
        };
        let net = SystemConnections;
        let remote = resource.remote_name.clone();
        let connected = mapping::connect_explicit(&net, &windows, ("", &remote), false);
        let result = connected.and_then(|()| {
            if !is_share(&resource) || net.readable(&remote) {
                Ok(())
            } else {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            }
        });
        if let Err(error) = result {
            if error.raw_os_error() != Some(ERROR_CANCELLED) {
                self.error(&error);
            }
            return KeyAnswer::default();
        }
        if is_share(&resource) {
            return KeyAnswer::go(KeyGo::ActiveFolder(remote.into()));
        }
        // Refused, the panel stays where it is and the program tells why: nothing more to do here.
        let _ = self
            .host
            .set_folder(Which::Active, &join(path, &name), None);
        KeyAnswer::default()
    }
}

#[cfg(test)]
#[path = "connecting_tests.rs"]
mod tests;
