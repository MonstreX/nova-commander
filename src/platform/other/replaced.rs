//! The stubs the Linux port has replaced (`platform/linux`), still every other system's: a folder's
//! entries as `std` tells them, no change times, identities or watching, no local time.

use std::fs::Metadata;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use super::unsupported;
use crate::platform::files::{Attributes, DirectoryRecord, WatchFlags};
use crate::platform::locale::LocalTime;

/// Hidden is a leading dot on Unix; there is no "system". A link to a folder is not told from one to a
/// file here.
pub fn attributes(path: &Path, metadata: &Metadata) -> Attributes {
    use std::os::unix::fs::MetadataExt;
    let hidden = path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'));
    Attributes::from_mode(metadata.mode(), hidden, metadata.is_dir(), false)
}

pub fn change_time(_path: &Path, _follow: bool) -> Option<SystemTime> {
    None
}

/// No one pass of its own here: the listing reads the folder through `std::fs`.
pub fn read_directory(
    _dir: &Path,
    _cancel: &std::sync::atomic::AtomicBool,
    _found: &mut dyn FnMut(DirectoryRecord),
) -> io::Result<()> {
    unsupported()
}

pub fn file_identity(_path: &Path) -> io::Result<(u32, u64)> {
    unsupported()
}

pub fn open_file_identity(_file: &std::fs::File) -> io::Result<(u32, u64)> {
    unsupported()
}

pub fn name_count(_path: &Path) -> io::Result<u32> {
    unsupported()
}

/// Change notifications come with the port (inotify, FSEvents); until then panels refresh on Ctrl+R.
pub fn watch_dir(_dir: &Path, _flags: std::sync::Arc<WatchFlags>) -> io::Result<()> {
    unsupported()
}

/// Local time comes with the port (the TZ database).
pub fn from_local_time(_local: LocalTime) -> Option<SystemTime> {
    None
}

pub fn local_time(_time: SystemTime) -> Option<LocalTime> {
    None
}
