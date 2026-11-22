//! The system's calls FAR's plugins make, as safe wrappers (owner, ADR-0016 addendum P6.0e): the plugins
//! hold no `unsafe`, the ABI does not grow for them.

use std::io;
use std::path::Path;

#[cfg(windows)]
#[path = "system_links.rs"]
mod links;

#[cfg(windows)]
pub use links::{junction, mount_volume};

#[cfg(windows)]
#[path = "system_job.rs"]
mod job;

#[cfg(windows)]
pub use job::Job;

#[cfg(windows)]
#[path = "system_shell.rs"]
mod shell;

#[cfg(windows)]
pub use shell::{GuiAt, Shell, ShellItem, Strings, keys_held, mouse_cell};

#[cfg(windows)]
#[path = "system_process.rs"]
mod process;

#[cfg(windows)]
pub use process::{ProcessRecord, logical_processors, processes};

#[cfg(windows)]
#[path = "system_process_open.rs"]
mod process_open;

#[cfg(windows)]
pub use process_open::{
    KillError, Owner, PRIORITY_CLASSES, ProcessDetails, process_details, step_priority, terminate,
};

#[cfg(windows)]
#[path = "system_process_memory.rs"]
mod process_memory;

#[cfg(windows)]
pub use process_memory::{Module, ProcessMemory, process_memory, process_modules};

#[cfg(windows)]
#[path = "system_handles.rs"]
mod handles;

#[cfg(windows)]
pub use handles::{HandleInfo, HandleName, process_handles};

/// The network's resources, as FAR's Network plugin browses them.
#[cfg(windows)]
#[path = "system_network.rs"]
pub mod network;

#[cfg(windows)]
#[path = "system_services.rs"]
mod services;

#[cfg(windows)]
#[path = "system_waited.rs"]
mod waited;

#[cfg(windows)]
pub use waited::{Waited, keep_library_loaded, waited};

#[cfg(windows)]
pub use services::{Service, services_by_process};

#[cfg(windows)]
#[path = "system_time.rs"]
mod time;

#[cfg(not(windows))]
#[path = "system_time_other.rs"]
mod time;

pub use time::{LocalTime, dos_time_of, dos_time_ticks, local_time, now_ticks};

/// Secrets kept for the user by the system (Windows' DPAPI; unsupported elsewhere yet).
#[cfg(windows)]
#[path = "system_secret.rs"]
mod secret;

#[cfg(not(windows))]
#[path = "system_secret_other.rs"]
mod secret;

pub use secret::{protect, unprotect};

#[path = "system_files.rs"]
mod files;

pub use files::{SetTimes, clear_archive, set_attributes, set_times};

#[cfg(windows)]
#[path = "system_windows.rs"]
mod windows;

#[cfg(windows)]
pub use windows::{Window, top_windows};

#[cfg(windows)]
#[path = "system_version.rs"]
mod version;

#[cfg(windows)]
pub use version::{FileVersion, file_version, fixed_version};

#[cfg(windows)]
#[path = "system_resources.rs"]
mod resources;

#[cfg(not(windows))]
#[path = "system_resources_other.rs"]
mod resources;

pub use resources::{
    RT_GROUP_ICON, RT_ICON, RT_VERSION, ResourceChange, ResourceName, resource, resources,
    update_resources,
};

/// The program registered under `name` in Windows' `App Paths` (the current user's first, then the
/// machine's: `SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\<name>`, its default value); none
/// when neither has it.
#[cfg(windows)]
pub fn app_path(name: &str) -> Option<String> {
    let key = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{name}");
    registry_texts(&key, None).into_iter().next()
}

/// The text value `value` (the key's default one for none) of `key`: the current user's, then the
/// machine's, those that have it.
#[cfg(windows)]
pub(crate) fn registry_texts(key: &str, value: Option<&str>) -> Vec<String> {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegGetValueW,
    };
    let key: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
    let value: Option<Vec<u16>> =
        value.map(|value| value.encode_utf16().chain(std::iter::once(0)).collect());
    let value_ptr = value
        .as_ref()
        .map_or(std::ptr::null(), |value| value.as_ptr());
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .filter_map(|root| {
            let mut buffer = vec![0u16; 1024];
            // A longer value: once more with the size the system said (`ERROR_MORE_DATA`).
            for _ in 0..2 {
                let mut size = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
                // SAFETY: the key and the value's name (or null: the default one) are NUL-terminated;
                // the buffer and its size in bytes are given and written back.
                let read = unsafe {
                    RegGetValueW(
                        root,
                        key.as_ptr(),
                        value_ptr,
                        RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
                        std::ptr::null_mut(),
                        buffer.as_mut_ptr().cast(),
                        &raw mut size,
                    )
                };
                let units = usize::try_from(size / 2).unwrap_or(0);
                match read {
                    0 => {
                        let text = String::from_utf16_lossy(&buffer[..units.min(buffer.len())]);
                        return Some(text.trim_end_matches(char::from(0)).to_string());
                    }
                    MORE_DATA => buffer = vec![0u16; units + 1],
                    _ => return None,
                }
            }
            None
        })
        .collect()
}

