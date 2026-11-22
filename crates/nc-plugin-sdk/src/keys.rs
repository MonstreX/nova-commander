//! A plugin's own keys on its panel (ADR-0016 §14, FAR's `ProcessPanelInput` and `KeyBar`): the panel names
//! them with their key bar labels; one pressed comes to the plugin with the entry under the cursor and the
//! selection, the program never waiting for it; the answer says what the program does next.

use std::ffi::c_void;
use std::path::PathBuf;

use crate::api::{
    KEY_ACTIVE_FILE, KEY_ACTIVE_FOLDER, KEY_COMMAND_LINE, KEY_OPEN, KEY_PASS, KEY_PASSIVE_FILE,
    KEY_PASSIVE_FOLDER, KEY_REREAD, KEY_REREAD_PASSIVE, KEY_REREAD_QUICK_VIEW, KEY_SHELL_OPEN,
    NcKeyAnswer, NcKeyEvent, NcStr,
};
use crate::panel::{Error, host_text};
use crate::{Plugin, State, files, state};

/// A key of the panel's own: its name as the program writes keys ("F7", "Alt+Shift+F2",
/// "Alt+Shift+0") and its key bar label (empty: the program's stays).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelKey {
    pub key: String,
    pub label: String,
}

impl PanelKey {
    pub fn new(key: &str, label: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
        }
    }
}

/// A key pressed on the panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyPress {
    /// The key's name as the panel named it.
    pub key: String,
    /// The entry under the cursor (its name); `None` on "..".
    pub current: Option<String>,
    /// The names selected; none: the current entry is meant (FAR's `FCTL_GETSELECTEDPANELITEM`).
    pub selected: Vec<String>,
    /// The other panel's folder on a disk, if it shows one.
    pub passive: Option<PathBuf>,
    /// The folder a name given without one is in: the disk's folder the panel was opened from (FAR's
    /// current folder, `GetFullPath`).
    pub folder: Option<PathBuf>,
}

impl KeyPress {
    /// The entries the key acts on: the selection, or else the entry under the cursor.
    pub fn targets(&self) -> Vec<String> {
        if self.selected.is_empty() {
            self.current.iter().cloned().collect()
        } else {
            self.selected.clone()
        }
    }
}

/// Where the program goes after the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyGo {
    /// The other panel to this folder (FAR's `FCTL_SETPANELDIRECTORY` on the passive panel).
    PassiveFolder(PathBuf),
    /// The other panel to this file's folder, its cursor on it (TmpPanel's `GoToFile`).
    PassiveFile(PathBuf),
    /// The panel to the plugin's panel instance of this name: in place of the panel (TmpPanel's switch of
    /// panels); answered to Enter or Ctrl+PgDn on an entry, over the panel as a file's plugin opens
    /// (FAR's `OpenFilePlugin`), ".." at its root back to the entry (arclite's partition).
    Open(String),
    /// The panel the key was pressed on to this folder on its disk, leaving the plugin's panels (FAR's
    /// `FCTL_SETPANELDIRECTORY` on the active panel).
    ActiveFolder(PathBuf),
    /// The panel the key was pressed on to this file's folder, its cursor on it (TmpPanel's `GoToFile` on
    /// the active panel).
    ActiveFile(PathBuf),
    /// The command line's text, an empty one too (FAR's `FCTL_SETCMDLINE`).
    CommandLine(String),
    /// The file or folder opened by the system (FAR's `ShellExecute` "open", TmpPanel's menu from a
    /// list).
    ShellOpen(PathBuf),
}

/// What the program does after a key of the panel's own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyAnswer {
    /// The panel read again (FAR's `FCTL_UPDATEPANEL`).
    pub reread: bool,
    /// The other panel read again.
    pub reread_passive: bool,
    /// The other panel, when it is a quick view, shows its file again.
    pub reread_quick_view: bool,
    pub go: Option<KeyGo>,
    /// The key left to the program, which does what it does on any panel (FAR's `ProcessPanelInput`
    /// returning FALSE); the rest is not read then.
    pub pass: bool,
}

