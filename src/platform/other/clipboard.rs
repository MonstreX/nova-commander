//! The clipboard's stubs.

use super::unsupported;
use std::io;
use std::path::PathBuf;

pub fn clipboard_text() -> io::Result<Option<String>> {
    unsupported()
}

pub fn set_clipboard_text(_text: &str) -> io::Result<()> {
    unsupported()
}

pub fn set_clipboard_column_text(_text: &str) -> io::Result<()> {
    unsupported()
}

pub fn clipboard_is_column() -> io::Result<bool> {
    unsupported()
}

pub fn set_clipboard_files(_paths: &[PathBuf], _cut: bool) -> io::Result<()> {
    unsupported()
}
