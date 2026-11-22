//! The drives FAR's drive menu lists (`far/diskmenu.cpp`, `ChangeDiskMenu`): every letter in use and
//! every remembered network drive, then, on request, the volumes with no letter. What is asked of each
//! drive follows the menu's columns, so a hidden column costs no disk access.

use crate::platform;
use crate::platform::volumes::{CdType, DriveKind, VolumeRoot};

/// What the drive menu shows (FAR's `DRIVE_SHOW_*` bits of `DriveMenuMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriveColumns {
    /// Ctrl+1.
    pub kind: bool,
    /// Ctrl+2: the share of a network drive, the folder of a SUBST drive.
    pub associated_path: bool,
    /// Ctrl+3: the label, and with `shell_name` the name the shell shows.
    pub label: bool,
    pub shell_name: bool,
    /// Ctrl+4.
    pub file_system: bool,
    /// Ctrl+5: the total and free sizes, in megabytes or (`size_float`) as "1.50 G".
    pub size: bool,
    pub size_float: bool,
    /// Ctrl+6 / Ctrl+8 / Ctrl+9: read the label, file system and size of these drives too.
    pub removable: bool,
    pub cd: bool,
    pub network: bool,
    /// Ctrl+7: the plugins' items under the drives; by their hotkeys or (default) by name.
    pub plugins: bool,
    pub sort_plugins_by_hotkey: bool,
    /// "Detect virtual disks": with the type or path column, an attached VHD or VHDX is named "virtual"
    /// (a mounted ISO is named as any CD drive) and the image file is the associated path of both (FAR's
    /// `DRIVE_SHOW_VIRTUAL`).
    pub virtual_disks: bool,
    /// Ctrl+H: the volumes with no letter.
    pub unmounted: bool,
}

impl Default for DriveColumns {
    /// FAR's default `DRIVE_SHOW_TYPE | DRIVE_SHOW_PLUGINS | DRIVE_SHOW_SIZE_FLOAT | DRIVE_SHOW_CDROM`,
    /// and the label (`DRIVE_SHOW_LABEL`, owner 2026-11-09: FAR leaves it off).
    fn default() -> Self {
        Self {
            kind: true,
            associated_path: false,
            label: true,
            shell_name: false,
            file_system: false,
            size: false,
            size_float: true,
            removable: false,
            cd: true,
            network: false,
            plugins: true,
            sort_plugins_by_hotkey: false,
            virtual_disks: false,
            unmounted: false,
        }
    }
}

impl DriveColumns {
    pub fn shows_size(&self) -> bool {
        self.size || self.size_float
    }
}

/// The type column's text, as FAR names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveType {
    Removable,
    Fixed,
    Network,
    /// A remembered network drive that is not connected.
    Disconnected,
    /// An optical drive: what it can do with "Show CD drive parameters" on, else CD-ROM.
    Cd(CdType),
    RamDisk,
    Subst,
    /// A VHD or VHDX image attached as a disk.
    Virtual,
}

/// One line of the drive menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    /// `C:\`, or `\\?\Volume{...}\` for a volume with no letter.
    pub root: String,
    pub letter: Option<char>,
    pub kind: DriveKind,
    /// A remembered network drive that is not connected now.
    pub disconnected: bool,
    /// The folder a SUBST drive stands for, when the type or path column asked for it.
    pub subst: Option<String>,
    /// The image file of a virtual disk, when "Detect virtual disks" and the type or path column asked
    /// for it.
    pub image: Option<String>,
    pub drive_type: Option<DriveType>,
    /// `None`: not shown or not readable (FAR's "not available" when the label was asked for).
    pub label: Option<String>,
    pub file_system: Option<String>,
    /// Total and free bytes.
    pub size: Option<(u64, u64)>,
    /// The share or the SUBST folder (Ctrl+2).
    pub associated_path: Option<String>,
    /// The label, file system and size were looked for (FAR's `ShowDiskInfo`): a label missing then
    /// is "not available".
    pub volume_read: bool,
}

