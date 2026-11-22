//! One change "Set" of the "Attributes" dialog makes to one object (ADR-0011 §1: each setter does one
//! thing), as a value: the task makes it itself, or the elevated helper makes it for it (ADR-0007).

use std::io;
use std::path::Path;

use super::{Attributes, FileTimes};

/// One change to one object, in FAR's order (`process_single_file`, after `ModifyReparsePoint`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Setting {
    /// A symbolic link or a junction leads to this target from now on (FAR's `ModifyReparsePoint`).
    LinkTarget(String),
    Owner(String),
    Compressed(bool),
    Encrypted(bool),
    Sparse(bool),
    RemoveReparsePoint,
    Attributes(Attributes),
    Times(FileTimes),
}

impl Setting {
    pub fn apply(&self, path: &Path) -> io::Result<()> {
        match self {
            Self::LinkTarget(target) => super::modify_link(path, target),
            Self::Owner(name) => super::set_owner(path, name),
            Self::Compressed(on) => super::set_compressed(path, *on),
            Self::Encrypted(on) => super::set_encrypted(path, *on),
            Self::Sparse(on) => super::set_sparse(path, *on),
            Self::RemoveReparsePoint => super::remove_reparse_point(path),
            Self::Attributes(attributes) => super::set_attributes(path, *attributes),
            Self::Times(times) => super::set_file_times(path, times),
        }
    }
}
