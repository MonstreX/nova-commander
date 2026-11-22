//! Links the system makes, as FAR's `MkSymLink` makes them (`far/flink.cpp`): a junction (`mklink /J`'s
//! reparse point) and a drive mounted on a folder (a volume mount point). Nothing that exists is ever
//! replaced. Hard links and symbolic links come from `std` (`std::fs::hard_link`,
//! `std::os::windows::fs::symlink_file`, `symlink_dir`).

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_WRITE_ATTRIBUTES,
    GetVolumeNameForVolumeMountPointW, SetVolumeMountPointW,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

/// `IO_REPARSE_TAG_MOUNT_POINT`.
const TAG_MOUNT_POINT: u32 = 0xA000_0003;
/// `MAXIMUM_REPARSE_DATA_BUFFER_SIZE`.
const MAXIMUM_BUFFER: usize = 16 * 1024;

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// The names a junction keeps: `\??\C:\x` and `C:\x`; `\??\UNC\server\share` and `\\server\share`.
fn junction_names(full: &Path) -> (Vec<u16>, Vec<u16>) {
    let path: Vec<u16> = full.as_os_str().encode_wide().collect();
    let plain = [r"\\?\", r"\\.\"]
        .iter()
        .find_map(|prefix| path.strip_prefix(units(prefix).as_slice()))
        .map_or_else(
            || path.clone(),
            |rest| match rest.strip_prefix(units(r"UNC\").as_slice()) {
                Some(share) => [units(r"\\"), share.to_vec()].concat(),
                None => rest.to_vec(),
            },
        );
    let substitute = match plain.strip_prefix(units(r"\\").as_slice()) {
        Some(share) => [units(r"\??\UNC\"), share.to_vec()].concat(),
        None => [units(r"\??\"), plain.clone()].concat(),
    };
    (substitute, plain)
}

/// A junction's `REPARSE_DATA_BUFFER`: the substitute name, a NUL, the print name, a NUL.
fn mount_point_buffer(substitute: &[u16], print: &[u16]) -> Option<Vec<u8>> {
    let substitute_length = substitute.len() * 2;
    let print_offset = substitute_length + 2;
    let print_length = print.len() * 2;
    let data_length = 8 + print_offset + print_length + 2;
    if 8 + data_length > MAXIMUM_BUFFER {
        return None;
    }
    let word = |value: usize| u16::try_from(value).ok().map(u16::to_le_bytes);
    let mut buffer = TAG_MOUNT_POINT.to_le_bytes().to_vec();
    for value in [
        data_length,
        0,
        0,
        substitute_length,
        print_offset,
        print_length,
    ] {
        buffer.extend(word(value)?);
    }
    for unit in substitute.iter().chain(&[0]).chain(print).chain(&[0]) {
        buffer.extend(unit.to_le_bytes());
    }
    Some(buffer)
}

/// `link`, a new folder, made a junction to `target` (its full path); the folder goes again when that
/// fails.
pub fn junction(target: &Path, link: &Path) -> io::Result<()> {
    let (substitute, print) = junction_names(&std::path::absolute(target)?);
    let buffer = mount_point_buffer(&substitute, &print).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "the target's name is too long")
    })?;
    std::fs::create_dir(link)?;
    let set = set_reparse_point(link, &buffer);
    if set.is_err() {
        let _ = std::fs::remove_dir(link);
    }
    set
}

fn set_reparse_point(path: &Path, buffer: &[u8]) -> io::Result<()> {
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut returned = 0u32;
    let length = u32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: the folder is open; the buffer is valid for its length; no output, no OVERLAPPED.
    let done = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            length,
            std::ptr::null_mut(),
            0,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    if done == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The drive `drive` (`E:\`) mounted on `folder` (FAR's `CreateVolumeMountPoint`); the folder is made
/// when it is not there (and goes again when the mount fails).
pub fn mount_volume(drive: &Path, folder: &Path) -> io::Result<()> {
    let with_slash = |path: &Path| {
        let mut text = path.as_os_str().to_os_string();
        if !text.to_string_lossy().ends_with('\\') {
            text.push("\\");
        }
        std::path::PathBuf::from(text)
    };
    let (drive, folder) = (with_slash(drive), with_slash(&std::path::absolute(folder)?));
    let mut volume = [0u16; 64];
    // SAFETY: a NUL-terminated name; the buffer's length is given (50 is the most a volume name takes).
    let named = unsafe {
        GetVolumeNameForVolumeMountPointW(wide(&drive).as_ptr(), volume.as_mut_ptr(), 64)
    };
    if named == 0 {
        return Err(io::Error::last_os_error());
    }
    let made = !folder.exists() && std::fs::create_dir(&folder).is_ok();
    // SAFETY: both NUL-terminated, the volume's name as the system wrote it.
    let mounted = unsafe { SetVolumeMountPointW(wide(&folder).as_ptr(), volume.as_ptr()) };
    if mounted == 0 {
        let error = io::Error::last_os_error();
        if made {
            let _ = std::fs::remove_dir(&folder);
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_junction_leads_to_its_folder_and_never_replaces() {
        let temp = std::env::temp_dir().join(format!("nc-sdk-junction-{}", std::process::id()));
        let target = temp.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("in.txt"), "in").unwrap();
        let link = temp.join("link");
        junction(&target, &link).unwrap();
        assert_eq!(std::fs::read_to_string(link.join("in.txt")).unwrap(), "in");
        assert!(junction(&target, &link).is_err(), "it exists");
        std::fs::remove_dir(&link).unwrap();
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn a_junctions_names_are_the_systems() {
        let (substitute, print) = junction_names(Path::new(r"\\?\C:\x"));
        assert_eq!(String::from_utf16_lossy(&substitute), r"\??\C:\x");
        assert_eq!(String::from_utf16_lossy(&print), r"C:\x");
        let (substitute, print) = junction_names(Path::new(r"\\server\share"));
        assert_eq!(
            String::from_utf16_lossy(&substitute),
            r"\??\UNC\server\share"
        );
        assert_eq!(String::from_utf16_lossy(&print), r"\\server\share");
    }
}
