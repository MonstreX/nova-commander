//! What the info panel (Ctrl+L) shows, as FAR's `InfoList::DisplayObject` (`far/infolist.cpp`) reads
//! it: the system, and the disk a folder is on - its kind, file system, label, serial number, size and
//! free space for the user, the SUBST folder, share or image file it stands for, and the device it is on
//! (ours, owner 2026-11-09). One call reads it all, off the UI thread (ADR-0012 §4).

use std::path::Path;

use super::can_be_virtual;
use super::drives::DriveType;
use crate::platform;
use crate::platform::system::SystemFacts;
use crate::platform::volumes::{DriveKind, StorageDevice, VolumeFacts};

/// Everything the info panel shows for the folder of the other panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub system: SystemFacts,
    pub disk: DiskInfo,
}

/// FAR's disk section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskInfo {
    /// `C:\`, a mount point folder, `\\server\share\`.
    pub root: String,
    /// FAR's `MInfo*` kind; `None`: unknown.
    pub kind: Option<DriveType>,
    /// The label, file system and serial; `None` when the volume cannot be read (FAR then titles the
    /// section with the root alone).
    pub volume: Option<VolumeFacts>,
    /// Total and available bytes, as the user may use them.
    pub size: Option<(u64, u64)>,
    /// The SUBST folder, the share or the image file the disk stands for.
    pub associated: Option<String>,
    pub device: Option<StorageDevice>,
}

/// What the info panel's settings change in what is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InfoOptions {
    pub names: platform::system::NameFormats,
    /// FAR's `ShowCDInfo` ("Show CD drive parameters"): a CD drive by what it can do (asked of the
    /// device), else "CD-ROM".
    pub show_cd_info: bool,
}

impl Default for InfoOptions {
    fn default() -> Self {
        Self {
            names: platform::system::NameFormats::default(),
            show_cd_info: true,
        }
    }
}

/// Reads the system and the disk of `folder`.
pub fn read_info(folder: &Path, options: InfoOptions) -> Info {
    Info {
        system: platform::system::system_facts(options.names),
        disk: disk_info(folder, options.show_cd_info),
    }
}

/// FAR's disk section for `folder`.
pub fn disk_info(folder: &Path, show_cd_info: bool) -> DiskInfo {
    let root = root_of(folder);
    let drive_kind = platform::volumes::drive_kind(&root);
    let volume = platform::volumes::volume_facts(&root);
    let subst = platform::volumes::subst_target(&root);
    let image = (subst.is_none() && can_be_virtual(drive_kind))
        .then(|| platform::volumes::virtual_disk_file(&root))
        .flatten();
    let network = (drive_kind == DriveKind::Remote && subst.is_none())
        .then(|| platform::volumes::network_name(&root))
        .flatten();
    let kind = if subst.is_some() {
        Some(DriveType::Subst)
    } else if image.is_some() {
        Some(DriveType::Virtual)
    } else {
        base_kind(drive_kind, &root, show_cd_info)
    };
    let device = (drive_kind != DriveKind::Remote && subst.is_none())
        .then(|| platform::volumes::storage_device(&root))
        .flatten();
    DiskInfo {
        size: platform::volumes::disk_size(&folder.to_string_lossy()),
        associated: subst.or(image).or(network),
        root,
        kind,
        volume,
        device,
    }
}

/// FAR's kinds by the drive's type; a CD drive by what it can do with `show_cd_info` (FAR's `ShowCDInfo`,
/// on by default).
fn base_kind(kind: DriveKind, root: &str, show_cd_info: bool) -> Option<DriveType> {
    match kind {
        DriveKind::Removable => Some(DriveType::Removable),
        DriveKind::Fixed => Some(DriveType::Fixed),
        DriveKind::Remote => Some(DriveType::Network),
        DriveKind::CdRom if !show_cd_info => Some(DriveType::Cd(platform::volumes::CdType::CdRom)),
        DriveKind::CdRom => Some(DriveType::Cd(platform::volumes::cd_type(root))),
        DriveKind::RamDisk => Some(DriveType::RamDisk),
        DriveKind::Unknown | DriveKind::NoRoot => None,
    }
}

/// The root FAR describes: of a link's target when the folder is a link (a junction to another disk),
/// a network drive's own root (not its share: FAR's `extract_root_directory`), else the volume's root
/// (a mount point folder included).
fn root_of(folder: &Path) -> String {
    let target = platform::files::link_target(folder).ok();
    let place = target.as_deref().unwrap_or(folder);
    let drive_root = place.ancestors().last().unwrap_or(place);
    if platform::volumes::drive_kind(&drive_root.to_string_lossy()) == DriveKind::Remote {
        return drive_root.to_string_lossy().into_owned();
    }
    platform::volumes::volume_of(place)
        .to_string_lossy()
        .into_owned()
}

/// Windows only: the stubs of other systems give no disk data yet.
#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_folder_on_the_system_disk_is_described() {
        let temp = tempfile::tempdir().unwrap();
        let info = read_info(temp.path(), InfoOptions::default());
        let disk = info.disk;
        assert_eq!(
            disk.root.to_lowercase(),
            temp.path()
                .ancestors()
                .last()
                .unwrap()
                .to_string_lossy()
                .to_lowercase()
        );
        assert_eq!(disk.kind, Some(DriveType::Fixed));
        let volume = disk.volume.unwrap();
        assert!(!volume.file_system.is_empty());
        let (total, available) = disk.size.unwrap();
        assert!(total > 0 && available <= total);
        assert_eq!(disk.associated, None);
        assert!(!info.system.user_name.is_empty());
    }

    #[test]
    fn a_folder_on_no_disk_has_only_its_root() {
        let disk = disk_info(Path::new(r"Q:\nothing"), true);
        assert_eq!(disk.root.to_lowercase(), r"q:\");
        assert_eq!((disk.kind, disk.volume, disk.size), (None, None, None));
    }
}
