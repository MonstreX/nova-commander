//! Volumes: where they are reached (a volume root: a drive root `C:\` - a letter is one kind of root -,
//! a volume with no letter, a mount point on Unix), what they are, and what FAR's drive menu does to
//! them. Every call takes a root, never a letter.

pub use super::os::{
    cd_type, cluster_size, connect_network_drive, credentials_refused, detach_virtual_disk,
    device_name, disconnect_network_drive, disk_size, drive_kind, eject_media, free_space,
    has_recycle_bin, hotplug_device, is_ejectable_media, is_persistent_network_drive, load_media,
    network_name, on_local_disk, recycle_bin_capacity, remove_hotplug_device, remove_subst,
    set_volume_label, shell_name, storage_device, subst_target, take_device_changes,
    universal_name, unmounted_volumes, virtual_disk_file, volume_facts, volume_id, volume_of,
    volume_roots,
};

/// Which volume a path is on, for the per-volume task queue: two paths on one disk are one volume (a
/// SUBST drive and its folder, a network drive and its UNC path, a mount point and its drive letter).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VolumeId {
    /// What the system numbers it by (Windows: the volume serial number; Unix: `st_dev`).
    Number(u64),
    /// Its root, where the system gives no number.
    Root(std::path::PathBuf),
}

/// A place the drive menu lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeRoot {
    /// `C:\`, a mount point on Unix.
    pub path: String,
    /// A drive letter, where the system has them.
    pub letter: Option<char>,
    /// A remembered network connection, connected now or not.
    pub remembered: bool,
}

/// What kind of drive a root is (`GetDriveTypeW`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Unknown,
    /// Nothing is mounted there (also a remembered network drive that is not connected).
    NoRoot,
    Removable,
    Fixed,
    Remote,
    CdRom,
    RamDisk,
}

/// What an optical drive can do, as FAR names it in the drive menu (`cd_type`, `far/cddrv.hpp`; its
/// CD-R and DVD-R are never chosen by `get_cd_type`, so they are not here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdType {
    CdRom,
    CdRw,
    CdRwDvd,
    DvdRom,
    DvdRw,
    DvdRam,
    BdRom,
    BdR,
    BdRe,
    HdDvdRom,
    HdDvdR,
    HdDvdRw,
    HdDvdRam,
}

/// A volume's label, file system and serial number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeFacts {
    pub label: String,
    pub file_system: String,
    /// The number the file system was given when it was made (FAR's "Serial number", `XXXX-XXXX`);
    /// `None` where the system has none.
    pub serial: Option<u32>,
}

/// The device a volume is on, as its driver describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageDevice {
    pub vendor: String,
    pub product: String,
    /// "NVMe", "SATA", "USB", ...; empty when unknown.
    pub bus: String,
}

/// A device that can be removed safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotplugDevice {
    /// The system's handle of the device (Windows: the device node, `DEVINST`).
    pub(super) node: u32,
    /// What the system calls it ("USB Mass Storage Device").
    pub description: String,
    /// Its own name ("SanDisk Ultra USB Device"); may be empty.
    pub friendly_name: String,
}
