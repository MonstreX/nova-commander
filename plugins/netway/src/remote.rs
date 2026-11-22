//! A connected session's panel (NetBox's `GetOpenPanelInfoEx` when connected, `TRemoteFilePanelItem`): the
//! server's folder with each item's size, last write, owner, group and rights; the title " host:/folder ";
//! the detailed mode (Ctrl+5) NetBox's "N,S,DM,O,G,R"; hidden the names starting with a dot, read-only
//! those nobody may write, links marked as the disk's reparse points.

use std::time::{Duration, SystemTime};

use nc_plugin_sdk::{FileEntry, PanelMode};

use crate::sftp::RemoteEntry;
use crate::ssh::Connection;

/// The columns' order of [`FileEntry::columns`] (NetBox's custom columns `C0` group, `C1` rights, `C2`
/// octal rights, `C3` the link's target), and the owner (FAR's `O`, which the program's entries do not
/// carry) as `C4`.
pub const GROUP: usize = 0;
pub const RIGHTS: usize = 1;
pub const OCTAL: usize = 2;
pub const OWNER: usize = 4;

/// The Windows attribute bits FAR's panel colours a remote item by (NetBox's `GetData`).
const READ_ONLY: u32 = 0x1;
const HIDDEN: u32 = 0x2;
const DIRECTORY: u32 = 0x10;
const REPARSE_POINT: u32 = 0x400;

/// A connected session's panel.
pub struct Remote {
    /// The SSH connection (none for FTP).
    pub connection: Option<Connection>,
    pub fs: crate::fs::Fs,
    /// The host as the title shows it.
    pub host: String,
    /// The session's name (the stored one, else `user@host`), the panel's kind after `netway:`.
    pub name: String,
    /// The synchro's number of the panel.
    pub number: u64,
    /// The folder the panel goes to once its root is read (the program opens a plugin's panel at its
    /// root); none: there already.
    pub start: Option<String>,
    /// WinSCP's `GetSessionUrl` with the user: `sftp://user@host[:port]`.
    pub url: String,
    /// The login again when the session is lost (WinSCP's `Reopen`): the user and password that logged in.
    pub plan: crate::login::Plan,
    /// The reconnections tried in a row (WinSCP's `NumberOfRetries`).
    pub retries: u32,
    /// The stored session's name (none: an address typed), for "Remember last used directory".
    pub stored: Option<String>,
    /// The folder the panel read last.
    pub folder: String,
    /// The folders the panel was at before (NetBox's `FPathHistory`), the latest last.
    pub history: Vec<String>,
    /// The files edited in the session, their folders and names (NetBox's `FEditHistories`), the latest
    /// last.
    pub edits: Vec<(String, String)>,
    /// NetBox's synchronized browsing (Ctrl+Alt+B).
    pub synchronized: bool,
    /// The folder the panel was seen showing (its title asked) and not yet taken as its move: taken at the
    /// panel's synchro (a reading for Find or a folder's size is no move).
    pub seen: std::cell::RefCell<Option<String>>,
    /// A folder a command takes the panel to without the other panel following (NetBox's `OpenDirectory`,
    /// `HomeDirectory`: `ChangeDirectory` past synchronized browsing), till its move is taken.
    pub unfollowed: std::cell::RefCell<Option<String>>,
    /// The connection of its own the program's tasks read and write through (S6.8b), at their first call.
    pub background: Option<crate::background_panel::Background>,
}

impl Remote {
    /// The panel at `folder` now (NetBox's path history): the folder it leaves kept last, the one it
    /// comes to out of the history.
    pub fn moved_to(&mut self, folder: &str) {
        if self.folder == folder {
            return;
        }
        let left = std::mem::replace(&mut self.folder, folder.to_string());
        self.history.retain(|old| old != folder && *old != left);
        if !left.is_empty() {
            self.history.push(left);
        }
    }

    /// The panel seen showing `folder` (its title drawn): a move to take at its synchro, asked of `host`.
    pub fn noticed(&self, folder: &str, host: &nc_plugin_sdk::Host) {
        let mut seen = self.seen.borrow_mut();
        let known = seen.as_deref().unwrap_or(&self.folder);
        if known != folder && !self.folder.is_empty() {
            *seen = Some(folder.to_string());
            host.synchro().ask(self.number);
        }
    }

    /// The panel's coming move to `folder` is a command's: synchronized browsing does not follow it.
    pub fn unfollowed_to(&self, folder: &str) {
        *self.unfollowed.borrow_mut() = Some(folder.trim_end_matches('/').to_string());
    }

    /// Whether the move to `folder` is the command's (taken: asked once).
    pub fn take_unfollowed(&mut self, folder: &str) -> bool {
        (self.unfollowed.get_mut().take())
            .is_some_and(|unfollowed| unfollowed == folder.trim_end_matches('/'))
    }

