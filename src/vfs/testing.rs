//! Providers for tests of code that waits for a provider:
//! - `endless://<name>/` lists two entries, `first` and `second`, then waits until it is cancelled (`stopped(name)`
//!   tells it saw the cancel);
//! - `gate://<name>/` lists nothing until `open(name)`, then fails with `PermissionDenied`;
//! - `changing://...` lists the folders `put` makes, reports `put` and `remove` to its watches; a
//!   folder `deny` marks fails with `PermissionDenied`.

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once, PoisonError};
use std::time::Duration;

use super::{Entry, Provider, VfsPath, Watch};

struct Endless;

impl Provider for Endless {
    fn id(&self) -> &str {
        "endless"
    }

    fn list(
        &self,
        dir: &VfsPath,
        cancel: &AtomicBool,
        found: &mut dyn FnMut(Entry),
    ) -> io::Result<()> {
        for name in ["first", "second"] {
            found(Entry {
                name: name.to_string(),
                ..Entry::default()
            });
        }
        while !cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(1));
        }
        mark(&STOPPED, session(dir));
        Err(io::ErrorKind::Interrupted.into())
    }
}

struct Gate;

impl Provider for Gate {
    fn id(&self) -> &str {
        "gate"
    }

    fn list(
        &self,
        dir: &VfsPath,
        cancel: &AtomicBool,
        _found: &mut dyn FnMut(Entry),
    ) -> io::Result<()> {
        while !is_marked(&OPENED, session(dir)) {
            if cancel.load(Ordering::Relaxed) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(io::ErrorKind::PermissionDenied.into())
    }
}

struct Changing;

impl Provider for Changing {
    fn id(&self) -> &str {
        "changing"
    }

    fn list(
        &self,
        dir: &VfsPath,
        _cancel: &AtomicBool,
        found: &mut dyn FnMut(Entry),
    ) -> io::Result<()> {
        for name in folder(dir)? {
            found(Entry {
                name,
                ..Entry::default()
            });
        }
        Ok(())
    }

    fn watch(&self, dir: &VfsPath) -> io::Result<Box<dyn Watch>> {
        folder(dir)?;
        // A watch sees the changes from its start on.
        if let Some(changed) = CHANGED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            changed.remove(&dir.to_string());
        }
        Ok(Box::new(ChangingWatch(dir.to_string())))
    }
}

#[derive(Debug)]
struct ChangingWatch(String);

impl Watch for ChangingWatch {
    fn take_change(&self) -> bool {
        CHANGED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
            .is_some_and(|changed| changed.remove(&self.0))
    }
}

static FOLDERS: Mutex<Option<HashMap<String, Vec<String>>>> = Mutex::new(None);
static CHANGED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

static DENIED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn folder(dir: &VfsPath) -> io::Result<Vec<String>> {
    if is_marked(&DENIED, &dir.to_string()) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    FOLDERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|folders| folders.get(&dir.to_string()).cloned())
        .ok_or_else(|| io::ErrorKind::NotFound.into())
}

static STOPPED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static OPENED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn session(dir: &VfsPath) -> &str {
    match dir {
        VfsPath::Mounted {
            base: super::path::Base::Session(session),
            ..
        } => session,
        _ => "",
    }
}

fn mark(set: &Mutex<Option<HashSet<String>>>, name: &str) {
    set.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashSet::new)
        .insert(name.to_string());
}

fn is_marked(set: &Mutex<Option<HashSet<String>>>, name: &str) -> bool {
    set.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|names| names.contains(name))
}

fn register() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        super::register(Arc::new(Endless));
        super::register(Arc::new(Gate));
        super::register(Arc::new(Changing));
    });
}

/// `endless://test/`, its provider registered.
pub fn endless() -> VfsPath {
    endless_named("test")
}

/// `endless://<name>/`: a test that checks `stopped` uses a name of its own.
pub fn endless_named(name: &str) -> VfsPath {
    register();
    VfsPath::parse(&format!("endless://{name}/"))
}

/// Whether the reading of `endless://<name>/` saw its cancel and ended.
pub fn stopped(name: &str) -> bool {
    is_marked(&STOPPED, name)
}

/// `gate://<name>/`, closed until `open(name)`.
pub fn gate(name: &str) -> VfsPath {
    register();
    VfsPath::parse(&format!("gate://{name}/"))
}

pub fn open(name: &str) {
    mark(&OPENED, name);
}

/// Makes (or changes) the `changing://` folder `path` with `names`; its watches see a change.
pub fn put(path: &str, names: &[&str]) -> VfsPath {
    register();
    let dir = VfsPath::parse(path);
    FOLDERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(
            dir.to_string(),
            names.iter().map(ToString::to_string).collect(),
        );
    mark(&CHANGED, &dir.to_string());
    dir
}

/// Removes the `changing://` folder `path`; its watches see a change.
pub fn remove(path: &str) {
    let dir = VfsPath::parse(path);
    if let Some(folders) = FOLDERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_mut()
    {
        folders.remove(&dir.to_string());
    }
    mark(&CHANGED, &dir.to_string());
}

/// Makes the `changing://` folder `path` deny access (`allow` lifts it); its watches see a change.
pub fn deny(path: &str, denied: bool) {
    let key = VfsPath::parse(path).to_string();
    let mut set = DENIED.lock().unwrap_or_else(PoisonError::into_inner);
    let set = set.get_or_insert_with(HashSet::new);
    if denied {
        set.insert(key.clone());
    } else {
        set.remove(&key);
    }
    mark(&CHANGED, &key);
}
