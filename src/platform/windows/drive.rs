//! What FAR's drive menu (`far/diskmenu.cpp`) asks the system about drives: the letters in use, the
//! remembered network drives, a drive's type, label, file system and size, the folder a SUBST drive
//! stands for, the share a network drive is mapped to, the shell's name of a drive, and the volumes that
//! have no letter.

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, HANDLE, INVALID_HANDLE_VALUE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::WNet::{
    NETRESOURCEW, RESOURCE_REMEMBERED, RESOURCETYPE_DISK, WNetCloseEnum, WNetEnumResourceW,
    WNetGetConnectionW, WNetOpenEnumW,
};
use windows_sys::Win32::Storage::FileSystem::{
    FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, GetDiskFreeSpaceExW, GetDiskFreeSpaceW,
    GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, GetVolumePathNamesForVolumeNameW,
    QueryDosDeviceW,
};
use windows_sys::Win32::UI::Shell::{SHFILEINFOW, SHGFI_DISPLAYNAME, SHGetFileInfoW};

use super::{drive_letter, wide};
use crate::platform::volumes::{DriveKind, VolumeFacts, VolumeRoot};

/// The drive roots in use and the remembered network drives, A to Z (FAR's drive menu).
pub fn volume_roots() -> Vec<VolumeRoot> {
    let remembered = remembered_network_drives();
    let letters = logical_drives() | remembered;
    (0..26u8)
        .filter(|bit| letters & (1 << bit) != 0)
        .map(|bit| {
            let letter = char::from(b'A' + bit);
            VolumeRoot {
                path: format!("{letter}:\\"),
                letter: Some(letter),
                remembered: remembered & (1 << bit) != 0,
            }
        })
        .collect()
}

/// The letters in use, bit 0 for A (`GetLogicalDrives`).
pub(super) fn logical_drives() -> u32 {
    // SAFETY: no arguments.
    unsafe { GetLogicalDrives() }
}

/// The letters of remembered network connections, whether connected now or not (FAR's
/// `GetSavedNetworkDrives`).
fn remembered_network_drives() -> u32 {
    let mut handle: HANDLE = std::ptr::null_mut();
    // SAFETY: a null NETRESOURCE starts at the root; `handle` is a valid out pointer.
    let opened = unsafe {
        WNetOpenEnumW(
            RESOURCE_REMEMBERED,
            RESOURCETYPE_DISK,
            0,
            std::ptr::null(),
            &raw mut handle,
        )
    };
    if opened != NO_ERROR {
        return 0;
    }
    let mut letters = 0;
    // u64 words keep the buffer aligned for NETRESOURCEW.
    let mut words = vec![0u64; 2048];
    loop {
        let mut count = u32::MAX;
        let mut size = u32::try_from(words.len() * 8).unwrap_or(u32::MAX);
        // SAFETY: `handle` is open; the buffer is writable for `size` bytes and outlives the call.
        let result = unsafe {
            WNetEnumResourceW(
                handle,
                &raw mut count,
                words.as_mut_ptr().cast(),
                &raw mut size,
            )
        };
        if result == ERROR_MORE_DATA {
            words.resize(usize::try_from(size).unwrap_or(0) / 8 + 1, 0);
            continue;
        }
        if result != NO_ERROR || count == 0 {
            break;
        }
        let entries = words.as_ptr().cast::<NETRESOURCEW>();
        for index in 0..usize::try_from(count).unwrap_or(0) {
            // SAFETY: the call put `count` NETRESOURCEW records at the start of the buffer; their
            // strings point into the same buffer, which is not touched until the next call.
            let local = unsafe { text_at((*entries.add(index)).lpLocalName) };
            letters |= letter_bit(&local);
        }
    }
    // SAFETY: `handle` came from WNetOpenEnumW and is closed once.
    unsafe { WNetCloseEnum(handle) };
    letters
}

/// The bit of `X:` in a letter mask, 0 for anything else.
fn letter_bit(name: &str) -> u32 {
    let mut chars = name.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some(letter), Some(':'), None) if letter.is_ascii_alphabetic() => {
            1 << (u32::from(letter.to_ascii_uppercase()) - u32::from('A'))
        }
        _ => 0,
    }
}

/// Whether `path` is on a local fixed disk (or a RAM disk). A share's path is none, told by its form
/// alone: asking the system about it waits for the network (a share that does not answer: its time-out;
/// R7 review B6).
pub fn on_local_disk(path: &std::path::Path) -> bool {
    let Some(std::path::Component::Prefix(prefix)) = path.components().next() else {
        return false;
    };
    let std::path::Prefix::Disk(letter) = prefix.kind() else {
        return false;
    };
    let root = format!("{}:\\", char::from(letter));
    matches!(drive_kind(&root), DriveKind::Fixed | DriveKind::RamDisk)
}

