//! A DLL loaded by its path (`LoadLibraryExW`): its own folder searched first for what it needs, then the
//! system's (`LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`), never the current
//! folder.

use std::ffi::{CString, c_void};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    LoadLibraryExW,
};

/// A loaded library; dropping it unloads it.
#[derive(Debug)]
pub struct Library(HMODULE);

impl Library {
    /// Loads the library at `path` (an absolute path).
    pub fn open(path: &Path) -> io::Result<Self> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let flags = LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS;
        // SAFETY: `wide` is NUL-terminated and outlives the call; no file handle is passed.
        let module = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), flags) };
        if module.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(module))
    }

    /// The address of the function `name` exports; none when it exports no such name.
    pub fn symbol(&self, name: &str) -> Option<*const c_void> {
        let name = CString::new(name).ok()?;
        // SAFETY: the module is loaded while `self` lives; `name` is NUL-terminated.
        let address = unsafe { GetProcAddress(self.0, name.as_ptr().cast()) };
        address.map(|function| function as *const c_void)
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: the module was loaded by `open` and is freed once, here.
        unsafe {
            FreeLibrary(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_library_is_loaded_and_its_functions_found() {
        let system = crate::platform::process::system_folder();
        let library = Library::open(&system.join("System32").join("kernel32.dll")).unwrap();
        assert!(library.symbol("GetTickCount").is_some());
        assert!(library.symbol("NoSuchFunctionHere").is_none());
        assert!(Library::open(Path::new(r"C:\no\such\library.dll")).is_err());
    }

    #[test]
    fn a_name_with_a_nul_finds_nothing() {
        let system = crate::platform::process::system_folder();
        let library = Library::open(&system.join("System32").join("kernel32.dll")).unwrap();
        assert!(library.symbol("GetTickCount\0").is_none());
        assert!(library.symbol("GetTick\0Count").is_none());
        assert!(library.symbol("").is_none());
    }
}
