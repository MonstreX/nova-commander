//! What a copy keeps besides the contents (ADR-0009 §9, FAR's `GetSecurity` / `SetSecurity` /
//! `ResetSecurity` and `set_file_time`): the access rights, all the timestamps, a folder's attributes.

use std::io;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::time::SystemTime;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Security::Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetFileSecurityW, InitializeAcl, SetFileSecurityW,
    UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN,
    FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM, FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_WRITE_ATTRIBUTES, FileBasicInfo,
    GetFileInformationByHandleEx, SetFileAttributesW, SetFileInformationByHandle,
};

use super::locale::{from_file_time, to_file_time};
use super::wide_path;
use crate::platform::files::{Attributes, FileTimes, Kept, SecurityDescriptor};

/// `ACL_REVISION` of `winnt.h`.
const ACL_REVISION: u32 = 2;

/// The access rights of what `path` leads to (its DACL, as FAR copies them): a link's own rights are
/// not those of its file.
pub fn read_security(path: &Path) -> io::Result<SecurityDescriptor> {
    let resolved = std::fs::canonicalize(path)?;
    let name = wide_path(&resolved);
    let mut needed = 0u32;
    // SAFETY: `name` is NUL-terminated; a null buffer of length 0 only asks for the size.
    unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &raw mut needed,
        );
    }
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }
    // In words: a security descriptor must be aligned.
    let length = usize::try_from(needed).unwrap_or(0);
    let mut words = vec![0u64; length.div_ceil(8)];
    let capacity = needed;
    // SAFETY: `words` holds at least `capacity` bytes and outlives the call; `name` as above.
    let read = unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION,
            words.as_mut_ptr().cast(),
            capacity,
            &raw mut needed,
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(SecurityDescriptor(words))
}

fn write_security(path: &Path, security: &SecurityDescriptor) -> io::Result<()> {
    let name = wide_path(path);
    // SAFETY: `name` is NUL-terminated; the descriptor was filled by `read_security` and is only read.
    let written = unsafe {
        SetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION,
            security.0.as_ptr().cast_mut().cast(),
        )
    };
    if written == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// FAR's `reset_security`: an empty DACL that is not protected: only what the folder above passes on.
pub fn reset_security(path: &Path) -> io::Result<()> {
    let name = wide_path(path);
    let mut empty = ACL {
        AclRevision: 0,
        Sbz1: 0,
        AclSize: 0,
        AceCount: 0,
        Sbz2: 0,
    };
    let size = u32::try_from(size_of::<ACL>()).unwrap_or(0);
    // SAFETY: `empty` is a valid ACL of `size` bytes.
    if unsafe { InitializeAcl(&raw mut empty, size, ACL_REVISION) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `name` is NUL-terminated; `empty` outlives the call; the owner, group and SACL are unused.
    let result = unsafe {
        SetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | UNPROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw const empty,
            std::ptr::null(),
        )
    };
    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(
            i32::try_from(result).unwrap_or(0),
        ));
    }
    Ok(())
}

/// The last change of the data or the attributes of `path` (a link's own with `follow` false).
pub fn change_time(path: &Path, follow: bool) -> Option<SystemTime> {
    let flags = if follow {
        FILE_FLAG_BACKUP_SEMANTICS
    } else {
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
    };
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(flags)
        .open(path)
        .ok()?;
    let mut info = FILE_BASIC_INFO::default();
    let size = u32::try_from(size_of::<FILE_BASIC_INFO>()).unwrap_or(0);
    // SAFETY: the handle is open for the call; `info` is a valid out buffer of `size` bytes.
    let read = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            (&raw mut info).cast(),
            size,
        )
    };
    if read == 0 {
        return None;
    }
    from_file_time(info.ChangeTime)
}

/// Gives `path` the access rights and the times, each told apart in the answer. The file is opened
/// before the rights are written: what is copied may well shut this user out, and setting the rights
/// moves the change time, so the times come last.
pub fn apply_kept(
    path: &Path,
    security: Option<&SecurityDescriptor>,
    times: Option<&FileTimes>,
) -> Kept {
    let file = times.map(|_| open_for_times(path));
    let rights = security.map_or(Ok(()), |security| write_security(path, security));
    let times = match (file, times) {
        (Some(Ok(file)), Some(times)) => set_times(&file, times),
        (Some(Err(error)), _) => Err(error),
        _ => Ok(()),
    };
    Kept { rights, times }
}

/// Sets the times of `path` (a link's own) that are given; the others stay (FAR's `ESetFileTime`).
pub fn set_file_times(path: &Path, times: &FileTimes) -> io::Result<()> {
    set_times(&open_for_times(path)?, times)
}

fn open_for_times(path: &Path) -> io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

/// A time that is not given stays as it is (0 in `FILE_BASIC_INFO`).
fn set_times(file: &std::fs::File, times: &FileTimes) -> io::Result<()> {
    let ticks = |time: Option<SystemTime>| time.and_then(to_file_time).unwrap_or(0);
    let info = FILE_BASIC_INFO {
        CreationTime: ticks(times.created),
        LastAccessTime: ticks(times.accessed),
        LastWriteTime: ticks(times.modified),
        ChangeTime: ticks(times.changed),
        FileAttributes: 0,
    };
    let size = u32::try_from(size_of::<FILE_BASIC_INFO>()).unwrap_or(0);
    // SAFETY: the handle is open for the call; `info` is valid for `size` bytes and only read.
    let set = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileBasicInfo,
            (&raw const info).cast(),
            size,
        )
    };
    if set == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// An object a copy wrote from a provider's file or folder takes the read-only, hidden and system marks
/// of `attributes` (FAR's plugins set them in `GetFiles`); what it has besides stays.
pub fn add_marks(path: &Path, attributes: Attributes) -> io::Result<()> {
    let marks = attributes.windows_bits()
        & (FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM);
    if marks == 0 {
        return Ok(());
    }
    let had = std::fs::symlink_metadata(path)?.file_attributes();
    let name = wide_path(path);
    // SAFETY: `name` is NUL-terminated and outlives the call.
    if unsafe { SetFileAttributesW(name.as_ptr(), (had | marks) & !FILE_ATTRIBUTE_DIRECTORY) } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A new folder gets the read-only, hidden, system and archive attributes of the one it copies (FAR's
/// `ShellSetAttr`). The kernel's copy does this for files.
pub fn copy_folder_attributes(source: &Path, destination: &Path) -> io::Result<()> {
    let kept = std::fs::metadata(source)?.file_attributes()
        & (FILE_ATTRIBUTE_READONLY
            | FILE_ATTRIBUTE_HIDDEN
            | FILE_ATTRIBUTE_SYSTEM
            | FILE_ATTRIBUTE_ARCHIVE);
    if kept == 0 {
        return Ok(());
    }
    let name = wide_path(destination);
    // SAFETY: `name` is NUL-terminated and outlives the call.
    if unsafe { SetFileAttributesW(name.as_ptr(), kept) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
