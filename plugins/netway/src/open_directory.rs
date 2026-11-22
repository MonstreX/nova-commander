//! NetBox's "Open directory" (Alt+Shift+F12) and "Add bookmark" (`TWinSCPFileSystem::OpenDirectory`,
//! `OpenDirectoryDialog`): the menu of the session's last folders and its bookmarks; Enter goes to the
//! folder, Del or F8 removes a bookmark (on the history: clears it), Ctrl+Enter puts the folder on the
//! command line, Ctrl+Ins or Ctrl+C copies it.

use nc_plugin_sdk::{KeyAnswer, KeyGo, Menu, MenuItem, Which};

use crate::NetWay;
use crate::bookmarks::{self, Line};

/// The menu's break keys (NetBox's `BreakKeys`).
const BREAK_KEYS: [&str; 5] = ["Del", "F8", "Ctrl+Enter", "Ctrl+C", "Ctrl+Ins"];

/// What a key on a line asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The panel to the folder.
    Go(String),
    /// The bookmark removed.
    Remove(String),
    /// The history cleared.
    ClearHistory,
    CommandLine(String),
    Clipboard(String),
    /// Nothing (Esc, the empty line).
    Nothing,
}

/// What `key` (a break key's number, none: Enter) on `line` asks.
pub fn asked(line: Option<&Line>, key: Option<usize>) -> Asked {
    let Some(line) = line else {
        return Asked::Nothing;
    };
    let folder = line.folder.clone().unwrap_or_default();
    match key {
        Some(0 | 1) if line.bookmark => Asked::Remove(folder),
        Some(0 | 1) => Asked::ClearHistory,
        Some(2) => Asked::CommandLine(folder),
        Some(3 | 4) => Asked::Clipboard(folder),
        _ if folder.is_empty() => Asked::Nothing,
        _ => Asked::Go(folder),
    }
}

fn menu_of(lines: &[Line], focused: usize, add: bool) -> Menu {
    let items: Vec<MenuItem> = (lines.iter())
        .map(|line| match &line.folder {
            Some(folder) => MenuItem::new(folder),
            None => MenuItem::separator(),
        })
        .collect();
    let title = match add {
        true => bookmarks::ADD_TITLE,
        false => bookmarks::OPEN_TITLE,
    };
    let mut menu = Menu::new(title, items)
        .bottom(bookmarks::HELP)
        .selected(focused)
        .break_keys(&BREAK_KEYS)
        .show_ampersand()
        .wrap();
    menu.reverse_auto_hotkeys = true;
    menu
}

impl NetWay {
    /// The menu shown until a folder is chosen or it is left; `add`: the current folder bookmarked first.
    pub(crate) fn open_directory(&mut self, instance: &str, add: bool) -> KeyAnswer {
        let Some(remote) = self.remotes.get(instance) else {
            return KeyAnswer::default();
        };
        let file = self.host.data_folder().join(bookmarks::FILE);
        let (key, current) = (remote.name.clone(), remote.folder.clone());
        let mut marks = bookmarks::load_marks(&file, &key);
        // WinSCP's `TBookmarkList::Insert`: a name taken is not added again.
        if add && !marks.iter().any(|mark| mark.name == current) {
            marks.push(bookmarks::Mark::remote(&current));
            let _ = bookmarks::save_marks(&file, &key, &marks);
        }
        let mut focus = None;
        loop {
            let history = (self.remotes.get(instance))
                .map(|remote| remote.history.clone())
                .unwrap_or_default();
            let remotes: Vec<String> = marks.iter().map(|mark| mark.remote.clone()).collect();
            let (lines, first) = bookmarks::lines(&history, &remotes, &current);
            let menu = menu_of(&lines, focus.unwrap_or(first).min(lines.len() - 1), add);
            let Some(chosen) = self.host.menu(&menu) else {
                return KeyAnswer::default();
            };
            focus = chosen.item;
            match asked(chosen.item.and_then(|at| lines.get(at)), chosen.key) {
                Asked::Go(folder) => {
                    if let Some(remote) = self.remotes.get(instance) {
                        remote.unfollowed_to(&folder);
                    }
                    let _ = self.host.set_folder(Which::Active, &folder, None);
                    return KeyAnswer::default();
                }
                Asked::Remove(folder) => {
                    // The one bookmark under the cursor (NetBox's), not a profile of the same folder.
                    let line = (marks.iter())
                        .position(|mark| mark.remote == folder && mark.local.is_empty())
                        .or_else(|| marks.iter().position(|mark| mark.remote == folder));
                    if let Some(at) = line {
                        marks.remove(at);
                    }
                    let _ = bookmarks::save_marks(&file, &key, &marks);
                }
                Asked::ClearHistory => {
                    if let Some(remote) = self.remotes.get_mut(instance) {
                        remote.history.clear();
                    }
                    focus = None;
                }
                Asked::CommandLine(folder) => return KeyAnswer::go(KeyGo::CommandLine(folder)),
                Asked::Clipboard(folder) => {
                    self.host.set_clipboard(&folder);
                }
                Asked::Nothing => return KeyAnswer::default(),
            }
        }
    }
}

