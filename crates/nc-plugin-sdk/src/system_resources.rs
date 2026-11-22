//! A program file's resources read and changed as FAR's arclite does it for an SFX module (rsrc.cpp's
//! `RsrcModule`, `enum_rsrc_names`, `enum_rsrc_langs`, `load_icon_rsrc`, `ResourceUpdate`): the file
//! loaded as data, its resources of a type listed by name and language, one read, and a set of changes
//! written in one update (nothing written when one fails).

use std::io;
use std::path::Path;

use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    BeginUpdateResourceW, EndUpdateResourceW, EnumResourceLanguagesW, EnumResourceNamesW,
    FindResourceExW, LOAD_LIBRARY_AS_DATAFILE, LoadLibraryExW, LoadResource, LockResource,
    SizeofResource, UpdateResourceW,
};
use windows_sys::core::{BOOL, PCWSTR};

/// The system's `RT_ICON`: one image of an icon.
pub const RT_ICON: u16 = 3;
/// The system's `RT_GROUP_ICON`: an icon's directory of its images.
pub const RT_GROUP_ICON: u16 = 14;
/// The system's `RT_VERSION`: the version information.
pub const RT_VERSION: u16 = 16;

/// A resource's name: a number (the system's `MAKEINTRESOURCE`) or a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceName {
    Id(u16),
    Text(String),
}

/// One change of a resource: its data put in, or the resource deleted (no data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceChange {
    pub kind: u16,
    pub name: ResourceName,
    pub language: u16,
    pub data: Option<Vec<u8>>,
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain([0])
        .collect()
}

/// A name as the system takes it: the number in the pointer's place, else the text's NUL-terminated
/// units (kept in `text` while the pointer is used).
fn name_pointer(name: &ResourceName, text: &mut Vec<u16>) -> PCWSTR {
    match name {
        ResourceName::Id(id) => usize::from(*id) as PCWSTR,
        ResourceName::Text(name) => {
            *text = name.encode_utf16().chain([0]).collect();
            text.as_ptr()
        }
    }
}

