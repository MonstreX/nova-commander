//! The network as FAR's Network plugin browses it (`NetResourceList::Enumerate`, `GetResourceInfo`,
//! `GetResourceParent`, `GetHiddenShares`): the resources under a resource (`WNetOpenEnum`,
//! `WNetEnumResource`), one resource by its name (`WNetGetResourceInformation`), its parent
//! (`WNetGetResourceParent`), a server's shares (`NetShareEnum`), a connection made with the user's own
//! credentials (`WNetAddConnection2` asking nothing).

use std::io;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, HANDLE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::NetManagement::NetApiBufferFree;
use windows_sys::Win32::NetworkManagement::WNet::{
    NETRESOURCEW, WNetCloseEnum, WNetEnumResourceW, WNetGetResourceInformationW,
    WNetGetResourceParentW, WNetOpenEnumW,
};
use windows_sys::Win32::Storage::FileSystem::{NetShareEnum, SHARE_INFO_1};
use windows_sys::Win32::System::WindowsProgramming::GetComputerNameW;

/// `RESOURCEDISPLAYTYPE_*` (`winnetwk.h`).
pub const DISPLAY_GENERIC: u32 = 0;
pub const DISPLAY_DOMAIN: u32 = 1;
pub const DISPLAY_SERVER: u32 = 2;
pub const DISPLAY_SHARE: u32 = 3;
pub const DISPLAY_NETWORK: u32 = 6;
/// `RESOURCETYPE_*`.
pub const TYPE_ANY: u32 = 0;
pub const TYPE_DISK: u32 = 1;
pub const TYPE_PRINT: u32 = 2;
/// `RESOURCEUSAGE_CONTAINER`.
pub const USAGE_CONTAINER: u32 = 2;
/// `RESOURCE_CONNECTED`, `RESOURCE_GLOBALNET`.
pub const SCOPE_CONNECTED: u32 = 1;
pub const SCOPE_GLOBALNET: u32 = 2;
/// `STYPE_*` of a share (`lmshare.h`), its low bits.
pub const SHARE_DISK: u32 = 0;
pub const SHARE_PRINT: u32 = 1;

/// A network resource (`NETRESOURCE`) with its texts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetResource {
    pub scope: u32,
    pub kind: u32,
    pub display: u32,
    pub usage: u32,
    pub local_name: String,
    pub remote_name: String,
    pub comment: String,
    pub provider: String,
}

/// A text the system wrote: up to its NUL; none for a null pointer.
fn text(at: *const u16) -> String {
    if at.is_null() {
        return String::new();
    }
    let mut length = 0usize;
    // SAFETY: the system's NUL-terminated text, alive while its buffer is.
    while unsafe { *at.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: `length` units were just read.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(at, length) })
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

impl NetResource {
    fn of(raw: &NETRESOURCEW) -> Self {
        Self {
            scope: raw.dwScope,
            kind: raw.dwType,
            display: raw.dwDisplayType,
            usage: raw.dwUsage,
            local_name: text(raw.lpLocalName),
            remote_name: text(raw.lpRemoteName),
            comment: text(raw.lpComment),
            provider: text(raw.lpProvider),
        }
    }
}

/// A resource as the system takes it, its texts kept alive with it.
struct Raw {
    texts: [Vec<u16>; 4],
    resource: NETRESOURCEW,
}

impl Raw {
    fn new(resource: &NetResource) -> Self {
        let texts = [
            wide(&resource.local_name),
            wide(&resource.remote_name),
            wide(&resource.comment),
            wide(&resource.provider),
        ];
        let mut raw = Self {
            texts,
            resource: NETRESOURCEW {
                dwScope: resource.scope,
                dwType: resource.kind,
                dwDisplayType: resource.display,
                dwUsage: resource.usage,
                ..NETRESOURCEW::default()
            },
        };
        // An empty text is a null pointer, as the system's callers leave it.
        let pointer = |text: &mut Vec<u16>, given: &str| {
            if given.is_empty() {
                std::ptr::null_mut()
            } else {
                text.as_mut_ptr()
            }
        };
        raw.resource.lpLocalName = pointer(&mut raw.texts[0], &resource.local_name);
        raw.resource.lpRemoteName = pointer(&mut raw.texts[1], &resource.remote_name);
        raw.resource.lpComment = pointer(&mut raw.texts[2], &resource.comment);
        raw.resource.lpProvider = pointer(&mut raw.texts[3], &resource.provider);
        raw
    }
}

