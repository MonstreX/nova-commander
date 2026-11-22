//! The panel's own keys (FAR's Network `ProcessKey`, `ChangeToDirectory`, `FE_CHANGEVIEWMODE`): Enter on
//! a share leaves for its folder (Ctrl+PgDn for the disk it is connected as, when that reads), connected
//! first when it cannot be read (with a name and password asked for when the network wants them); Enter
//! on a domain or computer enters it once it opens (a connection tried first, so too), else tells why; F3 does nothing (FAR: "avoid unnecessary slowdown"); Ctrl+0..9 are the
//! program's, the mode they choose kept for the panel's closing.

use std::io;
use std::path::PathBuf;

use nc_plugin_sdk::system::network::{
    DISPLAY_DOMAIN, DISPLAY_NETWORK, ERROR_ACCESS_DENIED, ERROR_CANCELLED, ERROR_INVALID_HANDLE,
    ERROR_INVALID_PASSWORD, ERROR_LOGON_FAILURE, ERROR_LOGON_TYPE_NOT_GRANTED, NetResource,
    USAGE_CONTAINER,
};
use nc_plugin_sdk::{KeyAnswer, KeyGo, KeyPress, Message, PanelKey};

use crate::NetworkPlugin;
use crate::asking::{SystemConnections, Windows};
use crate::browse::{Network, is_share};
use crate::mapping;
use crate::timed::not_answering;

/// The keys and their labels.
pub fn keys() -> Vec<PanelKey> {
    let mut keys = vec![
        PanelKey::new("Enter", ""),
        PanelKey::new("Ctrl+PgDn", ""),
        PanelKey::new("F3", ""),
        PanelKey::new("Clear", ""),
        PanelKey::new("Ctrl+PgUp", ""),
        PanelKey::new("Shift+F4", ""),
    ];
    keys.extend((0..10).map(|digit| PanelKey::new(&format!("Ctrl+{digit}"), "")));
    keys
}

/// The system's text of `error`, without the number Rust adds.
pub fn system_text(error: &io::Error) -> String {
    let text = error.to_string();
    match text.rfind(" (os error") {
        Some(at) => text[..at].trim_end().to_string(),
        None => text,
    }
}

/// The codes after which FAR's `ChangeToDirectory` connects a share with a name and password asked for.
pub fn share_asks_logon(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(
            ERROR_INVALID_PASSWORD
                | ERROR_LOGON_FAILURE
                | ERROR_ACCESS_DENIED
                | ERROR_INVALID_HANDLE
        )
    )
}

/// The codes after which FAR's `ChangeToDirectory` connects a domain or computer so.
pub fn container_asks_logon(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(
            ERROR_INVALID_PASSWORD
                | ERROR_LOGON_FAILURE
                | ERROR_ACCESS_DENIED
                | ERROR_LOGON_TYPE_NOT_GRANTED
        )
    )
}

/// FAR's `IsResourceReadable`: a container that opens, after a connection when it does not, else with a
/// name and password asked for (`explicit`) when the network wants them; domains and networks taken as
/// they are when the network is not scanned; one that does not answer yet told so. Its listing is kept
/// for the panel's reading that follows.
pub fn container_opens(
    net: &dyn Network,
    resource: &NetResource,
    scan: bool,
    explicit: &dyn Fn(&NetResource) -> io::Result<()>,
) -> io::Result<()> {
    let unscanned = resource.display == DISPLAY_DOMAIN || resource.display == DISPLAY_NETWORK;
    if !scan && unscanned {
        return Ok(());
    }
    let first = match net.enumerate(Some(resource)) {
        Err(error) if not_answering(&error) => return Err(error),
        Err(_) => match net
            .connect(resource)
            .and_then(|()| net.enumerate(Some(resource)))
        {
            Ok(listed) => listed,
            Err(error) if container_asks_logon(&error) => return explicit(resource),
            Err(error) => return Err(error),
        },
        Ok(listed) => listed,
    };
    net.keep(Some(resource), Ok(first));
    Ok(())
}

/// Whether `error` is a name and password not given (nothing to tell).
fn cancelled(error: &io::Error) -> bool {
    error.raw_os_error() == Some(ERROR_CANCELLED)
}

/// Where a share is entered (FAR's `ChangeToDirectory`): with Ctrl+PgDn the disk it is connected as when
/// one is, else its own path.
pub fn share_folder(share: &NetResource, connected: &[NetResource], page_down: bool) -> PathBuf {
    let local = (connected.iter())
        .find(|one| one.remote_name.eq_ignore_ascii_case(&share.remote_name))
        .map(|one| one.local_name.clone())
        .filter(|local| !local.is_empty());
    match local {
        Some(local) if page_down => PathBuf::from(format!("{local}\\")),
        _ => PathBuf::from(&share.remote_name),
    }
}