/// NetBox's "Edit history" menu of `edits` (full paths, an empty line last with the cursor on it).
pub fn edit_history_menu(edits: &[(String, String)]) -> Menu {
    let mut items: Vec<MenuItem> = (edits.iter())
        .map(|(folder, name)| MenuItem::new(&crate::sftp::child(folder, name)))
        .collect();
    items.push(MenuItem::new(""));
    let mut menu = Menu::new("Edit history", items)
        .selected(edits.len())
        .break_keys(&["F4"])
        .show_ampersand()
        .wrap();
    menu.reverse_auto_hotkeys = true;
    menu
}

impl NetWay {
    /// NetBox's "Edit history" (Alt+Shift+F11, `TWinSCPFileSystem::EditHistory`): a file edited in the session
    /// chosen (Enter or F4) is edited again - the panel at its folder, the cursor on it, F4 - unless it is
    /// gone from the server.
    pub(crate) fn edit_history(&mut self, instance: &str) -> KeyAnswer {
        let Some(remote) = self.remotes.get(instance) else {
            return KeyAnswer::default();
        };
        let edits = remote.edits.clone();
        let chosen = self.host.menu(&edit_history_menu(&edits));
        let Some((folder, name)) = chosen
            .and_then(|chosen| chosen.item)
            .and_then(|at| edits.get(at))
        else {
            return KeyAnswer::default();
        };
        let path = crate::sftp::child(folder, name);
        let there = (self.runtime.as_ref())
            .and_then(|runtime| {
                runtime
                    .wait(remote.fs.metadata(path.as_str()), &|| false)
                    .ok()
            })
            .is_some_and(|found| found.is_ok());
        if there {
            remote.unfollowed_to(folder);
        }
        if there && self.host.set_folder(Which::Active, folder, Some(name)) {
            self.host.post_keys("F4");
        }
        KeyAnswer::default()
    }
}

impl NetWay {
    /// NetBox's "Generate URL" of the connected session `instance` at its folder.
    pub(crate) fn generate_url(&self, instance: &str) {
        let Some(remote) = self.remotes.get(instance) else {
            return;
        };
        let plan = &remote.plan;
        let protocol = match remote.fs.protocol() {
            "SCP" => crate::session::Protocol::Scp,
            _ => plan.protocol,
        };
        let parts = crate::generate_url::Parts {
            protocol,
            encryption: plan.encryption,
            user: plan.user.clone(),
            password: plan.password.clone(),
            host: plan.host.clone(),
            port: plan.port,
            fingerprint: (remote.connection.as_ref()).and_then(|link| link.fingerprint.clone()),
            folder: remote.folder.clone(),
        };
        crate::generate_url::show(&self.host, &parts);
    }
}

#[cfg(test)]
#[path = "open_directory_tests.rs"]
mod tests;
