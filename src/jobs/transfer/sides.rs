//! What the engine asks of either end of an operation, on a disk or behind a provider (ADR-0013 §6).
//! Every name on a disk goes to the OS exactly as the panel shows it: `a ` is not its sibling `a`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::FileFacts;
use crate::platform::files::{exact_path, file_identity};
use crate::vfs::local::elevated::Request as Elevated;
use crate::vfs::{Entry, Removal, VfsPath, provider_for};

/// The path for the OS, when `path` is on a disk.
pub(super) fn disk(path: &VfsPath) -> Option<PathBuf> {
    path.as_local().map(exact_path)
}

/// Both ends on a disk: the OS copies and renames them itself.
pub(super) fn disks(one: &VfsPath, other: &VfsPath) -> Option<(PathBuf, PathBuf)> {
    Some((disk(one)?, disk(other)?))
}

/// One object itself, a link as a link.
pub(super) fn stat(path: &VfsPath) -> io::Result<Entry> {
    provider_for(path)?.stat(path)
}

/// A folder of its own, not a link that looks like one.
pub(super) fn is_real_folder(entry: &Entry) -> bool {
    entry.is_dir && !entry.points_elsewhere()
}

/// What a link on a disk leads to, as if it stood in the link's place.
pub(super) fn followed(path: &VfsPath) -> io::Result<Entry> {
    let on_disk = disk(path).ok_or(io::ErrorKind::Unsupported)?;
    let metadata = fs::metadata(&on_disk)?;
    Ok(Entry {
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        modified: metadata.modified().ok(),
        created: metadata.created().ok(),
        accessed: metadata.accessed().ok(),
        changed: crate::platform::files::change_time(&on_disk, true),
        link: None,
        link_target: None,
        ..stat(path)?
    })
}

/// Which file or folder `path` is on its volume, links followed; `None` behind a provider.
pub(super) fn identity(path: &VfsPath) -> Option<(u32, u64)> {
    file_identity(&disk(path)?).ok()
}

/// Whether `path` is a folder or leads to one.
pub(super) fn leads_to_folder(path: &VfsPath) -> io::Result<bool> {
    match disk(path) {
        Some(path) => fs::metadata(path).map(|metadata| metadata.is_dir()),
        None => stat(path).map(|entry| entry.is_dir),
    }
}

/// The names in a folder; the listing stops once `stop` is set (the task's cancel).
pub(super) fn names(dir: &VfsPath, stop: &AtomicBool) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    provider_for(dir)?.list(dir, stop, &mut |entry| {
        names.push(entry.name);
    })?;
    Ok(names)
}

/// A folder through the helper; `None` off the disk.
pub(super) fn elevated_make_dir(path: &VfsPath) -> Option<Elevated> {
    disk(path).map(Elevated::CreateDirectory)
}

/// A file or a link to one through the helper.
pub(super) fn elevated_remove_file(path: &VfsPath) -> Option<Elevated> {
    disk(path).map(Elevated::DeleteFile)
}

/// An empty folder through the helper.
pub(super) fn elevated_remove_dir(path: &VfsPath) -> Option<Elevated> {
    disk(path).map(Elevated::RemoveDirectory)
}

/// A link through the helper, as [`remove_link`].
pub(super) fn elevated_remove_link(path: &VfsPath) -> Option<Elevated> {
    if stat(path).ok()?.is_dir {
        elevated_remove_dir(path)
    } else {
        elevated_remove_file(path)
    }
}

pub(super) fn make_dir(path: &VfsPath) -> io::Result<()> {
    provider_for(path)?.make_dir(path)
}

/// Removes a file, also a read-only one (a moved file is not lost: its copy exists).
pub(super) fn remove_file(path: &VfsPath) -> io::Result<()> {
    provider_for(path)?.remove(path, Removal::File)
}

pub(super) fn remove_empty_dir(path: &VfsPath) -> io::Result<()> {
    provider_for(path)?.remove(path, Removal::EmptyDir)
}

/// Removes a link itself, never what it points to.
pub(super) fn remove_link(path: &VfsPath) -> io::Result<()> {
    let how = if stat(path)?.is_dir {
        Removal::EmptyDir
    } else {
        Removal::File
    };
    provider_for(path)?.remove(path, how)
}

