//! FAR's `UpdateItems` (`TmpClass.cpp:368-456`): the panel's paths looked at again as it is read. Paths that
//! follow one another in one folder are looked up in one reading of that folder (FAR's `SameFolderItems`:
//! one `FindFirstFile` over the folder, not one per file), names compared as the system compares them; a
//! path alone in its folder, and the paths of a folder that cannot be read, are looked at one by one. A
//! link is followed as for one path alone (`list::entry`).
//! A name the folder has not is gone as it is when alone, so "any elements" keeps it (FAR drops it there:
//! deviation D22).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nc_plugin_sdk::FileEntry;

use crate::list::{entry, made};

/// The entry of each of `paths`, in their order; `None` for one that is not there.
pub fn look(paths: &[PathBuf]) -> Vec<Option<FileEntry>> {
    let mut looked = Vec::with_capacity(paths.len());
    let mut at = 0;
    while at < paths.len() {
        let run = same_folder(&paths[at..]);
        let group = &paths[at..at + run];
        match (run >= 2).then(|| folder_of(&group[0])).flatten() {
            Some(folder) => looked.extend(in_folder(folder, group)),
            None => looked.extend(group.iter().map(|path| entry(path))),
        }
        at += run;
    }
    looked
}

/// How many paths from the first are in its folder, one after another (at least the first).
fn same_folder(paths: &[PathBuf]) -> usize {
    let Some(folder) = folder_of(&paths[0]) else {
        return 1;
    };
    1 + (paths[1..].iter())
        .take_while(|path| folder_of(path) == Some(folder))
        .count()
}

/// The folder a path names a file or folder in (FAR's `Dir`, up to its last `\`); none for a root, a
/// path ending in a separator or in `..` (one ending in `.` is its folder's own entry in the folder
/// above: the same object).
fn folder_of(path: &Path) -> Option<&Path> {
    path.file_name()?;
    let ends_in_separator = (path.to_string_lossy()).ends_with(std::path::is_separator);
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty() && !ends_in_separator)
}

/// The paths of `group`, all in `folder`, from one reading of it; each one by one when it cannot be read.
fn in_folder(folder: &Path, group: &[PathBuf]) -> Vec<Option<FileEntry>> {
    let mut wanted: HashMap<String, Vec<usize>> = HashMap::with_capacity(group.len());
    for (at, path) in group.iter().enumerate() {
        let name = path.file_name().map(|name| fold(&name.to_string_lossy()));
        wanted.entry(name.unwrap_or_default()).or_default().push(at);
    }
    let Ok(listing) = std::fs::read_dir(folder) else {
        return group.iter().map(|path| entry(path)).collect();
    };
    let mut looked = vec![None; group.len()];
    for item in listing {
        // A reading that breaks off (a share gone, a disk taken out): the names not met yet are looked at
        // one by one, not taken for gone (FAR's would keep their `REMOVE_FLAG`).
        let Ok(item) = item else {
            for at in wanted.into_values().flatten() {
                looked[at] = entry(&group[at]);
            }
            return looked;
        };
        let Some(places) = wanted.remove(&fold(&item.file_name().to_string_lossy())) else {
            continue;
        };
        let link = item.file_type().is_ok_and(|kind| kind.is_symlink());
        let metadata = item.metadata().ok().filter(|_| !link);
        for at in places {
            let path = &group[at];
            looked[at] = match &metadata {
                Some(metadata) => Some(made(path, metadata)),
                None => entry(path),
            };
        }
        if wanted.is_empty() {
            break;
        }
    }
    looked
}

/// A name as the system compares names: without case on Windows, each character by itself in upper case
/// (FAR's `upper`; a final sigma is no other letter than a sigma elsewhere).
fn fold(name: &str) -> String {
    match cfg!(windows) {
        true => name.chars().map(upper).collect(),
        false => name.to_string(),
    }
}

/// One character in upper case; one whose upper case is more than one character stays as it is.
fn upper(letter: char) -> char {
    let mut upper = letter.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(one), None) => one,
        _ => letter,
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
