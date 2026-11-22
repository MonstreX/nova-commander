//! A Linux folder's entries as the panels show them (S7.1a): the mode in far2l's attribute column, a
//! link to a folder taken as the folder (as FAR's listing does), the change time, and a file's identity
//! and number of names, all from `stat` (`std`'s, which asks `statx` for the birth time).

use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::platform::files::{Attributes, DirectoryRecord, LinkKind};

/// Hidden is a leading dot; a link is a folder when what it leads to is one, broken when that is gone
/// (far2l's `B`).
pub fn attributes(path: &Path, metadata: &Metadata) -> Attributes {
    let hidden = path
        .file_name()
        .is_some_and(|name| name.as_encoded_bytes().starts_with(b"."));
    let target = metadata.is_symlink().then(|| fs::metadata(path));
    let directory = metadata.is_dir()
        || target
            .as_ref()
            .is_some_and(|target| target.as_ref().is_ok_and(Metadata::is_dir));
    let broken = target.is_some_and(|target| target.is_err());
    Attributes::from_mode(metadata.mode(), hidden, directory, broken)
}

/// One pass over the folder: each entry's own `stat` (a link's, not its target's), as FAR's listing
/// tells a link from what it leads to.
pub fn read_directory(
    dir: &Path,
    cancel: &AtomicBool,
    found: &mut dyn FnMut(DirectoryRecord),
) -> io::Result<()> {
    for item in fs::read_dir(dir)? {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let item = item?;
        let record = match item.metadata() {
            Ok(metadata) => record_of(&item.path(), &metadata),
            Err(_) => DirectoryRecord::default(),
        };
        found(DirectoryRecord {
            name: item.file_name(),
            ..record
        });
    }
    Ok(())
}

/// What `stat` tells of an entry; its name is the caller's.
fn record_of(path: &Path, metadata: &Metadata) -> DirectoryRecord {
    let attributes = attributes(path, metadata);
    DirectoryRecord {
        attributes,
        size: metadata.len(),
        // `st_blocks` counts 512-byte units whatever the file system's block (stat(2)).
        allocated: Some(metadata.blocks().saturating_mul(512)),
        created: metadata.created().ok(),
        accessed: metadata.accessed().ok(),
        modified: metadata.modified().ok(),
        changed: changed(metadata),
        link: attributes.is_link().then_some(LinkKind::Symlink),
        ..DirectoryRecord::default()
    }
}

/// The inode's last change (`st_ctime`): its data, its mode, its owner or its names.
fn changed(metadata: &Metadata) -> Option<SystemTime> {
    let nanos = u32::try_from(metadata.ctime_nsec()).ok()?;
    match u64::try_from(metadata.ctime()) {
        Ok(after) => UNIX_EPOCH.checked_add(Duration::new(after, nanos)),
        Err(_) => {
            let before = Duration::from_secs(metadata.ctime().unsigned_abs());
            UNIX_EPOCH
                .checked_sub(before)?
                .checked_add(Duration::from_nanos(u64::from(nanos)))
        }
    }
}

/// The last change of the data or the attributes of `path` (a link's own with `follow` false).
pub fn change_time(path: &Path, follow: bool) -> Option<SystemTime> {
    let metadata = if follow {
        fs::metadata(path)
    } else {
        fs::symlink_metadata(path)
    };
    changed(&metadata.ok()?)
}

/// The device and inode of what `path` leads to (links followed): two names with the same identity are
/// one file (hard links, a link to a folder).
pub fn file_identity(path: &Path) -> io::Result<(u32, u64)> {
    Ok(identity(&fs::metadata(path)?))
}

/// The device and inode of a file open: the very file read, whatever is at its path now.
pub fn open_file_identity(file: &fs::File) -> io::Result<(u32, u64)> {
    Ok(identity(&file.metadata()?))
}

/// The device number folded to Windows' 32-bit volume serial the interface has: both halves kept, so
/// a device with a major number past 4095 still differs from its neighbours.
fn identity(metadata: &Metadata) -> (u32, u64) {
    let device = metadata.dev();
    let low = u32::try_from(device & u64::from(u32::MAX)).unwrap_or(0);
    let high = u32::try_from(device >> 32).unwrap_or(0);
    (low ^ high, metadata.ino())
}

/// How many names (hard links) the file at `path` has.
pub fn name_count(path: &Path) -> io::Result<u32> {
    let links = fs::metadata(path)?.nlink();
    Ok(u32::try_from(links).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn listed(dir: &Path) -> Vec<DirectoryRecord> {
        let mut records = Vec::new();
        read_directory(dir, &AtomicBool::new(false), &mut |record| {
            records.push(record)
        })
        .unwrap();
        records.sort_by(|a, b| a.name.cmp(&b.name));
        records
    }

    #[test]
    fn links_are_told_from_what_they_lead_to() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("file"), b"12345").unwrap();
        symlink("folder", root.join("to folder")).unwrap();
        symlink("file", root.join("to file")).unwrap();
        symlink("gone", root.join("to nothing")).unwrap();
        let records = listed(root);
        let shown: Vec<(&str, bool, bool, Option<LinkKind>, char)> = (records.iter())
            .map(|record| {
                let attributes = record.attributes;
                let kind = attributes.letters(10).chars().next().unwrap();
                let name = record.name.to_str().unwrap();
                (
                    name,
                    attributes.is_directory(),
                    attributes.is_link(),
                    record.link,
                    kind,
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("file", false, false, None, '-'),
                ("folder", true, false, None, 'd'),
                ("to file", false, true, Some(LinkKind::Symlink), 'l'),
                ("to folder", true, true, Some(LinkKind::Symlink), 'l'),
                ("to nothing", false, true, Some(LinkKind::Symlink), 'B'),
            ]
        );
        assert_eq!(records[0].size, 5);
        assert!(records.iter().all(|record| record.changed.is_some()));
    }

    #[test]
    fn the_mode_is_shown_as_ls_and_far2l_show_it() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join(".hidden");
        fs::write(&file, b"").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o4751)).unwrap();
        let attributes = attributes(&file, &fs::symlink_metadata(&file).unwrap());
        assert_eq!(attributes.letters(10), "-rwsr-x--x");
        assert_eq!(attributes.letters(6), "-rwsr-");
        assert!(attributes.is_hidden() && !attributes.is_directory());
    }

    #[test]
    fn hard_links_are_one_file_with_two_names() {
        let temp = tempfile::tempdir().unwrap();
        let (first, second) = (temp.path().join("a"), temp.path().join("b"));
        fs::write(&first, b"x").unwrap();
        fs::hard_link(&first, &second).unwrap();
        assert_eq!(name_count(&first).unwrap(), 2);
        assert_eq!(
            file_identity(&first).unwrap(),
            file_identity(&second).unwrap()
        );
        let open = fs::File::open(&second).unwrap();
        assert_eq!(
            open_file_identity(&open).unwrap(),
            file_identity(&first).unwrap()
        );
        fs::write(temp.path().join("c"), b"x").unwrap();
        assert_ne!(
            file_identity(&temp.path().join("c")).unwrap(),
            file_identity(&first).unwrap()
        );
    }

    #[test]
    fn the_change_time_is_the_link_s_own_unless_followed() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file");
        fs::write(&file, b"x").unwrap();
        let link = temp.path().join("link");
        symlink(&file, &link).unwrap();
        let own = change_time(&link, false).unwrap();
        assert_eq!(change_time(&link, true), change_time(&file, false));
        assert!(own <= SystemTime::now());
        assert_eq!(change_time(&temp.path().join("gone"), false), None);
    }
}