/// A name the system gave: a number when its pointer's high bits are none (`IS_INTRESOURCE`).
///
/// # Safety
/// `name` must be a number in the pointer's place or a NUL-terminated text.
unsafe fn name_of(name: PCWSTR) -> ResourceName {
    let value = name as usize;
    if value >> 16 == 0 {
        return ResourceName::Id(u16::try_from(value).unwrap_or(0));
    }
    let mut length = 0;
    // SAFETY: the caller's NUL-terminated text, read up to its NUL.
    while unsafe { *name.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: `length` units read above.
    let units = unsafe { std::slice::from_raw_parts(name, length) };
    ResourceName::Text(String::from_utf16_lossy(units))
}

/// A file loaded as data (arclite's `RsrcModule`), freed when dropped.
struct DataFile(HMODULE);

impl DataFile {
    fn open(path: &Path) -> io::Result<Self> {
        let wide = wide(path);
        // SAFETY: a NUL-terminated path; no file handle; the handle is freed by `Drop`.
        let handle = unsafe {
            LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_AS_DATAFILE,
            )
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }
}

impl Drop for DataFile {
    fn drop(&mut self) {
        // SAFETY: the handle `LoadLibraryExW` gave, freed once.
        unsafe { FreeLibrary(self.0) };
    }
}

unsafe extern "system" fn each_name(_: HMODULE, _: PCWSTR, name: PCWSTR, found: isize) -> BOOL {
    // SAFETY: `found` is the list `names_of` passed, alive through the enumeration; the system's name.
    unsafe { (*(found as *mut Vec<ResourceName>)).push(name_of(name)) };
    1
}

unsafe extern "system" fn each_language(
    _: HMODULE,
    _: PCWSTR,
    _: PCWSTR,
    language: u16,
    found: isize,
) -> BOOL {
    // SAFETY: `found` is the list `languages_of` passed, alive through the enumeration.
    unsafe { (*(found as *mut Vec<u16>)).push(language) };
    1
}

fn names_of(file: &DataFile, kind: u16) -> Vec<ResourceName> {
    let mut names: Vec<ResourceName> = Vec::new();
    // SAFETY: the loaded file; the callback pushes into `names`, which outlives the call. A type it does
    // not have fails the call, leaving the list empty (arclite does not check it either).
    unsafe {
        EnumResourceNamesW(
            file.0,
            usize::from(kind) as PCWSTR,
            Some(each_name),
            (&raw mut names) as isize,
        )
    };
    names
}

fn languages_of(file: &DataFile, kind: u16, name: &ResourceName) -> Vec<u16> {
    let mut text = Vec::new();
    let pointer = name_pointer(name, &mut text);
    let mut languages: Vec<u16> = Vec::new();
    // SAFETY: the loaded file; the name lives in `text` (or is a number); `languages` outlives the call.
    unsafe {
        EnumResourceLanguagesW(
            file.0,
            usize::from(kind) as PCWSTR,
            pointer,
            Some(each_language),
            (&raw mut languages) as isize,
        )
    };
    languages
}

/// `path`'s resources of type `kind`: each name with each of its languages, in the system's order
/// (arclite's `enum_rsrc_names` then `enum_rsrc_langs`).
pub fn resources(path: &Path, kind: u16) -> io::Result<Vec<(ResourceName, u16)>> {
    let file = DataFile::open(path)?;
    let mut found = Vec::new();
    for name in names_of(&file, kind) {
        for language in languages_of(&file, kind, &name) {
            found.push((name.clone(), language));
        }
    }
    Ok(found)
}

/// The data of `path`'s resource `kind`, `name`, `language` (arclite's `FindResourceEx`, `LoadResource`,
/// `LockResource`).
pub fn resource(path: &Path, kind: u16, name: &ResourceName, language: u16) -> io::Result<Vec<u8>> {
    let file = DataFile::open(path)?;
    let mut text = Vec::new();
    let pointer = name_pointer(name, &mut text);
    // SAFETY: the loaded file; the name lives in `text` (or is a number).
    let found = unsafe { FindResourceExW(file.0, usize::from(kind) as PCWSTR, pointer, language) };
    if found.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the file and the resource the system found in it.
    let size = unsafe { SizeofResource(file.0, found) };
    // SAFETY: as above; a data file's resource needs no freeing.
    let loaded = unsafe { LoadResource(file.0, found) };
    // SAFETY: the loaded resource; null when it has no data.
    let data = unsafe { LockResource(loaded) };
    if loaded.is_null() || data.is_null() {
        return Err(io::Error::last_os_error());
    }
    let length = usize::try_from(size).unwrap_or(0);
    // SAFETY: the resource's `size` bytes, mapped while the file is loaded (copied before it goes).
    Ok(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length) }.to_vec())
}

/// `changes` written into `path` in one update (arclite's `ResourceUpdate`: `BeginUpdateResource`, each
/// `UpdateResource`, `EndUpdateResource`); a failure discards them all.
pub fn update_resources(path: &Path, changes: &[ResourceChange]) -> io::Result<()> {
    let wide = wide(path);
    // SAFETY: a NUL-terminated path; the update is ended below on every way out.
    let update = unsafe { BeginUpdateResourceW(wide.as_ptr(), 0) };
    if update.is_null() {
        return Err(io::Error::last_os_error());
    }
    for change in changes {
        let mut text = Vec::new();
        let pointer = name_pointer(&change.name, &mut text);
        let (data, size) = change.data.as_ref().map_or((std::ptr::null(), 0), |data| {
            (data.as_ptr(), u32::try_from(data.len()).unwrap_or(u32::MAX))
        });
        // SAFETY: the update begun above; the name lives in `text`; `size` bytes of data (none to delete).
        let done = unsafe {
            UpdateResourceW(
                update,
                usize::from(change.kind) as PCWSTR,
                pointer,
                change.language,
                data.cast(),
                size,
            )
        };
        if done == 0 {
            let error = io::Error::last_os_error();
            // SAFETY: the update begun above, discarded once.
            unsafe { EndUpdateResourceW(update, 1) };
            return Err(error);
        }
    }
    // SAFETY: the update begun above, ended once with its changes written.
    if unsafe { EndUpdateResourceW(update, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
#[path = "system_resources_tests.rs"]
mod tests;
