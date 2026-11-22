//! Shift+Del in FAR's drive menu (`far/hotplug.cpp`): "safely remove" the device a drive lives on. The
//! drive's disk is found among the system's disk, CD and floppy devices by its device number; the device
//! removed is the nearest one up the device tree that can be removed (a USB stick, a card reader slot).

use std::ffi::c_void;
use std::io;

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_DEVCAP_DOCKDEVICE, CM_DEVCAP_REMOVABLE, CM_DRP_CAPABILITIES, CM_DRP_DEVICEDESC,
    CM_DRP_FRIENDLYNAME, CM_Get_Child, CM_Get_DevNode_Registry_PropertyW, CM_Get_DevNode_Status,
    CM_Get_Parent, CM_MapCrToWin32Err, CM_PROB_DEVICE_NOT_THERE, CM_PROB_DISABLED,
    CM_PROB_HELD_FOR_EJECT, CM_Request_Device_EjectW, CR_SUCCESS, DIGCF_DEVICEINTERFACE,
    DIGCF_PRESENT, HDEVINFO, PNP_VetoTypeUnknown, SP_DEVICE_INTERFACE_DATA,
    SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows_sys::Win32::Foundation::{ERROR_DRIVE_LOCKED, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Ioctl::{
    GUID_DEVINTERFACE_CDROM, GUID_DEVINTERFACE_DISK, GUID_DEVINTERFACE_FLOPPY,
    IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER,
};
use windows_sys::core::GUID;

use super::device::{control, open_device};
use super::wide;
use crate::platform::volumes::{DriveKind, HotplugDevice};

/// The removable device drive `X:` lives on; `None` for a disk that cannot be removed.
pub fn hotplug_device(root: &str) -> Option<HotplugDevice> {
    let letter = super::drive_letter(root).ok()?;
    let number = device_number(&wide(&format!(r"\\.\{letter}:")))?;
    // The interface class from the drive itself: device numbers repeat across classes.
    let class = if super::drive_kind(root) == DriveKind::CdRom {
        GUID_DEVINTERFACE_CDROM
    } else if super::drive::dos_device(letter)
        .is_some_and(|device| device.starts_with(r"\Device\Floppy"))
    {
        GUID_DEVINTERFACE_FLOPPY
    } else {
        GUID_DEVINTERFACE_DISK
    };
    let mut node = find_node(&class, number)?;
    loop {
        if is_hotplug(node) {
            return Some(HotplugDevice {
                node,
                description: property_here_or_below(node, CM_DRP_DEVICEDESC),
                friendly_name: property_here_or_below(node, CM_DRP_FRIENDLYNAME),
            });
        }
        let mut parent = 0u32;
        // SAFETY: `parent` is a valid out pointer; `node` came from the system.
        if unsafe { CM_Get_Parent(&raw mut parent, node, 0) } != CR_SUCCESS {
            return None;
        }
        node = parent;
    }
}

/// Asks the system to stop the device; it refuses while files on it are open ("the device is in use").
pub fn remove_hotplug_device(device: &HotplugDevice) -> io::Result<()> {
    let mut veto = PNP_VetoTypeUnknown;
    let mut veto_name = [0u16; 260];
    // SAFETY: `veto` and `veto_name` are valid for the sizes given.
    let result = unsafe {
        CM_Request_Device_EjectW(device.node, &raw mut veto, veto_name.as_mut_ptr(), 260, 0)
    };
    // The system may say CR_SUCCESS and still veto (FAR: "M$ bug").
    if result == CR_SUCCESS && veto == PNP_VetoTypeUnknown {
        return Ok(());
    }
    let code = if result == CR_SUCCESS {
        ERROR_DRIVE_LOCKED
    } else {
        // SAFETY: plain value conversion.
        unsafe { CM_MapCrToWin32Err(result, ERROR_DRIVE_LOCKED) }
    };
    Err(io::Error::from_raw_os_error(code.cast_signed()))
}

/// The device type and number of a disk, CD or floppy (`IOCTL_STORAGE_GET_DEVICE_NUMBER`).
fn device_number(path: &[u16]) -> Option<(u32, u32)> {
    let device = open_device(path, 0).ok()?;
    let mut number = [0u8; size_of::<STORAGE_DEVICE_NUMBER>()];
    control(&device, IOCTL_STORAGE_GET_DEVICE_NUMBER, &[], &mut number).ok()?;
    // SAFETY: the call filled the buffer with a STORAGE_DEVICE_NUMBER; read unaligned.
    let number = unsafe {
        number
            .as_ptr()
            .cast::<STORAGE_DEVICE_NUMBER>()
            .read_unaligned()
    };
    Some((number.DeviceType, number.DeviceNumber))
}

/// The device node of the interface of `class` with this device number.
fn find_node(class: &GUID, number: (u32, u32)) -> Option<u32> {
    // SAFETY: `class` is a valid GUID; no enumerator or window.
    let set: HDEVINFO = unsafe {
        SetupDiGetClassDevsW(
            class,
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if set == INVALID_HANDLE_VALUE as HDEVINFO {
        return None;
    }
    let found = (0..)
        .map_while(|index| interface(set, class, index))
        .find_map(|interface| {
            let (path, node) = interface_path(set, &interface)?;
            (device_number(&path) == Some(number)).then_some(node)
        });
    // SAFETY: `set` came from SetupDiGetClassDevsW and is destroyed once.
    unsafe { SetupDiDestroyDeviceInfoList(set) };
    found
}

fn interface(set: HDEVINFO, class: &GUID, index: u32) -> Option<SP_DEVICE_INTERFACE_DATA> {
    let mut data = SP_DEVICE_INTERFACE_DATA {
        cbSize: u32::try_from(size_of::<SP_DEVICE_INTERFACE_DATA>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `set` is open; `data` has its size set.
    let ok =
        unsafe { SetupDiEnumDeviceInterfaces(set, std::ptr::null(), class, index, &raw mut data) };
    (ok != 0).then_some(data)
}

/// The interface's device path (NUL-terminated) and its device node.
fn interface_path(set: HDEVINFO, interface: &SP_DEVICE_INTERFACE_DATA) -> Option<(Vec<u16>, u32)> {
    let mut needed = 0u32;
    // SAFETY: asks for the size only: no buffer.
    unsafe {
        SetupDiGetDeviceInterfaceDetailW(
            set,
            interface,
            std::ptr::null_mut(),
            0,
            &raw mut needed,
            std::ptr::null_mut(),
        )
    };
    let needed_bytes = usize::try_from(needed).ok()?;
    if needed_bytes < size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() {
        return None;
    }
    // u64 words keep the buffer aligned for the detail structure.
    let mut words = vec![0u64; needed_bytes.div_ceil(8)];
    let detail = words
        .as_mut_ptr()
        .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
    let mut info = SP_DEVINFO_DATA {
        cbSize: u32::try_from(size_of::<SP_DEVINFO_DATA>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: the buffer is at least `needed` bytes, aligned, with `cbSize` set to the structure's
    // fixed size as the API wants; `info` has its size set.
    let ok = unsafe {
        (*detail).cbSize = u32::try_from(size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>()).ok()?;
        SetupDiGetDeviceInterfaceDetailW(
            set,
            interface,
            detail,
            needed,
            std::ptr::null_mut(),
            &raw mut info,
        )
    };
    if ok == 0 {
        return None;
    }
    let offset = std::mem::offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
    let units = (needed_bytes - offset) / 2;
    // SAFETY: the path starts at `DevicePath` and runs to the end of the `needed` bytes the call filled.
    let path = unsafe {
        std::slice::from_raw_parts(words.as_ptr().cast::<u8>().add(offset).cast::<u16>(), units)
    };
    let end = path.iter().position(|&unit| unit == 0).unwrap_or(units);
    let mut path = path[..end].to_vec();
    path.push(0);
    Some((path, info.DevInst))
}

/// FAR's `IsDeviceHotplug` with "safe to remove" devices included: removable, present, enabled, not a
/// docking station.
fn is_hotplug(node: u32) -> bool {
    let mut capabilities = [0u8; 4];
    if !property(node, CM_DRP_CAPABILITIES, &mut capabilities) {
        return false;
    }
    let capabilities = u32::from_le_bytes(capabilities);
    let (mut status, mut problem) = (0u32, 0u32);
    // SAFETY: both outputs are valid pointers.
    if unsafe { CM_Get_DevNode_Status(&raw mut status, &raw mut problem, node, 0) } != CR_SUCCESS {
        return false;
    }
    ![
        CM_PROB_DEVICE_NOT_THERE,
        CM_PROB_HELD_FOR_EJECT,
        CM_PROB_DISABLED,
    ]
    .contains(&problem)
        && capabilities & CM_DEVCAP_REMOVABLE != 0
        && capabilities & CM_DEVCAP_DOCKDEVICE == 0
}

/// A text property of the node, or of its first child (FAR's `GetDevicePropertyRecursive`).
fn property_here_or_below(node: u32, which: u32) -> String {
    text_property(node, which)
        .or_else(|| {
            let mut child = 0u32;
            // SAFETY: `child` is a valid out pointer.
            let found = unsafe { CM_Get_Child(&raw mut child, node, 0) } == CR_SUCCESS;
            found.then(|| text_property(child, which)).flatten()
        })
        .map(|text| text.trim().to_string())
        .unwrap_or_default()
}

fn text_property(node: u32, which: u32) -> Option<String> {
    let mut buffer = [0u8; 1024];
    if !property(node, which, &mut buffer) {
        return None;
    }
    let units: Vec<u16> = buffer
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|&unit| unit != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

fn property(node: u32, which: u32, buffer: &mut [u8]) -> bool {
    let mut length = u32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: `buffer` is writable for `length` bytes; the type output may be null.
    let result = unsafe {
        CM_Get_DevNode_Registry_PropertyW(
            node,
            which,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast::<c_void>(),
            &raw mut length,
            0,
        )
    };
    result == CR_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_disk_is_found_and_is_no_hotplug_device() {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let letter = drive.chars().next().unwrap();
        let number = device_number(&wide(&format!(r"\\.\{letter}:"))).unwrap();
        assert!(
            find_node(&GUID_DEVINTERFACE_DISK, number).is_some(),
            "the system disk among the disks"
        );
        assert_eq!(hotplug_device(&format!("{drive}\\")), None);
    }
}
