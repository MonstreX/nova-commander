//! The files' stubs: what `std` does the same everywhere is done, the rest is unsupported.

use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

use super::unsupported;
use crate::platform::files::{Attributes, DirectoryRecord, WatchFlags};

/// Unix file systems tell names apart by case (macOS's usually do not; the port decides per volume).
pub const NAMES_DIFFER_BY_CASE: bool = true;

/// Hidden is a leading dot on Unix; there is no "system".
pub fn attributes(path: &Path, metadata: &Metadata) -> Attributes {
    let hidden = path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'));
    Attributes::from_flags(hidden, metadata.is_dir(), metadata.is_symlink())
}

pub fn compare_names(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}

pub fn compare_wide_names(a: &[u16], b: &[u16]) -> std::cmp::Ordering {
    String::from_utf16_lossy(a).cmp(&String::from_utf16_lossy(b))
}

pub fn copy_file(
    _source: &Path,
    _destination: &Path,
    _report: &mut dyn FnMut(u64, u64) -> bool,
) -> io::Result<()> {
    unsupported()
}

pub fn open_past_cache(_path: &Path) -> io::Result<std::fs::File> {
    unsupported()
}

pub fn read_security(_path: &Path) -> io::Result<crate::platform::files::SecurityDescriptor> {
    unsupported()
}

pub fn change_time(_path: &Path, _follow: bool) -> Option<std::time::SystemTime> {
    None
}

pub fn reset_security(_path: &Path) -> io::Result<()> {
    unsupported()
}

pub fn apply_kept(
    _path: &Path,
    _security: Option<&crate::platform::files::SecurityDescriptor>,
    _times: Option<&crate::platform::files::FileTimes>,
) -> crate::platform::files::Kept {
    crate::platform::files::Kept {
        rights: unsupported(),
        times: unsupported(),
    }
}

/// The marks are Windows' own: nothing to add here.
pub fn add_marks(_path: &Path, _attributes: crate::platform::files::Attributes) -> io::Result<()> {
    Ok(())
}

pub fn copy_folder_attributes(_source: &Path, _destination: &Path) -> io::Result<()> {
    unsupported()
}

pub fn copy_link(_source: &Path, _destination: &Path) -> io::Result<()> {
    unsupported()
}

/// No one pass of its own here: the listing reads the folder through `std::fs`.
pub fn read_directory(
    _dir: &Path,
    _cancel: &std::sync::atomic::AtomicBool,
    _found: &mut dyn FnMut(DirectoryRecord),
) -> io::Result<()> {
    unsupported()
}

/// Whether `name` is one name in a folder.
pub fn is_plain_name(name: &str) -> bool {
    !matches!(name, "" | "." | "..") && !name.contains(['/', '\0'])
}

/// Unix takes names as they are.
pub fn exact_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// Unix links are told by `std` (`symlink_metadata`); there are no reparse tags.
pub fn reparse_tag(_path: &Path) -> Option<u32> {
    None
}

/// A folder description file is looked for by the system's wildcards: none on other systems yet.
pub fn first_file_like(_folder: &Path, _pattern: &str) -> Option<std::ffi::OsString> {
    None
}

pub fn file_identity(_path: &Path) -> io::Result<(u32, u64)> {
    unsupported()
}

pub fn open_file_identity(_file: &std::fs::File) -> io::Result<(u32, u64)> {
    unsupported()
}

pub fn link_target(link: &Path) -> io::Result<PathBuf> {
    std::fs::read_link(link)
}

pub fn make_link(
    _kind: crate::platform::files::NewLink,
    _target: &Path,
    _link: &Path,
) -> io::Result<()> {
    unsupported()
}

pub fn move_file(_source: &Path, _target: &Path, _replace: bool) -> io::Result<()> {
    unsupported()
}

/// The freedesktop Trash and the Finder's Trash come with the port.
pub fn move_to_trash(_path: &Path) -> io::Result<()> {
    unsupported()
}

/// No shell to ask here.
pub fn can_recycle(_object: &Path) -> Option<bool> {
    None
}

/// The port's own replacement comes with it; saving falls back to a rename.
pub fn replace_file(
    _original: &Path,
    _replacement: &Path,
) -> Result<(), crate::platform::files::Replaced> {
    Err(crate::platform::files::Replaced::Untouched)
}

pub fn name_count(_path: &Path) -> io::Result<u32> {
    unsupported()
}

/// Change notifications come with the port (inotify, FSEvents); until then panels refresh on Ctrl+R.
pub fn watch_dir(_dir: &Path, _flags: std::sync::Arc<WatchFlags>) -> io::Result<()> {
    unsupported()
}

pub fn may_be_held(_error: &io::Error) -> bool {
    false
}

pub fn file_holders(_paths: &[&Path]) -> crate::platform::files::Holders {
    crate::platform::files::Holders::default()
}

pub fn set_read_only(_path: &Path, _read_only: bool) -> io::Result<()> {
    unsupported()
}

pub fn alternate_streams(_path: &Path) -> Vec<(String, u64)> {
    Vec::new()
}
