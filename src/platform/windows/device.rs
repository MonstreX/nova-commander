//! What FAR's drive menu does to a drive (`far/diskmenu.cpp`, `eject.cpp`, `network.cpp`): disconnect
//! and reconnect a network drive, delete a SUBST drive, eject and load the media of a CD or removable
//! drive, change a volume's label.

use std::ffi::c_void;
use std::io;
use std::os::windows::io::{FromRawHandle, OwnedHandle, RawHandle};

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INVALID_PASSWORD, ERROR_LOGON_FAILURE,
    ERROR_SESSION_CREDENTIAL_CONFLICT, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    NO_ERROR,
};
use windows_sys::Win32::NetworkManagement::WNet::{
    CONNECT_UPDATE_PROFILE, NETRESOURCEW, RESOURCETYPE_DISK, WNetAddConnection2W,
    WNetCancelConnection2W,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, DDD_REMOVE_DEFINITION, DefineDosDeviceW, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FlushFileBuffers, OPEN_EXISTING, SetVolumeLabelW,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    DISK_GEOMETRY, FSCTL_LOCK_VOLUME, FSCTL_UNLOCK_VOLUME, IOCTL_DISK_GET_DRIVE_GEOMETRY,
    IOCTL_STORAGE_EJECT_MEDIA, IOCTL_STORAGE_LOAD_MEDIA, IOCTL_STORAGE_MEDIA_REMOVAL,
    PREVENT_MEDIA_REMOVAL, RemovableMedia,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegOpenKeyExW,
};

use super::{drive_letter, wide};

/// Disconnects the network drive `X:`; `forget` also drops it from the connections restored at logon
/// (FAR's "Reconnect at logon" off).
pub fn disconnect_network_drive(root: &str, forget: bool) -> io::Result<()> {
    let letter = drive_letter(root)?;
    let name = wide(&format!("{letter}:"));
    let flags = if forget { CONNECT_UPDATE_PROFILE } else { 0 };
    // SAFETY: `name` is NUL-terminated.
    let result = unsafe { WNetCancelConnection2W(name.as_ptr(), flags, 0) };
    win32_result(result)
}

/// Whether `X:` is a connection restored at logon (`HKCU\Network\X`, FAR's `is_persistent_connection`).
pub fn is_persistent_network_drive(root: &str) -> bool {
    let Ok(letter) = drive_letter(root) else {
        return false;
    };
    let key_name = wide(&format!(r"Network\{letter}"));
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `key_name` is NUL-terminated; `key` is a valid out pointer.
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key_name.as_ptr(),
            0,
            KEY_READ,
            &raw mut key,
        )
    };
    if opened != NO_ERROR {
        return false;
    }
    // SAFETY: `key` was opened above and is closed once.
    unsafe { RegCloseKey(key) };
    true
}

/// The user name a remembered network drive connects as (`HKCU\Network\X\UserName`, FAR's
/// `GetStoredUserName`); none for the current user's own.
fn stored_user_name(letter: char) -> Option<String> {
    let key = format!(r"Network\{letter}");
    let name = super::registry::string(HKEY_CURRENT_USER, &key, "UserName")?;
    (!name.is_empty()).then_some(name)
}

/// Whether a connection was refused for its name or password (FAR's loop asking them again).
pub fn credentials_refused(error: &io::Error) -> bool {
    error.raw_os_error().is_some_and(|code| {
        [
            ERROR_ACCESS_DENIED,
            ERROR_INVALID_PASSWORD,
            ERROR_LOGON_FAILURE,
        ]
        .iter()
        .any(|refused| i32::try_from(*refused) == Ok(code))
    })
}