/// `GetDriveTypeW` of a root such as `C:\`.
pub fn drive_kind(root: &str) -> DriveKind {
    let name = wide(root);
    // SAFETY: `name` is NUL-terminated.
    match unsafe { GetDriveTypeW(name.as_ptr()) } {
        1 => DriveKind::NoRoot,
        2 => DriveKind::Removable,
        3 => DriveKind::Fixed,
        4 => DriveKind::Remote,
        5 => DriveKind::CdRom,
        6 => DriveKind::RamDisk,
        _ => DriveKind::Unknown,
    }
}

/// The volume's label and file system; `None` when the volume cannot be read (no disk in the drive).
pub fn volume_facts(root: &str) -> Option<VolumeFacts> {
    let name = wide(root);
    let mut label = [0u16; 261];
    let mut file_system = [0u16; 261];
    let mut serial = 0u32;
    // SAFETY: `name` is NUL-terminated; both buffers hold the sizes given; `serial` is a valid out
    // value; the other outputs may be null.
    let ok = unsafe {
        GetVolumeInformationW(
            name.as_ptr(),
            label.as_mut_ptr(),
            261,
            &raw mut serial,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            file_system.as_mut_ptr(),
            261,
        )
    };
    (ok != 0).then(|| VolumeFacts {
        label: until_nul(&label),
        file_system: until_nul(&file_system),
        serial: Some(serial),
    })
}

