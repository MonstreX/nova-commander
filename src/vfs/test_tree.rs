//! `tree://<session>/...`: a folder tree in memory for tests of jobs that delete, copy and move through
//! a provider. A test builds it (`file`, `dir`, `read_only`, `link`, `write`), makes calls fail (`fail`,
//! `no_trash`, `limit_writes`) and reads what happened (`exists`, `read`, `log`). Listing a link is
//! logged, so a test can tell the link was never followed. Each test uses a session of its own.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, Cursor, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError};

use super::{Capabilities, Entry, Provider, Removal, VfsPath, WriteMode};
use crate::platform::files::{Attributes, LinkKind};

const READ_ONLY: u32 = 0x1;
const DIRECTORY: u32 = 0x10;
const REPARSE_POINT: u32 = 0x400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Dir,
    /// A link to a folder elsewhere: a folder entry with nothing of its own inside.
    Link,
}

#[derive(Debug, Clone, Copy)]
struct Node {
    kind: Kind,
    read_only: bool,
    size: u64,
}

#[derive(Default)]
struct State {
    /// Every object by its full text (`tree://s/a/b`); the root `tree://s/` included.
    nodes: BTreeMap<String, Node>,
    /// Calls that fail: how many more times, and how.
    failures: HashMap<String, (u32, io::ErrorKind)>,
    no_trash: Vec<String>,
    log: Vec<String>,
    /// What the files hold, by full text; a file not here is empty.
    contents: BTreeMap<String, Vec<u8>>,
    /// A file written past this many bytes fails: the disk is full.
    write_limits: HashMap<String, usize>,
    /// Objects a look at them does not find, as if they appeared just after it.
    hidden: Vec<String>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state() -> MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn with<T>(change: impl FnOnce(&mut State) -> T) -> T {
    change(state().get_or_insert_with(State::default))
}

struct Tree;

impl Provider for Tree {
    fn id(&self) -> &str {
        "tree"
    }

    fn capabilities(&self, _path: &VfsPath) -> Capabilities {
        Capabilities { trash: true }
    }

    fn list(
        &self,
        dir: &VfsPath,
        cancel: &AtomicBool,
        found: &mut dyn FnMut(Entry),
    ) -> io::Result<()> {
        let entries = with(|state| {
            state.log.push(format!("list {dir}"));
            state.take_failure(dir)?;
            match state.nodes.get(&dir.to_string()) {
                Some(node) if node.kind != Kind::File => Ok(state.children(dir)),
                Some(_) => Err(io::Error::other("not a folder")),
                None => Err(io::ErrorKind::NotFound.into()),
            }
        })?;
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            found(entry);
        }
        Ok(())
    }

    fn stat(&self, path: &VfsPath) -> io::Result<Entry> {
        with(|state| state.take_call_failure("stat", path))?;
        let node = with(|state| {
            let key = path.to_string();
            let hidden = state.hidden.contains(&key);
            state.nodes.get(&key).copied().filter(|_| !hidden)
        });
        node.map(|node| entry(path, node))
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    fn open_read(&self, path: &VfsPath) -> io::Result<Box<dyn Read + Send>> {
        let bytes = with(|state| state.open_read(path))?;
        Ok(Box::new(Cursor::new(bytes)))
    }

    fn open_write(&self, path: &VfsPath, mode: WriteMode) -> io::Result<Box<dyn Write + Send>> {
        with(|state| state.open_write(path, mode))?;
        Ok(Box::new(Writer {
            key: path.to_string(),
        }))
    }

    fn make_dir(&self, path: &VfsPath) -> io::Result<()> {
        with(|state| {
            state.log.push(format!("make-dir {path}"));
            state.take_failure(path)?;
            state.check_free(path)?;
            state.nodes.insert(path.to_string(), Node::new(Kind::Dir));
            Ok(())
        })
    }

    fn rename(&self, from: &VfsPath, to: &VfsPath, replace: bool) -> io::Result<()> {
        with(|state| state.rename(from, to, replace))
    }

    fn remove(&self, path: &VfsPath, how: Removal) -> io::Result<()> {
        with(|state| state.remove(path, how))
    }
}

/// Writes straight into the tree, so a test sees a half-written file.
struct Writer {
    key: String,
}

impl Write for Writer {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        with(|state| {
            let written = state.contents.entry(self.key.clone()).or_default();
            if state
                .write_limits
                .get(&self.key)
                .is_some_and(|limit| written.len() + buffer.len() > *limit)
            {
                return Err(io::ErrorKind::StorageFull.into());
            }
            written.extend_from_slice(buffer);
            let size = written.len() as u64;
            if let Some(node) = state.nodes.get_mut(&self.key) {
                node.size = size;
            }
            Ok(buffer.len())
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Node {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            read_only: false,
            size: 0,
        }
    }
}

impl State {
    fn take_failure(&mut self, path: &VfsPath) -> io::Result<()> {
        self.take_failure_of(path.to_string())
    }

