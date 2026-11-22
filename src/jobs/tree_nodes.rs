//! A disk's tree as nodes (ADR-0014): each folder known, with the last write time it had when its folders
//! were read (none: not read yet), and the folders in it. A folder is known by its path under the root,
//! whatever its case; the first spelling met stays.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::platform::files::fold_case;

/// What joins the names of a key and of a line: no folder's name holds it.
const SEP: char = std::path::MAIN_SEPARATOR;

/// One folder of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub path: PathBuf,
    /// The folder's last write time when its folders were read.
    pub read: Option<SystemTime>,
    /// The keys of the folders in it.
    children: Vec<String>,
}

/// The folders of one root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nodes {
    root: PathBuf,
    map: HashMap<String, Node>,
}

/// The key of the folder `parts` lead to: its names as the system compares them, joined by [`SEP`].
fn key_of(parts: &[String]) -> String {
    (parts.iter().map(|part| fold_case(part)))
        .collect::<Vec<_>>()
        .join(std::path::MAIN_SEPARATOR_STR)
}

/// Whether the folder of `key` is the one of `above` or in it (the root's `above` is empty).
pub fn is_under(key: &str, above: &str) -> bool {
    above.is_empty()
        || (key.strip_prefix(above)).is_some_and(|rest| rest.is_empty() || rest.starts_with(SEP))
}

/// The key of the folder that holds the folder of `key`.
fn parent_key(key: &str) -> Option<&str> {
    if key.is_empty() {
        return None;
    }
    Some(key.rfind(SEP).map_or("", |at| &key[..at]))
}

impl Nodes {
    /// The tree of `root` alone.
    pub fn new(root: &Path) -> Self {
        let node = Node {
            path: root.to_path_buf(),
            read: None,
            children: Vec::new(),
        };
        Self {
            root: root.to_path_buf(),
            map: HashMap::from([(String::new(), node)]),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The folders known, the root among them.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// The names of `path` under the root; none for a path elsewhere.
    fn names(&self, path: &Path) -> Option<Vec<String>> {
        let rest = path.strip_prefix(&self.root).ok()?;
        Some(
            (rest.components())
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect(),
        )
    }

    pub fn key(&self, path: &Path) -> Option<String> {
        self.names(path).map(|names| key_of(&names))
    }

    pub fn get(&self, path: &Path) -> Option<&Node> {
        self.map.get(&self.key(path)?)
    }

    /// The keys and paths of the folders under the one of `key` (not it), the nearest first.
    pub fn under(&self, key: &str) -> Vec<(String, PathBuf)> {
        let mut found: Vec<(&String, &Node)> = (self.map.iter())
            .filter(|(other, _)| *other != key && is_under(other, key))
            .collect();
        found.sort_by_cached_key(|(other, _)| (other.matches(SEP).count(), *other));
        (found.into_iter())
            .map(|(other, node)| (other.clone(), node.path.clone()))
            .collect()
    }

    /// A folder renamed in its case only: it and the folders under it take the new name.
    pub fn respell(&mut self, from: &Path, to: &Path) {
        let Some(key) = self.key(from) else {
            return;
        };
        let depth = from.components().count();
        for (other, node) in &mut self.map {
            if is_under(other, &key) && !key.is_empty() {
                let rest: PathBuf = node.path.components().skip(depth).collect();
                node.path = to.join(rest);
            }
        }
    }

    /// Puts `path` in with the folders above it; true when something was new.
    pub fn insert(&mut self, path: &Path) -> bool {
        let Some(names) = self.names(path) else {
            return false;
        };
        // Known already (a folder listed again): nothing to go through.
        if self.map.contains_key(&key_of(&names)) {
            return false;
        }
        self.add(&names).0
    }

    /// A line of a tree's file: the folder the `names` under the root lead to, with the folders above
    /// it, and the time it was read at.
    pub fn put(&mut self, names: &[&str], read: Option<SystemTime>) {
        let (_, key) = self.add(names);
        if let Some(node) = self.map.get_mut(&key) {
            node.read = read;
        }
    }

    /// The folder of `names` and those above it put in: whether one was new, and its key.
    fn add<S: AsRef<str>>(&mut self, names: &[S]) -> (bool, String) {
        let mut new = false;
        let mut place = self.root.clone();
        let mut key = String::new();
        for name in names {
            let name = name.as_ref();
            place.push(name);
            let parent = key.len();
            if !key.is_empty() {
                key.push(SEP);
            }
            key.push_str(&fold_case(name));
            if self.map.contains_key(&key) {
                continue;
            }
            if let Some(node) = self.map.get_mut(&key[..parent]) {
                node.children.push(key.clone());
            }
            let node = Node {
                path: place.clone(),
                read: None,
                children: Vec::new(),
            };
            self.map.insert(key.clone(), node);
            new = true;
        }
        (new, key)
    }

    /// The time `path`'s folders were read at.
    pub fn set_read(&mut self, path: &Path, read: Option<SystemTime>) {
        if let Some(key) = self.key(path)
            && let Some(node) = self.map.get_mut(&key)
        {
            node.read = read;
        }
    }

    /// The folders in `path`, as known.
    pub fn children(&self, path: &Path) -> Vec<PathBuf> {
        let Some(node) = self.get(path) else {
            return Vec::new();
        };
        (node.children.iter())
            .filter_map(|key| self.map.get(key))
            .map(|child| child.path.clone())
            .collect()
    }

    /// Takes `path` out with what is under it (never the root); true when it was there.
    pub fn remove(&mut self, path: &Path) -> bool {
        let Some(key) = self.key(path).filter(|key| !key.is_empty()) else {
            return false;
        };
        if !self.map.contains_key(&key) {
            return false;
        }
        if let Some(parent) = parent_key(&key).and_then(|parent| self.map.get_mut(parent)) {
            parent.children.retain(|child| *child != key);
        }
        let mut gone = vec![key];
        while let Some(key) = gone.pop() {
            if let Some(node) = self.map.remove(&key) {
                gone.extend(node.children);
            }
        }
        true
    }

    /// Ctrl+R: no folder counts as read.
    pub fn forget_times(&mut self) {
        for node in self.map.values_mut() {
            node.read = None;
        }
    }

    /// The nodes, a folder before those in it (by key).
    pub fn in_order(&self) -> Vec<&Node> {
        let mut keys: Vec<&String> = self.map.keys().collect();
        keys.sort();
        keys.into_iter().map(|key| &self.map[key]).collect()
    }

    /// The paths of the folders under the root, the nearest first (a reading's order).
    pub fn by_depth(&self) -> Vec<PathBuf> {
        let mut keys: Vec<&String> = self.map.keys().filter(|key| !key.is_empty()).collect();
        keys.sort_by_cached_key(|key| (key.matches(SEP).count(), *key));
        keys.into_iter()
            .map(|key| self.map[key].path.clone())
            .collect()
    }

    /// The paths of every folder but the root.
    pub fn folders(&self) -> Vec<PathBuf> {
        (self.map.iter())
            .filter(|(key, _)| !key.is_empty())
            .map(|(_, node)| node.path.clone())
            .collect()
    }
}

#[cfg(test)]
#[path = "tree_nodes_tests.rs"]
mod tests;
