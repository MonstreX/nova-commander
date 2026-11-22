//! A temporary panel's list (FAR's TmpPanel): paths on a disk in the order they came, each once as it is
//! written (FAR's `RemoveDups` compares names exactly: `A.txt` and `a.txt` both stay, gh-1041); folders
//! put with their contents when asked; the names of files gone dropped as the panel is read, unless any
//! elements are kept (`AnyInPanel`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use nc_plugin_sdk::FileEntry;

/// One panel's paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct List {
    paths: Vec<PathBuf>,
    /// The paths' keys (`key`), so that a path is found at once among many.
    keys: HashSet<String>,
    /// The paths' entries as the search saw them, shown at the next reading instead of the disks'
    /// (FAR's `m_UpdateNotNeeded` after `SetFindList`); any other change of the list drops them.
    found: Option<Vec<FileEntry>>,
}

impl List {
    /// The paths, in their order.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Whether `path` is on the list (its name as it is written).
    pub fn holds(&self, path: &Path) -> bool {
        self.keys.contains(&key(path))
    }

    /// Alt+F7's found objects in place of the list, each once, their entries as the search saw them shown
    /// at the next reading (FAR's `SetFindList`: `clear()` first, `m_UpdateNotNeeded`).
    pub fn add_found(&mut self, found: &[FileEntry]) {
        self.clear();
        let mut shown = Vec::with_capacity(found.len());
        for entry in found {
            let path = (entry.real_path.clone()).unwrap_or_else(|| PathBuf::from(&entry.name));
            if self.add(path) {
                shown.push(entry.clone());
            }
        }
        self.found = Some(shown);
    }

    /// `path` put at the end, unless it is there; whether it was put.
    pub fn add(&mut self, path: PathBuf) -> bool {
        self.found = None;
        if !self.keys.insert(key(&path)) {
            return false;
        }
        self.paths.push(path);
        true
    }

    /// `folder` and everything in it (folders' links are not followed); `go_on` is asked before each
    /// entry (with the count so far and the entry): false stops the walk, and the answer is false.
    pub fn add_contents(
        &mut self,
        folder: &Path,
        go_on: &mut dyn FnMut(usize, &Path) -> bool,
    ) -> bool {
        self.add(folder.to_path_buf());
        let Ok(entries) = std::fs::read_dir(folder) else {
            return true;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !go_on(self.paths.len(), &path) {
                return false;
            }
            let walk = entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink());
            if walk {
                if !self.add_contents(&path, go_on) {
                    return false;
                }
            } else {
                self.add(path);
            }
        }
        true
    }

    /// Every path taken off (FAR's `Panel.clear()` before a list replaces them).
    pub fn clear(&mut self) {
        self.paths.clear();
        self.keys.clear();
        self.found = None;
    }

    /// `path` taken off; whether it was there.
    pub fn remove(&mut self, path: &Path) -> bool {
        self.remove_all([path]) == 1
    }

    /// The `paths` taken off in one pass over the list, however many (FAR's `REMOVE_FLAG` on each, then
    /// `RemoveEmptyItems`); how many of them were there.
    pub fn remove_all<'a>(&mut self, paths: impl IntoIterator<Item = &'a Path>) -> usize {
        self.found = None;
        let gone: HashSet<String> = (paths.into_iter())
            .map(key)
            .filter(|gone| self.keys.remove(gone))
            .collect();
        if !gone.is_empty() {
            (self.paths).retain(|known| !gone.contains(known.to_string_lossy().as_ref()));
        }
        gone.len()
    }

    /// The entries of the paths still there, with their real names unless `safe` (FAR's
    /// `SafeModePanel`: no `OPIF_REALNAMES`); the others are dropped (FAR's `UpdateItems`), or with `any`
    /// kept as they are written (`GetFileInfoAndValidate` with `Any`). Each folder is read once for the
    /// paths in it that follow one another (`update::look`).
    pub fn entries(&mut self, any: bool, safe: bool) -> Vec<FileEntry> {
        if let Some(found) = self.found.take() {
            return (found.into_iter())
                .map(|entry| match safe {
                    true => FileEntry {
                        real_path: None,
                        ..entry
                    },
                    false => entry,
                })
                .collect();
        }
        let mut looked = crate::update::look(&self.paths).into_iter();
        let mut entries = Vec::with_capacity(self.paths.len());
        let keys = &mut self.keys;
        self.paths.retain(|path| {
            let found = looked.next().flatten();
            match show(path, found, any, safe) {
                Some(made) => {
                    entries.push(made);
                    true
                }
                None => {
                    keys.remove(&key(path));
                    false
                }
            }
        });
        entries
    }
}

/// A path's key: its name as it is written (FAR's `RemoveDups`, `string_view ==`).
fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The entry `path` shows as: its file's (without its real name when `safe`), or with `any` one named as
/// it is written for a path that is not there (FAR's `FILE_ATTRIBUTE_ARCHIVE` item); `None`: dropped.
pub fn shown(path: &Path, any: bool, safe: bool) -> Option<FileEntry> {
    show(path, entry(path), any, safe)
}

/// What `path` shows as, `found` its entry when it is there.
fn show(path: &Path, found: Option<FileEntry>, any: bool, safe: bool) -> Option<FileEntry> {
    match found {
        Some(found) if safe => Some(FileEntry {
            real_path: None,
            ..found
        }),
        Some(found) => Some(found),
        None if any => Some(FileEntry::file(path.to_string_lossy().into_owned(), 0)),
        None => None,
    }
}

/// The entry of `path`, named by the whole path as FAR's TmpPanel names it, with its real name; `None`
/// when it is gone.
pub fn entry(path: &Path) -> Option<FileEntry> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(made(path, &metadata))
}

/// The entry of `path` from what the system told of it.
pub fn made(path: &Path, metadata: &std::fs::Metadata) -> FileEntry {
    let name = path.to_string_lossy().into_owned();
    let (hidden, system, read_only) = flags(path, metadata);
    FileEntry {
        is_dir: metadata.is_dir(),
        size: if metadata.is_dir() { 0 } else { metadata.len() },
        modified: metadata.modified().ok(),
        created: metadata.created().ok(),
        accessed: metadata.accessed().ok(),
        hidden,
        system,
        read_only,
        real_path: Some(path.to_path_buf()),
        ..FileEntry::file(name, 0)
    }
}

/// Hidden, system and read-only, as the system tells them.
#[cfg(windows)]
fn flags(_path: &Path, metadata: &std::fs::Metadata) -> (bool, bool, bool) {
    use std::os::windows::fs::MetadataExt;
    let bits = metadata.file_attributes();
    (bits & 0x2 != 0, bits & 0x4 != 0, bits & 0x1 != 0)
}

/// Hidden by a leading dot elsewhere.
#[cfg(not(windows))]
fn flags(path: &Path, metadata: &std::fs::Metadata) -> (bool, bool, bool) {
    let hidden = (path.file_name()).is_some_and(|name| name.to_string_lossy().starts_with('.'));
    (hidden, false, metadata.permissions().readonly())
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
