//! Virtual disks (VHD, VHDX, ISO), as FAR's `GetVHDInfo` and `detach_vhd` (`far/flink.cpp`): the file a
//! volume is a disk image of, and detaching it.

use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};

use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES};
use windows_sys::Win32::Storage::Vhd::{
    DETACH_VIRTUAL_DISK_FLAG_NONE, DetachVirtualDisk, GET_STORAGE_DEPENDENCY_FLAG_HOST_VOLUMES,
    GetStorageDependencyInformation, OPEN_VIRTUAL_DISK_FLAG_NONE, OpenVirtualDisk,
    STORAGE_DEPENDENCY_INFO, STORAGE_DEPENDENCY_INFO_VERSION_2, VIRTUAL_DISK_ACCESS_DETACH,
    VIRTUAL_STORAGE_TYPE,
};

use super::drive::{text_at, volume_paths};
use super::wide;

/// The file a volume is a disk image of (`C:\disks\work.vhdx`), for a root of a fixed or CD drive;
/// `None` for a real disk.
pub fn virtual_disk_file(root: &str) -> Option<String> {
    image_of(root).map(|(file, _)| file)
}

/// Detaches the virtual disk at `root` (FAR's `detach_vhd`); refused for a real disk.
pub fn detach_virtual_disk(root: &str) -> io::Result<()> {
    let (file, storage_type) = image_of(root).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{root} is not a virtual disk"),
        )
    })?;
    let path = if file.starts_with(r"\\") {
        wide(&file)
    } else {
        wide(&format!(r"\\?\{file}"))
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    // SAFETY: the storage type and the NUL-terminated path outlive the call; no parameters; `handle`
    // is a valid out value.
    let result = unsafe {
        OpenVirtualDisk(
            &raw const storage_type,
            path.as_ptr(),
            VIRTUAL_DISK_ACCESS_DETACH,
            OPEN_VIRTUAL_DISK_FLAG_NONE,
            std::ptr::null(),
            &raw mut handle,
        )
    };
    win32(result)?;
    // SAFETY: `handle` is a new handle owned by nothing else.
    let disk = unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) };
    // SAFETY: `disk` is an open virtual disk handle with detach access.
    win32(unsafe {
        DetachVirtualDisk(
            disk.as_raw_handle() as HANDLE,
            DETACH_VIRTUAL_DISK_FLAG_NONE,
            0,
        )
    })
}

fn win32(result: u32) -> io::Result<()> {
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(
            i32::try_from(result).unwrap_or(i32::MAX),
        ))
    }
}

/// The image file and its storage type, from the volume's storage dependency (its host volume and the
/// path on it, the host named by its drive letter where it has one, as FAR's `ConvertNameToReal`).
fn image_of(root: &str) -> Option<(String, VIRTUAL_STORAGE_TYPE)> {
    let volume = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(root)
        .ok()?;
    // u64 cells keep the structure aligned; FAR starts at 4096 bytes and grows on demand.
    let mut buffer = vec![0u64; 512];
    loop {
        let size = u32::try_from(buffer.len() * 8).ok()?;
        let info = buffer.as_mut_ptr().cast::<STORAGE_DEPENDENCY_INFO>();
        // SAFETY: `buffer` is at least one STORAGE_DEPENDENCY_INFO, aligned, and ours alone.
        unsafe { (*info).Version = STORAGE_DEPENDENCY_INFO_VERSION_2 };
        let mut used = 0u32;
        // SAFETY: an open handle; `info` holds `size` writable bytes; `used` is a valid out value.
        let result = unsafe {
            GetStorageDependencyInformation(
                volume.as_raw_handle() as HANDLE,
                GET_STORAGE_DEPENDENCY_FLAG_HOST_VOLUMES,
                size,
                info,
                &raw mut used,
            )
        };
        if result == ERROR_INSUFFICIENT_BUFFER && used > size {
            buffer.resize(usize::try_from(used).ok()?.div_ceil(8), 0);
            continue;
        }
        if result != ERROR_SUCCESS {
            return None;
        }
        // SAFETY: the system filled the structure; its first entry is there when it counts one.
        let info = unsafe { &*info };
        if info.NumberEntries == 0 {
            return None;
        }
        // SAFETY: version 2 was asked for and at least one entry was given.
        let entry = unsafe { info.Anonymous.Version2Entries[0] };
        // SAFETY: the names are NUL-terminated strings inside `buffer`, which is still alive.
        let (host, path) = unsafe {
            (
                text_at(entry.HostVolumeName),
                text_at(entry.DependentVolumeRelativePath),
            )
        };
        return Some((image_path(&host, &path), entry.VirtualStorageType));
    }
}

/// `\\?\Volume{...}\` and `\disks\a.vhdx` → `C:\disks\a.vhdx` when the host has a letter.
fn image_path(host: &str, path: &str) -> String {
    // The system names the host `\\?\Volume{...}` with no closing backslash; the lookup needs one.
    let volume = format!(r"{}\", host.trim_end_matches('\\'));
    let host = volume_paths(&volume)
        .and_then(|paths| paths.into_iter().next())
        .unwrap_or(volume);
    format!(
        r"{}\{}",
        host.trim_end_matches('\\'),
        path.trim_start_matches('\\')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_disk_is_not_a_virtual_one() {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let root = format!(r"{system}\");
        assert_eq!(virtual_disk_file(&root), None);
        let error = detach_virtual_disk(&root).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(virtual_disk_file(r"Q:\no such drive\"), None);
    }

    #[test]
    fn an_image_on_a_volume_with_no_letter_keeps_the_volume() {
        let volume = r"\\?\Volume{00000000-0000-0000-0000-000000000000}\";
        assert_eq!(
            image_path(volume, r"\disks\a.vhdx"),
            r"\\?\Volume{00000000-0000-0000-0000-000000000000}\disks\a.vhdx"
        );
    }
}
