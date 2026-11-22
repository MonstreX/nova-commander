//! F5 / F6 on the sessions panel (NetBox's `GetFilesEx` / `PutFilesEx` on the session list,
//! `ExportSession`, `ImportSessions`): sessions copied out to a disk's folder after "Export" asks where
//! (a folder with all its sessions; a file there asked over), a disk's session files copied in after
//! "Import session(s) from selected file(s)?". A session goes out as its file, `<name>.netway`, its kept
//! password with it (readable by its user on this computer alone); F6 copies as F5, as NetBox's. F3,
//! Ctrl+PgDn and Find file read a session's file as it is.

use std::fs;
use std::path::{Path, PathBuf};

use nc_plugin_sdk::{Error, ErrorKind, GetRequest, InputOptions, Message, PutRequest};

use crate::NetWay;
use crate::session::{Session, name_error};
use crate::sessions::folder_of;
use crate::store::{EXTENSION, Item, joined};

/// The history of the export's folder: FAR's copy dialog's (NetBox's "Copy").
const COPY_HISTORY: &str = "Copy";

/// WinSCP's `LocalInvalidChars`, each `-` in an exported file's name (`ValidLocalFileName`).
const LOCAL_INVALID: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// The file the session `name` goes out as (WinSCP's `ValidLocalFileName` of its whole name with the
/// plugin's extension for NetBox's): `Work/Prod/db` as `Work-Prod-db.netway`.
pub fn export_file_name(name: &str) -> String {
    let base: String = (name.chars())
        .map(|char| {
            if LOCAL_INVALID.contains(&char) {
                '-'
            } else {
                char
            }
        })
        .collect();
    format!("{base}.{EXTENSION}")
}

/// The session's name a file comes in as: its name without its extension, when that can be one.
pub fn import_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    name_error(stem).is_none().then(|| stem.to_string())
}

/// NetBox's question of the export: the one entry's name, or how many.
pub fn export_prompt(names: &[&str]) -> String {
    match names {
        [one] => format!("Export session \"{one}\" to:"),
        _ => format!("Export {} session(s) to:", names.len()),
    }
}

/// The folder typed, taken from `folder` (the program's) when it is relative.
fn typed_folder(typed: &str, folder: Option<&Path>) -> PathBuf {
    let typed = PathBuf::from(typed.trim().trim_matches('"'));
    match folder {
        Some(folder) if typed.is_relative() => folder.join(typed),
        _ => typed,
    }
}

fn cancelled() -> Error {
    Error::new(ErrorKind::Cancelled, "Cancelled")
}

fn failed(text: String) -> Error {
    Error::new(ErrorKind::Other, text)
}

/// The answers of a question with "Yes", "No" and "Cancel".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overwrite {
    Yes,
    No,
    Cancel,
}

impl NetWay {
    /// The entries `request` names in the panel's folder `path` written out: F5 / F6 after "Export"
    /// (NetBox's), the viewer's, the editor's and Find file's each session's file under its own name.
    pub(crate) fn export_sessions(
        &mut self,
        path: &str,
        request: &GetRequest,
    ) -> Result<(), Error> {
        let folder = folder_of(path);
        let items: Vec<Item> = (request.names.iter())
            .filter_map(|name| self.store.item_named(folder, name))
            .collect();
        if !request.mode.with_dialog() {
            return self.write_out(folder, &items, &request.destination);
        }
        if items.is_empty() {
            return Ok(());
        }
        let names: Vec<&str> = items.iter().map(item_name).collect();
        let destination = request.destination.to_string_lossy();
        let typed = (self.host)
            .input_history(
                "Export",
                &export_prompt(&names),
                &destination,
                COPY_HISTORY,
                InputOptions::default(),
            )
            .ok_or_else(cancelled)?;
        let target = typed_folder(&typed, request.folder.as_deref());
        fs::create_dir_all(&target)
            .map_err(|error| failed(format!("{}: {error}", target.display())))?;
        let sessions = items.iter().flat_map(|item| match item {
            Item::Session(name) => vec![joined(folder, name)],
            Item::Folder(name) => self.store.sessions_under(&joined(folder, name)),
        });
        for name in sessions.collect::<Vec<_>>() {
            self.export_one(&name, &target)?;
        }
        Ok(())
    }

