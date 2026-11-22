//! Ctrl+A's model on the other systems (ADR-0011): the Unix one (mode, owner and group, times) comes with
//! the port; until then every call says it is not available.

use std::io;
use std::path::Path;

use super::unsupported;
use crate::platform::files::{Attributes, FileProperties, FileTimes};

/// The Unix model of ADR-0011 (mode, owner and group, times) comes with the port.
pub fn file_properties(_path: &Path) -> io::Result<FileProperties> {
    unsupported()
}

pub fn set_attributes(_path: &Path, _attributes: Attributes) -> io::Result<()> {
    unsupported()
}

pub fn set_compressed(_path: &Path, _on: bool) -> io::Result<()> {
    unsupported()
}

pub fn set_encrypted(_path: &Path, _on: bool) -> io::Result<()> {
    unsupported()
}

pub fn set_sparse(_path: &Path, _on: bool) -> io::Result<()> {
    unsupported()
}

pub fn remove_reparse_point(_path: &Path) -> io::Result<()> {
    unsupported()
}

pub fn set_file_times(_path: &Path, _times: &FileTimes) -> io::Result<()> {
    unsupported()
}

pub fn set_owner(_path: &Path, _name: &str) -> io::Result<()> {
    unsupported()
}

pub fn modify_link(_link: &Path, _target: &str) -> io::Result<()> {
    unsupported()
}

/// Windows' properties window; a desktop's file manager on the port, maybe.
pub fn show_properties(_path: &Path, _folder: bool) -> io::Result<()> {
    unsupported()
}
