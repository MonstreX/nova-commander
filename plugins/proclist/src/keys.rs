//! The panel's own keys (FAR's proclist `ProcessKey`, `DeleteFiles`, `KillProcess` and its key bar):
//! Enter brings the process's window to the front, Ctrl+R reads the processes again, Ctrl+F puts the
//! process's path into the command line, Shift+F1 and Shift+F2 lower and raise the priorities, F8, Del,
//! Shift+Del and Alt+Del end the processes with FAR's questions (Shift+F8 the one under the cursor only), Ctrl+F3..F12 sort, Ctrl+0..9 read the panel
//! again in its new mode (FAR's `FE_CHANGEVIEWMODE`); F5 writes the processes' texts, Shift+F3 shows one
//! after its options (`texts`). The keys FAR blanks on its key bar do nothing.

use std::io;

use nc_plugin_sdk::system::{KillError, step_priority, terminate, top_windows};
use nc_plugin_sdk::{KeyAnswer, KeyPress, Menu, Message, PanelKey, Which};

use crate::ProcList;
use crate::sort;
use crate::table::Process;

/// FAR's `MDeleteTitle`.
const KILL_TITLE: &str = "Kill process";
/// FAR's deleting keys, which come to a plugin's `DeleteFiles` (`filelist.cpp`).
const KILL_KEYS: [&str; 5] = ["F8", "Shift+F8", "Del", "Shift+Del", "Alt+Del"];
/// The keys FAR's key bar shows empty: nothing done on them.
const BLANK_KEYS: [&str; 4] = ["F7", "Alt+F6", "Shift+F5", "Shift+F7"];

/// The keys and their labels (FAR's `FKeys`).
pub fn keys() -> Vec<PanelKey> {
    let mut keys = vec![
        PanelKey::new("Enter", ""),
        PanelKey::new("Ctrl+R", ""),
        PanelKey::new("Ctrl+F", ""),
        PanelKey::new("Shift+F1", "Prior-"),
        PanelKey::new("Shift+F2", "Prior+"),
        PanelKey::new("Ctrl+F12", ""),
        PanelKey::new("F5", ""),
        PanelKey::new("Shift+F3", "View..."),
    ];
    keys.extend(KILL_KEYS.iter().map(|key| PanelKey::new(key, "Kill")));
    // A label of a space: the key bar's place shown empty.
    keys.extend(BLANK_KEYS.iter().map(|key| PanelKey::new(key, " ")));
    let fixed = sort::FIXED.iter().filter_map(|one| one.key);
    keys.extend(fixed.chain(sort::IGNORED).map(|key| PanelKey::new(key, "")));
    keys.extend((0..10).map(|digit| PanelKey::new(&format!("Ctrl+{digit}"), "")));
    keys
}

/// The system's text of `error`, without the number Rust adds.
pub fn system_text(error: &io::Error) -> String {
    let text = error.to_string();
    match text.rfind(" (os error") {
        Some(at) => text[..at].trim_end().to_string(),
        None => text,
    }
}

/// A path for the command line (FAR's `PutToCmdLine`): in quotes when it has a space, `&` or `^`, a
/// space after it.
pub fn command_line_text(path: &str) -> String {
    if path.contains([' ', '&', '^']) {
        format!("\"{path}\" ")
    } else {
        format!("{path} ")
    }
}

/// The entries a deleting key acts on: Shift+F8 the one under the cursor only (FAR's
/// `ReturnCurrentFile`), the others the selection or else that one.
pub fn kill_targets(press: &KeyPress) -> Vec<String> {
    if press.key == "Shift+F8" {
        press.current.iter().cloned().collect()
    } else {
        press.targets()
    }
}

/// The first question before ending `names` (FAR's `MDeleteProcess`, `MDeleteProcesses`).
pub fn kill_question(names: &[String]) -> String {
    match names {
        [one] => format!("Do you wish to kill \"{one}\"?"),
        _ => "Kill selected processes?".to_string(),
    }
}

