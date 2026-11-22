//! NetBox's panel of stored sessions (`WinSCPFileSystem.cpp`: `GetFindDataEx`'s session list,
//! `GetOpenPanelInfoEx`, `TSessionPanelItem::SetPanelModes` / `SetKeyBarTitles`): the folders and the
//! sessions of a folder, the line "Press Shift-F4 to create new session item" in a folder of none, the
//! title "NetWay: Stored sessions [/folder]", every mode the sessions' names under "Session name", the key
//! bar's labels.

use nc_plugin_sdk::{FileEntry, PanelKey, PanelMode};

use crate::store::{Item, joined};

/// The plugin's name where NetBox says NetBox (its menus' items, the panel's title).
pub const TITLE: &str = "NetWay";
/// The panel's kind as the copy dialog names it (NetBox's `netbox`).
pub const FORMAT: &str = "netway";
/// NetBox's `NB_NEW_SESSION_HINT`, the line of a folder with no session.
pub const HINT: &str = "Press Shift-F4 to create new session item";

/// The keys FAR's key bar shows empty on the sessions (NetBox's `ClearKeyBarTitle`), left to the program:
/// F6 copies out as F5 does (NetBox's export takes no move).
const BLANK_KEYS: [&str; 15] = [
    "F6", "Shift+F1", "Shift+F2", "Shift+F3", "Shift+F7", "Shift+F8", "Alt+F6", "Ctrl+F4",
    "Ctrl+F5", "Ctrl+F6", "Ctrl+F7", "Ctrl+F8", "Ctrl+F9", "Ctrl+F10", "Ctrl+F11",
];

/// F5: NetBox's "Export", the program's copy out, which the plugin's export answers.
pub const EXPORT_KEY: &str = "F5";

/// The sessions' folder of the panel's `path` (`/` the root: ``, `/Work/Prod`: `Work/Prod`).
pub fn folder_of(path: &str) -> &str {
    path.trim_matches('/')
}

/// The panel's path of the sessions' `folder`.
pub fn path_of(folder: &str) -> String {
    format!("/{}", folder.trim_matches('/'))
}

/// The panel's entries for a folder's `items`; none: NetBox's hint alone.
pub fn entries(items: &[Item]) -> Vec<FileEntry> {
    if items.is_empty() {
        return vec![FileEntry::file(HINT, 0)];
    }
    (items.iter())
        .map(|item| match item {
            Item::Folder(name) => FileEntry::folder(name.clone()),
            Item::Session(name) => FileEntry::file(name.clone(), 0),
        })
        .collect()
}

/// NetBox's `NB_STORED_SESSION_TITLE` with the folder: "NetWay: Stored sessions [/Work]".
pub fn title(path: &str) -> String {
    format!("{TITLE}: Stored sessions [{}]", path_of(folder_of(path)))
}

/// NetBox's `TSessionPanelItem::SetPanelModes`: all ten modes the names alone under "Session name".
pub fn modes() -> Vec<Option<PanelMode>> {
    let mode = PanelMode {
        columns: "N".to_string(),
        widths: "0".to_string(),
        titles: vec![Some("Session name".to_string())],
        ..PanelMode::default()
    };
    vec![Some(mode); 10]
}

/// The sessions' own keys with NetBox's labels: Enter, F4, F5 "Export", Shift+F4 "NewSes", Shift+F5
/// "Duplicate", Shift+F6 "Rename", Ctrl+F; the keys NetBox blanks show empty.
pub fn keys() -> Vec<PanelKey> {
    let mut keys = vec![
        PanelKey::new("Enter", ""),
        PanelKey::new("F4", ""),
        PanelKey::new(EXPORT_KEY, "Export"),
        PanelKey::new("Shift+F4", "NewSes"),
        PanelKey::new("Shift+F5", "Duplicate"),
        PanelKey::new("Shift+F6", "Rename"),
        PanelKey::new("Ctrl+F", ""),
    ];
    // A label of a space: the key bar's place shown empty.
    keys.extend(BLANK_KEYS.iter().map(|key| PanelKey::new(key, " ")));
    keys
}

/// Whether `key` is left to the program: F5's export, the keys NetBox blanks and leaves to FAR.
pub fn is_programs(key: &str) -> bool {
    key == EXPORT_KEY || BLANK_KEYS.contains(&key)
}

/// NetBox's `InsertSessionNameOnCommandLine` for the entry under the cursor in `folder`: a session's
/// whole name, a folder's path or (on "..") the folder's own with a `/` after it; in quotes when it has a
/// space, a space after it. None on the hint.
pub fn command_line_name(folder: &str, item: Option<&Item>, on_dots: bool) -> Option<String> {
    let name = match item {
        Some(Item::Session(name)) => joined(folder, name),
        Some(Item::Folder(name)) => format!("{}/", joined(folder, name)),
        None if on_dots => format!("{}/", folder.trim_matches('/')),
        None => return None,
    };
    Some(if name.contains(' ') {
        format!("\"{name}\" ")
    } else {
        format!("{name} ")
    })
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
