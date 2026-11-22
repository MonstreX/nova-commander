//! A program file's resources off Windows: none to read or change (the calls fail as unsupported).

use std::io;
use std::path::Path;

pub const RT_ICON: u16 = 3;
pub const RT_GROUP_ICON: u16 = 14;
pub const RT_VERSION: u16 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceName {
    Id(u16),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceChange {
    pub kind: u16,
    pub name: ResourceName,
    pub language: u16,
    pub data: Option<Vec<u8>>,
}

fn unsupported() -> io::Error {
    io::Error::from(io::ErrorKind::Unsupported)
}

pub fn resources(_path: &Path, _kind: u16) -> io::Result<Vec<(ResourceName, u16)>> {
    Err(unsupported())
}

pub fn resource(
    _path: &Path,
    _kind: u16,
    _name: &ResourceName,
    _language: u16,
) -> io::Result<Vec<u8>> {
    Err(unsupported())
}

pub fn update_resources(_path: &Path, _changes: &[ResourceChange]) -> io::Result<()> {
    Err(unsupported())
}