impl KeyAnswer {
    /// The panel read again, nothing more.
    pub fn reread() -> Self {
        Self {
            reread: true,
            ..Self::default()
        }
    }

    /// The key left to the program.
    pub fn pass() -> Self {
        Self {
            pass: true,
            ..Self::default()
        }
    }

    pub fn go(go: KeyGo) -> Self {
        Self {
            go: Some(go),
            ..Self::default()
        }
    }

    pub(crate) fn raw(self, kept: &mut crate::Kept) -> NcKeyAnswer {
        let mut flags = 0;
        if self.reread {
            flags |= KEY_REREAD;
        }
        if self.reread_passive {
            flags |= KEY_REREAD_PASSIVE;
        }
        if self.reread_quick_view {
            flags |= KEY_REREAD_QUICK_VIEW;
        }
        if self.pass {
            flags |= KEY_PASS;
        }
        // A place that is no exact text is none (R7 review A8).
        let mut place = |flag: u32, path: &std::path::Path| match path.to_str() {
            Some(path) => {
                flags |= flag;
                kept.text(path.to_string())
            }
            None => NcStr::EMPTY,
        };
        let path = match self.go {
            Some(KeyGo::PassiveFolder(path)) => place(KEY_PASSIVE_FOLDER, &path),
            Some(KeyGo::PassiveFile(path)) => place(KEY_PASSIVE_FILE, &path),
            Some(KeyGo::ActiveFolder(path)) => place(KEY_ACTIVE_FOLDER, &path),
            Some(KeyGo::ActiveFile(path)) => place(KEY_ACTIVE_FILE, &path),
            Some(KeyGo::Open(instance)) => place(KEY_OPEN, std::path::Path::new(&instance)),
            Some(KeyGo::CommandLine(text)) => place(KEY_COMMAND_LINE, std::path::Path::new(&text)),
            Some(KeyGo::ShellOpen(path)) => place(KEY_SHELL_OPEN, &path),
            None => NcStr::EMPTY,
        };
        NcKeyAnswer { flags, path }
    }
}

/// The host's key event as the plugin reads it.
///
/// # Safety
/// `event` is the host's, its texts and list alive through the call.
unsafe fn press(event: &NcKeyEvent) -> KeyPress {
    let text = |raw: NcStr| {
        // SAFETY: the caller promises the host's live texts.
        unsafe { host_text(raw) }.to_string()
    };
    let selected = if event.selected.is_null() || event.selected_count == 0 {
        Vec::new()
    } else {
        // SAFETY: the host's `selected_count` texts at `selected`, alive through the call.
        unsafe { std::slice::from_raw_parts(event.selected, event.selected_count) }
            .iter()
            .map(|raw| text(*raw))
            .collect()
    };
    KeyPress {
        key: text(event.key),
        current: Some(text(event.current)).filter(|name| !name.is_empty()),
        selected,
        passive: Some(text(event.passive))
            .filter(|folder| !folder.is_empty())
            .map(PathBuf::from),
        folder: Some(text(event.folder))
            .filter(|folder| !folder.is_empty())
            .map(PathBuf::from),
    }
}

pub(crate) unsafe extern "C" fn panel_key<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    event: *const NcKeyEvent,
    answer: *mut NcKeyAnswer,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state: &mut State<P> = unsafe { state::<P>(raw) };
    state.kept.clear();
    if event.is_null() {
        return files::refuse(state, "No key".to_string());
    }
    // SAFETY: the host's texts and event live through the call.
    let (instance, path, press) = unsafe { (host_text(instance), host_text(path), press(&*event)) };
    files::code(
        state,
        |plugin| plugin.panel_key(instance, path, &press),
        |state, told: KeyAnswer| {
            let made = told.raw(&mut state.kept);
            if !answer.is_null() {
                // SAFETY: the host's place for the answer, alive through the call; its text is kept
                // until the plugin's next call.
                unsafe { *answer = made };
            }
        },
    )
}

/// The default: a key the plugin named but does not handle.
pub(crate) fn unhandled() -> Result<KeyAnswer, Error> {
    Ok(KeyAnswer::default())
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
