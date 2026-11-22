//! The files on a disk renamed as FAR's `ProcessName` does: an item, then with "Process subfolders" what is
//! in a folder, every level of it. A folder's content goes first, then the folder (FAR renames the folder
//! first and goes on under its old name, which Windows' names ignoring case still find), with a stack of
//! its own, not the thread's, however deep the tree. A link to a folder is renamed, never gone into (FAR's
//! `GetDirList` follows one: the program never renames outside what was selected). A name never goes over
//! another file (FAR's `MoveFile`), and one that cannot be changed is left as it is. Esc stops it before the
//! next rename.

use std::fs::DirEntry;
use std::path::{Path, PathBuf};

use nc_plugin_sdk::system::rename_no_replace;

use crate::case::{self, Rules};

/// An item to rename: its path, whether a folder, whether it may be gone into (no link), whether its
/// content is on the stack already.
struct Item {
    path: PathBuf,
    folder: bool,
    enters: bool,
    entered: bool,
}

/// How many names changed.
pub fn convert(
    path: &Path,
    folder: bool,
    rules: &Rules,
    subfolders: bool,
    stop: &dyn Fn() -> bool,
) -> usize {
    let link = std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink());
    let mut stack = vec![Item {
        path: path.to_path_buf(),
        folder,
        enters: subfolders && folder && !link,
        entered: false,
    }];
    let mut changed = 0;
    while let Some(item) = stack.pop() {
        if stop() {
            break;
        }
        if item.enters && !item.entered {
            let content = std::fs::read_dir(&item.path)
                .into_iter()
                .flatten()
                .flatten();
            stack.push(Item {
                entered: true,
                ..item
            });
            stack.extend(content.map(|entry| inner(&entry)));
            continue;
        }
        changed += usize::from(rename(&item.path, item.folder, rules));
    }
    changed
}

/// A folder's entry as the system's find data tells it (no file opened): a folder by its attribute (FAR's
/// `FILE_ATTRIBUTE_DIRECTORY`, a link's too), gone into when it is no link.
fn inner(entry: &DirEntry) -> Item {
    let link = entry.file_type().is_ok_and(|kind| kind.is_symlink());
    let folder = entry.metadata().is_ok_and(|meta| is_folder(&meta));
    Item {
        path: entry.path(),
        folder,
        enters: folder && !link,
        entered: false,
    }
}

#[cfg(windows)]
fn is_folder(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    meta.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0
}

#[cfg(not(windows))]
fn is_folder(meta: &std::fs::Metadata) -> bool {
    meta.is_dir()
}

/// `path` renamed in its case; whether it was.
fn rename(path: &Path, folder: bool, rules: &Rules) -> bool {
    let new = (path.file_name().and_then(|name| name.to_str()))
        .and_then(|name| case::new_name(name, folder, rules));
    new.is_some_and(|new| rename_no_replace(path, &path.with_file_name(new)).is_ok())
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
