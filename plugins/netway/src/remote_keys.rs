//! A connected session's panel's own keys and F7 / F8 (NetBox's `ProcessKeyEx` while connected,
//! `MakeDirectoryEx`, `DeleteFilesEx`, `TransferFiles`, `RenameFile`, `FileProperties`, `RemoteCreateLink`,
//! `InsertFileNameOnCommandLine`, `CopyFullFileNamesToClipboard`, `TRemoteFilePanelItem::SetKeyBarTitles`):
//! F7 "Make folder", F8 "Do you wish to delete ...", Shift+F5 "Duplicate" and Shift+F6 "Move" on the
//! server, Alt+Shift+F6 "Rename", Ctrl+A "Attributes", Alt+F6 "Add link" / "Edit link", Ctrl+Alt+F the
//! file's address into the command line, Alt+Shift+Ins the selected files' addresses onto the clipboard.
//! Each operation runs as a transfer does, with NetBox's progress window and Esc; the panel read again
//! after it. The rights of new folders kept with "Use same settings next time" (WinSCP's
//! `NewDirectoryProperties`).

use nc_plugin_sdk::{
    Error, ErrorKind, GetMode, KeyAnswer, KeyGo, KeyPress, Menu, MenuItem, PanelKey,
};

use crate::NetWay;
use crate::copy_panel::Work;
use crate::copy_params::CopyParams;
use crate::copy_progress::Shown;
use crate::remote_dialogs::{self, Current, Link};
use crate::rights_box::Rights;
use crate::sftp::child;

/// The settings' keys of the rights of new folders.
const NEW_FOLDER_SET: &str = "NewDirectory.SetRights";
const NEW_FOLDER_RIGHTS: &str = "NewDirectory.Rights";

/// The keys NetBox clears on the panel (Shift+F1 to Shift+F3, the archives'): left to the program.
const BLANK_KEYS: [&str; 3] = ["Shift+F1", "Shift+F2", "Shift+F3"];

/// The panel's own keys with NetBox's labels.
pub fn keys() -> Vec<PanelKey> {
    let mut keys = vec![
        PanelKey::new("Shift+F5", "Duplicate"),
        PanelKey::new("Shift+F6", "MoveTo"),
        PanelKey::new("Alt+Shift+F6", "Rename"),
        PanelKey::new("Ctrl+A", ""),
        PanelKey::new("Alt+F6", ""),
        PanelKey::new("Ctrl+Alt+F", ""),
        PanelKey::new("Alt+Shift+Ins", ""),
        PanelKey::new("Ctrl+G", ""),
        PanelKey::new("Alt+Shift+W", ""),
        PanelKey::new("Ctrl+Alt+B", ""),
        PanelKey::new("Alt+Shift+F11", "EditHs"),
        PanelKey::new("Alt+Shift+F12", "OpenFld"),
    ];
    keys.extend(BLANK_KEYS.iter().map(|key| PanelKey::new(key, " ")));
    keys
}

/// The window of an operation.
fn shown(title: &'static str) -> Shown {
    Shown {
        moving: false,
        temporary: false,
        remote_source: true,
        operation: Some(title),
    }
}

/// WinSCP's `InsertTokenOnCommandLine`: in quotes with a space, a space after.
pub fn token(text: &str) -> String {
    match text.contains(' ') {
        true => format!("\"{text}\" "),
        false => format!("{text} "),
    }
}

impl NetWay {
    /// The operation `work` on the panel `instance`: done, or the error to show (a stop is no error).
    fn operate(&self, instance: &str, title: &'static str, work: Work) -> KeyAnswer {
        let done = self.transfer(instance, shown(title), CopyParams::default(), work);
        if let Err(error) = done
            && error.kind != ErrorKind::Cancelled
        {
            self.error(&error.message);
        }
        KeyAnswer::reread()
    }