fn failed(code: u32) -> io::Error {
    io::Error::from_raw_os_error(i32::try_from(code).unwrap_or(i32::MAX))
}

/// An enumeration opened; closed when dropped.
struct Enumeration(HANDLE);

impl Drop for Enumeration {
    fn drop(&mut self) {
        // SAFETY: the handle is this value's own.
        unsafe { WNetCloseEnum(self.0) };
    }
}

/// The resources of `scope` and `kind` under `parent` (none: the top, the providers); FAR's
/// `NetResourceList::Enumerate`.
pub fn enumerate(
    scope: u32,
    kind: u32,
    parent: Option<&NetResource>,
) -> io::Result<Vec<NetResource>> {
    let raw = parent.map(Raw::new);
    let pointer = raw.as_ref().map_or(std::ptr::null(), |raw| &raw.resource);
    let mut handle: HANDLE = std::ptr::null_mut();
    // SAFETY: the resource (or null) and its texts outlive the call; the handle goes to a local.
    let opened = unsafe { WNetOpenEnumW(scope, kind, 0, pointer, &raw mut handle) };
    if opened != NO_ERROR {
        return Err(failed(opened));
    }
    let enumeration = Enumeration(handle);
    let mut found = Vec::new();
    let mut buffer = vec![0u64; 2048];
    loop {
        let mut count = u32::MAX;
        let mut size = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
        // SAFETY: the enumeration is open; the buffer is writable for `size` bytes and aligned for
        // the entries; the counts go to locals.
        let result = unsafe {
            WNetEnumResourceW(
                enumeration.0,
                &raw mut count,
                buffer.as_mut_ptr().cast(),
                &raw mut size,
            )
        };
        match result {
            NO_ERROR => {
                let entries = buffer.as_ptr().cast::<NETRESOURCEW>();
                for index in 0..usize::try_from(count).unwrap_or(0) {
                    // SAFETY: the system wrote `count` entries at the start, their texts after them.
                    found.push(NetResource::of(unsafe { &*entries.add(index) }));
                }
            }
            ERROR_NO_MORE_ITEMS => return Ok(found),
            // An entry bigger than the buffer: `size` is what it needs.
            ERROR_MORE_DATA => {
                let wanted = usize::try_from(size).unwrap_or(0).div_ceil(8);
                if wanted <= buffer.len() {
                    return Err(failed(result));
                }
                buffer.resize(wanted, 0);
            }
            other => return Err(failed(other)),
        }
    }
}

/// The system's answer about `resource` written into a buffer grown as it asks: its first entry.
fn asked(
    resource: &NetResource,
    call: impl Fn(&NETRESOURCEW, *mut core::ffi::c_void, &mut u32) -> u32,
) -> io::Result<NetResource> {
    let raw = Raw::new(resource);
    let mut buffer = vec![0u64; 512];
    loop {
        let mut size = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
        match call(&raw.resource, buffer.as_mut_ptr().cast(), &mut size) {
            NO_ERROR => {
                // SAFETY: the system wrote one resource at the buffer's start, its texts after it.
                let first = unsafe { &*buffer.as_ptr().cast::<NETRESOURCEW>() };
                return Ok(NetResource::of(first));
            }
            ERROR_MORE_DATA => {
                let wanted = usize::try_from(size).unwrap_or(0).div_ceil(8);
                if wanted <= buffer.len() {
                    return Err(failed(ERROR_MORE_DATA));
                }
                buffer.resize(wanted, 0);
            }
            other => return Err(failed(other)),
        }
    }
}