    /// A failure set for this one kind of call on `path` (`rename`, `remove-file`, `open-write`).
    fn take_call_failure(&mut self, call: &str, path: &VfsPath) -> io::Result<()> {
        self.take_failure_of(format!("{call} {path}"))
    }

    fn take_failure_of(&mut self, key: String) -> io::Result<()> {
        let Some((times, kind)) = self.failures.get_mut(&key) else {
            return Ok(());
        };
        let kind = *kind;
        *times -= 1;
        if *times == 0 {
            self.failures.remove(&key);
        }
        Err(io::Error::new(kind, format!("{key} failed")))
    }

    /// A new object's name is free and its folder exists.
    fn check_free(&self, path: &VfsPath) -> io::Result<()> {
        if self.nodes.contains_key(&path.to_string()) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let parent = path.parent().map(|parent| parent.to_string());
        if !parent.is_some_and(|parent| self.nodes.contains_key(&parent)) {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(())
    }

    fn open_read(&mut self, path: &VfsPath) -> io::Result<Vec<u8>> {
        let key = path.to_string();
        self.log.push(format!("open-read {key}"));
        self.take_failure(path)?;
        match self.nodes.get(&key) {
            Some(node) if node.kind == Kind::File => {
                Ok(self.contents.get(&key).cloned().unwrap_or_default())
            }
            Some(_) => Err(io::Error::other("not a file")),
            None => Err(io::ErrorKind::NotFound.into()),
        }
    }

    fn open_write(&mut self, path: &VfsPath, mode: WriteMode) -> io::Result<()> {
        let key = path.to_string();
        self.log.push(format!("open-write {key}"));
        self.take_failure(path)?;
        match mode {
            WriteMode::New => {
                self.check_free(path)?;
                self.nodes.insert(key, Node::new(Kind::File));
            }
            WriteMode::Append if !self.nodes.contains_key(&key) => {
                return Err(io::ErrorKind::NotFound.into());
            }
            WriteMode::Append => {}
        }
        Ok(())
    }

    /// Takes the object and everything under it to the new name.
    fn rename(&mut self, from: &VfsPath, to: &VfsPath, replace: bool) -> io::Result<()> {
        let (old, new) = (from.to_string(), to.to_string());
        self.log.push(format!("rename {old} {new}"));
        self.take_failure(from)?;
        self.take_call_failure("rename", from)?;
        if !self.nodes.contains_key(&old) {
            return Err(io::ErrorKind::NotFound.into());
        }
        match self.nodes.get(&new) {
            Some(_) if !replace => return Err(io::ErrorKind::AlreadyExists.into()),
            Some(node) if node.kind != Kind::File => {
                return Err(io::Error::other("a folder cannot be replaced"));
            }
            Some(_) => {
                self.contents.remove(&new);
            }
            None => self.check_free(to)?,
        }
        let inside = format!("{old}/");
        let moved = |key: &String| *key == old || key.starts_with(&inside);
        let renamed = |key: &str| format!("{new}{}", &key[old.len()..]);
        for key in self
            .nodes
            .keys()
            .filter(|key| moved(key))
            .cloned()
            .collect::<Vec<_>>()
        {
            if let Some(node) = self.nodes.remove(&key) {
                self.nodes.insert(renamed(&key), node);
            }
        }
        for key in self
            .contents
            .keys()
            .filter(|key| moved(key))
            .cloned()
            .collect::<Vec<_>>()
        {
            if let Some(bytes) = self.contents.remove(&key) {
                self.contents.insert(renamed(&key), bytes);
            }
        }
        Ok(())
    }

    fn children(&self, dir: &VfsPath) -> Vec<Entry> {
        // By the text: a name may hold what a path would read otherwise (`..\x`), as a hostile
        // server's listing can.
        let dir_key = dir.to_string();
        let inside = format!("{}/", dir_key.trim_end_matches('/'));
        self.nodes
            .iter()
            .filter_map(|(key, node)| {
                let name = key.strip_prefix(&inside)?;
                (!name.is_empty() && !name.contains('/')).then(|| entry(&dir.join(name), *node))
            })
            .collect()
    }

    fn remove(&mut self, path: &VfsPath, how: Removal) -> io::Result<()> {
        let key = path.to_string();
        self.log.push(format!("{} {key}", verb(how)));
        self.take_failure(path)?;
        self.take_call_failure(verb(how), path)?;
        let node = *self.nodes.get(&key).ok_or(io::ErrorKind::NotFound)?;
        let inside = format!("{}/", key.trim_end_matches('/'));
        match (how, node.kind) {
            (Removal::Trash, _) => {
                if self.no_trash.contains(&key) {
                    return Err(io::Error::other("no Recycle Bin here"));
                }
                self.nodes
                    .retain(|other, _| *other != key && !other.starts_with(&inside));
            }
            (Removal::File, Kind::File) | (Removal::EmptyDir, Kind::Link) => {
                self.nodes.remove(&key);
            }
            (Removal::EmptyDir, Kind::Dir) => {
                if self.nodes.keys().any(|other| other.starts_with(&inside)) {
                    return Err(io::ErrorKind::DirectoryNotEmpty.into());
                }
                self.nodes.remove(&key);
            }
            _ => return Err(io::Error::other("not that kind of object")),
        }
        let nodes = &self.nodes;
        self.contents.retain(|key, _| nodes.contains_key(key));
        Ok(())
    }
}

fn verb(how: Removal) -> &'static str {
    match how {
        Removal::File => "remove-file",
        Removal::EmptyDir => "remove-dir",
        Removal::Trash => "trash",
    }
}

fn entry(path: &VfsPath, node: Node) -> Entry {
    let mut bits = 0;
    if node.read_only {
        bits |= READ_ONLY;
    }
    if node.kind != Kind::File {
        bits |= DIRECTORY;
    }
    if node.kind == Kind::Link {
        bits |= REPARSE_POINT;
    }
    Entry {
        name: path.file_name().unwrap_or_default(),
        is_dir: node.kind != Kind::File,
        attributes: Attributes::from_windows_bits(bits),
        link: (node.kind == Kind::Link).then_some(LinkKind::Junction),
        size: node.size,
        ..Entry::default()
    }
}

fn register() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| super::register(Arc::new(Tree)));
}

