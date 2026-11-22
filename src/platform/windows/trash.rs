//! The Recycle Bin, as FAR's `os::fs::move_to_recycle_bin`: `SHFileOperationW` deleting with undo and no
//! UI of its own, one object a call; the shell's own error codes mapped to Windows' as FAR maps them.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_BAD_PATHNAME, ERROR_BUFFER_OVERFLOW,
    ERROR_CANCELLED, ERROR_DISK_FULL, ERROR_GEN_FAILURE, ERROR_INVALID_PARAMETER,
    ERROR_NOT_SAME_DEVICE, ERROR_PATH_NOT_FOUND, ERROR_READ_FAULT, ERROR_WRITE_FAULT,
};
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NO_UI, SHFILEOPSTRUCTW, SHFileOperationW,
};

/// No dialogs, questions or progress of the shell's own; undo, which is what puts it into the bin. The
/// structure's field is 16 bits wide; the check below keeps the cast from dropping a flag.
const FLAGS: u16 = (FOF_NO_UI | FOF_ALLOWUNDO) as u16;
const _: () = assert!(FOF_NO_UI | FOF_ALLOWUNDO <= u16::MAX as u32);

/// Moves a file or a whole folder to the Recycle Bin. The shell deletes for good what its Recycle Bin
/// cannot take (too big, a drive without one) without asking, as it does for FAR: whether a drive has
/// a Recycle Bin is asked before (`vfs` capabilities), and of the first item the shell itself
/// (`recycle_probe`, FAR's `can_recycle`).
pub fn move_to_trash(path: &Path) -> io::Result<()> {
    // A relative name would be taken against the process's folder, not the caller's.
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a full path", path.display()),
        ));
    }
    // The shell takes a list of names, each NUL-terminated, the list ending with one more NUL; it does
    // not take the `\\?\` form. A NUL inside would end the name early: the folder before it would go
    // (R7 review A2).
    let name: Vec<u16> = path.as_os_str().encode_wide().collect();
    if name.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a name with a NUL character",
        ));
    }
    let names: Vec<u16> = name.into_iter().chain([0, 0]).collect();
    let mut operation = SHFILEOPSTRUCTW {
        hwnd: std::ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: names.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: FLAGS,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };
    // SAFETY: `operation` is fully initialised; `pFrom` points to the double NUL-terminated list that
    // outlives the call; no window, no target, no name mappings asked for.
    let result = unsafe { SHFileOperationW(&raw mut operation) };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(windows_error(result)));
    }
    if operation.fAnyOperationsAborted != 0 {
        return Err(io::Error::from_raw_os_error(code(ERROR_CANCELLED)));
    }
    Ok(())
}

/// FAR's `SHErrorToWinError`: the shell's pre-Win32 codes that overlap Windows' with another meaning.
fn windows_error(shell: i32) -> i32 {
    let mapped = match shell {
        0x71 | 0x7E | 0x80 => ERROR_ALREADY_EXISTS,
        0x72 | 0x74 | 0x7A | 0x7D | 0x10074 => ERROR_INVALID_PARAMETER,
        0x73 => ERROR_NOT_SAME_DEVICE,
        0x75 => ERROR_CANCELLED,
        0x76 | 0x7C => ERROR_BAD_PATHNAME,
        0x78 => ERROR_ACCESS_DENIED,
        0x79 | 0x81 | 0xB7 => ERROR_BUFFER_OVERFLOW,
        0x82..=0x84 => ERROR_WRITE_FAULT,
        0x85 => ERROR_DISK_FULL,
        0x86..=0x88 => ERROR_READ_FAULT,
        0x402 => ERROR_PATH_NOT_FOUND,
        0x10000 => ERROR_GEN_FAILURE,
        other => return other,
    };
    code(mapped)
}

fn code(error: u32) -> i32 {
    i32::try_from(error).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_s_own_codes_become_windows_errors_as_far_maps_them() {
        assert_eq!(windows_error(0x78), code(ERROR_ACCESS_DENIED));
        assert_eq!(windows_error(0x7C), code(ERROR_BAD_PATHNAME));
        assert_eq!(windows_error(0x85), code(ERROR_DISK_FULL));
        // A Windows error the shell passes on stays as it is.
        assert_eq!(windows_error(5), 5);
    }

    #[test]
    fn a_relative_name_is_refused() {
        let error = move_to_trash(Path::new("relative.txt")).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn a_missing_object_is_an_error_and_nothing_else_is_touched() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("kept.txt"), b"x").unwrap();
        assert!(move_to_trash(&temp.path().join("missing.txt")).is_err());
        assert!(temp.path().join("kept.txt").exists());
    }
}
