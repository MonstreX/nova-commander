//! Reading what Ctrl+A shows of a file and setting it (FAR's `far/setattr.cpp` and `far/fileattr.cpp`):
//! the attributes and the four times from one handle, the owner, the link and its target, the other
//! names of a file with hard links. Compression, encryption and sparseness are set as FAR sets them:
//! read-only and system taken off for the call (they block the change) and put back.

use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::path::Path;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, INVALID_HANDLE_VALUE, MAX_PATH};
use windows_sys::Win32::Storage::FileSystem::{
    DecryptFileW, EncryptFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_ENCRYPTED,
    FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM, FILE_ATTRIBUTE_TEMPORARY, FILE_BASIC_INFO,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
    FILE_STANDARD_INFO, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FileBasicInfo, FileStandardInfo,
    FindClose, FindFirstFileNameW, FindNextFileNameW, GetFileAttributesW,
    GetFileInformationByHandleEx, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
    INVALID_FILE_ATTRIBUTES, SetFileAttributesW,
};
use windows_sys::Win32::System::Ioctl::{
    FSCTL_DELETE_REPARSE_POINT, FSCTL_SET_COMPRESSION, FSCTL_SET_SPARSE,
};

use super::device::control;
use super::locale::from_file_time;
use super::{link_target, owner, reparse_tag, reparse_text, wide_path};
use crate::platform::files::{Attributes, FileProperties, FileTimes, Flag, LinkFacts, LinkKind};

/// `COMPRESSION_FORMAT_DEFAULT` and `COMPRESSION_FORMAT_NONE`.
const COMPRESSION_DEFAULT: u16 = 1;
const COMPRESSION_NONE: u16 = 0;

/// What the "Attributes" dialog shows of `path` (a link's own, not what it leads to).
pub fn file_properties(path: &Path) -> io::Result<FileProperties> {
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let basic: FILE_BASIC_INFO = information(&file, FileBasicInfo)?;
    let standard: FILE_STANDARD_INFO = information(&file, FileStandardInfo)?;
    drop(file);
    let attributes = Attributes::from_windows_bits(basic.FileAttributes);
    let names = if standard.NumberOfLinks > 1 && !attributes.is_directory() {
        names_of(path)
    } else {
        Vec::new()
    };
    Ok(FileProperties {
        attributes,
        times: FileTimes {
            created: from_file_time(basic.CreationTime),
            accessed: from_file_time(basic.LastAccessTime),
            modified: from_file_time(basic.LastWriteTime),
            changed: from_file_time(basic.ChangeTime),
        },
        owner: owner::owner(path),
        link: link_facts(path, attributes),
        names,
        name_count: standard.NumberOfLinks,
    })
}

