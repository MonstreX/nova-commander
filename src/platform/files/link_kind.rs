//! What kind of link an entry is (`far/flink.cpp`): a symbolic link, a junction, or on Windows another
//! reparse point, which FAR shows by its tag's name.

/// The kind of a link or other special entry ([`super::Attributes::is_link`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// A symbolic link.
    Symlink,
    /// A junction or a volume mount point (Windows).
    Junction,
    /// Another Windows reparse point, by its tag: a cloud file, a deduplicated file, a WSL link, ...
    Reparse(u32),
}

/// `IO_REPARSE_TAG_MOUNT_POINT`: a junction or a volume mount point.
const TAG_MOUNT_POINT: u32 = 0xA000_0003;
/// `IO_REPARSE_TAG_SYMLINK`.
const TAG_SYMLINK: u32 = 0xA000_000C;
/// `IO_REPARSE_TAG_DEDUP`: a deduplicated file, which still has its size.
const TAG_DEDUP: u32 = 0x8000_0013;

/// Microsoft's reparse tags by the names FAR shows for them (`reparse_tag_to_string`).
const TAG_NAMES: [(u32, &str); 52] = [
    (TAG_MOUNT_POINT, "MOUNT_POINT"),
    (0xC000_0004, "HSM"),
    (0x8000_0005, "DRIVE_EXTENDER"),
    (0x8000_0006, "HSM2"),
    (0x8000_0007, "SIS"),
    (0x8000_0008, "WIM"),
    (0x8000_0009, "CSV"),
    (0x8000_000A, "DFS"),
    (0x8000_000B, "FILTER_MANAGER"),
    (TAG_SYMLINK, "SYMLINK"),
    (0xA000_0010, "IIS_CACHE"),
    (0x8000_0012, "DFSR"),
    (TAG_DEDUP, "DEDUP"),
    (0xC000_0014, "APPXSTRM"),
    (0x8000_0014, "NFS"),
    (0x8000_0015, "FILE_PLACEHOLDER"),
    (0x8000_0016, "DFM"),
    (0x8000_0017, "WOF"),
    (0x8000_0018, "WCI"),
    (0x9000_1018, "WCI_1"),
    (0xA000_0019, "GLOBAL_REPARSE"),
    (0x9000_001A, "CLOUD"),
    (0x9000_101A, "CLOUD_1"),
    (0x9000_201A, "CLOUD_2"),
    (0x9000_301A, "CLOUD_3"),
    (0x9000_401A, "CLOUD_4"),
    (0x9000_501A, "CLOUD_5"),
    (0x9000_601A, "CLOUD_6"),
    (0x9000_701A, "CLOUD_7"),
    (0x9000_801A, "CLOUD_8"),
    (0x9000_901A, "CLOUD_9"),
    (0x9000_A01A, "CLOUD_A"),
    (0x9000_B01A, "CLOUD_B"),
    (0x9000_C01A, "CLOUD_C"),
    (0x9000_D01A, "CLOUD_D"),
    (0x9000_E01A, "CLOUD_E"),
    (0x9000_F01A, "CLOUD_F"),
    (0x8000_001B, "APPEXECLINK"),
    (0x9000_001C, "PROJFS"),
    (0xA000_001D, "LX_SYMLINK"),
    (0x8000_001E, "STORAGE_SYNC"),
    (0xA000_001F, "WCI_TOMBSTONE"),
    (0x8000_0020, "UNHANDLED"),
    (0x8000_0021, "ONEDRIVE"),
    (0xA000_0022, "PROJFS_TOMBSTONE"),
    (0x8000_0023, "AF_UNIX"),
    (0x8000_0024, "LX_FIFO"),
    (0x8000_0025, "LX_CHR"),
    (0x8000_0026, "LX_BLK"),
    (0xA000_0027, "WCI_LINK"),
    (0xA000_1027, "WCI_LINK_1"),
    (0xA000_0028, "DATALESS_CIM"),
];

impl LinkKind {
    /// From a Windows reparse tag.
    pub fn from_reparse_tag(tag: u32) -> Self {
        match tag {
            TAG_SYMLINK => Self::Symlink,
            TAG_MOUNT_POINT => Self::Junction,
            other => Self::Reparse(other),
        }
    }

    /// Whether the entry stands for another name (`IsReparseTagNameSurrogate`: symbolic links,
    /// junctions, WSL links), not for a file kept elsewhere (cloud, deduplication).
    pub fn points_elsewhere(self) -> bool {
        match self {
            Self::Symlink | Self::Junction => true,
            Self::Reparse(tag) => tag & 0x2000_0000 != 0,
        }
    }

    /// Whether the entry still has a size of its own to show (a deduplicated file).
    pub fn has_own_size(self) -> bool {
        self == Self::Reparse(TAG_DEDUP)
    }

    /// The name FAR shows for a reparse tag it has no label of its own for; `None` for unknown tags.
    pub fn name(self) -> Option<&'static str> {
        let tag = match self {
            Self::Symlink => TAG_SYMLINK,
            Self::Junction => TAG_MOUNT_POINT,
            Self::Reparse(tag) => tag,
        };
        TAG_NAMES
            .iter()
            .find(|(known, _)| *known == tag)
            .map(|(_, name)| *name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_have_far_names() {
        assert_eq!(LinkKind::Symlink.name(), Some("SYMLINK"));
        assert_eq!(
            LinkKind::from_reparse_tag(0x9000_701A).name(),
            Some("CLOUD_7")
        );
        assert_eq!(LinkKind::Reparse(0x1234).name(), None);
        assert_eq!(
            LinkKind::from_reparse_tag(TAG_MOUNT_POINT),
            LinkKind::Junction
        );
        assert!(LinkKind::Symlink.points_elsewhere() && LinkKind::Junction.points_elsewhere());
        let dedup = LinkKind::from_reparse_tag(TAG_DEDUP);
        assert!(!dedup.points_elsewhere() && !LinkKind::Reparse(0x9000_101A).points_elsewhere());
        assert!(dedup.has_own_size() && !LinkKind::Symlink.has_own_size());
    }
}