fn readable(folder: &std::path::Path) -> io::Result<()> {
    std::fs::read_dir(folder).map(|_| ())
}

impl NetworkPlugin {
    pub(crate) fn key(&mut self, instance: &str, path: &str, press: &KeyPress) -> KeyAnswer {
        let key = press.key.as_str();
        if let Some(digit) = key
            .strip_prefix("Ctrl+")
            .and_then(|rest| rest.parse::<u32>().ok())
        {
            self.mode_chosen(instance, digit);
            return KeyAnswer::pass();
        }
        if let Some(answer) = self.connection_key(path, press) {
            return answer;
        }
        match key {
            "Enter" => self.enter(path, press, false),
            "Ctrl+PgDn" => self.enter(path, press, true),
            // FAR's `GetResourceParent`: up to the favorites, else the program's "..".
            "Ctrl+PgUp" => self.up_to_favorites(path).unwrap_or_else(KeyAnswer::pass),
            "Shift+F4" => self.tell_favorite(path, press),
            // FAR's F3 and Clear: nothing (the program would count a computer's folders).
            _ => KeyAnswer::default(),
        }
    }

    /// FAR's Enter (Ctrl+PgDn: `OPM_PGDN`): with a command typed, or on "..", the program's.
    fn enter(&mut self, path: &str, press: &KeyPress, page_down: bool) -> KeyAnswer {
        if !page_down && !self.host.command_line().is_empty() {
            return KeyAnswer::pass();
        }
        let Some(name) = press.current.as_deref() else {
            // "..": up to the favorites (FAR's `GetResourceParent`), else the program's.
            return match page_down {
                false => self.up_to_favorites(path).unwrap_or_else(KeyAnswer::pass),
                true => KeyAnswer::pass(),
            };
        };
        let found = self.browser.entry(
            &crate::favorites::favored(&self.net, &self.favorites.borrow()),
            &self.options,
            path,
            name,
        );
        let Ok(Some(resource)) = found else {
            return KeyAnswer::pass();
        };
        if is_share(&resource) {
            return self.leave_for(&resource, page_down);
        }
        // A printer: nothing to enter, the program's.
        if resource.usage & USAGE_CONTAINER == 0 {
            return KeyAnswer::pass();
        }
        let windows = Windows {
            host: self.host,
            last: &self.last_logon,
            favorites: &self.favorites,
        };
        let explicit = |resource: &NetResource| {
            mapping::connect_saved_or_asked(
                &SystemConnections,
                &windows,
                ("", &resource.remote_name),
                true,
            )
        };
        // A copy: the logon asked for may keep a name and password in the favorites meanwhile.
        let favorites = self.favorites.borrow().clone();
        let net = crate::favorites::favored(&self.net, &favorites);
        match container_opens(&net, &resource, self.options.scan_network, &explicit) {
            Ok(()) => KeyAnswer::pass(),
            Err(error) => {
                if !cancelled(&error) {
                    self.error(&error);
                }
                KeyAnswer::default()
            }
        }
    }

    /// A share's folder in the panel, connected first when it cannot be read (FAR's `AddConnection`),
    /// else with a name and password asked for when the network wants them (`AddConnectionExplicit`).
    fn leave_for(&mut self, share: &NetResource, page_down: bool) -> KeyAnswer {
        let connected = self.net.connected().unwrap_or_default();
        let folder = share_folder(share, &connected, page_down);
        let on_disk = folder.as_os_str() != share.remote_name.as_str();
        if let Err(unread) = readable(&folder) {
            // FAR's Ctrl+PgDn on a connected disk that does not read: told, never connected.
            if on_disk {
                self.error(&unread);
                return KeyAnswer::default();
            }
            let joined = match self.net.connect(share).and_then(|()| readable(&folder)) {
                Err(error) if share_asks_logon(&error) => {
                    let windows = Windows {
                        host: self.host,
                        last: &self.last_logon,
                        favorites: &self.favorites,
                    };
                    let remote = ("", share.remote_name.as_str());
                    mapping::connect_saved_or_asked(&SystemConnections, &windows, remote, true)
                        .and_then(|()| readable(&folder))
                }
                other => other,
            };
            if let Err(error) = joined {
                if !cancelled(&error) {
                    self.error(&error);
                }
                return KeyAnswer::default();
            }
        }
        KeyAnswer::go(KeyGo::ActiveFolder(folder))
    }

    /// FAR's `FMSG_ERRORTYPE` message: "Error" and the system's reason.
    pub(crate) fn error(&self, error: &io::Error) {
        let text = system_text(error);
        let message = Message::new("Error", &[&text]).warning().buttons(&["Ok"]);
        self.host.show(&message);
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
