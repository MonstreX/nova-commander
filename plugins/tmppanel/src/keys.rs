//! The temporary panel's own keys, as FAR's TmpPanel's `ProcessKey` and key bar (`TmpClass.cpp:520-631`):
//! F7 "Remove" takes the names off the list (the files stay; a quick view beside it shows its file again),
//! Alt+Shift+F3 "Goto" shows the entry in the other panel, with the common panel Alt+Shift+F12 "Switch"
//! chooses the current of the ten common panels from a menu and Alt+Shift+0..9 makes one current at once,
//! Alt+Shift+F2 "SavLst" saves the list to a list file (`savlst`). F3..F8 but F7 do nothing on an entry
//! whose file is gone; in safe mode F3..F8 and the deleting keys do nothing on an entry, Enter goes to a folder and Ctrl+PgUp to the entry's place, with any
//! elements Enter puts an element that is no file in the command line. Every other key is the program's.

use std::path::{Path, PathBuf};

use nc_plugin_sdk::{Error, KeyAnswer, KeyGo, KeyPress, PanelKey};

use crate::{Options, PANELS, TmpPanel};

/// The keys TmpPanel checks the entry under the cursor for (`TmpClass.cpp:575-579`: F3..F8 but F7, with
/// any modifiers but Ctrl alone).
const FILE_KEYS: [u8; 5] = [3, 4, 5, 6, 8];
const FILE_MODIFIERS: [&str; 7] = [
    "",
    "Shift+",
    "Alt+",
    "Alt+Shift+",
    "Ctrl+Shift+",
    "Ctrl+Alt+",
    "Ctrl+Alt+Shift+",
];

/// The deleting keys besides F8 (Del with an empty command line, Shift+Del, Alt+Del): FAR's
/// `DeleteFiles`, which TmpPanel lacks - nothing in safe mode.
const DELETE_KEYS: [&str; 3] = ["Del", "Shift+Del", "Alt+Del"];

/// The keys that read or delete an entry through the plugin when it has no real names: TmpPanel has no
/// `GetFiles` and no `DeleteFiles`, so in safe mode FAR does nothing on them.
const SAFE_SWALLOWED: [&str; 14] = [
    "F3",
    "Alt+F3",
    "F4",
    "Alt+F4",
    "F5",
    "Shift+F5",
    "Alt+F5",
    "F6",
    "Shift+F6",
    "F8",
    "Shift+F8",
    "Del",
    "Shift+Del",
    "Alt+Del",
];

/// The keys as the options name them, with FAR's labels (`MF7`, `MAltShiftF2`, `MAltShiftF3`,
/// `MAltShiftF12`; Switch and its keys only with the common panel, `TmpClass.cpp:135-136, 615`).
pub fn keys(options: &Options) -> Vec<PanelKey> {
    let mut keys = vec![
        PanelKey::new("F7", "Remove"),
        PanelKey::new("Alt+Shift+F2", "SavLst"),
        PanelKey::new("Alt+Shift+F3", "Goto"),
    ];
    if options.common_panel {
        keys.push(PanelKey::new("Alt+Shift+F12", "Switch"));
        keys.extend((0..PANELS).map(|panel| PanelKey::new(&format!("Alt+Shift+{panel}"), "")));
    }
    for number in FILE_KEYS {
        for modifiers in FILE_MODIFIERS {
            let key = format!("{modifiers}F{number}");
            if key != "Alt+Shift+F3" {
                keys.push(PanelKey::new(&key, ""));
            }
        }
    }
    if options.any_in_panel || options.safe_mode_panel {
        keys.push(PanelKey::new("Enter", ""));
    }
    if options.safe_mode_panel {
        keys.push(PanelKey::new("Ctrl+PgUp", ""));
        keys.extend(DELETE_KEYS.map(|key| PanelKey::new(key, "")));
    }
    keys
}

/// Whether the entry `name` is a file or a folder there (FAR's `IsCurrentFileCorrect`).
fn there(name: &str) -> bool {
    Path::new(name).exists()
}

