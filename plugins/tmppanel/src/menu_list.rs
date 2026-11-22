//! "Menu from file list" (FAR's TmpPanel `ShowMenuFromList`, `TmpPanel.cpp:164-221`): a list's lines as a
//! menu, each `%VAR%`-expanded and read as `|label|command` - the label shown (`-` a separator), the
//! command run; a line without a label is both. The title is the list file's name without its extension;
//! Up and Down go round the ends. Enter on a command that names a folder there takes the active panel to
//! it, on a file opens it by the system (`ShellExecute` "open"), on anything else puts it in the command
//! line; Shift+Enter opens it by the system whatever it is. Nothing opens a panel.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{KeyAnswer, KeyGo, Menu, MenuItem};

use crate::{TmpPanel, listfile};

/// The menu's lines: each item's label and its command (FAR's `ParseParam` of the expanded line).
pub fn entries(lines: &[String]) -> Vec<(MenuItem, String)> {
    (lines.iter())
        .map(|line| {
            let expanded = listfile::expand_env(line);
            let (label, command) = listfile::parse_param(&expanded).unwrap_or(("", ""));
            let item = match label {
                "-" => MenuItem::separator(),
                "" => MenuItem::new(&expanded),
                label => MenuItem::new(label),
            };
            let command = if command.is_empty() {
                expanded.clone()
            } else {
                command.to_string()
            };
            (item, command)
        })
        .collect()
}

/// The menu's title: the list file's name without its extension.
pub fn title(name: &str) -> String {
    let file = Path::new(name).file_name().map_or_else(
        || name.to_string(),
        |file| file.to_string_lossy().into_owned(),
    );
    match file.rfind('.') {
        Some(dot) => file[..dot].to_string(),
        None => file,
    }
}

/// What Enter on `command` does (FAR's `GetFileInfoAndValidate` without `Any`): a folder there takes the
/// active panel to it, a file is opened by the system, anything else goes to the command line.
pub fn chosen(command: &str, folder: Option<&Path>) -> KeyGo {
    match listfile::item(command, folder, false) {
        Some(path) if path.is_dir() => KeyGo::ActiveFolder(path),
        // A file, or a device (FAR's `FILE_ATTRIBUTE_ARCHIVE` item), from the program's folder as FAR's
        // `ShellExecute` takes it from its current one.
        Some(path) => KeyGo::ShellOpen(path),
        None => KeyGo::CommandLine(command.to_string()),
    }
}

/// What Shift+Enter opens by the system: a path there by its whole name, anything else as written.
pub fn shell_target(command: &str, folder: Option<&Path>) -> PathBuf {
    listfile::item(command, folder, false).unwrap_or_else(|| PathBuf::from(command))
}

impl TmpPanel {
    /// The menu of `lines` from the list file `name`: what the item chosen does; Esc nothing.
    pub(crate) fn menu_from_list(
        &self,
        lines: &[String],
        name: &str,
        folder: Option<&Path>,
    ) -> KeyAnswer {
        let (items, commands): (Vec<MenuItem>, Vec<String>) = entries(lines).into_iter().unzip();
        let menu = Menu::new(&title(name), items)
            .break_keys(&["Shift+Enter"])
            .wrap();
        let Some(chosen) = self.host.menu(&menu) else {
            return KeyAnswer::default();
        };
        let Some(command) = chosen.item.and_then(|item| commands.get(item)) else {
            return KeyAnswer::default();
        };
        if chosen.key.is_some() {
            // Shift+Enter: by the system, whatever it names.
            return KeyAnswer::go(KeyGo::ShellOpen(shell_target(command, folder)));
        }
        KeyAnswer::go(self::chosen(command, folder))
    }
}

#[cfg(test)]
#[path = "menu_list_tests.rs"]
mod tests;
