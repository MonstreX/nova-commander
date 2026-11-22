//! The probe's panels (ADR-0016 5c): a small tree in memory for each instance, made anew as it is first
//! reached. `/`: the folders `docs` and `locked`, `readme.txt` ("Hello probe!", modified a second after
//! 1970) and the hidden `.hidden`; `/docs`: `a.txt` ("a") and the empty folder `deep`; `/locked` is
//! denied; any other path is not found. Files are read, written, made, renamed and removed in it
//! (5c-2); a file named `full...` takes no more after it is made (a write refused mid-file). The instance
//! `refused` lists nothing: every path is denied. An entry put in keeps the times and marks it is given
//! (P6.0d); the instance `norename` renames nothing (a plugin that only writes). The instance `real` (5c-2b) lists the files of the
//! probe's own folder with their real names: the program reads them from the disk (the probe refuses
//! to), and deleting one only takes it off the list, as FAR's TmpPanel does.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nc_plugin_sdk::{Error, ErrorKind, FileEntry, PanelMode, Times, WriteMode};

/// One entry of a tree.
#[derive(Debug, Clone, Default)]
struct Node {
    /// When it was made: a folder lists its entries in this order (the plugin's own).
    order: usize,
    folder: bool,
    data: Vec<u8>,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
    hidden: bool,
    system: bool,
    read_only: bool,
}

/// Every instance's tree: entries by their path.
#[derive(Default)]
pub struct Panels {
    trees: HashMap<String, BTreeMap<String, Node>>,
    /// The folder the instance `real` shows, and the names taken off its list.
    real: PathBuf,
    real_gone: HashSet<String>,
}

fn not_found(path: &str) -> Error {
    Error::new(ErrorKind::NotFound, format!("The probe has no {path}"))
}

fn exists(path: &str) -> Error {
    Error::new(ErrorKind::Exists, format!("The probe has {path} already"))
}

/// The folder `path` is in.
fn parent(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some(("", _)) | None => "/",
        Some((parent, _)) => parent,
    }
}

fn first_tree() -> BTreeMap<String, Node> {
    let mut tree = unordered_first_tree();
    for (order, path) in [
        "/",
        "/docs",
        "/locked",
        "/readme.txt",
        "/.hidden",
        "/docs/a.txt",
        "/docs/deep",
    ]
    .iter()
    .enumerate()
    {
        if let Some(node) = tree.get_mut(*path) {
            node.order = order;
        }
    }
    tree
}

fn unordered_first_tree() -> BTreeMap<String, Node> {
    let folder = Node {
        folder: true,
        ..Node::default()
    };
    let file = |data: &[u8]| Node {
        data: data.to_vec(),
        ..Node::default()
    };
    BTreeMap::from([
        ("/".to_string(), folder.clone()),
        ("/docs".to_string(), folder.clone()),
        ("/locked".to_string(), folder.clone()),
        (
            "/readme.txt".to_string(),
            Node {
                modified: Some(UNIX_EPOCH + Duration::from_secs(1)),
                ..file(b"Hello probe!")
            },
        ),
        (
            "/.hidden".to_string(),
            Node {
                hidden: true,
                ..file(b"")
            },
        ),
        ("/docs/a.txt".to_string(), file(b"a")),
        ("/docs/deep".to_string(), folder),
    ])
}

impl Panels {
    /// The panels, `real` showing the files of `folder`.
    pub fn new(folder: PathBuf) -> Self {
        Self {
            real: folder,
            ..Self::default()
        }
    }

    /// The instance `real`'s entries at its root: the folder's files not taken off.
    fn real_entries(&self) -> Vec<FileEntry> {
        let Ok(found) = std::fs::read_dir(&self.real) else {
            return Vec::new();
        };
        let mut entries: Vec<FileEntry> = (found.flatten())
            .filter(|item| item.path().is_file())
            .map(|item| FileEntry {
                real_path: Some(item.path()),
                ..FileEntry::file(
                    item.file_name().to_string_lossy(),
                    item.metadata().map_or(0, |data| data.len()),
                )
            })
            .filter(|entry| !self.real_gone.contains(&entry.name))
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }

    /// A call of the instance `real` other than listing its root.
    fn real_call(&mut self, path: &str, remove: bool) -> Result<FileEntry, Error> {
        let name = path.trim_start_matches('/');
        let found = (self.real_entries().into_iter())
            .find(|entry| entry.name == name)
            .ok_or_else(|| not_found(path))?;
        if remove {
            self.real_gone.insert(name.to_string());
        }
        Ok(found)
    }

