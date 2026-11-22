//! What writing an archive anew leaves to do (arclite's `Archive::update` around 7-Zip's `UpdateItems`,
//! [`crate::update_library`]): a new name `{GUID}.tmp` beside the archive, the new file put in the old
//! one's place (`MoveFileEx`), a source opened as arclite opens it, the sources moved deleted
//! (`DeleteSrcFiles`).

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use nc_plugin_sdk::system::{SetTimes, now_ticks, set_times};

use crate::extract::{Abort, Fault};
use crate::opened::Opened;
use crate::update::{Faults, Plan};
use crate::update_options::UpdateOptions;

/// Why the writing ended before its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// The user stopped it.
    Abort,
    /// Writing the archive failed.
    Failed(Fault),
}

impl From<Abort> for Stop {
    fn from(_: Abort) -> Self {
        Self::Abort
    }
}

/// arclite's `get_temp_file_name`: a new name `{GUID}.tmp` in the archive's folder.
pub fn temp_name(archive: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let ticks = now_ticks().cast_unsigned();
    let more = u64::from(std::process::id()) << 32 | NEXT.fetch_add(1, Ordering::Relaxed);
    let name = format!(
        "{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}.tmp",
        ticks >> 32,
        (ticks >> 16) & 0xFFFF,
        ticks & 0xFFFF,
        (more >> 48) & 0xFFFF,
        more & 0xFFFF_FFFF_FFFF
    );
    archive.parent().unwrap_or(Path::new("")).join(name)
}

/// The update written into a new file beside the archive through 7-Zip's library: its path; nothing of
/// it left when it stops.
pub fn write(
    opened: &Opened,
    plan: &Plan,
    options: &UpdateOptions,
    faults: &mut Faults<'_>,
) -> Result<PathBuf, Stop> {
    crate::update_library::write(opened, plan, options, faults)
}

/// The source opened for reading: with others writing to it only when asked (arclite's `open_shared`:
/// `FILE_SHARE_WRITE`), as 7-Zip's `-ssw`.
pub(crate) fn open_source(path: &Path, shared: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // `FILE_SHARE_READ`, with `FILE_SHARE_WRITE` and `FILE_SHARE_DELETE` when shared.
        options.share_mode(if shared { 0x7 } else { 0x1 });
    }
    #[cfg(not(windows))]
    let _ = shared;
    options.open(path)
}

/// The new archive put in the old one's place (arclite's `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`),
/// keeping the old one's creation time (its `copy_ctime_from`).
pub fn replace(temp: &Path, archive: &Path) -> io::Result<()> {
    let created = std::fs::metadata(archive)
        .and_then(|meta| meta.created())
        .ok();
    std::fs::rename(temp, archive)?;
    if created.is_some() {
        let times = SetTimes {
            created,
            accessed: None,
            modified: None,
        };
        let _ = set_times(archive, times);
    }
    Ok(())
}

/// The sources deleted after they went in (arclite's `DeleteSrcFiles`): a folder with what is in it,
/// a read-only one made writable first; each failure asked about.
pub fn delete_sources(sources: &[PathBuf], faults: &mut Faults<'_>) -> Result<(), Abort> {
    for source in sources {
        delete_source(source, faults)?;
    }
    Ok(())
}

fn delete_source(path: &Path, faults: &mut Faults<'_>) -> Result<(), Abort> {
    let is_dir = std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir());
    if is_dir {
        let listed = std::fs::read_dir(path).map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .collect::<Vec<_>>()
        });
        for child in listed.unwrap_or_default() {
            delete_source(&child, faults)?;
        }
    }
    faults.attempt(|| {
        let remove = || {
            if is_dir {
                std::fs::remove_dir(path)
            } else {
                std::fs::remove_file(path)
            }
        };
        remove()
            .or_else(|error| {
                if error.kind() != io::ErrorKind::PermissionDenied {
                    return Err(error);
                }
                nc_plugin_sdk::system::set_attributes(path, 0)?;
                remove()
            })
            .map_err(|error| Fault::system(&error, path))
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "update_write_tests.rs"]
mod tests;
