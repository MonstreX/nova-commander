//! A file's or a folder's attributes and times set as an archive keeps them (arclite's `File::set_attr_nt`,
//! `set_time_nt` as it extracts): Windows' `SetFileAttributesW`; the times through a handle opened for its
//! attributes only, a folder's too (`FILE_FLAG_BACKUP_SEMANTICS`).

use std::fs::{FileTimes, OpenOptions};
use std::io;
use std::path::Path;
use std::time::SystemTime;

/// The times to set; `None` leaves that one as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SetTimes {
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub modified: Option<SystemTime>,
}

/// `path`'s attributes set to `attributes` (Windows' `FILE_ATTRIBUTE_*`; 0 as `FILE_ATTRIBUTE_NORMAL`),
/// through a handle opened as the times' (`FILE_BASIC_INFO`, its times 0: left as they are), so a path
/// past `MAX_PATH` takes them as std opens it.
#[cfg(windows)]
pub fn set_attributes(path: &Path, attributes: u32) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_NORMAL, FILE_BASIC_INFO, FileBasicInfo, SetFileInformationByHandle,
    };
    let file = open_for_times(path)?;
    let info = FILE_BASIC_INFO {
        CreationTime: 0,
        LastAccessTime: 0,
        LastWriteTime: 0,
        ChangeTime: 0,
        FileAttributes: if attributes == 0 {
            FILE_ATTRIBUTE_NORMAL
        } else {
            attributes
        },
    };
    let size = u32::try_from(size_of::<FILE_BASIC_INFO>()).unwrap_or(0);
    // SAFETY: the file's own handle, open through the call; the structure is a local of `size` bytes.
    let done = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileBasicInfo,
            (&raw const info).cast(),
            size,
        )
    };
    if done == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// `path`'s read-only bit as `attributes` says (`FILE_ATTRIBUTE_READONLY`): what a Unix file keeps of them.
#[cfg(not(windows))]
pub fn set_attributes(path: &Path, attributes: u32) -> io::Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_readonly(attributes & 1 != 0);
    std::fs::set_permissions(path, permissions)
}

/// Windows' "Archive" attribute taken off `path`, its others kept (WinSCP's `ClearArchive` of an upload's
/// source).
#[cfg(windows)]
pub fn clear_archive(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_ARCHIVE;
    let attributes = std::fs::metadata(path)?.file_attributes();
    match attributes & FILE_ATTRIBUTE_ARCHIVE {
        0 => Ok(()),
        _ => set_attributes(path, attributes & !FILE_ATTRIBUTE_ARCHIVE),
    }
}

/// No file elsewhere has an "Archive" attribute: nothing to take off.
#[cfg(not(windows))]
pub fn clear_archive(path: &Path) -> io::Result<()> {
    std::fs::metadata(path).map(|_| ())
}

/// `path`'s times (a file's or a folder's) set to those given.
pub fn set_times(path: &Path, times: SetTimes) -> io::Result<()> {
    let mut set = FileTimes::new();
    if let Some(modified) = times.modified {
        set = set.set_modified(modified);
    }
    if let Some(accessed) = times.accessed {
        set = set.set_accessed(accessed);
    }
    #[cfg(windows)]
    if let Some(created) = times.created {
        use std::os::windows::fs::FileTimesExt;
        set = set.set_created(created);
    }
    open_for_times(path)?.set_times(set)
}

#[cfg(windows)]
fn open_for_times(path: &Path) -> io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
    };
    OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