impl ProcList {
    pub(crate) fn key(&mut self, instance: &str, press: &KeyPress) -> KeyAnswer {
        let key = press.key.as_str();
        match key {
            "Enter" => self.enter(instance, press),
            "Ctrl+R" => {
                self.read_now();
                KeyAnswer::pass()
            }
            "Ctrl+F" => {
                self.put_path(press);
                KeyAnswer::default()
            }
            "Shift+F1" | "Shift+F2" => self.priorities(press, key == "Shift+F2"),
            "Ctrl+F12" => self.sort_menu(instance),
            "F5" => self.write_texts(press),
            "Shift+F3" => self.view_with_options(press),
            _ if KILL_KEYS.contains(&key) => self.kill(press),
            _ if key.starts_with("Ctrl+") && key.len() == 6 => {
                // A view mode's key: the program changes the mode, then the panel is read in it.
                self.host.synchro().ask(0);
                KeyAnswer::pass()
            }
            _ => match sort::of_key(key) {
                Some(fixed) => self.sort_by(instance, fixed.mode, fixed.descending),
                None => KeyAnswer::default(),
            },
        }
    }

    /// The processes read again now; refused, the panel keeps the last read and the log tells why.
    fn read_now(&mut self) {
        let refused = (self.refresher.as_ref()).and_then(|refresher| refresher.read_now().err());
        if let Some(error) = refused {
            self.host
                .log(&format!("The processes could not be read: {error}"));
        }
    }

    /// The processes of the entries `keys`; `_Total` is none.
    fn targets(&self, keys: &[String]) -> Vec<Process> {
        let Some(refresher) = &self.refresher else {
            return Vec::new();
        };
        let table = refresher.table();
        (keys.iter())
            .filter_map(|key| table.by_key(key))
            .filter(|process| !process.is_total())
            .cloned()
            .collect()
    }

    /// Whether `pid` has processes under it in the tree.
    fn has_branch(&self, pid: u32) -> bool {
        (self.refresher.as_ref())
            .is_some_and(|refresher| crate::tree::has_children(&refresher.table().processes, pid))
    }

    fn current(&self, press: &KeyPress) -> Option<Process> {
        let current = press.current.as_ref()?;
        let table = self.refresher.as_ref()?.table();
        table.by_key(current).cloned()
    }

    /// FAR's Enter: with a command typed, or on "..", the program's; in the tree a branch folded or
    /// unfolded (owner, 2026-11-15); else the process's window to the front when Alt+Tab would show it.
    fn enter(&mut self, instance: &str, press: &KeyPress) -> KeyAnswer {
        if !self.host.command_line().is_empty() {
            return KeyAnswer::pass();
        }
        let Some(process) = self.current(press) else {
            return KeyAnswer::pass();
        };
        if self.instance(instance).sort == sort::TREE && self.has_branch(process.pid) {
            crate::tree::toggle(&mut self.instance(instance).unfolded, &process);
            return KeyAnswer::reread();
        }
        if let Some(window) = top_windows().get(&process.pid) {
            window.switch_to();
        }
        KeyAnswer::default()
    }

    fn put_path(&self, press: &KeyPress) {
        if let Some(process) = self.current(press) {
            let path = process.path.unwrap_or_default();
            self.host.insert_command_line(&command_line_text(&path));
        }
    }

    fn warning(&self, title: &str, lines: &[&str], buttons: &[&str]) -> Option<usize> {
        let message = Message::new(title, lines).warning().buttons(buttons);
        self.host.show(&message)
    }