    /// One session copied into `target` as its file; one there asked over (NetBox's "File ... already
    /// exists. Overwrite?").
    fn export_one(&self, name: &str, target: &Path) -> Result<(), Error> {
        let file = target.join(export_file_name(name));
        if file.exists() {
            let question = format!("File {} already exists. Overwrite?", file.display());
            match self.ask_overwrite(&question) {
                Overwrite::Yes => {}
                Overwrite::No => return Ok(()),
                Overwrite::Cancel => return Err(cancelled()),
            }
        }
        (fs::copy(self.store.file(name), &file))
            .map(|_| ())
            .map_err(|error| failed(format!("{}: {error}", file.display())))
    }

    /// The sessions among `items` written into `destination` under their own names, with no window
    /// (the viewer's, the editor's, Find file's); nothing to write: stopped.
    fn write_out(&self, folder: &str, items: &[Item], destination: &Path) -> Result<(), Error> {
        let sessions: Vec<&String> = (items.iter())
            .filter_map(|item| match item {
                Item::Session(name) => Some(name),
                Item::Folder(_) => None,
            })
            .collect();
        if sessions.is_empty() {
            return Err(cancelled());
        }
        for name in sessions {
            let file = destination.join(name);
            (fs::copy(self.store.file(&joined(folder, name)), &file))
                .map_err(|error| failed(format!("{}: {error}", file.display())))?;
        }
        Ok(())
    }

    /// The disk's files `request` names taken in as sessions of the panel's folder `path`: those that are
    /// session files, after NetBox's question; a session of the name asked over.
    pub(crate) fn import_sessions(
        &mut self,
        path: &str,
        request: &PutRequest,
    ) -> Result<(), Error> {
        let files: Vec<(&PathBuf, String)> = (request.sources.iter())
            .filter(|source| source.is_file())
            .filter_map(|source| {
                let text = fs::read_to_string(source).ok()?;
                Session::is_session_text(&text).then_some((source, text))
            })
            .collect();
        if files.is_empty() {
            let first = request
                .sources
                .first()
                .map(|source| source.display().to_string());
            let text = format!(
                "File \"{}\" does not contain any session data.",
                first.unwrap_or_default()
            );
            return Err(failed(text));
        }
        if !request.mode.silent && !self.agrees("Import session(s) from selected file(s)?") {
            return Err(cancelled());
        }
        let folder = folder_of(path);
        for (source, text) in files {
            let Some(name) = import_name(source) else {
                continue;
            };
            let name = joined(folder, &name);
            if self.store.exists(&name) && !request.mode.silent {
                let question = format!("Session named '{name}' already exists. Overwrite?");
                match self.ask_overwrite(&question) {
                    Overwrite::Yes => {}
                    Overwrite::No => continue,
                    Overwrite::Cancel => return Err(cancelled()),
                }
            }
            (self.store.write_text(&name, &text))
                .map_err(|error| failed(format!("Cannot save session '{name}'.\n{error}")))?;
        }
        Ok(())
    }

    /// NetBox's confirmation with "Yes" and "No": true for "Yes".
    fn agrees(&self, question: &str) -> bool {
        let message = Message::new("Confirmation", &[question]).buttons(&["&Yes", "&No"]);
        self.host.show(&message) == Some(0)
    }

    /// NetBox's confirmation with "Yes", "No" and "Cancel"; Esc cancels.
    fn ask_overwrite(&self, question: &str) -> Overwrite {
        let message =
            Message::new("Confirmation", &[question]).buttons(&["&Yes", "&No", "&Cancel"]);
        match self.host.show(&message) {
            Some(0) => Overwrite::Yes,
            Some(1) => Overwrite::No,
            _ => Overwrite::Cancel,
        }
    }
}

fn item_name(item: &Item) -> &str {
    match item {
        Item::Folder(name) | Item::Session(name) => name,
    }
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
