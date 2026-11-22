//! Whether deleting on a volume goes to the Recycle Bin, for a whole folder or panel: what the shell asks
//! itself - a local fixed drive (FAR's own fallback), the policy "Do not move deleted files to the Recycle
//! Bin", and the volume's "Don't move files to the Recycle Bin" (`NukeOnDelete` of its Recycle Bin
//! properties); and the biggest object its Recycle Bin takes. The shell's answer for one object (FAR's
//! `can_recycle`) is `recycle_probe`.

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW,
};

use super::drive::drive_kind;
use crate::platform::volumes::DriveKind;

const POLICY: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer";
const VOLUMES: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket\Volume";

/// Whether deleting on the volume at `root` (`C:\`) moves to its Recycle Bin.
pub fn has_recycle_bin(root: &str) -> bool {
    if drive_kind(root) != DriveKind::Fixed {
        return false;
    }
    let policy = |key| dword(key, POLICY, "NoRecycleFiles") == Some(1);
    if policy(HKEY_CURRENT_USER) || policy(HKEY_LOCAL_MACHINE) {
        return false;
    }
    let nuke = volume_guid(root).and_then(|guid| {
        dword(
            HKEY_CURRENT_USER,
            &format!(r"{VOLUMES}\{guid}"),
            "NukeOnDelete",
        )
    });
    nuke != Some(1)
}

const MB: u64 = 1024 * 1024;

/// The biggest object the Recycle Bin of the volume at `root` takes, in bytes: a bigger one the shell
/// deletes for good with no question, though it says it would recycle it (review R1, checked on a test
/// disk). The policy "Maximum allowed Recycle Bin size" (a percentage of the volume), else the volume's
/// "Custom size" (`MaxCapacity`, MB), else the shell's default; `None` when the volume is not known.
pub fn recycle_bin_capacity(root: &str) -> Option<u64> {
    let total = || super::drive::disk_size(root).map(|(total, _)| total);
    let policy = |key| dword(key, POLICY, "RecycleBinSize");
    if let Some(percent) = policy(HKEY_CURRENT_USER).or_else(|| policy(HKEY_LOCAL_MACHINE)) {
        return Some(total()? / 100 * u64::from(percent.min(100)));
    }
    let custom = volume_guid(root).and_then(|guid| {
        dword(
            HKEY_CURRENT_USER,
            &format!(r"{VOLUMES}\{guid}"),
            "MaxCapacity",
        )
    });
    match custom {
        Some(megabytes) => Some(u64::from(megabytes) * MB),
        None => Some(default_capacity(total()?)),
    }
}

/// The shell's size of a volume's Recycle Bin never set, in whole megabytes as it writes it: 10% of the
/// volume's first 40 GB and 5% of the rest.
fn default_capacity(total: u64) -> u64 {
    let megabytes = total / MB;
    let first = megabytes.min(40 * 1024);
    (first / 10 + (megabytes - first) / 20) * MB
}

/// `{1234...}` of the volume mounted at `root`.
fn volume_guid(root: &str) -> Option<String> {
    let mut root = root.to_string();
    if !root.ends_with('\\') {
        root.push('\\');
    }
    let name: Vec<u16> = root.encode_utf16().chain([0]).collect();
    let mut volume = [0u16; 64];
    let size = u32::try_from(volume.len()).unwrap_or(0);
    // SAFETY: `name` is NUL-terminated; `volume` holds `size` characters.
    let ok = unsafe { GetVolumeNameForVolumeMountPointW(name.as_ptr(), volume.as_mut_ptr(), size) };
    if ok == 0 {
        return None;
    }
    let end = volume
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(volume.len());
    let text = String::from_utf16_lossy(&volume[..end]);
    // `\\?\Volume{guid}\`
    let start = text.find('{')?;
    let stop = text.rfind('}')?;
    text.get(start..=stop).map(str::to_string)
}

/// A DWORD value of the registry; `None` when it is not there.
fn dword(key: HKEY, subkey: &str, value: &str) -> Option<u32> {
    let subkey: Vec<u16> = subkey.encode_utf16().chain([0]).collect();
    let value: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let mut data = 0u32;
    let mut size = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4);
    // SAFETY: both names are NUL-terminated; `data` holds `size` bytes; no type is asked back.
    let status = unsafe {
        RegGetValueW(
            key,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut data).cast(),
            &raw mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_drive_has_a_volume_name_and_a_missing_drive_no_bin() {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let guid = volume_guid(&format!("{system}\\")).unwrap();
        assert!(guid.starts_with('{') && guid.ends_with('}'), "{guid}");
        let mounted: Vec<char> = crate::platform::volumes::volume_roots()
            .iter()
            .filter_map(|root| root.letter)
            .collect();
        let unmounted = ('D'..='Z')
            .find(|letter| !mounted.contains(letter))
            .unwrap();
        assert!(!has_recycle_bin(&format!("{unmounted}:\\")));
    }

    #[test]
    fn a_bin_never_set_is_the_shells_default() {
        // What the shell wrote for volumes of these sizes here (MaxCapacity, MB): a 64 MB test disk, 476
        // GB, 512 GB, 954 GB and 2 TB.
        let sizes = [
            (65_990_656, 6),
            (487_337_226_240, 25_286),
            (536_854_130_688, 27_647),
            (999_888_515_072, 49_726),
            (2_000_263_573_504, 97_427),
        ];
        for (total, megabytes) in sizes {
            assert_eq!(default_capacity(total), megabytes * MB, "{total}");
        }
        assert_eq!(default_capacity(0), 0);
    }

    #[test]
    fn the_system_drives_bin_takes_something_and_a_missing_drive_is_not_known() {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        assert!(recycle_bin_capacity(&format!("{system}\\")).is_some_and(|bytes| bytes > 0));
        let mounted: Vec<char> = crate::platform::volumes::volume_roots()
            .iter()
            .filter_map(|root| root.letter)
            .collect();
        let unmounted = ('D'..='Z')
            .find(|letter| !mounted.contains(letter))
            .unwrap();
        assert_eq!(recycle_bin_capacity(&format!("{unmounted}:\\")), None);
    }

    #[test]
    fn a_missing_registry_value_is_none() {
        assert_eq!(
            dword(HKEY_CURRENT_USER, r"Software\vc-no-such-key", "x"),
            None
        );
    }
}