    /// Shift+F1, Shift+F2 (FAR's priority classes one step down or up), asked first for more than one.
    fn priorities(&mut self, press: &KeyPress, up: bool) -> KeyAnswer {
        if press.selected.len() > 1 {
            let lines = ["Do you want to change priorities of the selected processes?"];
            let asked = Message::new("Change Priority", &lines).buttons(&["&Yes", "&No"]);
            if self.host.show(&asked) != Some(0) {
                return KeyAnswer::default();
            }
        }
        for process in self.targets(&press.targets()) {
            if process.pid == 0 {
                continue;
            }
            if let Err(error) = step_priority(process.pid, up) {
                self.warning("Error", &[&system_text(&error)], &["Ok"]);
            }
        }
        self.read_now();
        KeyAnswer::reread()
    }

    /// FAR's `KillProcess`: refused, asked whether to try with the debug privilege.
    fn end(&self, pid: u32) -> Result<(), io::Error> {
        let denied = || io::Error::from_raw_os_error(5);
        match terminate(pid, false) {
            Ok(()) => Ok(()),
            Err(KillError::Other(error)) => Err(error),
            Err(KillError::Denied) => {
                let lines = [
                    "Cannot kill the process",
                    "Retry with debug privileges?",
                    "It may be dangerous.",
                ];
                if self.warning(KILL_TITLE, &lines, &["&Yes", "&No"]) != Some(0) {
                    return Err(denied());
                }
                terminate(pid, true).map_err(|error| match error {
                    KillError::Other(error) => error,
                    KillError::Denied => denied(),
                })
            }
        }
    }

    /// F8, Del (FAR's `DeleteFiles`): asked, asked again for more than one, each ended; the first that
    /// cannot be stops it.
    fn kill(&mut self, press: &KeyPress) -> KeyAnswer {
        let targets = self.targets(&kill_targets(press));
        if targets.is_empty() {
            return KeyAnswer::default();
        }
        let names: Vec<String> = targets.iter().map(|process| process.name.clone()).collect();
        let first =
            Message::new(KILL_TITLE, &[&kill_question(&names)]).buttons(&["Kill", "Cancel"]);
        if self.host.show(&first) != Some(0) {
            return KeyAnswer::default();
        }
        if targets.len() > 1 {
            let again = format!("Do you wish to kill {} processes?", targets.len());
            if self.warning(KILL_TITLE, &[&again], &["Kill", "Cancel"]) != Some(0) {
                return KeyAnswer::default();
            }
        }
        for process in &targets {
            if let Err(error) = self.end(process.pid) {
                let cannot = format!("Cannot kill \"{}\"", process.name);
                self.warning(KILL_TITLE, &[&cannot, &system_text(&error)], &["Ok"]);
                break;
            }
        }
        self.read_now();
        KeyAnswer::reread()
    }

    /// The panel sorted by `mode` (Ctrl+F3..F11, a menu's item): the same again turns the order round.
    fn sort_by(&mut self, instance: &str, mode: u32, descending: bool) -> KeyAnswer {
        let reversed = (self.host.panel(Which::Active)).is_some_and(|state| state.reverse_sort);
        let state = self.instance(instance);
        let (code, reverse) = sort::picked(state.sort, mode, reversed, descending);
        state.sort = mode;
        self.host.set_sort(Which::Active, code, reverse);
        KeyAnswer::reread()
    }

    /// Ctrl+F12: FAR's "Sort by" menu.
    fn sort_menu(&mut self, instance: &str) -> KeyAnswer {
        let panel = self.host.panel(Which::Active).map_or(0, |state| state.sort);
        let current = self.instance(instance).sort;
        let (items, modes) = sort::menu(current, sort::known(panel));
        let checked = items.iter().position(|item| item.checked).unwrap_or(0);
        let menu = Menu::new("Sort by", items)
            .auto_hotkeys()
            .wrap()
            .selected(checked);
        let chosen = (self.host.menu(&menu))
            .and_then(|chosen| chosen.item)
            .and_then(|item| modes.get(item).copied().flatten());
        match chosen {
            Some(mode) => self.sort_by(instance, mode, sort::descending(mode)),
            None => KeyAnswer::default(),
        }
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
