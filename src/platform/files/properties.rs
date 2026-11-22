//! What Ctrl+A shows and changes of one file (ADR-0011, FAR's `far/setattr.cpp`): its attributes, its four
//! times, its owner, what kind of link it is and where it leads, its other names (hard links). Each
//! setter does one thing; the order of changes is the caller's (FAR's `process_single_file`).

use super::{Attributes, FileTimes, LinkKind};

/// One file as the "Attributes" dialog reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProperties {
    pub attributes: Attributes,
    /// Write, creation, access and change; one the system does not keep is `None`.
    pub times: FileTimes,
    /// `DOMAIN\name` (the SID's text when no account has it); `None` when it cannot be read.
    pub owner: Option<String>,
    pub link: Option<LinkFacts>,
    /// Every name of a file that has more than one (hard links), full paths; empty otherwise.
    pub names: Vec<String>,
    /// How many names the file has (1 for most).
    pub name_count: u32,
}

/// A link, a volume mount point or another reparse point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkFacts {
    pub kind: LinkKind,
    /// Where it leads, in FAR's form (`C:\x`; a volume's `\?\Volume{...}\`); empty when the system
    /// does not say (an unknown reparse point).
    pub target: String,
    /// A volume mounted there (FAR's "Volume:"), also a drive's root.
    pub volume: bool,
    /// Whether the system said where it leads (FAR's `KnownReparseTag`): an unknown reparse point is
    /// shown by its tag's name.
    pub known: bool,
    /// A symbolic link or a junction: its target may be changed in the dialog (FAR's
    /// `IsEditableReparsePoint`).
    pub editable: bool,
}