    /// The tree of `instance` as a call reaches `path` in it; the instance `real` has none (it takes no
    /// files, folders or renames: its entries are the disk's).
    fn tree(&mut self, instance: &str, path: &str) -> Result<&mut BTreeMap<String, Node>, Error> {
        if is_real(instance) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!("The probe's real panel takes nothing at {path}"),
            ));
        }
        if instance == "refused" {
            return Err(Error::new(
                ErrorKind::Denied,
                "The probe refuses this panel",
            ));
        }
        if path == "/locked" || path.starts_with("/locked/") {
            return Err(Error::new(ErrorKind::Denied, "The probe keeps it locked"));
        }
        Ok(self
            .trees
            .entry(instance.to_string())
            .or_insert_with(first_tree))
    }

    pub fn list(&mut self, instance: &str, path: &str) -> Result<Vec<FileEntry>, Error> {
        if is_real(instance) {
            return match path {
                "/" => Ok(self.real_entries()),
                _ => Err(not_found(path)),
            };
        }
        let tree = self.tree(instance, path)?;
        if !tree.get(path).is_some_and(|node| node.folder) {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("The probe has no folder {path}"),
            ));
        }
        let mut found: Vec<(&String, &Node)> = (tree.iter())
            .filter(|(key, _)| key.as_str() != "/" && parent(key) == path)
            .collect();
        found.sort_by_key(|(_, node)| node.order);
        Ok(found
            .into_iter()
            .map(|(key, node)| entry(key, node))
            .collect())
    }

    pub fn stat(&mut self, instance: &str, path: &str) -> Result<FileEntry, Error> {
        if is_real(instance) {
            return match path {
                "/" => Ok(FileEntry::folder("")),
                _ => self.real_call(path, false),
            };
        }
        let tree = self.tree(instance, path)?;
        let node = tree.get(path).ok_or_else(|| not_found(path))?;
        Ok(entry(path, node))
    }

    pub fn read(
        &mut self,
        instance: &str,
        path: &str,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, Error> {
        if instance == "ownreal" {
            // Its own `GetFiles`: the real file read by the probe itself.
            let file = self.real.join(path.trim_start_matches('/'));
            return read_at(&file, offset, buffer)
                .map_err(|error| Error::new(ErrorKind::Other, error.to_string()));
        }
        if is_real(instance) {
            // A real file is the program's to read from the disk.
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!("The probe never reads its real file {path}"),
            ));
        }
        let tree = self.tree(instance, path)?;
        let node = (tree.get(path))
            .filter(|node| !node.folder)
            .ok_or_else(|| not_found(path))?;
        let from = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(node.data.len());
        let count = buffer.len().min(node.data.len() - from);
        buffer[..count].copy_from_slice(&node.data[from..from + count]);
        Ok(count)
    }

    pub fn write(
        &mut self,
        instance: &str,
        path: &str,
        mode: WriteMode,
        data: &[u8],
    ) -> Result<(), Error> {
        let tree = self.tree(instance, path)?;
        match mode {
            WriteMode::New => {
                make(tree, path, Node::default())?;
                tree.entry(path.to_string())
                    .or_default()
                    .data
                    .extend_from_slice(data);
            }
            WriteMode::Append
                if path
                    .rsplit('/')
                    .next()
                    .is_some_and(|name| name.starts_with("full")) =>
            {
                return Err(Error::new(ErrorKind::Other, "The probe is full"));
            }
            WriteMode::Append => {
                let node = (tree.get_mut(path))
                    .filter(|node| !node.folder)
                    .ok_or_else(|| not_found(path))?;
                node.data.extend_from_slice(data);
            }
        }
        Ok(())
    }

    pub fn make_dir(&mut self, instance: &str, path: &str) -> Result<(), Error> {
        let tree = self.tree(instance, path)?;
        make(
            tree,
            path,
            Node {
                folder: true,
                ..Node::default()
            },
        )
    }

    pub fn rename(
        &mut self,
        instance: &str,
        from: &str,
        to: &str,
        replace: bool,
    ) -> Result<(), Error> {
        if instance == "norename" {
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!("The probe renames nothing here: {from}"),
            ));
        }
        self.tree(instance, to)?;
        let tree = self.tree(instance, from)?;
        if !tree.contains_key(from) || from == "/" {
            return Err(not_found(from));
        }
        if !tree.get(parent(to)).is_some_and(|node| node.folder) {
            return Err(not_found(parent(to)));
        }
        if tree.contains_key(to) && (!replace || tree.get(to).is_some_and(|node| node.folder)) {
            // A folder is never replaced, as a disk's is not.
            return Err(exists(to));
        }
        if to.starts_with(&format!("{from}/")) {
            return Err(Error::new(
                ErrorKind::Other,
                format!("The probe cannot move {from} into itself"),
            ));
        }
        let inside = format!("{from}/");
        let moved: Vec<String> = (tree.keys())
            .filter(|key| key.as_str() == from || key.starts_with(&inside))
            .cloned()
            .collect();
        for key in moved {
            if let Some(node) = tree.remove(&key) {
                tree.insert(format!("{to}{}", &key[from.len()..]), node);
            }
        }
        Ok(())
    }

    pub fn remove(&mut self, instance: &str, path: &str, folder: bool) -> Result<(), Error> {
        if is_real(instance) && !folder {
            // Off the list, the file left where it is.
            return self.real_call(path, true).map(drop);
        }
        let tree = self.tree(instance, path)?;
        let node = tree.get(path).ok_or_else(|| not_found(path))?;
        if node.folder != folder || path == "/" {
            return Err(Error::new(
                ErrorKind::Other,
                format!("The probe's {path} is not that kind of entry"),
            ));
        }
        let inside = format!("{path}/");
        if tree.keys().any(|key| key.starts_with(&inside)) {
            return Err(Error::new(
                ErrorKind::Other,
                format!("The probe's {path} is not empty"),
            ));
        }
        tree.remove(path);
        Ok(())
    }
}