/// The resource named `remote` (`\\SERVER`, `\\SERVER\share`) as the network knows it (FAR's
/// `GetResourceInfo`).
pub fn resource_information(remote: &str) -> io::Result<NetResource> {
    let resource = NetResource {
        scope: SCOPE_GLOBALNET,
        kind: TYPE_ANY,
        display: DISPLAY_GENERIC,
        remote_name: remote.to_string(),
        ..NetResource::default()
    };
    asked(&resource, |raw, buffer, size| {
        let mut system: windows_sys::core::PWSTR = std::ptr::null_mut();
        // SAFETY: the resource and its texts are alive; the buffer is writable for `size` bytes.
        unsafe { WNetGetResourceInformationW(raw, buffer, size, &raw mut system) }
    })
}

/// The resource above `resource` (FAR's `GetResourceParent`: its provider asked first - here only when
/// the resource does not name one: a workgroup the network lists slowly answers that question only as
/// slowly, 154 seconds on the owner's machine, while its parent comes at once).
pub fn resource_parent(resource: &NetResource) -> io::Result<NetResource> {
    let provider = if resource.provider.is_empty() {
        resource_information(&resource.remote_name)?.provider
    } else {
        resource.provider.clone()
    };
    let asked_for = NetResource {
        scope: SCOPE_GLOBALNET,
        kind: TYPE_ANY,
        display: DISPLAY_GENERIC,
        usage: 0,
        provider,
        ..resource.clone()
    };
    asked(&asked_for, |raw, buffer, size| {
        // SAFETY: the resource and its texts are alive; the buffer is writable for `size` bytes.
        unsafe { WNetGetResourceParentW(raw, buffer, size) }
    })
}

/// A share of a server (`NetShareEnum` level 1): its name, kind (`STYPE_*`) and remark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub name: String,
    pub kind: u32,
    pub remark: String,
}

/// The shares of `server` (`\\SERVER`), the hidden ones too; empty when it cannot be asked.
pub fn server_shares(server: &str) -> Vec<Share> {
    let name = wide(server);
    let mut shares = Vec::new();
    let mut resume = 0u32;
    loop {
        let mut buffer: *mut u8 = std::ptr::null_mut();
        let (mut read, mut total) = (0u32, 0u32);
        // SAFETY: the name is NUL-terminated and alive; the buffer is the system's, freed below.
        let result = unsafe {
            NetShareEnum(
                name.as_ptr(),
                1,
                &raw mut buffer,
                u32::MAX,
                &raw mut read,
                &raw mut total,
                &raw mut resume,
            )
        };
        if result != NO_ERROR && result != ERROR_MORE_DATA {
            return shares;
        }
        if !buffer.is_null() {
            let entries = buffer.cast::<SHARE_INFO_1>().cast_const();
            for index in 0..usize::try_from(read).unwrap_or(0) {
                // SAFETY: the system wrote `read` entries into its buffer.
                let entry = unsafe { &*entries.add(index) };
                shares.push(Share {
                    name: text(entry.shi1_netname),
                    kind: entry.shi1_type & 0xFF,
                    remark: text(entry.shi1_remark),
                });
            }
            // SAFETY: the buffer is the system's own, given by the call above.
            unsafe { NetApiBufferFree(buffer.cast_const().cast()) };
        }
        if result == NO_ERROR {
            return shares;
        }
    }
}

/// This computer's NetBIOS name (`GetComputerName`).
pub fn computer_name() -> Option<String> {
    let mut buffer = [0u16; 64];
    let mut size = u32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: the buffer holds `size` units; the length goes back into it.
    if unsafe { GetComputerNameW(buffer.as_mut_ptr(), &raw mut size) } == 0 {
        return None;
    }
    let length = usize::try_from(size).unwrap_or(0).min(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

#[path = "system_network_connect.rs"]
mod connect;
pub use connect::*;

#[cfg(test)]
#[path = "system_network_tests.rs"]
mod tests;
