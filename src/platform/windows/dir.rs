//! A directory read as FAR reads it (`platform.fs.cpp`, `enum_files`): one pass of
//! `FILE_ID_BOTH_DIR_INFO` records over a handle to it, 64 KiB at a time, each record with everything a
//! panel shows (attributes, size, allocated size, the four times, a link's tag): no call per entry, no
//! second pass.

use std::ffi::OsString;
use std::io;
use std::mem::offset_of;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use super::locale::from_file_time;
use crate::platform::files::{Attributes, DirectoryRecord, LinkKind};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_BOTH_DIR_INFO,
    FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, GetFileInformationByHandleEx,
    OPEN_EXISTING,
};

/// 64 KiB, 8-byte aligned: the records inside are 8-byte aligned.
const BUFFER_WORDS: usize = 8 * 1024;

/// Every entry of `dir` but `.` and `..`, in the order the file system gives them, handed to `found`;
/// stops with `Interrupted` once `cancel` is set.
pub fn read_directory(
    dir: &Path,
    cancel: &AtomicBool,
    found: &mut dyn FnMut(DirectoryRecord),
) -> io::Result<()> {
    let handle = DirectoryHandle::open(dir)?;
    let mut buffer = vec![0u64; BUFFER_WORDS];
    let mut class = FileIdBothDirectoryRestartInfo;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let byte_count = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
        // SAFETY: `handle` is an open directory handle; the buffer is writable for `byte_count` bytes
        // and stays alive for the call.
        let ok = unsafe {
            GetFileInformationByHandleEx(handle.0, class, buffer.as_mut_ptr().cast(), byte_count)
        } != 0;
        if !ok {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == i32::try_from(ERROR_NO_MORE_FILES).ok() {
                return Ok(());
            }
            return Err(error);
        }
        // SAFETY: the u64 buffer is reinterpreted as the bytes it consists of; same length, same
        // lifetime, u8 has no alignment or validity requirements.
        let bytes =
            unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), buffer.len() * 8) };
        parse_records(bytes, found);
        class = FileIdBothDirectoryInfo;
    }
}

/// Walks the `FILE_ID_BOTH_DIR_INFO` records of one buffer with bounds-checked reads.
fn parse_records(bytes: &[u8], found: &mut dyn FnMut(DirectoryRecord)) {
    let mut offset = 0;
    loop {
        let Some(record) = bytes.get(offset..) else {
            return;
        };
        let (Some(next), Some(made)) = (read_u32(record, 0), record_of(record)) else {
            return;
        };
        if made.name != "." && made.name != ".." {
            found(made);
        }
        if next == 0 {
            return;
        }
        offset += next as usize;
    }
}

/// One record's entry; `None` when it does not fit in the buffer.
fn record_of(record: &[u8]) -> Option<DirectoryRecord> {
    let field = |at: usize| read_i64(record, at);
    let time = |at: usize| field(at).map(from_file_time);
    let bits = read_u32(record, offset_of!(FILE_ID_BOTH_DIR_INFO, FileAttributes))?;
    Some(DirectoryRecord {
        name: record_name(record)?,
        attributes: Attributes::from_windows_bits(bits),
        size: u64::try_from(field(offset_of!(FILE_ID_BOTH_DIR_INFO, EndOfFile))?).unwrap_or(0),
        allocated: Some(
            u64::try_from(field(offset_of!(FILE_ID_BOTH_DIR_INFO, AllocationSize))?).unwrap_or(0),
        ),
        created: time(offset_of!(FILE_ID_BOTH_DIR_INFO, CreationTime))?,
        accessed: time(offset_of!(FILE_ID_BOTH_DIR_INFO, LastAccessTime))?,
        modified: time(offset_of!(FILE_ID_BOTH_DIR_INFO, LastWriteTime))?,
        changed: time(offset_of!(FILE_ID_BOTH_DIR_INFO, ChangeTime))?,
        link: link_kind(record, bits),
    })
}

/// For a reparse point the record's `EaSize` holds its tag (FAR reads the same from `FindFirstFile`'s
/// `dwReserved0`); `None` for everything else and for a tag that could not be read.
fn link_kind(record: &[u8], attributes: u32) -> Option<LinkKind> {
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        return None;
    }
    read_u32(record, offset_of!(FILE_ID_BOTH_DIR_INFO, EaSize))
        .filter(|tag| *tag != 0)
        .map(LinkKind::from_reparse_tag)
}

fn record_name(record: &[u8]) -> Option<OsString> {
    let length = read_u32(record, offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength))? as usize;
    let start = offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
    let raw = record.get(start..start + length)?;
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    Some(OsString::from_wide(&units))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn read_i64(bytes: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// A directory opened for listing; closed on drop.
struct DirectoryHandle(HANDLE);

impl DirectoryHandle {
    fn open(dir: &Path) -> io::Result<Self> {
        let wide = super::wide_path(dir);
        // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call; the other arguments are
        // plain flags and null pointers allowed by the API.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }
}

impl Drop for DirectoryHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by CreateFileW and is closed exactly once, here. A failed close
        // leaves nothing to recover.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
#[path = "dir_tests.rs"]
mod tests;
