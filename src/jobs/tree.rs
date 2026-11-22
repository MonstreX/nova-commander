//! FAR's folder tree (`far/treelist.cpp` `FillLastData`; `ScanTree` of folders only): the tree's lines
//! in its order with what each draws, and every folder under one (links to folders walked into as FAR's
//! default "Scan symbolic links" does, never into a folder a link is inside of) for the trees' files.
//! The tree panel's worker is `tree_worker` (ADR-0014).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as Atomic};

use super::folder_size::identity;
use crate::platform::files::compare_wide_names;
use crate::vfs::{Entry, VfsPath, provider_for};

/// A folder's line: its path, its depth under the root (the root is 0) and, for each level from the
/// first to its own, whether the folder there is the last of its parent (FAR's `TreeItem::Last`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeItem {
    pub path: PathBuf,
    pub depth: usize,
    pub last: Vec<bool>,
}

/// A folder being walked: the folders in it still to go into.
struct Level {
    folder: VfsPath,
    folders: std::vec::IntoIter<Entry>,
    place: Option<Option<(u32, u64)>>,
}

impl Level {
    fn place(&mut self) -> Option<(u32, u64)> {
        *self.place.get_or_insert_with(|| identity(&self.folder))
    }
}

fn open(folder: &VfsPath, stop: &AtomicBool) -> Level {
    let mut entries = Vec::new();
    if let Ok(provider) = provider_for(folder) {
        // A folder that cannot be read gives what was listed of it.
        let _ = provider.list(folder, stop, &mut |entry| {
            if entry.is_dir && !entry.is_parent {
                entries.push(entry);
            }
        });
    }
    Level {
        folder: folder.clone(),
        folders: entries.into_iter(),
        place: None,
    }
}

/// Every folder under `root`, as met.
fn walk(root: &VfsPath, count: &AtomicUsize, stop: &AtomicBool) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut levels = vec![open(root, stop)];
    while let Some(level) = levels.last_mut() {
        if stop.load(Atomic::SeqCst) {
            break;
        }
        let Some(folder) = level.folders.next() else {
            levels.pop();
            continue;
        };
        let path = level.folder.join(&folder.name);
        if let Some(local) = path.as_local() {
            found.push(local.to_path_buf());
        }
        count.store(found.len(), Atomic::SeqCst);
        let leads_back = folder.points_elsewhere() && {
            let place = identity(&path);
            place.is_some() && levels.iter_mut().any(|level| level.place() == place)
        };
        if !leads_back {
            levels.push(open(&path, stop));
        }
    }
    found
}

/// Every folder under `folder` (FAR's `ReadSubTree`), for the trees' files.
pub(super) fn folders_under(folder: &Path) -> Vec<PathBuf> {
    let (count, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
    walk(&VfsPath::local(folder), &count, &stop)
}

/// Each name's place in FAR's name order, the names it takes as the same and that differ beyond their
/// case (`01`, `1`) by their small letters; one place for the same name in two cases. Each name is
/// encoded once and sorted once: a big tree's paths are then sorted by their names' places.
fn places_of<'a>(names: impl Iterator<Item = &'a str>) -> HashMap<&'a str, u32> {
    let distinct: HashSet<&str> = names.collect();
    let mut sorted: Vec<(&str, Vec<u16>, String)> = (distinct.into_iter())
        .map(|name| (name, name.encode_utf16().collect(), name.to_lowercase()))
        .collect();
    sorted.sort_by(|a, b| compare_wide_names(&a.1, &b.1).then_with(|| a.2.cmp(&b.2)));
    let mut places = HashMap::with_capacity(sorted.len());
    let mut place = 0;
    for (at, (name, wide, lower)) in sorted.iter().enumerate() {
        if let Some((_, before, before_lower)) = at.checked_sub(1).map(|at| &sorted[at])
            && (compare_wide_names(before, wide).is_ne() || before_lower != lower)
        {
            place += 1;
        }
        places.insert(*name, place);
    }
    places
}

/// The lines of `root` and `folders` (those under it, each once whatever its case, as FAR's tree keeps
/// them: the first given stays) in the tree's order (name by name with FAR's name order, a folder
/// before what is in it), with FAR's `FillLastData`.
pub fn make_items(root: &Path, folders: Vec<PathBuf>) -> Vec<TreeItem> {
    let named: Vec<(Vec<String>, PathBuf)> = (folders.into_iter())
        .filter_map(|folder| {
            let rest = folder.strip_prefix(root).ok()?;
            let names: Vec<String> = (rest.components())
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            (!names.is_empty()).then_some((names, folder))
        })
        .collect();
    let places = places_of(
        named
            .iter()
            .flat_map(|(names, _)| names.iter().map(String::as_str)),
    );
    let mut ranked: Vec<(Vec<u32>, PathBuf)> = (named.iter())
        .map(|(names, path)| {
            let ranks = names.iter().map(|name| places[name.as_str()]).collect();
            (ranks, path.clone())
        })
        .collect();
    // A stable sort: of the same folder in two cases the first given comes first and stays.
    ranked.sort_by(|a, b| a.0.cmp(&b.0));
    ranked.dedup_by(|a, b| a.0 == b.0);
    lay_out(
        root,
        ranked.into_iter().map(|(ranks, path)| (ranks.len(), path)),
    )
}

/// The lines of folders already in the tree's order, given with their depths.
pub fn lay_out(root: &Path, folders: impl Iterator<Item = (usize, PathBuf)>) -> Vec<TreeItem> {
    let mut items = vec![TreeItem {
        path: root.to_path_buf(),
        depth: 0,
        last: Vec::new(),
    }];
    items.extend(folders.map(|(depth, path)| TreeItem {
        path,
        depth,
        last: Vec::new(),
    }));
    // From the end: a folder is the last of its parent when no folder of its depth came after it
    // before a shallower one.
    let mut seen: Vec<bool> = Vec::new();
    let mut last = vec![true; items.len()];
    for at in (1..items.len()).rev() {
        let depth = items[at].depth;
        seen.resize(depth + 1, false);
        last[at] = !seen[depth];
        seen[depth] = true;
        seen.truncate(depth + 1);
    }
    let mut above: Vec<bool> = Vec::new();
    for (item, own) in items.iter_mut().zip(last).skip(1) {
        above.resize(item.depth - 1, true);
        above.push(own);
        item.last.clone_from(&above);
    }
    items
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