/// Connects the remembered network drive `X:` to `share` again (FAR's `ConnectToNetworkResource`):
/// without `login` as the user it was saved with (another session to the server with other names: as
/// that one), else with that name and password. A refusal of them is the caller's to ask about
/// (`credentials_refused`).
pub fn connect_network_drive(
    root: &str,
    share: &str,
    login: Option<(&str, &str)>,
) -> io::Result<()> {
    let letter = drive_letter(root)?;
    let mut local = wide(&format!("{letter}:"));
    let mut remote = wide(share.trim_end_matches('\\'));
    let resource = NETRESOURCEW {
        dwType: RESOURCETYPE_DISK,
        lpLocalName: local.as_mut_ptr(),
        lpRemoteName: remote.as_mut_ptr(),
        ..Default::default()
    };
    let connect = |password: Option<&str>, user: Option<&str>| {
        let password = password.map(wide);
        let user = user.filter(|user| !user.is_empty()).map(wide);
        let text =
            |text: &Option<Vec<u16>>| text.as_ref().map_or(std::ptr::null(), |text| text.as_ptr());
        // SAFETY: `resource` points at the NUL-terminated `local` and `remote`, the password and the
        // user name are NUL-terminated or null (the saved ones), all alive for the call.
        unsafe { WNetAddConnection2W(&raw const resource, text(&password), text(&user), 0) }
    };
    if let Some((user, password)) = login {
        return win32_result(connect(Some(password), Some(user)));
    }
    let stored = stored_user_name(letter);
    match connect(None, stored.as_deref()) {
        ERROR_SESSION_CREDENTIAL_CONFLICT => win32_result(connect(None, None)),
        result => win32_result(result),
    }
}

/// Deletes the SUBST drive `X:` (`DefineDosDevice(DDD_REMOVE_DEFINITION)`). Refused for a letter that
/// is not a SUBST drive: the call would take whatever the letter stands for.
pub fn remove_subst(root: &str) -> io::Result<()> {
    let letter = drive_letter(root)?;
    if super::subst_target(root).is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{letter}: is not a substituted drive"),
        ));
    }
    let name = wide(&format!("{letter}:"));
    // SAFETY: `name` is NUL-terminated; the target may be null when removing.
    let ok = unsafe { DefineDosDeviceW(DDD_REMOVE_DEFINITION, name.as_ptr(), std::ptr::null()) };
    bool_result(ok)
}

/// FAR's `EjectVolume`: locks the volume, flushes it, allows removal and ejects the media. A CD drive is
/// opened for reading, a removable one for writing when it may be.
pub fn eject_media(root: &str, removable: bool) -> io::Result<()> {
    let letter = drive_letter(root)?;
    let (device, writable) = if removable {
        match open_volume(letter, GENERIC_READ | GENERIC_WRITE) {
            Ok(device) => (device, true),
            Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED.cast_signed()) => {
                (open_volume(letter, GENERIC_READ)?, false)
            }
            Err(error) => return Err(error),
        }
    } else {
        (open_volume(letter, GENERIC_READ)?, false)
    };
    control(&device, FSCTL_LOCK_VOLUME, &[], &mut [])?;
    let result = eject_locked(&device, writable);
    // The volume stays locked until the handle closes anyway; unlocking is tidier.
    let _ = control(&device, FSCTL_UNLOCK_VOLUME, &[], &mut []);
    result
}

fn eject_locked(device: &OwnedHandle, writable: bool) -> io::Result<()> {
    if writable {
        // SAFETY: `device` is an open handle. A failed flush is not a reason to keep the media.
        let _ = unsafe { FlushFileBuffers(raw(device)) };
    }
    let allow = PREVENT_MEDIA_REMOVAL {
        PreventMediaRemoval: false,
    };
    control(
        device,
        IOCTL_STORAGE_MEDIA_REMOVAL,
        bytes_of(&allow),
        &mut [],
    )?;
    control(device, IOCTL_STORAGE_EJECT_MEDIA, &[], &mut [])
}

/// Closes the tray of a CD drive, loads the media of a removable one (FAR's `LoadVolume`).
pub fn load_media(root: &str) -> io::Result<()> {
    let device = open_volume(drive_letter(root)?, GENERIC_READ)?;
    control(&device, IOCTL_STORAGE_LOAD_MEDIA, &[], &mut [])
}