/// The drives in FAR's order: letters A to Z, then the volumes with no letter.
pub fn list_drives(columns: &DriveColumns) -> Vec<Drive> {
    let mut drives: Vec<Drive> = platform::volumes::volume_roots()
        .into_iter()
        .map(|root| describe(root, columns))
        .collect();
    if columns.unmounted {
        drives.extend(
            platform::volumes::unmounted_volumes()
                .into_iter()
                .map(|path| {
                    let root = VolumeRoot {
                        path,
                        letter: None,
                        remembered: false,
                    };
                    describe(root, columns)
                }),
        );
    }
    drives
}

/// A drive as FAR names it in its messages: `C:` for a drive's root, the root itself otherwise.
pub fn drive_name(root: &str) -> String {
    let path = std::path::Path::new(root);
    match platform::files::paths::volume_letter(path) {
        Some(letter) if platform::files::paths::is_root(path) => format!("{letter}:"),
        _ => root.to_string(),
    }
}

/// FAR's `process_location`.
fn describe(volume: VolumeRoot, columns: &DriveColumns) -> Drive {
    let VolumeRoot {
        path: root,
        letter,
        remembered,
    } = volume;
    let mut kind = platform::volumes::drive_kind(&root);
    let disconnected = kind == DriveKind::NoRoot && remembered && letter.is_some();
    if disconnected {
        kind = DriveKind::Remote;
    }
    let wants_kind = columns.kind || columns.associated_path;
    let subst = wants_kind
        .then(|| platform::volumes::subst_target(&root))
        .flatten();
    // FAR's order: a SUBST drive is not looked at as an image.
    let image = (wants_kind && columns.virtual_disks && subst.is_none() && can_be_virtual(kind))
        .then(|| platform::volumes::virtual_disk_file(&root))
        .flatten();
    // FAR's `get_cdrom_type`, with the type column and Ctrl+8.
    let cd = (columns.kind && columns.cd && kind == DriveKind::CdRom)
        .then(|| platform::volumes::cd_type(&root));
    let found = Found {
        disconnected,
        subst: subst.is_some(),
        image: image.is_some(),
        cd,
    };
    let mut drive = Drive {
        drive_type: columns.kind.then(|| drive_type(kind, found)).flatten(),
        associated_path: (subst.clone())
            .or_else(|| image.clone())
            .filter(|_| columns.associated_path),
        root,
        letter,
        kind,
        disconnected,
        subst,
        image,
        label: None,
        file_system: None,
        size: None,
        volume_read: reads_volume(kind, columns),
    };
    if drive.volume_read {
        read_volume(&mut drive, columns);
    }
    // A SUBST of a share is "remote" too; its folder is already there.
    if columns.associated_path && kind == DriveKind::Remote && drive.subst.is_none() {
        drive.associated_path = platform::volumes::network_name(&drive.root);
    }
    drive
}

/// A drive kind an image can be attached as (FAR's `DriveCanBeVirtual`: VHD on a fixed drive, ISO on
/// a CD drive).
pub fn can_be_virtual(kind: DriveKind) -> bool {
    matches!(kind, DriveKind::Fixed | DriveKind::CdRom)
}

/// What was found about a drive besides its kind.
#[derive(Debug, Clone, Copy, Default)]
struct Found {
    disconnected: bool,
    subst: bool,
    image: bool,
    /// What an optical drive can do, when it was asked.
    cd: Option<CdType>,
}

/// FAR's order: a CD drive is named by what it can do (an ISO too), then SUBST, then virtual, then
/// the kind.
fn drive_type(kind: DriveKind, found: Found) -> Option<DriveType> {
    if kind == DriveKind::CdRom {
        return Some(DriveType::Cd(found.cd.unwrap_or(CdType::CdRom)));
    }
    if found.subst {
        return Some(DriveType::Subst);
    }
    if found.image {
        return Some(DriveType::Virtual);
    }
    match kind {
        DriveKind::Removable => Some(DriveType::Removable),
        DriveKind::Fixed => Some(DriveType::Fixed),
        DriveKind::Remote if found.disconnected => Some(DriveType::Disconnected),
        DriveKind::Remote => Some(DriveType::Network),
        DriveKind::RamDisk => Some(DriveType::RamDisk),
        _ => None,
    }
}

/// FAR's `ShowDiskInfo`: removable, CD and network drives are read only when their column is on.
fn reads_volume(kind: DriveKind, columns: &DriveColumns) -> bool {
    (columns.removable || kind != DriveKind::Removable)
        && (columns.cd || kind != DriveKind::CdRom)
        && (columns.network || kind != DriveKind::Remote)
}