/// The empty root `tree://<session>/`.
pub fn root(session: &str) -> VfsPath {
    register();
    let root = VfsPath::parse(&format!("tree://{session}/"));
    put(&root, Kind::Dir, false);
    root
}

fn put(path: &VfsPath, kind: Kind, read_only: bool) -> VfsPath {
    with(|state| {
        state.nodes.insert(
            path.to_string(),
            Node {
                kind,
                read_only,
                size: 0,
            },
        )
    });
    path.clone()
}

pub fn file(path: &VfsPath) -> VfsPath {
    put(path, Kind::File, false)
}

/// A file holding `bytes`.
pub fn write(path: &VfsPath, bytes: &[u8]) -> VfsPath {
    sized(path, bytes.len() as u64);
    with(|state| state.contents.insert(path.to_string(), bytes.to_vec()));
    path.clone()
}

/// What a file holds.
pub fn read(path: &VfsPath) -> Vec<u8> {
    with(|state| {
        state
            .contents
            .get(&path.to_string())
            .cloned()
            .unwrap_or_default()
    })
}

/// Writing `path` past `bytes` fails: the disk is full.
pub fn limit_writes(path: &VfsPath, bytes: usize) {
    with(|state| state.write_limits.insert(path.to_string(), bytes));
}

/// A file of `size` bytes.
pub fn sized(path: &VfsPath, size: u64) -> VfsPath {
    file(path);
    with(|state| {
        if let Some(node) = state.nodes.get_mut(&path.to_string()) {
            node.size = size;
        }
    });
    path.clone()
}

pub fn dir(path: &VfsPath) -> VfsPath {
    put(path, Kind::Dir, false)
}

pub fn read_only(path: &VfsPath) -> VfsPath {
    put(path, Kind::File, true)
}

/// A link to a folder: deleting it must not look inside.
pub fn link(path: &VfsPath) -> VfsPath {
    put(path, Kind::Link, false)
}

/// The next `times` calls on `path` (listing, removing, opening, renaming it) fail with `kind`.
pub fn fail(path: &VfsPath, kind: io::ErrorKind, times: u32) {
    with(|state| state.failures.insert(path.to_string(), (times, kind)));
}

/// The next `times` calls of this kind on `path` fail with `kind` (`rename`, `remove-file`).
pub fn fail_call(call: &str, path: &VfsPath, kind: io::ErrorKind, times: u32) {
    with(|state| {
        state
            .failures
            .insert(format!("{call} {path}"), (times, kind))
    });
}

/// A look at `path` finds nothing, though it is there: it appeared just after the look.
pub fn hide(path: &VfsPath) {
    with(|state| state.hidden.push(path.to_string()));
}

/// The Recycle Bin refuses `path`.
pub fn no_trash(path: &VfsPath) {
    with(|state| state.no_trash.push(path.to_string()));
}

pub fn exists(path: &VfsPath) -> bool {
    with(|state| state.nodes.contains_key(&path.to_string()))
}

/// The entry of `path` as its folder lists it.
pub fn entry_of(path: &VfsPath) -> Entry {
    let node = with(|state| state.nodes.get(&path.to_string()).copied());
    node.map_or_else(Entry::default, |node| entry(path, node))
}

/// The calls made in `session`, in order (`list tree://s/a`, `remove-file ...`, `trash ...`).
pub fn log(session: &str) -> Vec<String> {
    let prefix = format!("tree://{session}/");
    with(|state| {
        state
            .log
            .iter()
            .filter(|line| {
                line.split_once(' ')
                    .is_some_and(|(_, path)| path.starts_with(&prefix))
            })
            .cloned()
            .collect()
    })
}
