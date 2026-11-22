//! A key of a plugin's panel's own sent to it (ADR-0016 §14, FAR's `ProcessPanelInput`): the entry under the
//! cursor, the selection and the other panel's folder handed over, the answer copied before the plugin's
//! next call. Not a quiet call: the plugin may show windows (nothing waits for it).

use std::path::PathBuf;

use nc_plugin_api::{
    KEY_ACTIVE_FILE, KEY_ACTIVE_FOLDER, KEY_COMMAND_LINE, KEY_OPEN, KEY_PASS, KEY_PASSIVE_FILE,
    KEY_PASSIVE_FOLDER, KEY_REREAD, KEY_REREAD_PASSIVE, KEY_REREAD_QUICK_VIEW, KEY_SHELL_OPEN,
    NcKeyAnswer, NcKeyEvent, NcStr,
};

use super::{Loaded, text};

/// A key pressed on a plugin's panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyPress {
    pub key: String,
    /// The entry under the cursor (its name); empty on "..".
    pub current: String,
    pub selected: Vec<String>,
    /// The other panel's folder on a disk; empty: none.
    pub passive: String,
    /// The folder for names given without one (the disk's the plugin was opened from); empty: none.
    pub folder: String,
}

/// What the program does after the key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyAnswer {
    pub reread: bool,
    pub reread_passive: bool,
    /// The other panel, when it is a quick view, shows its file again.
    pub reread_quick_view: bool,
    pub go: Option<KeyGo>,
    /// The key left to the program: it does what the key does on any panel (FAR's `ProcessPanelInput`
    /// returning FALSE).
    pub pass: bool,
}

/// Where the program goes after the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyGo {
    PassiveFolder(PathBuf),
    PassiveFile(PathBuf),
    /// The plugin's panel instance of this name in the panel.
    Open(String),
    /// The panel the key was pressed on to this folder on its disk.
    ActiveFolder(PathBuf),
    /// The panel the key was pressed on to this file's folder, its cursor on it.
    ActiveFile(PathBuf),
    /// The command line's text (an empty one too).
    CommandLine(String),
    /// The file or folder opened by the system (FAR's `ShellExecute` "open").
    ShellOpen(PathBuf),
}

impl Loaded {
    /// `press` on the panel `instance` at `path`.
    pub fn panel_key(
        &self,
        instance: &str,
        path: &str,
        press: &KeyPress,
    ) -> Result<KeyAnswer, String> {
        let table = self.table();
        let selected: Vec<NcStr> = (press.selected.iter())
            .map(|name| NcStr::new(name))
            .collect();
        let event = NcKeyEvent {
            key: NcStr::new(&press.key),
            current: NcStr::new(&press.current),
            selected: if selected.is_empty() {
                std::ptr::null()
            } else {
                selected.as_ptr()
            },
            selected_count: selected.len(),
            passive: NcStr::new(&press.passive),
            folder: NcStr::new(&press.folder),
        };
        let mut answer = NcKeyAnswer::NONE;
        // SAFETY: as in `info`; the texts, the event and the answer's place live through the call.
        let code = unsafe {
            (table.panel_key)(
                table.state,
                NcStr::new(instance),
                NcStr::new(path),
                &raw const event,
                &raw mut answer,
            )
        };
        if code != 0 {
            return Err(self.last_error());
        }
        Ok(answer_of(&answer))
    }
}

/// A place to go by a path.
type Place = fn(PathBuf) -> KeyGo;

/// The plugin's answer as the program's: one place to go, the first its flags name (an empty path goes
/// nowhere, but for the command line); a key passed is only that.
pub(super) fn answer_of(raw: &NcKeyAnswer) -> KeyAnswer {
    let flag = |flag: u32| raw.flags & flag != 0;
    if flag(KEY_PASS) {
        return KeyAnswer {
            pass: true,
            ..KeyAnswer::default()
        };
    }
    let path = text(raw.path);
    let places: [(u32, Place); 5] = [
        (KEY_PASSIVE_FOLDER, KeyGo::PassiveFolder),
        (KEY_PASSIVE_FILE, KeyGo::PassiveFile),
        (KEY_ACTIVE_FOLDER, KeyGo::ActiveFolder),
        (KEY_ACTIVE_FILE, KeyGo::ActiveFile),
        (KEY_SHELL_OPEN, KeyGo::ShellOpen),
    ];
    let go = if path.is_empty() {
        None
    } else if let Some((_, place)) = places.iter().find(|(bit, _)| flag(*bit)) {
        Some(place(PathBuf::from(&path)))
    } else if flag(KEY_OPEN) {
        Some(KeyGo::Open(path.clone()))
    } else {
        None
    };
    let go = go.or_else(|| flag(KEY_COMMAND_LINE).then_some(KeyGo::CommandLine(path)));
    KeyAnswer {
        reread: flag(KEY_REREAD),
        reread_passive: flag(KEY_REREAD_PASSIVE),
        reread_quick_view: flag(KEY_REREAD_QUICK_VIEW),
        go,
        pass: false,
    }
}

#[cfg(test)]
#[path = "panel_keys_tests.rs"]
mod tests;
