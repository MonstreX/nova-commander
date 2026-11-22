//! The connections' questions in the program's windows (FAR's Network `AskMapDrive` menu,
//! `GetNameAndPassword` and `ConfirmCancelConnection` dialogs, `GetDriveToDisconnect` and the open
//! files messages, "Error"), and the system's calls they make (the SDK's).

use std::cell::RefCell;
use std::io;

use nc_plugin_sdk::system::network::{self, Logon, NetResource, SCOPE_CONNECTED, TYPE_ANY};
use nc_plugin_sdk::{Dialog, DialogItem, Host, Menu, MenuItem, Message};

use crate::favorites::{self, Favorites};
use crate::keys::system_text;
use crate::mapping::{Asker, Connections, Given};

/// The name and password last given (FAR's `LastName`, `LastPassword`): the next dialog starts with
/// them.
#[derive(Debug, Clone, Default)]
pub struct LastLogon {
    pub user: String,
    pub password: String,
}

/// The program's windows.
pub struct Windows<'a> {
    pub host: Host,
    pub last: &'a RefCell<LastLogon>,
    /// The favorites the names and passwords are kept in (saved to the plugin's setting).
    pub favorites: &'a RefCell<Favorites>,
}

/// The logon dialog's "Remember password".
const REMEMBER: usize = 5;

/// The logon dialog's items: its name and password fields, its "Ok".
const USER: usize = 1;
const PASSWORD: usize = 3;
const LOGON_OK: usize = 7;
/// The disconnection's check box and "Yes".
const RECONNECT: usize = 3;
const YES: usize = 5;

/// FAR's `GetNameAndPassword` for `remote`: "User name" with its history, "User password", "Remember
/// password" (off at first, as FAR's `bSelected`).
pub fn logon_dialog(remote: &str, last: &LastLogon) -> Dialog {
    let items = vec![
        DialogItem::text("User &name").at(5, 2, 64, 2),
        DialogItem::edit(&last.user)
            .history("NetworkUser", true)
            .at(5, 3, 64, 3),
        DialogItem::text("User &password").at(5, 4, 64, 4),
        DialogItem::password(&last.password).at(5, 5, 64, 5),
        DialogItem::separator().at(-1, 6, -1, 6),
        DialogItem::check("&Remember password", false).at(5, 7, 26, 7),
        DialogItem::separator().at(-1, 8, -1, 8),
        DialogItem::button("&Ok").default_button().at(24, 9, 31, 9),
        DialogItem::button("&Cancel").at(33, 9, 44, 9),
    ];
    Dialog::new(remote, items).size(70, 12)
}

/// FAR's `ConfirmCancelConnection`: the question, the share, "Reconnect at Logon" (disabled for a disk
/// not restored at logon), "Yes" and "Cancel".
pub fn disconnect_dialog(local: &str, remote: &str, remembered: bool, reconnect: bool) -> Dialog {
    let question = format!("Do you want to disconnect from the drive {local}?");
    let width = question.chars().count().max(remote.chars().count()).max(30);
    let right = i32::try_from(width).unwrap_or(60) + 4;
    // "[ Yes ]" and "[ Cancel ]" centred, as FAR's builder puts its buttons.
    let yes = (right + 6 - 22) / 2;
    let items = vec![
        DialogItem::text(&question).at(5, 2, right, 2),
        DialogItem::text(remote).at(5, 3, right, 3),
        DialogItem::separator().at(-1, 4, -1, 4),
        DialogItem::check("&Reconnect at Logon", reconnect)
            .disabled(!remembered)
            .at(5, 5, 28, 5),
        DialogItem::separator().at(-1, 6, -1, 6),
        DialogItem::button("&Yes")
            .default_button()
            .at(yes, 7, yes + 8, 7),
        DialogItem::button("&Cancel").at(yes + 10, 7, yes + 21, 7),
    ];
    let width = u32::try_from(width).unwrap_or(60) + 10;
    Dialog::new("Disconnect network drive", items).size(width, 10)
}

/// FAR's `AskMapDrive` menu: the free letters, its title and bottom the connection's kind.
pub fn letter_menu(free: &[char], permanent: bool, selected: usize) -> Menu {
    let items = (free.iter())
        .map(|letter| MenuItem::new(&format!("&{letter}:")))
        .collect();
    let (title, bottom) = if permanent {
        ("Permanent connection to", "F6 - temporary")
    } else {
        ("Temporary connection to", "F6 - permanent")
    };
    Menu::new(title, items)
        .bottom(bottom)
        .selected(selected)
        .break_keys(&["F6"])
}

