//! The sessions panel's keys (NetBox's `ProcessKeyEx` on the session list, `EditConnectSession`,
//! `DuplicateOrRenameSession`, `InsertSessionNameOnCommandLine`, `MakeDirectoryEx`, `DeleteFilesEx`):
//! F4 edits the session under the cursor (adds one while none is stored), Shift+F4 adds one and asks its
//! name ("Save session as"), Shift+F5 duplicates and Shift+F6 renames it (a name with folders moves it
//! there), Ctrl+F puts its name into the command line, F7 makes a folder, F8 deletes sessions and folders
//! with their sessions after NetBox's question. After each the panel goes to the session's folder with the
//! cursor on it. Enter connects to the session (S6.5a: its panel over the sessions, ".." at its root back).

use nc_plugin_sdk::{
    Error, ErrorKind, GetMode, InputOptions, KeyAnswer, KeyGo, KeyPress, Message, Which,
};

use crate::NetWay;
use crate::address::Address;
use crate::session::{Session, name_error, path_error};
use crate::session_dialog::{self, Action};
use crate::sessions::{self, folder_of, path_of};
use crate::store::{Item, joined, split};

/// NetBox's history of the folders' names (`MAKE_SESSION_FOLDER_HISTORY`), the program's own name.
const FOLDER_HISTORY: &str = "NetWaySessionFolder";

/// A name as typed: `\` taken for `/` (NetBox's `ReplaceChar`), no `/` at its ends.
pub fn typed_name(text: &str) -> String {
    text.trim().replace('\\', "/").trim_matches('/').to_string()
}

impl NetWay {
    /// A key of the sessions panel at `path`.
    pub(crate) fn session_key(&mut self, path: &str, press: &KeyPress) -> KeyAnswer {
        let folder = folder_of(path).to_string();
        let item = (press.current.as_deref()).and_then(|name| self.store.item_named(&folder, name));
        let session = match &item {
            Some(Item::Session(name)) => Some(joined(&folder, name)),
            _ => None,
        };
        match (press.key.as_str(), session) {
            ("Enter", Some(name)) => self.connect_answer(stored(name)),
            ("Enter", None) => enter_folder(press, item.as_ref()),
            ("F4", Some(name)) => self.edit(&name),
            ("F4", None) if self.store.has_none() => self.add(&folder),
            ("Shift+F4", _) => self.add(&folder),
            ("Shift+F5", Some(name)) => self.duplicate_or_rename(&name, true),
            ("Shift+F6", Some(name)) => self.duplicate_or_rename(&name, false),
            ("Ctrl+F", _) => {
                let on_dots = press.current.is_none();
                (sessions::command_line_name(&folder, item.as_ref(), on_dots))
                    .map_or_else(KeyAnswer::default, |text| {
                        KeyAnswer::go(KeyGo::CommandLine(text))
                    })
            }
            (key, _) if sessions::is_programs(key) => KeyAnswer::pass(),
            _ => KeyAnswer::default(),
        }
    }

    /// NetBox's error message.
    pub(crate) fn error(&self, text: &str) {
        let lines: Vec<&str> = text.split('\n').collect();
        let message = Message::new("Error", &lines).warning().buttons(&["&Ok"]);
        self.host.show(&message);
    }

    fn warning(&self, text: &str) {
        let message = Message::new("Warning", &[text]).warning().buttons(&["&Ok"]);
        self.host.show(&message);
    }

    /// Shift+F4 (and F4 with none stored): the dialog "Add session", then the name it is saved as; its
    /// Connect connects without saving (NetBox's `EditConnectSession` of new data).
    fn add(&mut self, folder: &str) -> KeyAnswer {
        let Some(edited) = session_dialog::edit(&self.host, &Session::default(), Action::Add)
        else {
            return KeyAnswer::default();
        };
        let session = edited.session;
        if edited.connect {
            let address = Address {
                session,
                ..Address::default()
            };
            return self.connect_answer(address);
        }
        let suggested = joined(folder, &session.default_name());
        let Some(typed) = self
            .host
            .input("Save session as", "Save session as:", &suggested)
        else {
            return KeyAnswer::default();
        };
        let name = typed_name(&typed);
        if let Some(problem) = path_error(&name) {
            self.error(&problem);
            return KeyAnswer::default();
        }
        if self.store.exists(&name) {
            self.error(&format!("Session named '{name}' already exists."));
            return KeyAnswer::default();
        }
        self.save(&name, &session);
        self.show_session(&name)
    }

    /// F4 on a session: "Edit session", saved under its name.
    fn edit(&mut self, name: &str) -> KeyAnswer {
        let loaded = match self.store.load(name) {
            Ok(loaded) => loaded,
            Err(error) => {
                self.error(&format!("Cannot initialize session '{name}'.\n{error}"));
                return KeyAnswer::default();
            }
        };
        if loaded.password_lost {
            self.warning("The session's password was saved by another user or on another computer: type it again.");
        }
        match session_dialog::edit(&self.host, &loaded.session, Action::Edit) {
            Some(edited) if edited.connect => {
                self.save(name, &edited.session);
                self.connect_answer(stored(name.to_string()))
            }
            Some(edited) => {
                self.save(name, &edited.session);
                KeyAnswer::reread()
            }
            None => KeyAnswer::default(),
        }
    }