/// Whether both paths are served by one provider in one place, so one can be renamed to the other.
pub(super) fn same_place(one: &VfsPath, other: &VfsPath) -> bool {
    one.root() == other.root()
}

/// A file's size and last write time as the question shows them (of what a link leads to).
pub(super) fn facts(path: &VfsPath) -> FileFacts {
    let (size, modified) = match disk(path) {
        Some(on_disk) => fs::metadata(on_disk).map_or((0, None), |metadata| {
            (metadata.len(), metadata.modified().ok())
        }),
        None => stat(path).map_or((0, None), |entry| (entry.size, entry.modified)),
    };
    FileFacts {
        path: path.clone(),
        size,
        modified,
    }
}

/// Whether both names lead to the same item, also through a hard link or a junction (so a copy would
/// read what it overwrites).
pub(super) fn is_same(source: &VfsPath, destination: &VfsPath) -> bool {
    match (source.as_local(), destination.as_local()) {
        (Some(_), Some(_)) => {
            disks(source, destination).is_some_and(|(source, target)| same_file(&source, &target))
        }
        (None, None) => source == destination,
        _ => false,
    }
}

fn same_file(source: &Path, destination: &Path) -> bool {
    if let (Ok(source), Ok(destination)) = (file_identity(source), file_identity(destination)) {
        return source == destination;
    }
    match (fs::canonicalize(source), fs::canonicalize(destination)) {
        (Ok(source), Ok(destination)) => source == destination,
        _ => false,
    }
}

/// Whether `destination` lies inside the folder `source` (a link is copied as a link, so it may go
/// into the folder it points to).
pub(super) fn is_inside(source: &VfsPath, destination: &VfsPath) -> bool {
    stat(source).is_ok_and(|entry| is_real_folder(&entry)) && leads_inside(source, destination)
}

/// Whether `destination` lies inside the folder `source` is or leads to.
pub(super) fn leads_inside(source: &VfsPath, destination: &VfsPath) -> bool {
    let Some(parent) = destination.parent() else {
        return false;
    };
    match disks(source, &parent) {
        Some((source, parent)) => match (fs::canonicalize(source), fs::canonicalize(parent)) {
            (Ok(source), Ok(parent)) => parent.starts_with(source),
            _ => false,
        },
        None => std::iter::successors(Some(parent), VfsPath::parent).any(|dir| dir == *source),
    }
}

/// `.<name>.nc-tmp` next to `path` (`.<name>.<n>.nc-tmp` when that is taken).
/// Whether `path` is one of [`temp_sibling`]'s names.
pub(super) fn is_temporary(path: &VfsPath) -> bool {
    path.file_name()
        .is_some_and(|name| name.starts_with('.') && name.ends_with(".nc-tmp"))
}

pub(super) fn temp_sibling(path: &VfsPath) -> VfsPath {
    let name = path.file_name().unwrap_or_default();
    let beside = |sibling: String| sibling_of(path, &sibling);
    // A name the suffix would take past a name's 255 characters goes without it (R7 review C10: such a
    // file could not be overwritten).
    let long = name.encode_utf16().count() + ".nc-tmp".len() + 12 > 255;
    let name = if long { String::new() } else { name };
    (0u32..)
        .map(|attempt| match attempt {
            0 => beside(format!(".{name}.nc-tmp")),
            n => beside(format!(".{name}.{n}.nc-tmp")),
        })
        .find(|candidate| stat(candidate).is_err())
        .unwrap_or_else(|| beside(format!(".{name}.nc-tmp")))
}

/// FAR's Rename: `name (2).ext`, `name (3).ext`, ... the first that is free.
pub(super) fn free_name(path: &VfsPath) -> VfsPath {
    let name = path.file_name().unwrap_or_default();
    let name = Path::new(&name);
    let stem = name
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let extension = name
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()));
    (2u32..)
        .map(|n| {
            let extension = extension.as_deref().unwrap_or("");
            sibling_of(path, &format!("{stem} ({n}){extension}"))
        })
        .find(|candidate| stat(candidate).is_err())
        .unwrap_or_else(|| path.clone())
}

fn sibling_of(path: &VfsPath, name: &str) -> VfsPath {
    path.parent().unwrap_or_else(|| path.clone()).join(name)
}
