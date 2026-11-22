//! The volumes' stubs: no drive letters, Recycle Bin or network drives until the port.

use std::io;
use std::path::{Path, PathBuf};

use super::unsupported;
use crate::platform::volumes::{DriveKind, HotplugDevice, VolumeFacts, VolumeId, VolumeRoot};

/// The freedesktop Trash and the Finder's Trash come with the port.
pub fn has_recycle_bin(_root: &str) -> bool {
    false
}

/// No Recycle Bin here: nothing to measure.
pub fn recycle_bin_capacity(_root: &str) -> Option<u64> {
    None
}

pub fn connect_network_drive(
    _root: &str,
    _share: &str,
    _login: Option<(&str, &str)>,
) -> io::Result<()> {
    unsupported()
}

pub fn credentials_refused(_error: &io::Error) -> bool {
    false
}

pub fn disconnect_network_drive(_root: &str, _forget: bool) -> io::Result<()> {
    unsupported()
}

pub fn disk_size(_root: &str) -> Option<(u64, u64)> {
    None
}

pub fn drive_kind(_root: &str) -> DriveKind {
    DriveKind::Unknown
}

pub fn eject_media(_root: &str, _removable: bool) -> io::Result<()> {
    unsupported()
}

pub fn free_space(_dir: &Path) -> Option<u64> {
    None
}

pub fn hotplug_device(_root: &str) -> Option<HotplugDevice> {
    None
}

pub fn is_ejectable_media(_root: &str) -> bool {
    false
}

pub fn is_persistent_network_drive(_root: &str) -> bool {
    false
}

pub fn load_media(_root: &str) -> io::Result<()> {
    unsupported()
}

pub fn device_name(_root: &str) -> Option<String> {
    None
}

pub fn network_name(_root: &str) -> Option<String> {
    None
}

pub fn remove_hotplug_device(_device: &HotplugDevice) -> io::Result<()> {
    unsupported()
}

pub fn remove_subst(_root: &str) -> io::Result<()> {
    unsupported()
}

pub fn cd_type(_root: &str) -> crate::platform::volumes::CdType {
    crate::platform::volumes::CdType::CdRom
}

pub fn storage_device(_root: &str) -> Option<crate::platform::volumes::StorageDevice> {
    None
}

/// The disks' kinds come with the port: none is taken for a local one.
pub fn on_local_disk(_path: &std::path::Path) -> bool {
    false
}

/// The names come with the port; nothing is known here.
pub fn system_facts(
    _names: crate::platform::system::NameFormats,
) -> crate::platform::system::SystemFacts {
    crate::platform::system::SystemFacts::default()
}

/// The sound comes with the port.
pub fn beep() {}

pub fn virtual_disk_file(_root: &str) -> Option<String> {
    None
}

pub fn detach_virtual_disk(_root: &str) -> io::Result<()> {
    unsupported()
}

pub fn set_volume_label(_root: &str, _label: &str) -> io::Result<()> {
    unsupported()
}

pub fn shell_name(_root: &str) -> Option<String> {
    None
}

pub fn cluster_size(_root: &str) -> Option<u64> {
    None
}

pub fn file_type_name(_name: &str) -> Option<String> {
    None
}

pub fn subst_target(_root: &str) -> Option<String> {
    None
}

pub fn universal_name(path: &Path) -> PathBuf {
    path.to_path_buf()
}

pub fn unmounted_volumes() -> Vec<String> {
    Vec::new()
}

pub fn volume_facts(_root: &str) -> Option<VolumeFacts> {
    None
}

/// The port listens for mounts (udev, DiskArbitration); until then the menu is read again on Ctrl+R.
pub fn take_device_changes() -> Vec<String> {
    Vec::new()
}

pub fn volume_id(path: &Path) -> VolumeId {
    VolumeId::Root(volume_of(path))
}

pub fn volume_of(path: &Path) -> PathBuf {
    path.ancestors()
        .last()
        .map_or_else(PathBuf::new, Path::to_path_buf)
}

pub fn volume_roots() -> Vec<VolumeRoot> {
    Vec::new()
}
