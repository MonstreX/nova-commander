//! Libraries are not loaded on other systems yet (`dlopen` when they are ported).

use std::ffi::c_void;
use std::io;
use std::path::Path;

#[derive(Debug)]
pub struct Library;

impl Library {
    pub fn open(_path: &Path) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "plugins are not loaded on this system yet",
        ))
    }

    pub fn symbol(&self, _name: &str) -> Option<*const c_void> {
        None
    }
}