    /// A key of the connected panel `instance` at `path`.
    pub(crate) fn remote_key(&mut self, instance: &str, path: &str, press: &KeyPress) -> KeyAnswer {
        let ours = keys()
            .iter()
            .any(|key| key.key == press.key && key.label != " ");
        if ours && !self.reopen(instance, path) {
            return KeyAnswer::default();
        }
        let targets = press.targets();
        match press.key.as_str() {
            "Shift+F5" | "Shift+F6" if !targets.is_empty() => {
                let moving = press.key == "Shift+F6";
                let Some((folder, mask)) =
                    remote_dialogs::ask_transfer(&self.host, path, &targets, moving)
                else {
                    return KeyAnswer::default();
                };
                let target = crate::copy_panel::remote_folder(&folder, path);
                let work = Work::Move {
                    folder: path,
                    names: &targets,
                    target: (&target, &mask),
                    duplicate: !moving,
                };
                self.operate(instance, if moving { "Moving" } else { "Copying" }, work)
            }
            "Alt+Shift+F6" => match &press.current {
                Some(name) => self.rename(instance, path, name),
                None => KeyAnswer::default(),
            },
            "Ctrl+A" if !targets.is_empty() => self.attributes(instance, path, &targets),
            "Alt+F6" => self.link(instance, path, press.current.as_deref()),
            "Ctrl+Alt+F" => self.file_name_line(instance, path, press.current.as_deref()),
            "Alt+Shift+Ins" => self.names_to_clipboard(instance, path, press),
            "Ctrl+G" => self.applied(instance, path, &targets),
            "Alt+Shift+W" => self.commands_menu(instance, path, press),
            "Ctrl+Alt+B" => self.toggle_browsing(instance),
            "Alt+Shift+F11" => self.edit_history(instance),
            "Alt+Shift+F12" => self.open_directory(instance, false),
            _ => KeyAnswer::pass(),
        }
    }

    fn applied(&mut self, instance: &str, path: &str, targets: &[String]) -> KeyAnswer {
        match self.apply_command(instance, path, targets) {
            true => KeyAnswer::reread(),
            false => KeyAnswer::default(),
        }
    }

    /// Alt+Shift+W: NetBox's "NetBox commands" of a connected panel (the ones this version has).
    fn commands_menu(&mut self, instance: &str, path: &str, press: &KeyPress) -> KeyAnswer {
        let no_edits = (self.remotes.get(instance)).is_none_or(|remote| remote.edits.is_empty());
        let synchronized = (self.remotes.get(instance)).is_some_and(|remote| remote.synchronized);
        let items = vec![
            MenuItem::new("View/change file a&ttributes  Ctrl-A"),
            MenuItem::new("Create/edit &link             Alt-F6"),
            MenuItem::new("Apply co&mmand                Ctrl-G"),
            MenuItem::new("&Synchronize"),
            MenuItem::new("&Keep Remote Directory Up To Date"),
            MenuItem::new("Compare directories"),
            MenuItem::new("Server and protocol &information"),
            // No session log is kept (NetBox's item while logging is off).
            MenuItem::new("Display session log &file").disabled(true),
            MenuItem::new("&Edit history                 Alt-Shift-F11").disabled(no_edits),
            MenuItem::separator(),
            MenuItem::new("Add &bookmark"),
            MenuItem::new("&Open directory               Alt-Shift-F12"),
            MenuItem::new("Location &Profiles"),
            MenuItem::new("&Home directory"),
            MenuItem::new("Synchronize bro&wsing         Ctrl-Alt-B").checked(synchronized),
            MenuItem::separator(),
            MenuItem::new("&Configure"),
            MenuItem::new("Generate &URL"),
            MenuItem::new("&Cleanup Configuration"),
        ];
        let menu = Menu::new("NetBox commands", items).wrap();
        let chosen = self.host.menu(&menu).and_then(|chosen| chosen.item);
        let targets = press.targets();
        match chosen {
            Some(0) if !targets.is_empty() => self.attributes(instance, path, &targets),
            Some(1) => self.link(instance, path, press.current.as_deref()),
            Some(2) => self.applied(instance, path, &targets),
            Some(3) => self.synchronize(instance, press),
            Some(4) => self.keep_up_to_date(instance, press),
            Some(5) => self.compare_directories(instance),
            Some(6) => {
                self.information(instance);
                KeyAnswer::default()
            }
            Some(8) => self.edit_history(instance),
            Some(10) => self.open_directory(instance, true),
            Some(11) => self.open_directory(instance, false),
            Some(12) => self.location_profiles(instance),
            Some(13) => {
                self.home_directory(instance);
                KeyAnswer::default()
            }
            Some(14) => self.toggle_browsing(instance),
            Some(16) => {
                crate::configuration::configure(&self.host);
                KeyAnswer::default()
            }
            Some(17) => {
                self.generate_url(instance);
                KeyAnswer::default()
            }
            Some(18) => {
                crate::cleanup::cleanup(&self.host);
                KeyAnswer::default()
            }
            _ => KeyAnswer::default(),
        }
    }

