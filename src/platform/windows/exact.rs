//! Names exactly as they are on disk. Win32 drops a space or a dot at the end of a path's last name
//! (`a ` is `a`), so a call with such a name reaches a sibling the user did not choose; the `\\?\` form
//! turns that off (FAR's `nt_path` in its delete and attribute calls). Also the reparse tag of one
//! object, for a link's kind outside a directory listing.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf, Prefix};

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FindClose, FindFirstFileW,
    FindNextFileW, WIN32_FIND_DATAW,
};

/// `path` in the `\\?\` form (`\\?\C:\a `, `\\?\UNC\server\share\a `), so every name is taken as it is.
/// Built from the text, never through `GetFullPathNameW` (which drops those ends too); a path that is
/// not absolute, has `.` or `..` in it or is in that form already stays as it is.
pub fn exact_path(path: &Path) -> PathBuf {
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return path.to_path_buf();
    };
    // `components` hides a `.` in the middle; the text shows it.
    let dotted = path
        .to_string_lossy()
        .split(['\\', '/'])
        .any(|name| name == "." || name == "..");
    if dotted || !path.has_root() {
        return path.to_path_buf();
    }
    let text: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .map(|unit| {
            if unit == u16::from(b'/') {
                u16::from(b'\\')
            } else {
                unit
            }
        })
        .collect();
    let encode = |text: &str| -> Vec<u16> { text.encode_utf16().collect() };
    let exact = match prefix.kind() {
        Prefix::Disk(_) => [encode(r"\\?\"), text].concat(),
        // `\\server\share` becomes `\\?\UNC\server\share`: one of the two backslashes stays.
        Prefix::UNC(..) => [encode(r"\\?\UNC"), text[1..].to_vec()].concat(),
        _ => return path.to_path_buf(),
    };
    PathBuf::from(OsString::from_wide(&exact))
}

/// Whether `name` is one name in a folder: nothing the OS would read as a path or as a file's stream.
pub fn is_plain_name(name: &str) -> bool {
    !matches!(name, "" | "." | "..") && !name.contains(['\\', '/', ':', '\0'])
}

/// The reparse tag of the object at `path` (what kind of link it is), `None` when it is no reparse
/// point or cannot be read; the object itself, not what it points to.
pub fn reparse_tag(path: &Path) -> Option<u32> {
    let name: Vec<u16> = exact_path(path)
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect();
    // SAFETY: WIN32_FIND_DATAW is plain data for which all zeroes is a valid value.
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    // SAFETY: `name` is NUL-terminated and outlives the call; `data` is a valid out pointer.
    let handle = unsafe { FindFirstFileW(name.as_ptr(), &raw mut data) };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: the handle came from FindFirstFileW and is closed once, here.
    unsafe {
        FindClose(handle);
    }
    (data.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0).then_some(data.dwReserved0)
}

/// The name of the first file in `folder` that `pattern` (wildcards `*` and `?`, as the system matches
/// them) finds, folders passed over (FAR's `get_find_data` of a folder description file).
pub fn first_file_like(folder: &Path, pattern: &str) -> Option<OsString> {
    let name: Vec<u16> = exact_path(&folder.join(pattern))
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect();
    // SAFETY: WIN32_FIND_DATAW is plain data for which all zeroes is a valid value.
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    // SAFETY: `name` is NUL-terminated and outlives the call; `data` is a valid out pointer.
    let handle = unsafe { FindFirstFileW(name.as_ptr(), &raw mut data) };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    let found = loop {
        if data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
            let len = data.cFileName.iter().position(|&unit| unit == 0);
            let len = len.unwrap_or(data.cFileName.len());
            break Some(OsString::from_wide(&data.cFileName[..len]));
        }
        // SAFETY: the handle is open; `data` is a valid out pointer.
        if unsafe { FindNextFileW(handle, &raw mut data) } == 0 {
            break None;
        }
    };
    // SAFETY: the handle came from FindFirstFileW and is closed once, here.
    unsafe {
        FindClose(handle);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_paths_get_the_exact_form_and_others_stay() {
        assert_eq!(exact_path(Path::new(r"C:\a\b ")), Path::new(r"\\?\C:\a\b "));
        assert_eq!(exact_path(Path::new("C:/a/b.")), Path::new(r"\\?\C:\a\b."));
        assert_eq!(
            exact_path(Path::new(r"\\server\share\x ")),
            Path::new(r"\\?\UNC\server\share\x ")
        );
        for unchanged in [r"relative ", r"C:\a\..\b", r"C:\a\.\b", r"\\?\C:\a", r"C:a"] {
            assert_eq!(exact_path(Path::new(unchanged)), Path::new(unchanged));
        }
    }

    #[test]
    fn a_junction_has_the_mount_point_tag_and_a_folder_none() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = temp.path().join("link");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap()
            .status;
        assert!(status.success());
        assert_eq!(reparse_tag(&link), Some(0xA000_0003));
        assert_eq!(reparse_tag(&target), None);
        assert_eq!(reparse_tag(&temp.path().join("missing")), None);
    }
}
