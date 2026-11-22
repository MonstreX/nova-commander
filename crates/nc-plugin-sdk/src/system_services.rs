//! The services running and the processes they run in (`EnumServicesStatusExW` with
//! `SC_ENUM_PROCESS_INFO`): a service host's services by name, as Task Manager names a `svchost.exe`
//! ("Service Host: <service>").

use std::collections::HashMap;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, GetLastError};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW, OpenSCManagerW,
    SC_ENUM_PROCESS_INFO, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_ACTIVE, SERVICE_WIN32,
};

/// A running service's names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    pub display_name: String,
}

/// The service manager opened; closed when dropped.
struct Manager(windows_sys::Win32::System::Services::SC_HANDLE);

impl Drop for Manager {
    fn drop(&mut self) {
        // SAFETY: the handle is this value's own.
        unsafe { CloseServiceHandle(self.0) };
    }
}

/// A text the system wrote into the buffer: up to its NUL.
fn text(at: *const u16) -> String {
    if at.is_null() {
        return String::new();
    }
    let mut length = 0usize;
    // SAFETY: the system's text in the enumeration's buffer, NUL-terminated, alive while it is.
    while unsafe { *at.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: `length` units were just read.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(at, length) })
}

/// Every running Win32 service by the process it runs in; empty when the manager cannot be asked.
pub fn services_by_process() -> HashMap<u32, Vec<Service>> {
    // SAFETY: the local computer's active database; no pointers but nulls.
    let raw = unsafe {
        OpenSCManagerW(
            std::ptr::null(),
            std::ptr::null(),
            SC_MANAGER_ENUMERATE_SERVICE,
        )
    };
    if raw.is_null() {
        return HashMap::new();
    }
    let manager = Manager(raw);
    let mut buffer = vec![0u64; 32 * 1024];
    let mut services = HashMap::new();
    let mut resume = 0u32;
    loop {
        let size = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
        let (mut needed, mut count) = (0u32, 0u32);
        // SAFETY: the manager is open; the buffer is writable for `size` bytes and aligned for the
        // entries; the counts go to locals.
        let done = unsafe {
            EnumServicesStatusExW(
                manager.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_ACTIVE,
                buffer.as_mut_ptr().cast(),
                size,
                &raw mut needed,
                &raw mut count,
                &raw mut resume,
                std::ptr::null(),
            )
        } != 0;
        // SAFETY: no arguments; the thread's last error of the call just made, before anything else.
        let error = unsafe { GetLastError() };
        let entries = buffer.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>();
        for index in 0..usize::try_from(count).unwrap_or(0) {
            // SAFETY: the system wrote `count` entries at the buffer's start, their texts after them.
            let entry = unsafe { &*entries.add(index) };
            let service = Service {
                name: text(entry.lpServiceName),
                display_name: text(entry.lpDisplayName),
            };
            (services.entry(entry.ServiceStatusProcess.dwProcessId))
                .or_insert_with(Vec::new)
                .push(service);
        }
        // More come with the resume point (`ERROR_MORE_DATA`); a buffer too small for one grows. Any
        // other failure, or none written and no room more wanted, ends it.
        if done || error != ERROR_MORE_DATA {
            break;
        }
        let wanted = usize::try_from(needed).unwrap_or(0).div_ceil(8);
        if wanted > buffer.len() {
            buffer.resize(wanted, 0);
        } else if count == 0 {
            break;
        }
    }
    services
}

#[cfg(test)]
#[path = "system_services_tests.rs"]
mod tests;