/// `ERROR_MORE_DATA`.
#[cfg(windows)]
const MORE_DATA: u32 = 234;

/// `bytes` in the system's ANSI code page (`console`: the OEM code page, a console's own unless set
/// otherwise: a program's output, FAR's `GetConsoleOutputCP` of the console it shares) as text.
#[cfg(windows)]
pub fn from_code_page(bytes: &[u8], console: bool) -> String {
    use windows_sys::Win32::Globalization::{CP_ACP, CP_OEMCP, MultiByteToWideChar};
    if bytes.is_empty() {
        return String::new();
    }
    let page = if console { CP_OEMCP } else { CP_ACP };
    let length = i32::try_from(bytes.len()).unwrap_or(i32::MAX);
    // SAFETY: the bytes and their length; first the size asked, then a buffer of that size written.
    unsafe {
        let units = MultiByteToWideChar(page, 0, bytes.as_ptr(), length, std::ptr::null_mut(), 0);
        let mut text = vec![0u16; usize::try_from(units).unwrap_or(0)];
        let written =
            MultiByteToWideChar(page, 0, bytes.as_ptr(), length, text.as_mut_ptr(), units);
        text.truncate(usize::try_from(written).unwrap_or(0));
        String::from_utf16_lossy(&text)
    }
}

/// `bytes` as text where no code page is known: Latin-1.
#[cfg(not(windows))]
pub fn from_code_page(bytes: &[u8], _console: bool) -> String {
    bytes.iter().map(|&byte| char::from(byte)).collect()
}

/// `from` renamed to `to`, never over another file (FAR's `MoveFile`: Windows' `MoveFileExW` without
/// `MOVEFILE_REPLACE_EXISTING`). A name changing only its case is the same file where names ignore case;
/// in a folder that tells case apart another file of that name stays and the rename fails.
#[cfg(windows)]
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide = |path: &Path| -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both are NUL-terminated wide strings alive through the call; no flags.
    let done = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0)
    };
    if done == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// `from` renamed to `to`, never over another file (one of another name there: refused).
#[cfg(not(windows))]
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    let same = |a: &Path, b: &Path| {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::symlink_metadata(a), std::fs::symlink_metadata(b)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    };
    if std::fs::symlink_metadata(to).is_ok() && !same(from, to) {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    std::fs::rename(from, to)
}

/// The user's decimal separator (`LOCALE_SDECIMAL`), as FAR's `FormatFileSize` writes a size's fraction;
/// `.` when the system tells none.
#[cfg(windows)]
pub fn decimal_separator() -> char {
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SDECIMAL};
    let mut buffer = [0u16; 8];
    let capacity = i32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: the user's locale (null); the buffer holds `capacity` units.
    let written = unsafe {
        GetLocaleInfoEx(
            std::ptr::null(),
            LOCALE_SDECIMAL,
            buffer.as_mut_ptr(),
            capacity,
        )
    };
    let length = usize::try_from(written).unwrap_or(0).saturating_sub(1);
    char::decode_utf16(buffer[..length.min(buffer.len())].iter().copied())
        .next()
        .and_then(Result::ok)
        .unwrap_or('.')
}

#[cfg(not(windows))]
pub fn decimal_separator() -> char {
    '.'
}

/// Up to `buffer.len()` bytes of `file` from `offset`, its own position untouched where the system
/// allows (Windows' `ReadFile` with an offset moves it): several readers of one file each keep theirs.
#[cfg(windows)]
pub fn read_at(file: &std::fs::File, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buffer, offset)
}

#[cfg(unix)]
pub fn read_at(file: &std::fs::File, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buffer, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two places of one file read in turns, each from where it was asked.
    #[test]
    fn a_file_is_read_at_an_offset() {
        let temp = std::env::temp_dir().join(format!("nc-sdk-read-at-{}", std::process::id()));
        std::fs::write(&temp, b"0123456789").unwrap();
        let file = std::fs::File::open(&temp).unwrap();
        let mut buffer = [0u8; 3];
        assert_eq!(read_at(&file, 7, &mut buffer).unwrap(), 3);
        assert_eq!(&buffer, b"789");
        assert_eq!(read_at(&file, 2, &mut buffer).unwrap(), 3);
        assert_eq!(&buffer, b"234");
        assert_eq!(read_at(&file, 10, &mut buffer).unwrap(), 0, "past the end");
        drop(file);
        std::fs::remove_file(&temp).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_name_nothing_registered_has_no_app_path() {
        assert_eq!(app_path("no-such-program-vc-sdk.exe"), None);
    }

    #[test]
    fn a_rename_never_goes_over_another_file() {
        let temp = std::env::temp_dir().join(format!("nc-sdk-rename-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let (a, b) = (temp.join("a.txt"), temp.join("b.txt"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        assert!(rename_no_replace(&a, &b).is_err(), "refused");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "b", "the other kept");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "a");
        let upper = temp.join("A.TXT");
        rename_no_replace(&a, &upper).unwrap();
        let names: Vec<String> = (std::fs::read_dir(&temp).unwrap().flatten())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&"A.TXT".to_string()),
            "its case changed: {names:?}"
        );
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