    fn rename(&self, instance: &str, path: &str, name: &str) -> KeyAnswer {
        let Some(new) = remote_dialogs::ask_rename(&self.host, name) else {
            return KeyAnswer::default();
        };
        let names = [name.to_string()];
        let work = Work::Move {
            folder: path,
            names: &names,
            target: (path, &new),
            duplicate: false,
        };
        self.operate(instance, "Moving", work)
    }

    /// What "Attributes" shows of `names` in `folder`, read from the server.
    fn current(&self, instance: &str, folder: &str, names: &[String]) -> Option<Current> {
        let (remote, runtime) = (self.remotes.get(instance)?, self.runtime.as_ref()?);
        let sftp = &remote.fs;
        let load = async {
            let listed = sftp.list_raw(folder).await.unwrap_or_default();
            let mut current = Current {
                names: names.to_vec(),
                ..Current::default()
            };
            let mut rights = Vec::new();
            let (mut uids, mut gids) = (Vec::new(), Vec::new());
            for name in names {
                let path = child(folder, name);
                let Ok(meta) = sftp.metadata(path.as_str()).await else {
                    continue;
                };
                current.any_folder |= meta.is_dir();
                rights.push(u16::try_from(meta.permissions.unwrap_or(0) & 0o7777).unwrap_or(0));
                uids.push(meta.uid);
                gids.push(meta.gid);
                if names.len() == 1 && listed.iter().any(|item| item.name == *name && item.is_link)
                {
                    current.link_to = sftp.read_link(path.as_str()).await.ok();
                }
                let entry = listed.iter().find(|item| item.name == *name);
                current.owner = entry.and_then(|item| item.owner.clone());
                current.group = entry.and_then(|item| item.group.clone());
            }
            current.rights = Rights::common(&rights);
            current.uid = same(&uids);
            current.gid = same(&gids);
            if names.len() > 1 {
                current.owner = None;
                current.group = None;
            }
            current
        };
        runtime.wait(load, &|| self.host.cancelled()).ok()
    }

    fn attributes(&self, instance: &str, path: &str, names: &[String]) -> KeyAnswer {
        let Some(current) = self.current(instance, path, names) else {
            return KeyAnswer::default();
        };
        let Some(properties) = remote_dialogs::ask_attributes(&self.host, &current) else {
            return KeyAnswer::default();
        };
        let work = Work::Properties {
            folder: path,
            names,
            properties,
        };
        self.operate(instance, "Setting properties", work)
    }

    /// NetBox's `RemoteCreateLink`: a link under the cursor edited (deleted, then made anew), else a new
    /// one pointing to the file under the cursor.
    fn link(&self, instance: &str, path: &str, current: Option<&str>) -> KeyAnswer {
        let link_to = current.and_then(|name| {
            let current = self.current(instance, path, &[name.to_string()])?;
            current.link_to
        });
        let edit = link_to.is_some();
        let start = match (&link_to, current) {
            (Some(to), Some(name)) => Link {
                name: name.to_string(),
                point_to: to.clone(),
                symbolic: true,
            },
            (_, name) => Link {
                name: String::new(),
                point_to: name.unwrap_or_default().to_string(),
                symbolic: true,
            },
        };
        let Some(link) = remote_dialogs::ask_link(&self.host, &start, edit) else {
            return KeyAnswer::default();
        };
        if edit {
            let names = [link.name.clone()];
            let delete = Work::Delete {
                folder: path,
                names: &names,
            };
            // The new link only once the old one went (stopped or failed: nothing more, said once).
            let deleted = self.transfer(instance, shown("Deleting"), CopyParams::default(), delete);
            if let Err(error) = deleted {
                if error.kind != ErrorKind::Cancelled {
                    self.error(&error.message);
                }
                return KeyAnswer::reread();
            }
        }
        let at = child(path, &link.name);
        let work = Work::Link {
            path: &at,
            point_to: &link.point_to,
            symbolic: link.symbolic,
        };
        self.operate(instance, "Copying", work)
    }

