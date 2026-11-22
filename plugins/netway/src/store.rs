//! The stored sessions (ADR-0023 §3): one file per session, `<name>.netway`, under the plugin's data
//! folder's `Sessions`, NetBox's session folders its folders. A session is named by its path from there,
//! the folders and its name joined by `/` (`Work/Prod/db`), as NetBox names them; names are the disk's,
//! so they compare as the disk does (on Windows whatever their case, as NetBox's).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::secret;
use crate::session::Session;

/// A session's file's extension.
pub const EXTENSION: &str = "netway";

/// One item of a sessions' folder.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Item {
    Folder(String),
    Session(String),
}

/// A session read: its settings, and whether its password could not be read back (kept by another user
/// or on another computer: the session comes without it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub session: Session,
    pub password_lost: bool,
}

/// The sessions' folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    root: PathBuf,
}

/// `folder` and `name` joined as a session's path (`Work` and `db`: `Work/db`; the root's `db`).
pub fn joined(folder: &str, name: &str) -> String {
    let folder = folder.trim_matches('/');
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// A session's path split into its folder and its name.
pub fn split(name: &str) -> (&str, &str) {
    name.rsplit_once('/').unwrap_or(("", name))
}

/// NetBox's `NB_SESSION_ALREADY_EXISTS_ERROR`.
fn taken(name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("Session named '{name}' already exists."),
    )
}

fn not_found(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("{what} is not there"))
}

impl Store {
    /// The sessions under `root` (the plugin's data folder's `Sessions`), made when the first is saved.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The disk's folder of the sessions' `folder` (`/`-separated, `""` or `/` the root).
    pub fn folder_path(&self, folder: &str) -> PathBuf {
        (folder.split('/').filter(|part| !part.is_empty()))
            .fold(self.root.clone(), |path, part| path.join(part))
    }

    /// The disk's file of the session `name`.
    pub fn file(&self, name: &str) -> PathBuf {
        let (folder, name) = split(name);
        self.folder_path(folder).join(format!("{name}.{EXTENSION}"))
    }

    /// The folders and sessions in `folder`, sorted; the root with none yet is empty, another folder
    /// not there is `NotFound`.
    pub fn items(&self, folder: &str) -> io::Result<Vec<Item>> {
        let path = self.folder_path(folder);
        let entries = match fs::read_dir(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound && path == self.root => {
                return Ok(Vec::new());
            }
            other => other?,
        };
        let mut items: Vec<Item> = entries.flatten().filter_map(|entry| item(&entry)).collect();
        items.sort();
        Ok(items)
    }

    /// Whether no session is stored anywhere (NetBox's F4 then adds one).
    pub fn has_none(&self) -> bool {
        !has_session(&self.root)
    }

    /// Whether the session `name` is there.
    pub fn exists(&self, name: &str) -> bool {
        self.file(name).is_file()
    }

    /// Whether the folder `folder` is there.
    pub fn folder_exists(&self, folder: &str) -> bool {
        self.folder_path(folder).is_dir()
    }

    /// The session `name`, its password read back when it can be.
    pub fn load(&self, name: &str) -> io::Result<Loaded> {
        load_file(&self.file(name))
    }

    /// `session` written as `name` (its folders made), over the one there: whether its password was
    /// kept (none given, or the system keeps no secrets here: false).
    pub fn save(&self, name: &str, session: &Session) -> io::Result<bool> {
        let kept = (!session.password.is_empty())
            .then(|| secret::keep(&session.password).ok())
            .flatten();
        self.write_text(name, &session.text(kept.as_deref()))?;
        Ok(kept.is_some())
    }

    /// The session `name`'s file as it is (its kept password and the keys not known with it).
    pub fn text(&self, name: &str) -> io::Result<String> {
        fs::read_to_string(self.file(name))
    }

