//! The built-in `local` provider: disks, shares and mount points as the OS sees them (ADR-0013).

mod drives;
pub mod elevated;
pub mod elevated_session;
mod info;
mod listing;
pub mod ops;

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub use drives::{Drive, DriveColumns, DriveType, can_be_virtual, drive_name, list_drives};
pub use info::{DiskInfo, Info, InfoOptions, read_info};

use super::{Capabilities, Entry, Provider, RandomRead, Removal, VfsPath, Watch, WriteMode};
use crate::platform;
use crate::platform::files::{Attributes, DirWatch, FileTimes};

pub struct LocalProvider;

/// A local file read anywhere in it.
struct Random {
    file: File,
    path: PathBuf,
    /// The file's identity when opened (none where the system does not tell).
    identity: Option<(u32, u64)>,
}

impl RandomRead for Random {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        let mut filled = 0;
        while filled < buf.len() {
            match self.file.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        Ok(filled)
    }

    fn len(&mut self) -> io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }

    fn replaced(&mut self) -> bool {
        // A path with nothing at it is no other file: the one open stays.
        let now = platform::files::file_identity(&self.path).ok();
        self.identity.is_some() && now.is_some() && now != self.identity
    }
}

impl Provider for LocalProvider {
    fn id(&self) -> &str {
        "local"
    }

    fn list(
        &self,
        dir: &VfsPath,
        cancel: &AtomicBool,
        found: &mut dyn FnMut(Entry),
    ) -> io::Result<()> {
        let path = local_path(dir)?;
        // The folder exactly: `d ` is not its sibling `d`.
        listing::read_dir(&platform::files::exact_path(path), cancel, found)
    }

    /// A Recycle Bin where the shell keeps one: a local fixed drive whose bin is not turned off.
    fn capabilities(&self, path: &VfsPath) -> Capabilities {
        let trash = local_path(path).is_ok_and(|path| {
            let root = platform::volumes::volume_of(path);
            platform::volumes::has_recycle_bin(&root.to_string_lossy())
        });
        Capabilities { trash }
    }

    fn stat(&self, path: &VfsPath) -> io::Result<Entry> {
        listing::stat(&exact(path)?)
    }

    fn open_read(&self, path: &VfsPath) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(File::open(exact(path)?)?))
    }

    fn open_random(&self, path: &VfsPath) -> io::Result<Box<dyn RandomRead>> {
        // The standard library opens it sharing reading, writing and deleting: a log stays writable.
        let path = exact(path)?;
        let file = File::open(&path)?;
        // The file open, not what the path leads to a moment later.
        let identity = platform::files::open_file_identity(&file).ok();
        Ok(Box::new(Random {
            file,
            path,
            identity,
        }))
    }

    fn open_write(&self, path: &VfsPath, mode: WriteMode) -> io::Result<Box<dyn Write + Send>> {
        let mut options = OpenOptions::new();
        match mode {
            WriteMode::New => options.write(true).create_new(true),
            WriteMode::Append => options.append(true),
        };
        Ok(Box::new(options.open(exact(path)?)?))
    }

    fn make_dir(&self, path: &VfsPath) -> io::Result<()> {
        std::fs::create_dir(exact(path)?)
    }

    fn rename(&self, from: &VfsPath, to: &VfsPath, replace: bool) -> io::Result<()> {
        platform::files::move_file(&exact(from)?, &exact(to)?, replace)
    }

    fn remove(&self, path: &VfsPath, how: Removal) -> io::Result<()> {
        ops::remove(local_path(path)?, how)
    }

    fn set_times(
        &self,
        path: &VfsPath,
        times: &FileTimes,
        attributes: Option<Attributes>,
    ) -> io::Result<()> {
        let exact = exact(path)?;
        // Both, whichever fails: the times' failure told first.
        let times = platform::files::apply_kept(&exact, None, Some(times)).times;
        let marks = match attributes {
            Some(attributes) => platform::files::add_marks(&exact, attributes),
            None => Ok(()),
        };
        times.and(marks)
    }

    fn watch(&self, dir: &VfsPath) -> io::Result<Box<dyn Watch>> {
        let path = local_path(dir)?;
        let path = platform::files::exact_path(path);
        Ok(Box::new(platform::files::watch_dir(&path)?))
    }
}

impl Watch for platform::files::DirWatch {
    fn take_change(&self) -> bool {
        DirWatch::take_change(self)
    }
}

/// FAR's `os::com::can_recycle`: whether the shell would put `object` into the Recycle Bin (a file too big
/// for it, a drive without one: no); `None` when it could not be asked.
pub fn can_recycle(object: &Path) -> Option<bool> {
    platform::files::can_recycle(object)
}

/// Whether the Recycle Bin of `object`'s volume takes an object of `size` bytes: the shell deletes a
/// bigger one for good with no question, though `can_recycle` says yes (review R1); one not known does.
pub fn bin_takes(object: &Path, size: u64) -> bool {
    let root = platform::volumes::volume_of(object);
    platform::volumes::recycle_bin_capacity(&root.to_string_lossy())
        .is_none_or(|capacity| size <= capacity)
}

/// Whether `path` is on a network drive or share (FAR's "Network drives autorefresh" asks).
pub fn is_remote(path: &Path) -> bool {
    let root = platform::volumes::volume_of(path);
    platform::volumes::drive_kind(&root.to_string_lossy()) == platform::volumes::DriveKind::Remote
}

/// The share whose root `root` is: `\\server\share` (no slash at its end); none for another folder
/// (FAR's `NetRoot`).
pub fn share_of(root: &Path) -> Option<String> {
    use std::path::{Component, Prefix};
    let mut components = root.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return None;
    };
    if components.any(|component| !matches!(component, Component::RootDir)) {
        return None;
    }
    match prefix.kind() {
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => Some(format!(
            r"\\{}\{}",
            server.to_string_lossy(),
            share.to_string_lossy()
        )),
        _ => None,
    }
}

/// The name exactly as the panel shows it: `a ` is not its sibling `a`.
fn exact(path: &VfsPath) -> io::Result<PathBuf> {
    Ok(platform::files::exact_path(local_path(path)?))
}

fn local_path(dir: &VfsPath) -> io::Result<&Path> {
    dir.as_local()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{dir} is not local")))
}

#[cfg(test)]
mod tests;
