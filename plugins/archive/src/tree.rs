//! An archive's folders as arclite lists them (`Archive::make_index`, `find_dir`, `get_dir_list`): every
//! item under its folder, a folder named only on the way to an item there too (no item of its own: the
//! archive file's times, 7-Zip's `m_num_indices` past the items). A folder is one whatever its names'
//! case (arclite's `DirInfo` compares with `lstrcmpiW`), the first spelling shown; `.` on a path is
//! passed over and `..` goes back to the folder before (the root stays the root). Files keep every name,
//! two told apart by case both listed.

use std::collections::HashMap;

use crate::item::Item;

/// One file or folder of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub is_dir: bool,
    /// The archive's item it is; none for a folder only implied.
    pub item: Option<usize>,
    pub children: Vec<usize>,
}

/// The tree; node 0 its root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub nodes: Vec<Node>,
    /// A folder's child folder by its name folded to lower case.
    folders: HashMap<(usize, String), usize>,
}

/// A name as folders are matched: without its case.
fn folded(name: &str) -> String {
    name.to_lowercase()
}

impl Tree {
    pub fn new(items: &[Item]) -> Self {
        let root = Node {
            name: String::new(),
            is_dir: true,
            item: None,
            children: Vec::new(),
        };
        let mut tree = Self {
            nodes: vec![root],
            folders: HashMap::new(),
        };
        for (index, item) in items.iter().enumerate() {
            tree.add(index, item);
        }
        tree
    }

    fn add(&mut self, index: usize, item: &Item) {
        let Some((last, parents)) = item.path.split_last() else {
            return;
        };
        let mut way = vec![0];
        for name in parents {
            match name.as_str() {
                "." => {}
                ".." => {
                    if way.len() > 1 {
                        way.pop();
                    }
                }
                _ => {
                    let here = way.last().copied().unwrap_or(0);
                    way.push(self.folder(here, name));
                }
            }
        }
        let parent = way.last().copied().unwrap_or(0);
        if matches!(last.as_str(), "." | "..") {
            return;
        }
        if item.is_dir {
            let node = self.folder(parent, last);
            // A folder named twice is the first item's.
            if self.nodes[node].item.is_none() {
                self.nodes[node].item = Some(index);
            }
            return;
        }
        let node = self.push(parent, last, false);
        self.nodes[node].item = Some(index);
    }

    /// The folder `name` in `parent` whatever its case, made when it is not there.
    fn folder(&mut self, parent: usize, name: &str) -> usize {
        let key = (parent, folded(name));
        if let Some(&found) = self.folders.get(&key) {
            return found;
        }
        let node = self.push(parent, name, true);
        self.folders.insert(key, node);
        node
    }

    fn push(&mut self, parent: usize, name: &str, is_dir: bool) -> usize {
        let node = self.nodes.len();
        self.nodes.push(Node {
            name: name.to_string(),
            is_dir,
            item: None,
            children: Vec::new(),
        });
        self.nodes[parent].children.push(node);
        node
    }

    /// The folder at `path` (`/`-separated, `/` the root) whatever its case; none when there is none.
    pub fn find_dir(&self, path: &str) -> Option<usize> {
        (path.split('/').filter(|part| !part.is_empty())).try_fold(0, |folder, name| {
            self.folders.get(&(folder, folded(name))).copied()
        })
    }

    /// The file or folder at `path`: a folder first when both are named so; a file by its name as
    /// written, else by its name whatever its case.
    pub fn find(&self, path: &str) -> Option<usize> {
        if let Some(folder) = self.find_dir(path) {
            return Some(folder);
        }
        let (parent, name) = path.trim_end_matches('/').rsplit_once('/')?;
        let folder = self.find_dir(parent)?;
        let files = || {
            (self.nodes[folder].children.iter().copied()).filter(|&child| !self.nodes[child].is_dir)
        };
        files()
            .find(|&child| self.nodes[child].name == name)
            .or_else(|| files().find(|&child| folded(&self.nodes[child].name) == folded(name)))
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