    /// The session written; a failure or a password the system could not keep told.
    fn save(&self, name: &str, session: &Session) {
        match self.store.save(name, session) {
            Err(error) => self.error(&format!("Cannot save session '{name}'.\n{error}")),
            Ok(false) if !session.password.is_empty() => {
                self.warning(
                    "The password was not saved: this system keeps no secrets for NetWay.",
                );
            }
            Ok(_) => {}
        }
    }

    /// Shift+F5 / Shift+F6: NetBox's "Duplicate session" / "Rename session" with the session's whole
    /// name.
    fn duplicate_or_rename(&mut self, name: &str, duplicate: bool) -> KeyAnswer {
        let title = if duplicate {
            "Duplicate session"
        } else {
            "Rename session"
        };
        let Some(typed) = self.host.input(title, "New session name:", name) else {
            return KeyAnswer::default();
        };
        let to = typed_name(&typed);
        if to.is_empty() || to == name {
            return KeyAnswer::default();
        }
        if let Some(problem) = path_error(&to) {
            self.error(&problem);
            return KeyAnswer::default();
        }
        // Another case of its name where it is the same file renames it (NetBox: "change of letter case
        // during duplication degrades the operation to rename").
        let done = if duplicate && !self.store.same(name, &to) {
            self.store.copy(name, &to)
        } else {
            self.store.rename(name, &to)
        };
        match done {
            Ok(()) => self.show_session(&to),
            Err(error) => {
                self.error(&error.to_string());
                KeyAnswer::reread()
            }
        }
    }

    /// The panel to the session's folder, the cursor on it (NetBox's `FSessionsFolder` and
    /// `FocusSession`).
    fn show_session(&self, name: &str) -> KeyAnswer {
        let (folder, base) = split(name);
        // Refused, the panel stays and the program tells why: read again where it is.
        if self
            .host
            .set_folder(Which::Active, &path_of(folder), Some(base))
        {
            KeyAnswer::default()
        } else {
            KeyAnswer::reread()
        }
    }

    /// F7 (NetBox's `MakeDirectoryEx` on the session list): "Make folder", its name, the folder made.
    pub(crate) fn make_session_folder(
        &mut self,
        path: &str,
        name: &str,
        mode: GetMode,
    ) -> Result<String, Error> {
        let name = if name.is_empty() && !mode.silent {
            let options = InputOptions {
                no_ampersand: true,
                ..InputOptions::default()
            };
            let asked = (self.host).input_history(
                "Make folder",
                "New folder name:",
                "",
                FOLDER_HISTORY,
                options,
            );
            asked.ok_or_else(|| Error::new(ErrorKind::Cancelled, "Cancelled"))?
        } else {
            name.to_string()
        };
        let name = name.trim().to_string();
        if let Some(problem) = name_error(&name) {
            return Err(Error::new(ErrorKind::Other, problem));
        }
        let folder = joined(folder_of(path), &name);
        (self.store.make_folder(&folder)).map_err(|error| {
            Error::new(
                ErrorKind::Other,
                format!("Can't create folder '{name}'.\n{error}"),
            )
        })?;
        Ok(name)
    }

    /// F8 (NetBox's `DeleteFilesEx` on the session list): "Do you wish to delete selected session(s)",
    /// then the sessions and the folders with all theirs; the hint is passed by.
    pub(crate) fn delete_sessions(
        &mut self,
        path: &str,
        names: &[String],
        mode: GetMode,
    ) -> Result<(), Error> {
        let asks = !mode.silent && self.host.confirms("Delete").unwrap_or(true);
        if asks {
            let message = Message::new(
                "Confirmation",
                &["Do you wish to delete selected session(s)"],
            )
            .buttons(&["&Ok", "&Cancel"]);
            if self.host.show(&message) != Some(0) {
                return Err(Error::new(ErrorKind::Cancelled, "Cancelled"));
            }
        }
        let folder = folder_of(path);
        for name in names {
            let Some(item) = self.store.item_named(folder, name) else {
                continue;
            };
            (self.store.remove(folder, &item))
                .map_err(|error| Error::new(ErrorKind::Other, format!("{name}: {error}")))?;
        }
        Ok(())
    }
}

/// The stored session `name` to connect to.
fn stored(name: String) -> Address {
    Address {
        stored: Some(name),
        ..Address::default()
    }
}

/// Enter on what is not a session: a folder or ".." is the program's (it goes in or up); the hint does
/// nothing.
fn enter_folder(press: &KeyPress, item: Option<&Item>) -> KeyAnswer {
    match item {
        Some(Item::Folder(_)) => KeyAnswer::pass(),
        _ if press.current.is_none() => KeyAnswer::pass(),
        _ => KeyAnswer::default(),
    }
}

#[cfg(test)]
#[path = "session_keys_tests.rs"]
mod tests;
