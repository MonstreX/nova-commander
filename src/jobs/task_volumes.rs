//! The volumes an operation works on (its tasks wait for each other per volume, ADR-0010): asked of the
//! disks, which may not answer, so worked out off the screen's thread before the task is added
//! (ADR-0019, review R19).

use std::collections::HashSet;
use std::path::PathBuf;

use super::delete;
use super::transfer::Request;
use crate::platform::volumes::VolumeId;
use crate::vfs::VfsPath;

/// The volume of each folder a copy's or move's sources are in (one per panel folder, not one per
/// item), and of its target.
pub fn of_transfer(request: &Request) -> Vec<VolumeId> {
    let folders: HashSet<VfsPath> = (request.sources.iter())
        .map(|source| source.parent().unwrap_or_else(|| source.clone()))
        .chain(std::iter::once(request.target.clone()))
        .collect();
    sorted(&folders)
}

/// The volumes of the folders a deletion deletes in.
pub fn of_delete(request: &delete::Request) -> Vec<VolumeId> {
    let folders: HashSet<VfsPath> = (request.items.iter())
        .map(|item| item.path.parent().unwrap_or_else(|| item.path.clone()))
        .collect();
    sorted(&folders)
}

fn sorted(folders: &HashSet<VfsPath>) -> Vec<VolumeId> {
    let mut volumes: Vec<VolumeId> = folders.iter().map(volume_of).collect();
    volumes.sort();
    volumes.dedup();
    volumes
}

/// The volume a folder is on, for the queue: a local one's identity, a provider's own root.
fn volume_of(folder: &VfsPath) -> VolumeId {
    match folder.as_local() {
        Some(path) => crate::platform::volumes::volume_id(path),
        None => VolumeId::Root(PathBuf::from(folder.root().to_string())),
    }
}