    /// A session's file `text` written as `name` (its folders made), over the one there.
    pub fn write_text(&self, name: &str, text: &str) -> io::Result<()> {
        let file = self.file(name);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        // Written beside it, then put in its place: a failure leaves the old file whole.
        let temporary = file.with_extension(format!("{EXTENSION}.new"));
        fs::write(&temporary, text)?;
        fs::rename(&temporary, &file).inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        })
    }

    /// The whole names of the sessions in `folder` and under it, sorted as the panel shows them (a
    /// folder's before its sessions, NetBox's `ProcessSessions` on a folder).
    pub fn sessions_under(&self, folder: &str) -> Vec<String> {
        let Ok(items) = self.items(folder) else {
            return Vec::new();
        };
        (items.into_iter())
            .flat_map(|item| match item {
                Item::Folder(name) => self.sessions_under(&joined(folder, &name)),
                Item::Session(name) => vec![joined(folder, &name)],
            })
            .collect()
    }

    /// The session `from` renamed (or moved to another folder) as `to`, never over another session: a
    /// change of its letters' case alone renames it.
    pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        self.check_free(from, to)?;
        let target = self.file(to);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(self.file(from), target)
    }

    /// The session `from` copied as `to` (its kept password with it), never over a session, itself
    /// included (another case of its name on Windows).
    pub fn copy(&self, from: &str, to: &str) -> io::Result<()> {
        if self.same(from, to) {
            return Err(taken(to));
        }
        self.check_free(from, to)?;
        let target = self.file(to);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(self.file(from), target).map(|_| ())
    }

    /// Whether `from` and `to` name one session's file: another case of its name where the disk does not
    /// tell cases apart (Windows), never two sessions where it does (Linux).
    pub fn same(&self, from: &str, to: &str) -> bool {
        let (Ok(one), Ok(other)) = (
            fs::canonicalize(self.file(from)),
            fs::canonicalize(self.file(to)),
        ) else {
            return false;
        };
        one == other
    }

    /// `from` is there and `to` names no other session.
    fn check_free(&self, from: &str, to: &str) -> io::Result<()> {
        if !self.exists(from) {
            return Err(not_found(from));
        }
        if self.exists(to) && !self.same(from, to) {
            return Err(taken(to));
        }
        Ok(())
    }

    /// The folder `folder` made (with the folders above it).
    pub fn make_folder(&self, folder: &str) -> io::Result<()> {
        fs::create_dir_all(self.folder_path(folder))
    }

    /// The item `name` of `folder` removed: a session, or a folder with all its sessions.
    pub fn remove(&self, folder: &str, item: &Item) -> io::Result<()> {
        match item {
            Item::Session(name) => fs::remove_file(self.file(&joined(folder, name))),
            Item::Folder(name) => fs::remove_dir_all(self.folder_path(&joined(folder, name))),
        }
    }

    /// The item called `name` in `folder`: a folder first (NetBox's panel shows a folder for a name
    /// both are), then a session.
    pub fn item_named(&self, folder: &str, name: &str) -> Option<Item> {
        let path = joined(folder, name);
        if self.folder_exists(&path) {
            Some(Item::Folder(name.to_string()))
        } else {
            self.exists(&path).then(|| Item::Session(name.to_string()))
        }
    }
}

/// The session file `path` (the store's, or one of a disk's), its password read back when it can be.
pub fn load_file(path: &Path) -> io::Result<Loaded> {
    let text = fs::read_to_string(path)?;
    let (mut session, kept) = Session::parse(&text);
    let mut password_lost = false;
    if let Some(kept) = kept.filter(|kept| !kept.trim().is_empty()) {
        match secret::read(&kept) {
            Ok(password) => session.password = password,
            Err(_) => password_lost = true,
        }
    }
    Ok(Loaded {
        session,
        password_lost,
    })
}

/// A folder, or a session's file; anything else is not the store's.
fn item(entry: &fs::DirEntry) -> Option<Item> {
    let kind = entry.file_type().ok()?;
    let name = entry.file_name().into_string().ok()?;
    if kind.is_dir() {
        return Some(Item::Folder(name));
    }
    let path = Path::new(&name);
    let session = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION));
    let stem = path.file_stem()?.to_str()?;
    (kind.is_file() && session).then(|| Item::Session(stem.to_string()))
}

/// Whether a session's file is in `folder` or under it.
fn has_session(folder: &Path) -> bool {
    let Ok(entries) = fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().any(|entry| match item(&entry) {
        Some(Item::Session(_)) => true,
        Some(Item::Folder(_)) => has_session(&entry.path()),
        None => false,
    })
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