    /// NetBox's `GetFullFilePath`: the session's address and the item's path.
    fn full_path(&self, instance: &str, path: &str, name: &str) -> String {
        let url = self.remotes.get(instance).map(|remote| remote.url.as_str());
        format!("{}{}", url.unwrap_or_default(), child(path, name))
    }

    /// NetBox's `InsertFileNameOnCommandLine(true)`: the file's address, or the folder's on "..".
    fn file_name_line(&self, instance: &str, path: &str, current: Option<&str>) -> KeyAnswer {
        let text = match current {
            Some(name) => self.full_path(instance, path, name),
            None => format!("{}/", path.trim_end_matches('/')),
        };
        KeyAnswer::go(KeyGo::CommandLine(token(&text)))
    }

    /// NetBox's `CopyFullFileNamesToClipboard`: each selected file's address on a line, or the folder's
    /// on "..".
    fn names_to_clipboard(&self, instance: &str, path: &str, press: &KeyPress) -> KeyAnswer {
        let targets = press.targets();
        let lines: Vec<String> = match targets.is_empty() {
            true => vec![format!("{}/", path.trim_end_matches('/'))],
            false => (targets.iter())
                .map(|name| self.full_path(instance, path, name))
                .collect(),
        };
        self.host.set_clipboard(&lines.join("\r\n"));
        KeyAnswer::default()
    }

    /// NetBox's `DeleteFilesEx` on a connected panel: its question (none when silent), then the items.
    pub(crate) fn delete_remote(
        &mut self,
        instance: &str,
        path: &str,
        names: &[String],
        mode: GetMode,
    ) -> Result<(), Error> {
        if names.is_empty() || (!mode.silent && !remote_dialogs::ask_delete(&self.host, names)) {
            return Err(Error::new(ErrorKind::Cancelled, "Cancelled"));
        }
        if !self.reopen(instance, path) {
            return Err(Error::new(ErrorKind::Cancelled, "Cancelled"));
        }
        let work = Work::Delete {
            folder: path,
            names,
        };
        let shown = shown("Deleting");
        self.transfer(instance, shown, CopyParams::default(), work)
    }

    /// NetBox's `MakeDirectoryEx` on a connected panel: "Make folder" (none when a name is given), the
    /// rights of new folders kept when asked; the name made.
    pub(crate) fn make_remote_folder(
        &mut self,
        instance: &str,
        path: &str,
        name: &str,
        mode: GetMode,
    ) -> Result<String, Error> {
        if !self.reopen(instance, path) {
            return Err(Error::new(ErrorKind::Cancelled, "Cancelled"));
        }
        let kept = self.host.bool_setting(NEW_FOLDER_SET, false);
        let rights = (self.host.setting(NEW_FOLDER_RIGHTS).as_deref())
            .and_then(remote_dialogs::folder_rights_of_text);
        let rights = if kept { rights } else { None };
        let made = match (mode.silent && !name.is_empty(), name) {
            (true, name) => remote_dialogs::NewFolder {
                name: name.to_string(),
                rights,
                save: false,
            },
            (false, name) => remote_dialogs::ask_folder(&self.host, name, rights)
                .ok_or_else(|| Error::new(ErrorKind::Cancelled, "Cancelled"))?,
        };
        if made.save {
            self.host.set_bool(NEW_FOLDER_SET, made.rights.is_some());
            if let Some(rights) = made.rights {
                self.host.set_setting(
                    NEW_FOLDER_RIGHTS,
                    &remote_dialogs::folder_rights_text(rights),
                );
            }
        }
        let at = match made.name.starts_with('/') {
            true => made.name.clone(),
            false => child(path, &made.name),
        };
        let work = Work::Folder {
            path: &at,
            rights: made.rights,
        };
        let shown = shown("Creating folder");
        self.transfer(instance, shown, CopyParams::default(), work)?;
        Ok(made.name)
    }
}

/// The value all share, none when they differ or none is known.
fn same(values: &[Option<u32>]) -> Option<u32> {
    let first = (*values.first()?)?;
    values
        .iter()
        .all(|value| *value == Some(first))
        .then_some(first)
}

#[cfg(test)]
#[path = "remote_keys_tests.rs"]
mod tests;
