//! A saved file put in place of the old one keeping what the old one carried besides its content
//! (`ReplaceFileW`: its ACL and owner, attributes, creation time, named streams, short name, object id),
//! and the count of a file's names (hard links), for which a saved file is written in place instead.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::ERROR_UNABLE_TO_MOVE_REPLACEMENT;

use crate::platform::files::Replaced;
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, GetFileInformationByHandle,
    REPLACEFILE_IGNORE_ACL_ERRORS, REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW,
};

/// `replacement` (a file in the same folder) put in place of `original`, which keeps its metadata.
pub fn replace_file(original: &Path, replacement: &Path) -> Result<(), Replaced> {
    let wide = |path: &Path| -> Vec<u16> { path.as_os_str().encode_wide().chain([0]).collect() };
    let (original, replacement) = (wide(original), wide(replacement));
    // SAFETY: both names are NUL-terminated and outlive the call; no backup name, no reserved pointers.
    let ok = unsafe {
        ReplaceFileW(
            original.as_ptr(),
            replacement.as_ptr(),
            std::ptr::null(),
            // FAR's flags (`file_io.cpp:145`): a merge of attributes or ACL that cannot be done does
            // not fail the replacement (the rest is still kept).
            REPLACEFILE_IGNORE_MERGE_ERRORS | REPLACEFILE_IGNORE_ACL_ERRORS,
            std::ptr::null(),
            std::ptr::null(),
        )
    } != 0;
    if ok {
        return Ok(());
    }
    match io::Error::last_os_error().raw_os_error()
        == i32::try_from(ERROR_UNABLE_TO_MOVE_REPLACEMENT).ok()
    {
        true => Err(Replaced::Moved),
        false => Err(Replaced::Untouched),
    }
}

/// How many names (hard links) the file at `path` has.
pub fn name_count(path: &Path) -> io::Result<u32> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    // No data access: opens files other programs hold.
    let file = std::fs::OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the handle is open for the call; `info` is a valid out pointer.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info.nNumberOfLinks)
}
