//! WinSCP's synchronization checklist (`TTerminal::SynchronizeCollect`, `DoSynchronizeCollectDirectory`,
//! `DoSynchronizeCollectFile`) over the two folders' trees: each file or folder on one side only, or changed,
//! becomes an item with its action - upload or download (new or an update), delete on the target side
//! ("Delete files") - checked or not ("Existing files only" leaves the new ones unchecked), the folders on
//! both sides compared inside. Times are compared to the second; "Mirror files" takes any other time, a
//! "Synchronize timestamps" update sets the time only.

/// WinSCP's `TSynchronizeMode`: the target side (both: each side the newer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    Both,
    #[default]
    Remote,
    Local,
}

/// WinSCP's `TChecklistAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    UploadNew,
    DownloadNew,
    UploadUpdate,
    DownloadUpdate,
    DeleteRemote,
    DeleteLocal,
}

/// A file or folder of one side's tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Seconds since 1970.
    pub modified: Option<u64>,
    /// How exact `modified` is (a server's).
    pub precision: crate::sftp::Precision,
    pub children: Vec<Node>,
}

/// An item of the checklist: its path under the two folders ("a/b.txt"), its action, both sides' sizes and
/// times as there are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub path: String,
    pub is_dir: bool,
    pub action: Action,
    pub checked: bool,
    pub local: Option<(u64, Option<u64>)>,
    pub remote: Option<(u64, Option<u64>)>,
}

/// The synchronization's settings (WinSCP's `sp*` params with the mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub mode: Mode,
    pub delete: bool,
    pub existing_only: bool,
    pub timestamps: bool,
    pub mirror: bool,
    pub by_time: bool,
    pub by_size: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Remote,
            delete: false,
            existing_only: false,
            timestamps: false,
            mirror: false,
            by_time: true,
            by_size: false,
        }
    }
}

/// Which side changed: (the remote one: download, the local one: upload).
fn changed(local: &Node, remote: &Node, settings: &Settings) -> (bool, bool) {
    let sizes_differ = local.size != remote.size;
    let compare_time =
        settings.by_time && (!settings.timestamps || !settings.by_size || !sizes_differ);
    let time = match (compare_time, local.modified, remote.modified) {
        // The disk's time cut to the server's precision first (WinSCP's `ReduceDateTimePrecision`).
        (true, Some(left), Some(right)) => remote.precision.reduced(left).cmp(&right),
        _ => std::cmp::Ordering::Equal,
    };
    let plain = !settings.timestamps && !settings.mirror;
    let mode = settings.mode;
    match time {
        // The local file older: the remote one changed, unless mirroring (or setting times) to the server.
        std::cmp::Ordering::Less => match plain || mode != Mode::Remote {
            true => (true, false),
            false => (false, true),
        },
        std::cmp::Ordering::Greater => match plain || mode != Mode::Local {
            true => (false, true),
            false => (true, false),
        },
        std::cmp::Ordering::Equal if settings.by_size && sizes_differ && !settings.timestamps => {
            (true, true)
        }
        std::cmp::Ordering::Equal => (false, false),
    }
}

fn sides(node: &Node) -> Option<(u64, Option<u64>)> {
    Some((node.size, node.modified))
}

fn joined(prefix: &str, name: &str) -> String {
    match prefix.is_empty() {
        true => name.to_string(),
        false => format!("{prefix}/{name}"),
    }
}

/// The items of the remote node `remote` (with its local twin, if any): WinSCP's download side.
fn remote_item(
    remote: &Node,
    local: Option<&Node>,
    settings: &Settings,
    path: String,
) -> Option<Item> {
    let (modified, _) = local.map_or((false, false), |local| changed(local, remote, settings));
    let new = local.is_none();
    let mode = settings.mode;
    if !(new || modified) {
        return None;
    }
    let downloads = mode != Mode::Remote && (!settings.timestamps || modified);
    let action = match (downloads, new) {
        (true, true) => Action::DownloadNew,
        (true, false) => Action::DownloadUpdate,
        // WinSCP's: listed without "Delete files" too, unchecked (`Checked = spDelete`).
        (false, true) if mode == Mode::Remote && !settings.timestamps => Action::DeleteRemote,
        (false, _) => return None,
    };
    Some(Item {
        path,
        is_dir: remote.is_dir,
        action,
        checked: match action {
            Action::DeleteRemote => settings.delete,
            _ => modified || !settings.existing_only,
        },
        local: local.and_then(sides),
        remote: sides(remote),
    })
}

/// The items of the local node `local` (with its remote twin, if any): WinSCP's upload side.
fn local_item(
    local: &Node,
    remote: Option<&Node>,
    settings: &Settings,
    path: String,
) -> Option<Item> {
    let (_, modified) = remote.map_or((false, false), |remote| changed(local, remote, settings));
    let new = remote.is_none();
    let mode = settings.mode;
    if !(new || modified) {
        return None;
    }
    let uploads = mode != Mode::Local && (!settings.timestamps || modified);
    let action = match (uploads, new) {
        (true, true) => Action::UploadNew,
        (true, false) => Action::UploadUpdate,
        (false, true) if mode == Mode::Local && !settings.timestamps => Action::DeleteLocal,
        (false, _) => return None,
    };
    Some(Item {
        path,
        is_dir: local.is_dir,
        action,
        checked: match action {
            Action::DeleteLocal => settings.delete,
            _ => modified || !settings.existing_only,
        },
        local: sides(local),
        remote: remote.and_then(sides),
    })
}

/// Two names the same without case, any letter's (WinSCP's `SameText` on Windows): "Äpfel" is "äpfel".
pub fn same_name(one: &str, other: &str) -> bool {
    // Either side may be the one with a letter beyond ASCII (the Kelvin sign is a "k"): both ways the same.
    one.eq_ignore_ascii_case(other)
        || (!(one.is_ascii() && other.is_ascii()) && one.to_lowercase() == other.to_lowercase())
}

/// The checklist of the two trees under `prefix` (names matched without case, as Windows has them; a file
/// and a folder of one name are left alone).
pub fn collect(local: &[Node], remote: &[Node], settings: &Settings, prefix: &str) -> Vec<Item> {
    let mut items = Vec::new();
    let twin = |name: &str, nodes: &'_ [Node]| -> Option<usize> {
        nodes.iter().position(|node| same_name(&node.name, name))
    };
    for node in remote {
        let path = joined(prefix, &node.name);
        let other = twin(&node.name, local).map(|at| &local[at]);
        match other {
            Some(other) if other.is_dir != node.is_dir => {}
            Some(other) if node.is_dir => {
                items.extend(collect(&other.children, &node.children, settings, &path));
            }
            _ => items.extend(remote_item(node, other, settings, path)),
        }
    }
    for node in local {
        let other = twin(&node.name, remote).map(|at| &remote[at]);
        if other.is_some_and(|other| other.is_dir || node.is_dir) {
            continue;
        }
        items.extend(local_item(
            node,
            other,
            settings,
            joined(prefix, &node.name),
        ));
    }
    // WinSCP's checklist is sorted by its paths.
    items.sort_by(|left, right| left.path.cmp(&right.path));
    items
}

#[cfg(test)]
#[path = "sync_collect_tests.rs"]
mod tests;