    /// The file `name` in `folder` edited: last in the edit history, once.
    pub fn edited(&mut self, folder: &str, name: &str) {
        let edit = (folder.to_string(), name.to_string());
        self.edits.retain(|old| *old != edit);
        self.edits.push(edit);
    }
}

/// WinSCP's `GetSessionUrl` of an SSH session (`scheme` "sftp" or "scp": the protocol it runs, one fallen
/// back to SCP an SCP one) with its user: the port only when not 22, an IPv6 host in brackets.
pub fn session_url(scheme: &str, user: &str, host: &str, port: u16) -> String {
    let host = match host.contains(':') {
        true => format!("[{host}]"),
        false => host.to_string(),
    };
    let port = match port {
        22 => String::new(),
        port => format!(":{port}"),
    };
    let user = match user {
        "" => String::new(),
        user => format!("{user}@"),
    };
    format!("{scheme}://{user}{host}{port}")
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Remote({}, {})", self.name, self.host)
    }
}

impl Remote {
    /// NetBox's title " host:/folder " (the program puts the spaces).
    pub fn title(&self, path: &str) -> String {
        format!("{}:{path}", self.host)
    }

    /// NetBox's `netbox:<session>`.
    pub fn format(&self) -> String {
        format!("{}:{}", crate::address::OWN_PREFIX, self.name)
    }
}

/// The panel's entry of a server's item.
pub fn entry(item: &RemoteEntry) -> FileEntry {
    let hidden = item.name.starts_with('.');
    let read_only = item.permissions.is_some_and(|mode| mode & 0o222 == 0);
    let mut columns = vec![String::new(); OWNER + 1];
    columns[GROUP] = item.group.clone().unwrap_or_default();
    columns[RIGHTS] = item.permissions.map(rights).unwrap_or_default();
    columns[OCTAL] = item.permissions.map(octal).unwrap_or_default();
    columns[OWNER] = item.owner.clone().unwrap_or_default();
    let attributes = if item.is_link {
        let bits = [
            (item.is_dir, DIRECTORY),
            (hidden, HIDDEN),
            (read_only, READ_ONLY),
        ];
        (bits.iter()).fold(
            REPARSE_POINT,
            |all, (on, bit)| if *on { all | bit } else { all },
        )
    } else {
        0
    };
    FileEntry {
        name: item.name.clone(),
        is_dir: item.is_dir,
        size: if item.is_dir { 0 } else { item.size },
        modified: (item.modified)
            .map(|seconds| SystemTime::UNIX_EPOCH + Duration::from_secs(seconds.into())),
        hidden,
        read_only,
        attributes,
        columns,
        ..FileEntry::default()
    }
}

/// WinSCP's `TRights::GetText`: `rwxr-xr-x`, the set-id and sticky bits as `s` / `S` and `t` / `T`.
pub fn rights(mode: u32) -> String {
    let special = [(0o4000, 's'), (0o2000, 's'), (0o1000, 't')];
    (0..3)
        .flat_map(|class| {
            let shift = 6 - class * 3;
            let (bit, mark) = special[class];
            let on = |right: u32| mode & (right << shift) != 0;
            let execute = match (on(1), mode & bit != 0) {
                (true, true) => mark,
                (false, true) => mark.to_ascii_uppercase(),
                (true, false) => 'x',
                (false, false) => '-',
            };
            [
                if on(4) { 'r' } else { '-' },
                if on(2) { 'w' } else { '-' },
                execute,
            ]
        })
        .collect()
}

/// WinSCP's `TRights::GetOctal`: `755`, with the special bits `4755`.
pub fn octal(mode: u32) -> String {
    match mode & 0o7000 {
        0 => format!("{:03o}", mode & 0o777),
        _ => format!("{:04o}", mode & 0o7777),
    }
}

/// FAR's detailed mode, Ctrl+5 (NetBox's `SetPanelMode(5)`).
pub const DETAILED: usize = 5;

/// NetBox's modes: the detailed one its own, the others the program's.
pub fn modes() -> Vec<Option<PanelMode>> {
    let detailed = PanelMode {
        columns: format!("N,S,DM,C{OWNER},C{GROUP},C{RIGHTS}"),
        widths: "0,8,14,0,0,9".to_string(),
        titles: vec![
            None,
            None,
            None,
            Some("Owner".to_string()),
            Some("Group".to_string()),
            Some("Rights".to_string()),
        ],
        status_columns: "NR".to_string(),
        status_widths: "0".to_string(),
        full_screen: true,
        ..PanelMode::default()
    };
    let mut modes = vec![None; 10];
    modes[DETAILED] = Some(detailed);
    modes
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