#[cfg(not(windows))]
fn open_for_times(path: &Path) -> io::Result<std::fs::File> {
    OpenOptions::new().read(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    /// A file's and a folder's times set as given, one not given kept; the attributes set and cleared.
    #[test]
    fn times_and_attributes_are_set() {
        let temp = std::env::temp_dir().join(format!("nc-sdk-set-times-{}", std::process::id()));
        std::fs::create_dir_all(temp.join("sub")).unwrap();
        let file = temp.join("a.txt");
        std::fs::write(&file, "a").unwrap();
        let moment = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        let times = SetTimes {
            modified: Some(moment),
            created: Some(moment - Duration::from_secs(60)),
            ..SetTimes::default()
        };
        set_times(&file, times).unwrap();
        set_times(&temp.join("sub"), times).unwrap();
        for path in [&file, &temp.join("sub")] {
            let meta = std::fs::metadata(path).unwrap();
            assert_eq!(meta.modified().unwrap(), moment, "{}", path.display());
            #[cfg(windows)]
            assert_eq!(meta.created().unwrap(), moment - Duration::from_secs(60));
        }
        set_attributes(&file, 0x1).unwrap();
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly());
        set_attributes(&file, 0).unwrap();
        assert!(!std::fs::metadata(&file).unwrap().permissions().readonly());
        assert!(set_times(&temp.join("none"), times).is_err());
        std::fs::remove_dir_all(&temp).unwrap();
    }

    /// A file past `MAX_PATH` takes its attributes and times as a short one does.
    #[test]
    fn a_long_path_takes_them_too() {
        let temp = std::env::temp_dir().join(format!("nc-sdk-long-{}", std::process::id()));
        let deep = temp.join("a".repeat(120)).join("b".repeat(120));
        std::fs::create_dir_all(&deep).unwrap();
        let file = deep.join(format!("{}.txt", "c".repeat(60)));
        assert!(file.as_os_str().len() > 260, "past MAX_PATH");
        std::fs::write(&file, "x").unwrap();
        let moment = UNIX_EPOCH + Duration::from_secs(1_500_000_000);
        let times = SetTimes {
            modified: Some(moment),
            ..SetTimes::default()
        };
        set_times(&file, times).unwrap();
        set_attributes(&file, 0x1).unwrap();
        let meta = std::fs::metadata(&file).unwrap();
        assert!(meta.permissions().readonly());
        assert_eq!(meta.modified().unwrap(), moment);
        set_attributes(&file, 0).unwrap();
        std::fs::remove_dir_all(&temp).unwrap();
    }

    /// The access time set alone: the others kept as they were; then the write time alone keeps it.
    #[test]
    fn each_time_is_set_on_its_own() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("t.txt");
        std::fs::write(&file, "t").unwrap();
        let first = UNIX_EPOCH + Duration::from_secs(1_400_000_000);
        let all = SetTimes {
            created: Some(first),
            accessed: Some(first),
            modified: Some(first),
        };
        set_times(&file, all).unwrap();
        let accessed = first + Duration::from_secs(3600);
        let only_accessed = SetTimes {
            accessed: Some(accessed),
            ..SetTimes::default()
        };
        set_times(&file, only_accessed).unwrap();
        let meta = std::fs::metadata(&file).unwrap();
        assert_eq!(meta.accessed().unwrap(), accessed, "set");
        assert_eq!(meta.modified().unwrap(), first, "not given: kept");
        #[cfg(windows)]
        assert_eq!(meta.created().unwrap(), first, "not given: kept");
        let modified = first + Duration::from_secs(7200);
        let only_modified = SetTimes {
            modified: Some(modified),
            ..SetTimes::default()
        };
        set_times(&file, only_modified).unwrap();
        let meta = std::fs::metadata(&file).unwrap();
        assert_eq!(meta.modified().unwrap(), modified);
        assert_eq!(meta.accessed().unwrap(), accessed, "not given: kept");
        set_times(&file, SetTimes::default()).unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().modified().unwrap(),
            modified
        );
    }

    /// The attributes set as given, not only the read-only bit: hidden on, then all of them cleared by 0.
    #[cfg(windows)]
    #[test]
    fn the_attributes_are_set_as_given() {
        use std::os::windows::fs::MetadataExt;
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("h.txt");
        std::fs::write(&file, "h").unwrap();
        set_attributes(&file, 0x2 | 0x1).unwrap();
        let attributes = std::fs::metadata(&file).unwrap().file_attributes();
        assert_eq!(
            attributes & 0x3,
            0x3,
            "hidden and read-only: {attributes:#x}"
        );
        set_attributes(&file, 0x2).unwrap();
        let attributes = std::fs::metadata(&file).unwrap().file_attributes();
        assert_eq!(attributes & 0x3, 0x2, "read-only cleared: {attributes:#x}");
        set_attributes(&file, 0).unwrap();
        let attributes = std::fs::metadata(&file).unwrap().file_attributes();
        assert_eq!(attributes & 0x3, 0, "cleared: {attributes:#x}");
        assert!(set_attributes(&temp.path().join("none"), 0x1).is_err());
    }

    /// The archive attribute taken off, hidden kept; a file without it left; none there an error.
    #[cfg(windows)]
    #[test]
    fn the_archive_attribute_is_cleared_alone() {
        use std::os::windows::fs::MetadataExt;
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("a.txt");
        std::fs::write(&file, "a").unwrap();
        set_attributes(&file, 0x20 | 0x2).unwrap();
        clear_archive(&file).unwrap();
        let attributes = std::fs::metadata(&file).unwrap().file_attributes();
        assert_eq!(attributes & 0x22, 0x2, "{attributes:#x}");
        clear_archive(&file).unwrap();
        assert!(clear_archive(&temp.path().join("none")).is_err());
    }
}