/// `GetFileInformationByHandleEx` for one class of plain data.
fn information<T: Default>(file: &std::fs::File, class: i32) -> io::Result<T> {
    let mut info = T::default();
    let size = u32::try_from(size_of::<T>()).unwrap_or(0);
    // SAFETY: the handle is open; `info` is a valid out buffer of `size` bytes of the class's type.
    let read = unsafe {
        GetFileInformationByHandleEx(file.as_raw_handle(), class, (&raw mut info).cast(), size)
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

/// A link, a volume mounted on a folder or a drive's root (FAR's `IsMountPoint`), or another reparse
/// point.
fn link_facts(path: &Path, attributes: Attributes) -> Option<LinkFacts> {
    if attributes.is_directory()
        && let Some(volume) = mounted_volume(path)
    {
        return Some(LinkFacts {
            kind: LinkKind::Junction,
            target: volume,
            volume: true,
            known: true,
            editable: false,
        });
    }
    if !attributes.is_link() {
        return None;
    }
    let kind = LinkKind::from_reparse_tag(reparse_tag(path)?);
    let (target, known, editable) = match (reparse_text(path), kind) {
        (Some(text), _) => (text.target, true, text.editable),
        // A link whose buffer could not be read (no rights to open it): what the system resolves.
        (None, LinkKind::Symlink | LinkKind::Junction) => {
            let target = link_target(path).map(|target| target.display().to_string());
            (target.unwrap_or_default(), true, false)
        }
        (None, LinkKind::Reparse(_)) => (String::new(), false, false),
    };
    let volume = kind == LinkKind::Junction && is_volume(&target);
    Some(LinkFacts {
        kind,
        target,
        volume,
        known,
        editable: editable && !volume,
    })
}

/// Whether a junction leads to a volume (FAR's `Volume:`): its name as Windows gives it (`\\?\Volume{`)
/// or as the buffer keeps it (`\??\Volume{`, FAR's `ParsePath` takes both).
fn is_volume(target: &str) -> bool {
    [r"\\?\Volume{", r"\??\Volume{"]
        .iter()
        .any(|prefix| target.starts_with(prefix))
}

/// The volume mounted at a folder or a drive's root (`\\?\Volume{...}\`).
fn mounted_volume(path: &Path) -> Option<String> {
    let mut text = path.as_os_str().to_os_string();
    if !text.to_string_lossy().ends_with('\\') {
        text.push("\\");
    }
    let name = wide_path(Path::new(&text));
    let mut volume = [0u16; 64];
    let length = u32::try_from(volume.len()).unwrap_or(0);
    // SAFETY: `name` is NUL-terminated; `volume` holds `length` units.
    let found =
        unsafe { GetVolumeNameForVolumeMountPointW(name.as_ptr(), volume.as_mut_ptr(), length) };
    (found != 0).then(|| until_nul(&volume))
}

/// Every name of a file with hard links, full paths (FAR's `enum_names` with the volume's root).
fn names_of(path: &Path) -> Vec<String> {
    let Some(root) = volume_path(path) else {
        return Vec::new();
    };
    let name = wide_path(path);
    let mut buffer = vec![0u16; usize::try_from(MAX_PATH).unwrap_or(260)];
    // SAFETY: `name` is NUL-terminated; each try gives the buffer's own length.
    let find = with_room(&mut buffer, |buffer, length| unsafe {
        let find = FindFirstFileNameW(name.as_ptr(), 0, length, buffer.as_mut_ptr());
        (find != INVALID_HANDLE_VALUE).then_some(find)
    });
    let Some(find) = find else {
        return Vec::new();
    };
    let mut names = vec![format!("{root}{}", until_nul(&buffer))];
    // SAFETY: `find` is open; each try gives the buffer's own length.
    while with_room(&mut buffer, |buffer, length| unsafe {
        (FindNextFileNameW(find, length, buffer.as_mut_ptr()) != 0).then_some(())
    })
    .is_some()
    {
        names.push(format!("{root}{}", until_nul(&buffer)));
    }
    // SAFETY: `find` came from `FindFirstFileNameW` and is closed once, here.
    unsafe { FindClose(find) };
    names
}

/// A call that fills `buffer`, tried again with the length it asks for when the name does not fit
/// (`ERROR_MORE_DATA`).
fn with_room<T>(
    buffer: &mut Vec<u16>,
    mut call: impl FnMut(&mut Vec<u16>, &mut u32) -> Option<T>,
) -> Option<T> {
    loop {
        let mut length = u32::try_from(buffer.len()).unwrap_or(0);
        if let Some(done) = call(buffer, &mut length) {
            return Some(done);
        }
        let more = io::Error::last_os_error().raw_os_error() == Some(ERROR_MORE_DATA as i32);
        let needed = usize::try_from(length).unwrap_or(0);
        if !more || needed <= buffer.len() {
            return None;
        }
        buffer.resize(needed, 0);
    }
}

/// The folder the file's volume is mounted at, without its last separator, as FAR's `GetPathRoot`:
/// `C:`, `C:\mnt\volume`, `\\server\share`.
fn volume_path(path: &Path) -> Option<String> {
    let name = wide_path(path);
    let mut root = vec![0u16; usize::try_from(MAX_PATH).unwrap_or(260) + name.len()];
    let length = u32::try_from(root.len()).unwrap_or(0);
    // SAFETY: `name` is NUL-terminated; `root` holds `length` units.
    if unsafe { GetVolumePathNameW(name.as_ptr(), root.as_mut_ptr(), length) } == 0 {
        return None;
    }
    let root = until_nul(&root);
    let plain = match root.strip_prefix(r"\\?\UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => root.strip_prefix(r"\\?\").unwrap_or(&root).to_string(),
    };
    Some(plain.trim_end_matches('\\').to_string())
}

fn until_nul(units: &[u16]) -> String {
    let end = units
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

fn current_bits(name: &[u16]) -> io::Result<u32> {
    // SAFETY: `name` is NUL-terminated.
    let bits = unsafe { GetFileAttributesW(name.as_ptr()) };
    if bits == INVALID_FILE_ATTRIBUTES {
        return Err(io::Error::last_os_error());
    }
    Ok(bits)
}

fn set_bits(name: &[u16], bits: u32) -> io::Result<()> {
    // SAFETY: `name` is NUL-terminated.
    if unsafe { SetFileAttributesW(name.as_ptr(), bits) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The plain attributes (FAR's `ESetFileAttributes`): compression, encryption, sparseness and a
/// reparse point have their own calls; a folder cannot be temporary.
pub fn set_attributes(path: &Path, attributes: Attributes) -> io::Result<()> {
    let own = [
        Flag::Compressed,
        Flag::Encrypted,
        Flag::Sparse,
        Flag::ReparsePoint,
    ];
    let plain = own
        .iter()
        .fold(attributes, |plain, flag| plain.with(*flag, false));
    let mut bits = plain.windows_bits();
    if bits & FILE_ATTRIBUTE_DIRECTORY != 0 {
        bits &= !FILE_ATTRIBUTE_TEMPORARY;
    }
    set_bits(&wide_path(path), bits & !FILE_ATTRIBUTE_DIRECTORY)
}

/// FAR's `without_ro`: read-only and system block compression and encryption; they are taken off for
/// `action` and put back after it.
pub(super) fn without_read_only(
    path: &Path,
    action: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let name = wide_path(path);
    let bits = current_bits(&name)?;
    let blocking = FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_SYSTEM;
    if bits & blocking == 0 {
        return action();
    }
    set_bits(&name, bits & !blocking & !FILE_ATTRIBUTE_DIRECTORY)?;
    let done = action();
    let _ = set_bits(&name, bits & !FILE_ATTRIBUTE_DIRECTORY);
    done
}

fn open_for_data(path: &Path, access: u32) -> io::Result<OwnedHandle> {
    let file = std::fs::OpenOptions::new()
        .access_mode(access)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    Ok(OwnedHandle::from(file))
}

/// NTFS compression on or off (FAR's `ESetFileCompression`); an encrypted file is decrypted first.
pub fn set_compressed(path: &Path, on: bool) -> io::Result<()> {
    without_read_only(path, || {
        let encrypted = current_bits(&wide_path(path))? & FILE_ATTRIBUTE_ENCRYPTED != 0;
        if on && encrypted {
            set_encryption(path, false)?;
        }
        let state = if on {
            COMPRESSION_DEFAULT
        } else {
            COMPRESSION_NONE
        };
        let file = open_for_data(path, FILE_READ_DATA | FILE_WRITE_DATA)?;
        control(&file, FSCTL_SET_COMPRESSION, &state.to_le_bytes(), &mut [])
    })
}

/// EFS encryption on or off (FAR's `ESetFileEncryption`).
pub fn set_encrypted(path: &Path, on: bool) -> io::Result<()> {
    without_read_only(path, || set_encryption(path, on))
}

fn set_encryption(path: &Path, on: bool) -> io::Result<()> {
    let name = wide_path(path);
    // SAFETY: `name` is NUL-terminated.
    let done = unsafe {
        if on {
            EncryptFileW(name.as_ptr())
        } else {
            DecryptFileW(name.as_ptr(), 0)
        }
    };
    if done == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A sparse file or not (FAR's `ESetFileSparse`); folders are left alone by the caller.
pub fn set_sparse(path: &Path, on: bool) -> io::Result<()> {
    without_read_only(path, || {
        let file = open_for_data(path, FILE_WRITE_DATA)?;
        control(&file, FSCTL_SET_SPARSE, &[u8::from(on)], &mut [])
    })
}

/// Takes the reparse point off (FAR's `DeleteReparsePoint`): a link becomes an empty folder or file.
pub fn remove_reparse_point(path: &Path) -> io::Result<()> {
    let tag = reparse_tag(path).ok_or_else(|| io::Error::other("not a reparse point"))?;
    // `REPARSE_GUID_DATA_BUFFER`'s header: the tag, a zero length, a reserved word, a zero GUID.
    let mut header = tag.to_le_bytes().to_vec();
    header.resize(24, 0);
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    control(
        &OwnedHandle::from(file),
        FSCTL_DELETE_REPARSE_POINT,
        &header,
        &mut [],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::files::{NewLink, make_link};

    fn file(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "data").unwrap();
        path
    }

    #[test]
    fn a_file_shows_its_attributes_times_and_owner() {
        let temp = tempfile::tempdir().unwrap();
        let path = file(temp.path(), "a.txt");
        let read = file_properties(&path).unwrap();
        assert!(read.attributes.has(Flag::Archive));
        assert!(read.times.modified.is_some() && read.times.created.is_some());
        assert!(read.times.accessed.is_some() && read.times.changed.is_some());
        assert!(read.owner.is_some());
        assert_eq!((read.link, read.name_count), (None, 1));
        assert!(read.names.is_empty());
    }

    #[test]
    fn a_file_with_hard_links_lists_all_its_names() {
        let temp = tempfile::tempdir().unwrap();
        let path = file(temp.path(), "a.txt");
        std::fs::hard_link(&path, temp.path().join("b.txt")).unwrap();
        let read = file_properties(&path).unwrap();
        assert_eq!(read.name_count, 2);
        let mut names: Vec<String> = read.names.iter().map(|name| name.to_lowercase()).collect();
        names.sort();
        let full = |name: &str| {
            std::fs::canonicalize(temp.path())
                .unwrap()
                .join(name)
                .display()
                .to_string()
        };
        let expected: Vec<String> = ["a.txt", "b.txt"]
            .iter()
            .map(|name| full(name).trim_start_matches(r"\\?\").to_lowercase())
            .collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn a_volume_is_known_by_either_spelling_of_its_name() {
        assert!(is_volume(
            r"\\?\Volume{5f2c0a1e-0000-0000-0000-100000000000}\"
        ));
        assert!(is_volume(
            r"\??\Volume{5f2c0a1e-0000-0000-0000-100000000000}\"
        ));
        assert!(!is_volume(r"C:\Volume{x}") && !is_volume(r"\??\C:\x"));
    }

    #[test]
    fn a_junction_shows_its_kind_and_target() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = temp.path().join("j");
        make_link(NewLink::Junction, &target, &link).unwrap();
        let facts = file_properties(&link).unwrap().link.unwrap();
        assert_eq!(facts.kind, LinkKind::Junction);
        assert_eq!(Path::new(&facts.target), target);
        assert!(!facts.volume);
    }

    #[test]
    fn a_drive_root_shows_its_volume() {
        let root = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\";
        let facts = file_properties(Path::new(&root)).unwrap().link.unwrap();
        assert!(
            facts.volume && facts.target.starts_with(r"\\?\Volume{"),
            "{facts:?}"
        );
    }

    #[test]
    fn plain_attributes_are_set_and_a_folder_is_never_made_temporary() {
        let temp = tempfile::tempdir().unwrap();
        let path = file(temp.path(), "a.txt");
        let read = file_properties(&path).unwrap().attributes;
        set_attributes(
            &path,
            read.with(Flag::Hidden, true).with(Flag::ReadOnly, true),
        )
        .unwrap();
        let after = file_properties(&path).unwrap().attributes;
        assert!(after.has(Flag::Hidden) && after.has(Flag::ReadOnly));
        set_attributes(&path, read).unwrap();
        let folder = temp.path().join("d");
        std::fs::create_dir(&folder).unwrap();
        let attributes = file_properties(&folder).unwrap().attributes;
        set_attributes(&folder, attributes.with(Flag::Temporary, true)).unwrap();
        assert!(
            !file_properties(&folder)
                .unwrap()
                .attributes
                .has(Flag::Temporary)
        );
    }

    #[test]
    fn a_read_only_file_is_made_sparse_and_stays_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let path = file(temp.path(), "a.txt");
        let read = file_properties(&path).unwrap().attributes;
        set_attributes(&path, read.with(Flag::ReadOnly, true)).unwrap();
        set_sparse(&path, true).unwrap();
        let after = file_properties(&path).unwrap().attributes;
        assert!(after.has(Flag::Sparse) && after.has(Flag::ReadOnly));
        set_sparse(&path, false).unwrap();
        assert!(!file_properties(&path).unwrap().attributes.has(Flag::Sparse));
        set_attributes(&path, read).unwrap();
    }

    #[test]
    fn compression_goes_on_and_off_where_the_volume_has_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = file(temp.path(), "a.txt");
        if set_compressed(&path, true).is_err() {
            return; // FAT and ReFS volumes have no compression.
        }
        assert!(
            file_properties(&path)
                .unwrap()
                .attributes
                .has(Flag::Compressed)
        );
        set_compressed(&path, false).unwrap();
        assert!(
            !file_properties(&path)
                .unwrap()
                .attributes
                .has(Flag::Compressed)
        );
    }

    #[test]
    fn removing_a_junctions_reparse_point_leaves_an_empty_folder() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        file(&target, "x.txt");
        let link = temp.path().join("j");
        make_link(NewLink::Junction, &target, &link).unwrap();
        remove_reparse_point(&link).unwrap();
        let read = file_properties(&link).unwrap();
        assert!(read.link.is_none() && read.attributes.is_directory());
        assert_eq!(std::fs::read_dir(&link).unwrap().count(), 0);
        assert!(target.join("x.txt").exists(), "what it led to stays");
    }
}