impl Asker for Windows<'_> {
    fn letter(&self, free: &[char], permanent: bool) -> Option<(char, bool)> {
        let mut permanent = permanent;
        let mut selected = 0;
        loop {
            let chosen = self.host.menu(&letter_menu(free, permanent, selected))?;
            let item = chosen.item?;
            if chosen.key.is_none() {
                return free.get(item).map(|letter| (*letter, permanent));
            }
            selected = item;
            permanent = !permanent;
        }
    }

    fn logon(&self, remote: &str) -> Option<Given> {
        let dialog = logon_dialog(remote, &self.last.borrow());
        let closed = self.host.dialog(&dialog);
        if closed.button != Some(LOGON_OK) {
            return None;
        }
        let text = |item: usize| {
            (closed.values.get(item)).map_or_else(String::new, |value| value.text.clone())
        };
        let (user, password) = (text(USER), text(PASSWORD));
        *self.last.borrow_mut() = LastLogon {
            user: user.clone(),
            password: password.clone(),
        };
        let remember = (closed.values.get(REMEMBER)).is_some_and(|value| value.checked());
        Some(Given {
            user,
            password,
            remember,
        })
    }

    fn saved_logon(&self, remote: &str) -> Option<(String, String)> {
        self.favorites.borrow().logon(remote)
    }

    fn save_logon(&self, remote: &str, user: &str, password: &str) {
        let mut favorites = self.favorites.borrow_mut();
        favorites.write(remote, user, password);
        self.host.set_setting(favorites::SETTING, &favorites.text());
    }

    fn confirm_disconnect(
        &self,
        local: &str,
        remote: &str,
        remembered: bool,
        reconnect: bool,
    ) -> Option<bool> {
        let closed = (self.host).dialog(&disconnect_dialog(local, remote, remembered, reconnect));
        let reconnect = (closed.values.get(RECONNECT)).is_some_and(|value| value.checked());
        (closed.button == Some(YES)).then_some(reconnect)
    }

    fn which_local(&self, remote: &str, locals: &[String]) -> Option<usize> {
        let mapped = format!("The drive {remote} is mapped to multiple drive letters");
        let lines = [mapped.as_str(), "Which one do you want to disconnect?"];
        let buttons: Vec<&str> = locals.iter().map(String::as_str).collect();
        let message = Message::new("Disconnect network drive", &lines).buttons(&buttons);
        self.host.show(&message)
    }

    fn force(&self, local: &str) -> bool {
        let cannot = format!("Cannot disconnect from {local}");
        let lines = [
            cannot.as_str(),
            Message::SEPARATOR,
            "If you do not close the open files, data may be lost.",
            "Do you want to disconnect the device anyway?",
        ];
        let message = Message::new("Error", &lines)
            .warning()
            .buttons(&["&Ok", "&Cancel"]);
        self.host.show(&message) == Some(0)
    }

    fn error(&self, lines: &[String], error: Option<&io::Error>) {
        let mut all: Vec<String> = lines.to_vec();
        all.extend(error.map(system_text));
        let all: Vec<&str> = all.iter().map(String::as_str).collect();
        let message = Message::new("Error", &all).warning().buttons(&["&Ok"]);
        self.host.show(&message);
    }
}

/// The SDK's connections.
pub struct SystemConnections;

impl Connections for SystemConnections {
    fn connect_as(
        &self,
        local: &str,
        remote: &str,
        logon: Option<Logon<'_>>,
        remember: bool,
    ) -> io::Result<()> {
        network::connect(local, remote, logon, remember)
    }

    fn disconnect(&self, name: &str, forget: bool, force: bool) -> io::Result<()> {
        network::disconnect(name, forget, force)
    }

    fn connections(&self) -> Vec<NetResource> {
        network::enumerate(SCOPE_CONNECTED, TYPE_ANY, None).unwrap_or_default()
    }

    fn remembered(&self, letter: char) -> bool {
        network::is_remembered(letter)
    }

    fn used_letters(&self) -> u32 {
        network::used_letters()
    }

    fn readable(&self, path: &str) -> bool {
        let folder = if path.ends_with(':') {
            format!("{path}\\")
        } else {
            path.to_string()
        };
        std::fs::read_dir(folder).is_ok()
    }

    fn hands_off(&self, letter: char) -> bool {
        let on_disk = std::env::current_dir().is_ok_and(|dir| {
            (dir.to_string_lossy().chars().next())
                .is_some_and(|first| first.eq_ignore_ascii_case(&letter))
        });
        let home = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.ancestors().last().map(std::path::Path::to_path_buf));
        match home {
            Some(home) if on_disk => std::env::set_current_dir(home).is_ok(),
            _ => false,
        }
    }

    fn off_network(&self) {
        let on_network =
            std::env::current_dir().is_ok_and(|dir| dir.to_string_lossy().starts_with(r"\\"));
        if on_network {
            // Only a better chance for the next try (FAR's `%TMP%`); nothing to tell when it fails.
            let _ = std::env::set_current_dir(std::env::temp_dir());
        }
    }
}

#[cfg(test)]
#[path = "asking_tests.rs"]
mod tests;