fn read_volume(drive: &mut Drive, columns: &DriveColumns) {
    if columns.label || columns.file_system {
        if columns.label && columns.shell_name {
            drive.label = platform::volumes::shell_name(&drive.root);
        }
        let read_label = columns.label && drive.label.is_none();
        if let Some(facts) = platform::volumes::volume_facts(&drive.root) {
            if read_label {
                drive.label = Some(facts.label);
            }
            if columns.file_system {
                drive.file_system = Some(facts.file_system);
            }
        }
    }
    if columns.shows_size() {
        drive.size = platform::volumes::disk_size(&drive.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_drive() -> char {
        std::env::var("SystemDrive")
            .ok()
            .and_then(|drive| drive.chars().next())
            .unwrap_or('C')
            .to_ascii_uppercase()
    }

    #[test]
    fn the_system_drive_is_listed_with_what_the_columns_ask_for() {
        let letter = system_drive();
        let find = |columns: &DriveColumns| {
            // No SCSI commands to a real optical drive from the tests.
            list_drives(&DriveColumns {
                cd: false,
                ..*columns
            })
            .into_iter()
            .find(|drive| drive.letter == Some(letter))
            .unwrap()
        };

        let plain = find(&DriveColumns::default());
        assert_eq!(plain.root, format!("{letter}:\\"));
        assert_eq!(plain.drive_type, Some(DriveType::Fixed));
        assert!(plain.size.is_some());
        assert_eq!(plain.file_system, None, "not asked for");
        assert!(plain.label.is_some(), "the label is shown by default");

        let all = find(&DriveColumns {
            label: true,
            file_system: true,
            associated_path: true,
            ..DriveColumns::default()
        });
        assert!(all.file_system.is_some_and(|fs| !fs.is_empty()));
        assert!(all.label.is_some());
        assert_eq!(all.associated_path, None, "a local disk");

        let detected = find(&DriveColumns {
            virtual_disks: true,
            associated_path: true,
            ..DriveColumns::default()
        });
        assert_eq!(detected.image, None, "a real disk");
        assert_eq!(detected.drive_type, Some(DriveType::Fixed));

        let bare = find(&DriveColumns {
            kind: false,
            size_float: false,
            ..DriveColumns::default()
        });
        assert_eq!((bare.drive_type, bare.size), (None, None));
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_is_named_by_its_letter_and_a_volume_by_its_root() {
        assert_eq!(drive_name("D:\\"), "D:");
        let volume = "\\\\?\\Volume{0}\\";
        assert_eq!(drive_name(volume), volume);
        assert_eq!(drive_name("D:\\folder"), "D:\\folder");
    }

    #[test]
    fn only_asked_for_drive_kinds_are_read() {
        let columns = DriveColumns::default();
        assert!(reads_volume(DriveKind::Fixed, &columns));
        assert!(reads_volume(DriveKind::CdRom, &columns));
        assert!(!reads_volume(DriveKind::Removable, &columns));
        assert!(!reads_volume(DriveKind::Remote, &columns));
        let found = |disconnected, subst, image| Found {
            disconnected,
            subst,
            image,
            cd: None,
        };
        let cases = [
            (
                DriveKind::Remote,
                found(true, false, false),
                Some(DriveType::Disconnected),
            ),
            (
                DriveKind::Fixed,
                found(false, true, false),
                Some(DriveType::Subst),
            ),
            (
                DriveKind::Fixed,
                found(false, false, true),
                Some(DriveType::Virtual),
            ),
            (
                DriveKind::CdRom,
                found(false, false, true),
                Some(DriveType::Cd(CdType::CdRom)),
            ),
            (
                DriveKind::CdRom,
                Found {
                    cd: Some(CdType::BdRe),
                    ..Found::default()
                },
                Some(DriveType::Cd(CdType::BdRe)),
            ),
            (DriveKind::Unknown, Found::default(), None),
        ];
        for (kind, found, expected) in cases {
            assert_eq!(drive_type(kind, found), expected, "{kind:?} {found:?}");
        }
        assert!(can_be_virtual(DriveKind::Fixed) && can_be_virtual(DriveKind::CdRom));
        assert!(!can_be_virtual(DriveKind::Removable) && !can_be_virtual(DriveKind::Remote));
    }
}
