//! One entry of a folder, whatever provider listed it (ADR-0013).

use std::time::SystemTime;

use crate::platform::files::{Attributes, LinkKind};

/// What is known of a folder's size (FAR's `size_state`), which F3 puts in `Entry::size`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FolderSize {
    #[default]
    Unknown,
    /// Being counted: the size so far.
    Counting,
    Precise,
    /// Counted before the folder was last read.
    Outdated,
}

/// One row of a file panel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub folder_size: FolderSize,
    /// Space the entry takes on disk (FAR's "Allocated" column), when the OS reports it.
    pub allocated: Option<u64>,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    /// Change time of data or attributes, when the OS reports it.
    pub changed: Option<SystemTime>,
    pub attributes: Attributes,
    /// Index in the order the file system listed the directory (FAR's "Unsorted").
    pub position: usize,
    /// The synthetic ".." entry that leads to the parent directory.
    pub is_parent: bool,
    /// The kind of a link (symbolic link, junction) or other special entry; `None` otherwise.
    pub link: Option<LinkKind>,
    /// What a link points to, as FAR shows it (`C:\dir`, or relative as it was made). Not read with the
    /// listing (one more file system call per link, for folders full of junctions): the status line
    /// reads it for the entry under the cursor. `None` when not read or not readable.
    pub link_target: Option<String>,
    /// The file on a disk a provider's entry is (a plugin's real names, FAR's `PFLAGS_REALNAMES`):
    /// viewed, edited, run, copied and moved as that file; `None` for a disk's own entries.
    pub real_path: Option<std::path::PathBuf>,
    /// A provider's texts for its modes' columns (a plugin's `C0`..`C99` and `Z`); boxed, as most entries
    /// have none.
    pub texts: Option<Box<EntryTexts>>,
}

/// What a provider gives an entry for its own columns (FAR's `CustomColumnData` and `Description`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EntryTexts {
    /// `C0`, `C1`...
    pub columns: Vec<String>,
    /// `Z`.
    pub description: String,
}

impl EntryTexts {
    /// The texts, `None` when there are none.
    pub fn of(columns: Vec<String>, description: String) -> Option<Box<Self>> {
        (!columns.is_empty() || !description.is_empty()).then(|| {
            Box::new(Self {
                columns,
                description,
            })
        })
    }
}

impl Entry {
    /// The ".." entry that leads to the folder above.
    pub fn parent() -> Self {
        Self {
            name: "..".to_string(),
            is_dir: true,
            is_parent: true,
            ..Self::default()
        }
    }

    /// Lower-case extension without the dot; empty for directories and names without one.
    #[cfg(test)]
    pub fn extension(&self) -> String {
        if self.is_dir {
            return String::new();
        }
        match self.name.rsplit_once('.') {
            Some((stem, extension)) if !stem.is_empty() => extension.to_lowercase(),
            _ => String::new(),
        }
    }

    /// A link that stands for another name (symbolic link, junction), not a file kept elsewhere.
    pub fn points_elsewhere(&self) -> bool {
        self.link.is_some_and(LinkKind::points_elsewhere)
    }
}
