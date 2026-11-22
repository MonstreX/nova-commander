//! The system's processes as FAR's proclist reads them on the local computer (`PerfThread::RefreshImpl`'s
//! `NtQuerySystemInformation(SystemProcessInformation)`): one call gives every process with its times,
//! memory, handles and input/output, read here from the system's records into plain values. FAR takes
//! most of these from the performance registry as well; the system's records hold every one of them, at
//! a fraction of the time.

use std::io;

use windows_sys::Wdk::System::SystemInformation::{
    NtQuerySystemInformation, SystemProcessInformation,
};

/// One process as the system's records give it; times in 100 ns units (its start since 1601).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessRecord {
    pub pid: u32,
    /// The process that started it (FAR's `Creating Process ID`).
    pub parent: u32,
    /// Its image's name (`ImageName`); empty for the idle process.
    pub name: String,
    pub threads: u32,
    pub base_priority: i32,
    pub handles: u32,
    pub session: u32,
    pub created: i64,
    pub user_time: u64,
    pub kernel_time: u64,
    pub virtual_bytes: u64,
    pub virtual_bytes_peak: u64,
    pub page_faults: u64,
    pub working_set: u64,
    pub working_set_peak: u64,
    pub working_set_private: u64,
    pub paged_pool: u64,
    pub nonpaged_pool: u64,
    pub page_file: u64,
    pub page_file_peak: u64,
    pub private_bytes: u64,
    pub read_operations: u64,
    pub write_operations: u64,
    pub other_operations: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub other_bytes: u64,
}

/// The size of the system's record before its threads' (64-bit `SYSTEM_PROCESS_INFORMATION`).
const RECORD: usize = 256;
/// The buffer is too small (`STATUS_INFO_LENGTH_MISMATCH`, `STATUS_BUFFER_OVERFLOW`,
/// `STATUS_BUFFER_TOO_SMALL`).
const TOO_SMALL: [u32; 3] = [0xC000_0004, 0x8000_0005, 0xC000_0023];

const _: () = assert!(
    size_of::<usize>() == 8,
    "the records are read as a 64-bit process sees them"
);

/// Every process running now.
pub fn processes() -> io::Result<Vec<ProcessRecord>> {
    let buffer = snapshot()?;
    let bytes = as_bytes(&buffer);
    let base = bytes.as_ptr() as usize;
    let mut records = Vec::new();
    let mut at = 0usize;
    while let Some(record) = bytes.get(at..at + RECORD) {
        records.push(parse(record, bytes, base));
        let next = usize::try_from(u32_at(record, 0)).unwrap_or(0);
        if next == 0 {
            break;
        }
        at += next;
    }
    Ok(records)
}

/// The system's records, in a buffer grown until they fit.
fn snapshot() -> io::Result<Vec<u64>> {
    let mut words = 64 * 1024;
    loop {
        let mut buffer = vec![0u64; words];
        let size = u32::try_from(words * 8).unwrap_or(u32::MAX);
        let mut needed = 0u32;
        // SAFETY: the buffer is writable for `size` bytes and 8-byte aligned; the size needed is
        // written to a local.
        let status = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                buffer.as_mut_ptr().cast(),
                size,
                &raw mut needed,
            )
        };
        // NTSTATUS is a signed 32-bit code; its bits are compared.
        let code = status.cast_unsigned();
        if status >= 0 {
            return Ok(buffer);
        }
        if !TOO_SMALL.contains(&code) || words >= 64 * 1024 * 1024 {
            return Err(io::Error::other(format!("NTSTATUS 0x{code:08X}")));
        }
        // More processes may start before the next call: some room over what was said.
        let said = usize::try_from(needed).unwrap_or(0) / 8;
        words = said.max(words * 2) + 8 * 1024;
    }
}

/// The buffer's bytes.
fn as_bytes(buffer: &[u64]) -> &[u8] {
    // SAFETY: the words are plain integers; their bytes are as long as they are and live as long.
    unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), buffer.len() * 8) }
}

fn u32_at(record: &[u8], at: usize) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&record[at..at + 4]);
    u32::from_le_bytes(bytes)
}

fn u64_at(record: &[u8], at: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&record[at..at + 8]);
    u64::from_le_bytes(bytes)
}

/// The record's image name: its `UNICODE_STRING` points into the same buffer (`base` its address), else
/// nothing is read.
fn image_name(record: &[u8], bytes: &[u8], base: usize) -> String {
    let length = usize::from(u16::from_le_bytes([record[56], record[57]]));
    let address = usize::try_from(u64_at(record, 64)).unwrap_or(0);
    let Some(start) = address.checked_sub(base) else {
        return String::new();
    };
    let Some(text) = bytes.get(start..start.saturating_add(length)) else {
        return String::new();
    };
    let units: Vec<u16> = (text.as_chunks::<2>().0.iter())
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// One record read (the offsets of the 64-bit `SYSTEM_PROCESS_INFORMATION`).
fn parse(record: &[u8], bytes: &[u8], base: usize) -> ProcessRecord {
    let word = |at| u64_at(record, at);
    let id = |at| u32::try_from(word(at)).unwrap_or(u32::MAX);
    ProcessRecord {
        threads: u32_at(record, 4),
        working_set_private: word(8),
        created: word(32).cast_signed(),
        user_time: word(40),
        kernel_time: word(48),
        name: image_name(record, bytes, base),
        base_priority: u32_at(record, 72).cast_signed(),
        pid: id(80),
        parent: id(88),
        handles: u32_at(record, 96),
        session: u32_at(record, 100),
        virtual_bytes_peak: word(112),
        virtual_bytes: word(120),
        page_faults: u64::from(u32_at(record, 128)),
        working_set_peak: word(136),
        working_set: word(144),
        paged_pool: word(160),
        nonpaged_pool: word(176),
        page_file: word(184),
        page_file_peak: word(192),
        private_bytes: word(200),
        read_operations: word(208),
        write_operations: word(216),
        other_operations: word(224),
        read_bytes: word(232),
        write_bytes: word(240),
        other_bytes: word(248),
    }
}

/// The logical processors of every group (FAR's `get_logical_processor_count`): what a process's
/// processor time is shared by.
pub fn logical_processors() -> usize {
    use windows_sys::Win32::System::Threading::{ALL_PROCESSOR_GROUPS, GetActiveProcessorCount};
    // SAFETY: no pointers; the count of every group's active processors.
    let count = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) };
    usize::try_from(count).unwrap_or(1).max(1)
}

#[cfg(test)]
#[path = "system_process_tests.rs"]
mod tests;
