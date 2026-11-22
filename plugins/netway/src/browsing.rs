//! NetBox's synchronized browsing and "Compare directories" (`TWinSCPFileSystem::ToggleSynchronizeBrowsing`,
//! `SetDirectoryEx`'s `FSynchronisingBrowse`, `SynchronizeBrowsing`, `CompareDirectories`): while browsing is
//! synchronized, the server's folder entered or left takes the other panel (a disk's) along - into the same
//! subfolder, or up as many levels - asking to make a local folder that is not there, and turning itself
//! off when it cannot follow; comparing selects, in both panels, the files that differ (only on one side,
//! or modified at another time; by size too when asked).

use nc_plugin_sdk::{PanelItem, PanelKind, PanelState};

use crate::sftp::RemoteEntry;

/// NetBox's `NB_SYNCHRONIZE_BROWSING_ON` / `_OFF`, `_LOCAL_PATH_REQUIRED`, `NB_SYNC_DIR_BROWSE_ERROR`,
/// `NB_SYNC_DIR_BROWSE_CREATE`, `NB_COMPARE_DIRECTORIES_*`.
pub const ON: &str = "Synchronized browsing enabled.";
pub const OFF: &str = "Synchronized browsing disabled.";
pub const LOCAL_REQUIRED: &str = "Synchronized browsing is supported only against regular panel.";
pub const FAILED: &str = "Cannot open corresponding directory in the opposite panel. Directory browsing synchronization failed. It has been turned off.";
pub const COMPARE_TITLE: &str = "Compare Directories";
pub const NO_DIFFERENCES: &str = "No differences found.";

/// NetBox's question to make the local folder.
pub fn create_question(folder: &str) -> String {
    format!(
        "Cannot open corresponding directory in the opposite panel.\n \nDo you want to try to create directory '{folder}'?"
    )
}

/// NetBox's `NB_COMPARE_DIRECTORIES_RESULT`.
pub fn compared(local: usize, remote: usize) -> String {
    format!("Differences found: {local} local files, {remote} remote files")
}

/// A disk's panel showing files (the panel synchronized browsing and comparing need).
pub fn regular(state: Option<&PanelState>) -> bool {
    state.is_some_and(|state| state.plugin.is_none() && state.kind == PanelKind::Files)
}

/// The local folder that follows the server's going from `before` to `now`, with the local panel at
/// `local` (NetBox's: into the same subfolders, or up as many levels; elsewhere: none).
pub fn local_path(before: &str, now: &str, local: &str) -> Option<String> {
    let slashed = |path: &str| format!("{}/", path.trim_end_matches('/'));
    let (before, now) = (slashed(before), slashed(now));
    let local = local.trim_end_matches(['\\', '/']).to_string();
    if let Some(deeper) = now.strip_prefix(&before) {
        let deeper = deeper.trim_end_matches('/').replace('/', "\\");
        return Some(match deeper.is_empty() {
            true => local,
            false => format!("{local}\\{deeper}"),
        });
    }
    let higher = before.strip_prefix(&now)?;
    let levels = higher.trim_end_matches('/').split('/').count();
    // A Windows path's folders (the disk's drive first), whatever the system the code is built for.
    let mut parts: Vec<&str> = local.split('\\').collect();
    for _ in 0..levels {
        if parts.len() <= 1 {
            return None;
        }
        parts.pop();
    }
    Some(match parts.as_slice() {
        [drive] => format!("{drive}\\"),
        _ => parts.join("\\"),
    })
}

/// The seconds since 1970 of a local item's time.
fn seconds(time: Option<std::time::SystemTime>) -> Option<u64> {
    let since = time?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(since.as_secs())
}

/// The files that differ (NetBox's `CompareDirectories` over WinSCP's `SynchronizeCollect`): the local and
/// the remote names - a file only on one side, or modified at another second, or (`by_size`) of another
/// size; names matched without case (Windows's); folders left out.
pub fn differences(
    local: &[PanelItem],
    remote: &[RemoteEntry],
    by_size: bool,
) -> (Vec<String>, Vec<String>) {
    let local: Vec<&PanelItem> = (local.iter())
        .filter(|item| !item.folder && !item.parent)
        .collect();
    let remote: Vec<&RemoteEntry> = (remote.iter()).filter(|entry| !entry.is_dir).collect();
    let differs = |item: &PanelItem, entry: &RemoteEntry| {
        let local = seconds(item.modified).map(|left| entry.precision.reduced(left));
        let times = (local, entry.modified.map(u64::from));
        let time_differs = matches!(times, (Some(left), Some(right)) if left != right);
        time_differs || (by_size && item.size != entry.size)
    };
    let same = crate::sync_collect::same_name;
    let twin = |name: &str| (remote.iter()).find(|entry| same(&entry.name, name));
    let local_names: Vec<String> = (local.iter())
        .filter(|item| twin(&item.name).is_none_or(|entry| differs(item, entry)))
        .map(|item| item.name.clone())
        .collect();
    let remote_names: Vec<String> = (remote.iter())
        .filter(|entry| {
            let other = (local.iter()).find(|item| same(&item.name, &entry.name));
            other.is_none_or(|item| differs(item, entry))
        })
        .map(|entry| entry.name.clone())
        .collect();
    (local_names, remote_names)
}

#[cfg(test)]
#[path = "browsing_tests.rs"]
mod tests;
