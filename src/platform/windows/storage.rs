//! The device a volume is on, as the storage driver describes it (`IOCTL_STORAGE_QUERY_PROPERTY`): its
//! vendor, product and bus. FAR reads the product name to tell CD drives apart; the info panel shows
//! the device (owner, 2026-11-09: more than FAR where useful).

use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW;
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_DESCRIPTOR_HEADER,
    STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY, StorageDeviceProperty,
};

use crate::platform::volumes::StorageDevice;

/// The device under the volume at `root` (`C:\`, `\\?\Volume{...}\`); `None` for a network drive or
/// a device that does not say.
pub fn storage_device(root: &str) -> Option<StorageDevice> {
    // No access is needed to ask for the device's description.
    let device = OpenOptions::new()
        .access_mode(0)
        .open(device_path(root)?)
        .ok()?;
    let described = Described::read(&device)?;
    Some(StorageDevice {
        vendor: described
            .text(described.vendor_offset())
            .unwrap_or_default(),
        product: described
            .text(described.product_offset())
            .unwrap_or_default(),
        bus: bus_name(described.bus()).to_string(),
    })
    .filter(|device| !device.product.is_empty())
}

/// `\\.\C:` for a drive root, `\\.\Volume{...}` for a volume, and for a mount point folder its
/// volume's (`GetVolumeNameForVolumeMountPointW`).
fn device_path(root: &str) -> Option<String> {
    let trimmed = root.trim_end_matches('\\');
    if let Some(rest) = trimmed.strip_prefix(r"\\?\") {
        return Some(format!(r"\\.\{rest}"));
    }
    if trimmed.len() == 2 {
        return Some(format!(r"\\.\{trimmed}"));
    }
    let name = super::wide(&format!("{trimmed}\\"));
    let mut volume = [0u16; 64];
    // SAFETY: `name` is NUL-terminated; `volume` holds 64 characters, enough for a volume GUID path.
    let ok = unsafe { GetVolumeNameForVolumeMountPointW(name.as_ptr(), volume.as_mut_ptr(), 64) };
    if ok == 0 {
        return None;
    }
    let end = volume
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(volume.len());
    let volume = String::from_utf16_lossy(&volume[..end]);
    let volume = volume.trim_end_matches('\\');
    volume
        .strip_prefix(r"\\?\")
        .map(|rest| format!(r"\\.\{rest}"))
}

/// The product name the device gives (`STORAGE_DEVICE_DESCRIPTOR`'s product id).
pub(super) fn product_id(device: &File) -> Option<String> {
    let described = Described::read(device)?;
    described.text(described.product_offset())
}

/// A `STORAGE_DEVICE_DESCRIPTOR` and the strings after it.
struct Described {
    /// u64 cells keep the structure aligned.
    buffer: Vec<u64>,
    size: usize,
}

impl Described {
    fn read(device: &File) -> Option<Self> {
        let mut header = [0u64; 1];
        if !query(device, &mut header) {
            return None;
        }
        // SAFETY: the buffer is aligned and holds a STORAGE_DESCRIPTOR_HEADER the system filled.
        let size = unsafe { (*header.as_ptr().cast::<STORAGE_DESCRIPTOR_HEADER>()).Size };
        let size = usize::try_from(size).ok()?;
        if size < size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
            return None;
        }
        let mut buffer = vec![0u64; size.div_ceil(8)];
        query(device, &mut buffer).then_some(Self { buffer, size })
    }

    fn descriptor(&self) -> &STORAGE_DEVICE_DESCRIPTOR {
        // SAFETY: the buffer is aligned and holds the descriptor the system filled, at least its size.
        unsafe { &*self.buffer.as_ptr().cast::<STORAGE_DEVICE_DESCRIPTOR>() }
    }

    fn vendor_offset(&self) -> u32 {
        self.descriptor().VendorIdOffset
    }

    fn product_offset(&self) -> u32 {
        self.descriptor().ProductIdOffset
    }

    fn bus(&self) -> i32 {
        self.descriptor().BusType
    }

    /// The text at `offset`, up to its NUL, trimmed; `None` for no offset or an empty text.
    fn text(&self, offset: u32) -> Option<String> {
        // SAFETY: plain bytes of the buffer, `size` of them written.
        let bytes =
            unsafe { std::slice::from_raw_parts(self.buffer.as_ptr().cast::<u8>(), self.size) };
        let text = bytes.get(usize::try_from(offset).ok().filter(|&at| at != 0)?..)?;
        let end = text
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(text.len());
        let text = String::from_utf8_lossy(&text[..end]).trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}

/// Fills `output` with the device's description; false when the device refuses.
fn query(device: &File, output: &mut [u64]) -> bool {
    let request = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut returned = 0u32;
    let size = u32::try_from(output.len() * 8).unwrap_or(0);
    // SAFETY: an open device; `request` is the input and `output` (aligned, `size` bytes) the output,
    // both outliving the synchronous call; `returned` is a valid out value.
    let ok = unsafe {
        DeviceIoControl(
            device.as_raw_handle() as HANDLE,
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&raw const request).cast(),
            u32::try_from(size_of::<STORAGE_PROPERTY_QUERY>()).unwrap_or(0),
            output.as_mut_ptr().cast(),
            size,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    ok != 0
}

/// `STORAGE_BUS_TYPE` as people name the buses.
fn bus_name(bus: i32) -> &'static str {
    match bus {
        0x01 => "SCSI",
        0x02 => "ATAPI",
        0x03 => "ATA",
        0x04 => "IEEE 1394",
        0x05 => "SSA",
        0x06 => "Fibre Channel",
        0x07 => "USB",
        0x08 => "RAID",
        0x09 => "iSCSI",
        0x0A => "SAS",
        0x0B => "SATA",
        0x0C => "SD",
        0x0D => "MMC",
        0x0E | 0x0F => "Virtual",
        0x10 => "Storage Spaces",
        0x11 => "NVMe",
        0x12 => "SCM",
        0x13 => "UFS",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_drive_names_its_device() {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let device = storage_device(&format!(r"{system}\")).unwrap();
        assert!(!device.product.is_empty());
        assert_eq!(storage_device(r"Q:\no such drive\"), None);
    }

    #[test]
    fn device_paths_of_roots_volumes_and_mount_points() {
        assert_eq!(device_path(r"D:\").as_deref(), Some(r"\\.\D:"));
        assert_eq!(
            device_path(r"\\?\Volume{1}\").as_deref(),
            Some(r"\\.\Volume{1}")
        );
        assert_eq!(device_path(r"C:\no such mount point\"), None);
    }

    #[test]
    fn buses_are_named() {
        assert_eq!(bus_name(0x11), "NVMe");
        assert_eq!(bus_name(0x07), "USB");
        assert_eq!(bus_name(0x0F), "Virtual");
        assert_eq!(bus_name(0x7F), "");
    }
}