/// The size of the volume's cluster, in bytes (FAR's `GetDirInfo`: `GetDiskFreeSpace` of the root).
pub fn cluster_size(root: &str) -> Option<u64> {
    let name = wide(root);
    let (mut sectors, mut bytes, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
    // SAFETY: `name` is NUL-terminated; the four outputs are valid for the call.
    let ok = unsafe {
        GetDiskFreeSpaceW(
            name.as_ptr(),
            &raw mut sectors,
            &raw mut bytes,
            &raw mut free,
            &raw mut total,
        )
    };
    (ok != 0).then(|| u64::from(sectors) * u64::from(bytes))
}

/// The size of the volume and its free space, both as the user may use them.
pub fn disk_size(root: &str) -> Option<(u64, u64)> {
    let name = wide(root);
    let (mut free, mut total) = (0u64, 0u64);
    // SAFETY: `name` is NUL-terminated; the outputs are valid; the last may be null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            name.as_ptr(),
            &raw mut free,
            &raw mut total,
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some((total, free))
}

/// The folder a SUBST drive stands for (FAR's `GetSubstName`): its device is `\??\C:\folder`.
pub fn subst_target(root: &str) -> Option<String> {
    let device = dos_device(drive_letter(root).ok()?)?;
    let folder = device.strip_prefix(r"\??\")?;
    // A share given to SUBST: `\??\UNC\server\share`.
    Some(match folder.strip_prefix(r"UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => folder.to_string(),
    })
}

pub(super) fn dos_device(letter: char) -> Option<String> {
    query_dos_device(&format!("{letter}:"))
}

/// The kernel's device a drive or a volume stands for (FAR's `resolve_kernel_link` of the drive menu's
/// F4): `C:\` is `\Device\HarddiskVolume3`, a volume without a letter by its `Volume{...}` name.
pub fn device_name(root: &str) -> Option<String> {
    let name = root.trim_end_matches('\\');
    let name = name.strip_prefix(r"\\?\").unwrap_or(name);
    query_dos_device(name).filter(|device| !device.is_empty())
}

fn query_dos_device(name: &str) -> Option<String> {
    let name = wide(name);
    let mut buffer = vec![0u16; 1024];
    let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: `name` is NUL-terminated; `buffer` holds `length` characters.
    let written = unsafe { QueryDosDeviceW(name.as_ptr(), buffer.as_mut_ptr(), length) };
    (written != 0).then(|| until_nul(&buffer))
}

/// The share a network drive is mapped to, connected or only remembered (`WNetGetConnectionW`).
pub fn network_name(root: &str) -> Option<String> {
    let letter = drive_letter(root).ok()?;
    let name = wide(&format!("{letter}:"));
    let mut buffer = vec![0u16; 1024];
    let mut length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: `name` is NUL-terminated; `buffer` holds `length` characters.
    let result = unsafe { WNetGetConnectionW(name.as_ptr(), buffer.as_mut_ptr(), &raw mut length) };
    // ERROR_CONNECTION_UNAVAIL (1201): remembered but not connected; the name is still given.
    (result == NO_ERROR || result == 1201)
        .then(|| until_nul(&buffer))
        .filter(|name| !name.is_empty())
}

/// The name the shell shows for a drive, "Local Disk (C:)" (FAR's `DRIVE_SHOW_LABEL_USE_SHELL`).
pub fn shell_name(root: &str) -> Option<String> {
    let name = wide(root);
    let mut info: SHFILEINFOW = empty_file_info();
    let size = u32::try_from(size_of::<SHFILEINFOW>()).unwrap_or(0);
    // SAFETY: `name` is NUL-terminated; `info` is a writable SHFILEINFOW of `size` bytes.
    let ok = unsafe { SHGetFileInfoW(name.as_ptr(), 0, &raw mut info, size, SHGFI_DISPLAYNAME) };
    (ok != 0)
        .then(|| until_nul(&info.szDisplayName))
        .filter(|name| !name.is_empty())
}

fn empty_file_info() -> SHFILEINFOW {
    SHFILEINFOW {
        hIcon: std::ptr::null_mut(),
        iIcon: 0,
        dwAttributes: 0,
        szDisplayName: [0; 260],
        szTypeName: [0; 80],
    }
}

/// The volumes with no drive letter, as `\\?\Volume{...}\` (FAR's Ctrl+H in the drive menu).
pub fn unmounted_volumes() -> Vec<String> {
    let mut volumes = Vec::new();
    let mut buffer = [0u16; 64];
    // SAFETY: `buffer` holds 64 characters, enough for a volume GUID path.
    let find = unsafe { FindFirstVolumeW(buffer.as_mut_ptr(), 64) };
    if find == INVALID_HANDLE_VALUE {
        return volumes;
    }
    loop {
        let volume = until_nul(&buffer);
        if !has_letter(&volume) {
            volumes.push(volume);
        }
        // SAFETY: `find` is an open volume search; `buffer` holds 64 characters.
        if unsafe { FindNextVolumeW(find, buffer.as_mut_ptr(), 64) } == 0 {
            break;
        }
    }
    // SAFETY: `find` came from FindFirstVolumeW and is closed once.
    unsafe { FindVolumeClose(find) };
    volumes
}

/// Whether a volume is mounted at a drive letter (`X:\` among its paths).
fn has_letter(volume: &str) -> bool {
    // A volume mounted at too many folders to list is not one FAR would show either.
    volume_paths(volume).is_none_or(|paths| {
        paths
            .iter()
            .any(|path| path.len() == 3 && letter_bit(&path[..2]) != 0)
    })
}

/// The paths a volume (`\\?\Volume{...}\`) is mounted at, drive roots first; `None` when they cannot
/// be read.
pub(super) fn volume_paths(volume: &str) -> Option<Vec<String>> {
    let name = wide(volume);
    let mut buffer = vec![0u16; 1024];
    let mut needed = 0u32;
    let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: `name` is NUL-terminated; `buffer` holds `length` characters; `needed` is valid.
    let ok = unsafe {
        GetVolumePathNamesForVolumeNameW(
            name.as_ptr(),
            buffer.as_mut_ptr(),
            length,
            &raw mut needed,
        )
    };
    (ok != 0).then(|| {
        buffer
            .split(|&unit| unit == 0)
            .take_while(|path| !path.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    })
}

/// The text before the first NUL of a pointer from the system; empty for null.
///
/// # Safety
/// `text` is null or points to a NUL-terminated UTF-16 string.
pub(super) unsafe fn text_at(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    let mut length = 0;
    // SAFETY: the caller promises a NUL-terminated string; reading stops at the NUL.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the `length` characters before the NUL were just read.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) })
}

fn until_nul(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_drive_is_fixed_with_a_file_system_and_a_size() {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let root = format!("{drive}\\");
        assert_ne!(logical_drives() & letter_bit(&drive), 0);
        assert_eq!(drive_kind(&root), DriveKind::Fixed);
        assert!(!volume_facts(&root).unwrap().file_system.is_empty());
        let (total, free) = disk_size(&root).unwrap();
        assert!(total >= free && total > 0);
        assert_eq!(subst_target(&root), None, "a real disk");
        assert!(shell_name(&root).is_some_and(|name| name.contains(&drive)));
    }

    #[test]
    fn letter_bits() {
        assert_eq!(letter_bit("A:"), 1);
        assert_eq!(letter_bit("z:"), 1 << 25);
        assert_eq!(letter_bit(r"\\server"), 0);
        assert_eq!(letter_bit("C:x"), 0);
    }
}