impl TmpPanel {
    /// One of [`keys`] pressed on the panel `instance`.
    pub(crate) fn key(&mut self, instance: &str, press: &KeyPress) -> Result<KeyAnswer, Error> {
        self.list(instance)?;
        let common = self.options.common_panel;
        Ok(match press.key.as_str() {
            "F7" => self.remove_names(instance, press)?,
            "Alt+Shift+F3" => goto(press),
            "Alt+Shift+F12" if common => self.switch_menu(),
            "Alt+Shift+F2" => self.save_list(instance, press)?,
            "Enter" => self.enter(press),
            "Ctrl+PgUp" if self.options.safe_mode_panel => ctrl_pgup(press),
            key if DELETE_KEYS.contains(&key) => self.delete_key(press),
            other => match other.strip_prefix("Alt+Shift+").map(str::parse::<usize>) {
                Some(Ok(panel)) if common => self.switch_to(panel),
                Some(Ok(_)) => KeyAnswer::pass(),
                _ => self.file_key(press),
            },
        })
    }

    /// F7: TmpPanel's `ProcessRemoveKey` - the selected names (or the one under the cursor) off the
    /// list in one pass however many (`RemoveEmptyItems`); a quick view beside it shows its file again
    /// (`TmpClass.cpp:651-657`).
    fn remove_names(&mut self, instance: &str, press: &KeyPress) -> Result<KeyAnswer, Error> {
        let targets = press.targets();
        self.list(instance)?
            .remove_all(targets.iter().map(Path::new));
        Ok(KeyAnswer {
            reread_quick_view: true,
            ..KeyAnswer::reread()
        })
    }

    /// F3..F8 but F7: nothing on an entry gone (`TmpClass.cpp:575-579`), nor in safe mode on those that
    /// would need the plugin's `GetFiles` or `DeleteFiles`; else the program's.
    fn file_key(&self, press: &KeyPress) -> KeyAnswer {
        // Safe mode: nothing to act on whatever the cursor is on, a selection included (no `GetFiles`,
        // no `DeleteFiles`: FAR cannot copy or delete from the panel).
        let safe = self.options.safe_mode_panel && SAFE_SWALLOWED.contains(&press.key.as_str());
        if safe && !press.targets().is_empty() {
            return KeyAnswer::default();
        }
        match press.current.as_deref() {
            Some(current) if !there(current) => KeyAnswer::default(),
            _ => KeyAnswer::pass(),
        }
    }

    /// Del, Shift+Del, Alt+Del in safe mode: nothing on an entry or a selection (no `DeleteFiles`); else
    /// the program's.
    fn delete_key(&self, press: &KeyPress) -> KeyAnswer {
        match !press.targets().is_empty() && self.options.safe_mode_panel {
            true => KeyAnswer::default(),
            false => KeyAnswer::pass(),
        }
    }

    /// Enter (with any elements or in safe mode): an element that is no file put in the command line
    /// (TmpPanel's help, `TmpEng.hlf:109-110`; its code puts an empty text there, `TmpClass.cpp:581-588`:
    /// deviation D10); in safe mode a folder shown on its disk (FAR's `SetDirectory` closing the panel
    /// there, `TmpClass.cpp:148-158`) and a file left (no `GetFiles`); else the program's.
    fn enter(&self, press: &KeyPress) -> KeyAnswer {
        let Some(current) = press.current.as_deref() else {
            return KeyAnswer::pass();
        };
        if !there(current) {
            return match self.options.any_in_panel {
                true => KeyAnswer::go(KeyGo::CommandLine(current.to_string())),
                false => KeyAnswer::pass(),
            };
        }
        if !self.options.safe_mode_panel {
            return KeyAnswer::pass();
        }
        match Path::new(current).is_dir() {
            true => KeyAnswer::go(KeyGo::ActiveFolder(PathBuf::from(current))),
            false => KeyAnswer::default(),
        }
    }
}

/// Alt+Shift+F3: TmpPanel's - a folder shown in the other panel, a file's folder with the cursor on it;
/// nothing on ".." or a name gone.
fn goto(press: &KeyPress) -> KeyAnswer {
    let Some(current) = press.current.as_deref().map(PathBuf::from) else {
        return KeyAnswer::default();
    };
    match std::fs::metadata(&current) {
        Ok(found) if found.is_dir() => KeyAnswer::go(KeyGo::PassiveFolder(current)),
        Ok(_) => KeyAnswer::go(KeyGo::PassiveFile(current)),
        Err(_) => KeyAnswer::default(),
    }
}

/// Ctrl+PgUp in safe mode (`TmpClass.cpp:590-601`): the panel to the entry's folder, the cursor on it
/// (`GoToFile` on the active panel); on ".." or an entry gone the program's (back where it came from).
fn ctrl_pgup(press: &KeyPress) -> KeyAnswer {
    match press.current.as_deref().filter(|current| there(current)) {
        Some(current) => KeyAnswer::go(KeyGo::ActiveFile(PathBuf::from(current))),
        None => KeyAnswer::pass(),
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