impl Panels {
    /// The entry `path` given the times and marks of what was put in.
    pub fn set_times(&mut self, instance: &str, path: &str, times: &Times) -> Result<(), Error> {
        let tree = self.tree(instance, path)?;
        let node = tree.get_mut(path).ok_or_else(|| not_found(path))?;
        node.modified = times.modified.or(node.modified);
        node.created = times.created.or(node.created);
        node.accessed = times.accessed.or(node.accessed);
        if let Some(marks) = times.attributes {
            (node.hidden, node.system, node.read_only) =
                (marks.hidden, marks.system, marks.read_only);
        }
        Ok(())
    }
}

/// `node` put at `path`: its folder there, no entry of that name.
fn make(tree: &mut BTreeMap<String, Node>, path: &str, node: Node) -> Result<(), Error> {
    if !tree.get(parent(path)).is_some_and(|node| node.folder) {
        return Err(not_found(parent(path)));
    }
    if tree.contains_key(path) {
        return Err(exists(path));
    }
    let order = tree.values().map(|known| known.order).max().unwrap_or(0) + 1;
    tree.insert(path.to_string(), Node { order, ..node });
    Ok(())
}

/// An entry with its texts for the modes' columns: `C0` its kind, `C1` its name's length, `Z` "About ...".
fn entry(path: &str, node: &Node) -> FileEntry {
    let name = path.rsplit('/').next().unwrap_or_default().to_string();
    let kind = if node.folder { "folder" } else { "file" };
    FileEntry {
        columns: vec![kind.to_string(), name.chars().count().to_string()],
        description: format!("About {name}"),
        name,
        is_dir: node.folder,
        size: node.data.len() as u64,
        modified: node.modified,
        created: node.created,
        accessed: node.accessed,
        hidden: node.hidden,
        system: node.system,
        read_only: node.read_only,
        ..FileEntry::default()
    }
}

/// Up to `buffer.len()` bytes of `file` from `offset` (fewer only at its end).
fn read_at(file: &std::path::Path, offset: u64, buffer: &mut [u8]) -> std::io::Result<usize> {
    use std::io::{Read, Seek, SeekFrom};
    let mut opened = std::fs::File::open(file)?;
    opened.seek(SeekFrom::Start(offset))?;
    let mut done = 0;
    while done < buffer.len() {
        match opened.read(&mut buffer[done..])? {
            0 => break,
            count => done += count,
        }
    }
    Ok(done)
}

/// The instances listing the probe's folder with real names: `real`, and `ownreal` which reads and deletes
/// them itself (FAR's plugin with its own `GetFiles` / `DeleteFiles`).
fn is_real(instance: &str) -> bool {
    matches!(instance, "real" | "ownreal")
}

/// The probe's own modes (5c-3): Ctrl+3 its kind and description with their titles and the detailed
/// status line, Ctrl+6 the name's length on the whole screen with its own status line; the program's at
/// the other keys. The instances `real` and `untitled` have none.
pub fn modes(instance: &str) -> Vec<Option<PanelMode>> {
    if matches!(instance, "real" | "ownreal" | "untitled") {
        return Vec::new();
    }
    // "-" stands for the program's own title (`None`).
    let titles = |titles: &[&str]| {
        (titles.iter())
            .map(|title| (*title != "-").then(|| title.to_string()))
            .collect()
    };
    let kinds = PanelMode {
        columns: "N,C0,Z".to_string(),
        widths: "0,6,0".to_string(),
        titles: titles(&["Item", "Kind", "About"]),
        detailed_status: true,
        ..PanelMode::default()
    };
    let letters = PanelMode {
        columns: "N,C1".to_string(),
        widths: "0,7".to_string(),
        titles: titles(&["-", "Letters"]),
        status_columns: "C0,NR".to_string(),
        status_widths: "6,0".to_string(),
        full_screen: true,
        ..PanelMode::default()
    };
    vec![None, None, None, Some(kinds), None, None, Some(letters)]
}