/// Whether a removable drive's media can be ejected (FAR's `IsEjectableMedia`: the drive reports
/// `RemovableMedia`, as a card reader does; a floppy reports its own media types and does not qualify).
pub fn is_ejectable_media(root: &str) -> bool {
    let Ok(device) = drive_letter(root).and_then(|letter| open_volume(letter, 0)) else {
        return false;
    };
    let mut geometry = [0u8; size_of::<DISK_GEOMETRY>()];
    if control(&device, IOCTL_DISK_GET_DRIVE_GEOMETRY, &[], &mut geometry).is_err() {
        return false;
    }
    // SAFETY: the buffer holds a DISK_GEOMETRY the call filled; read unaligned.
    let geometry: DISK_GEOMETRY =
        unsafe { geometry.as_ptr().cast::<DISK_GEOMETRY>().read_unaligned() };
    geometry.MediaType == RemovableMedia
}

/// Shift+F6 in the drive menu: the volume's new label (empty removes it).
pub fn set_volume_label(root: &str, label: &str) -> io::Result<()> {
    let (root, label) = (wide(root), wide(label));
    // SAFETY: both strings are NUL-terminated.
    let ok = unsafe { SetVolumeLabelW(root.as_ptr(), label.as_ptr()) };
    bool_result(ok)
}

/// `\\.\X:`, the volume as a device.
fn open_volume(letter: char, access: u32) -> io::Result<OwnedHandle> {
    open_device(&wide(&format!(r"\\.\{letter}:")), access)
}

/// A device by its NUL-terminated path, opened for `access` (0: for queries only).
pub(super) fn open_device(name: &[u16], access: u32) -> io::Result<OwnedHandle> {
    // SAFETY: `name` is NUL-terminated; no security attributes or template.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `handle` is a new handle owned by nothing else.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
}

/// `DeviceIoControl` with an input and an output buffer (either may be empty).
pub(super) fn control(
    device: &OwnedHandle,
    code: u32,
    input: &[u8],
    output: &mut [u8],
) -> io::Result<()> {
    let mut returned = 0u32;
    let input_at = if input.is_empty() {
        std::ptr::null()
    } else {
        input.as_ptr()
    };
    let output_length = output.len();
    let output_at = if output.is_empty() {
        std::ptr::null_mut()
    } else {
        output.as_mut_ptr()
    };
    // SAFETY: `device` is open; the buffers are valid for the lengths given (null when empty); the call
    // is synchronous (no OVERLAPPED).
    let ok = unsafe {
        DeviceIoControl(
            raw(device),
            code,
            input_at.cast::<c_void>(),
            u32::try_from(input.len()).unwrap_or(0),
            output_at.cast::<c_void>(),
            u32::try_from(output_length).unwrap_or(0),
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    bool_result(ok)
}

fn raw(device: &OwnedHandle) -> HANDLE {
    use std::os::windows::io::AsRawHandle;
    device.as_raw_handle() as HANDLE
}

fn bytes_of<T>(value: &T) -> &[u8] {
    // SAFETY: any initialised value may be read as bytes; the slice borrows `value`.
    unsafe { std::slice::from_raw_parts(std::ptr::from_ref(value).cast::<u8>(), size_of::<T>()) }
}

fn win32_result(result: u32) -> io::Result<()> {
    if result == NO_ERROR {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(
            i32::try_from(result).unwrap_or(i32::MAX),
        ))
    }
}

fn bool_result(ok: windows_sys::core::BOOL) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_disk_is_not_ejectable_media_and_a_missing_drive_fails_cleanly() {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        assert!(!is_ejectable_media(&format!("{drive}\\")));
        let mounted = super::super::drive::logical_drives();
        let free = (3..26u8)
            .find(|bit| mounted & (1 << bit) == 0)
            .map(|bit| format!("{}:\\", char::from(b'A' + bit)))
            .unwrap();
        assert!(load_media(&free).is_err());
        assert!(remove_subst(&free).is_err());
        assert!(!is_persistent_network_drive(&free));
        let no_letter = r"\\?\Volume{0}\";
        assert_eq!(
            remove_subst(no_letter).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
}
