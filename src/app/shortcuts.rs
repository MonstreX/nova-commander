//! FAR's folder shortcuts (`far/shortcuts.cpp`, `Panel::ExecShortcutFolder`): ten lists of folders.
//! Ctrl+Shift+0..9 adds the active panel's folder to the end of a list (Alt+Shift+0..9 too: Windows
//! Terminal keeps Ctrl+Shift+1..9 for its tabs); RCtrl+0..9 goes to its folder,
//! or, with several or none, shows the list to choose from. On the info panel the other panel goes
//! there; with the panel hidden, and in a dialog's edit line, the folder is put into the line (FAR's
//! `Edit::ProcessInsPath`). Commands > Folder shortcuts shows the ten (`shortcuts_menu`).

use ratatui::crossterm::event::KeyEvent;

use super::console_input::{ctrl_shift_digit as add_digit, right_ctrl_digit};
use super::dialog::Dialog;
use super::edit_insert::focused_input;
use super::shortcuts_menu::ShortcutMenu;
use super::{App, Overlay};
use crate::settings::FolderShortcut;
use crate::terminal::Environment;
use crate::vfs::VfsPath;

/// RCtrl+0 .. RCtrl+9.
pub const LISTS: usize = 10;

/// The name of a shortcut that is a separator line of its menu.
pub(super) const SEPARATOR: &str = "--";

/// What the folder chosen is for.
#[derive(Debug, Default)]
pub(super) enum Target {
    /// The panel goes there.
    #[default]
    Go,
    /// Into the command line (the panel is hidden).
    CommandLine,
    /// Into the focused edit line of this dialog, which comes back.
    Dialog(Box<Dialog>),
}

/// The lists and the menus that come back.
#[derive(Debug, Default)]
pub(super) struct ShortcutState {
    /// The ten lists (FAR's `Shortcuts\0` .. `Shortcuts\9`).
    pub(super) lists: Vec<Vec<FolderShortcut>>,
    pub(super) target: Target,
    /// Commands > Folder shortcuts under a list opened from it.
    pub(super) all: Option<ShortcutMenu>,
    /// The menu a dialog of it stands over.
    pub(super) behind: Option<ShortcutMenu>,
}

impl ShortcutState {
    pub(super) fn new(saved: &[Vec<FolderShortcut>]) -> Self {
        let mut lists = saved.to_vec();
        lists.resize(LISTS, Vec::new());
        Self {
            lists,
            ..Self::default()
        }
    }

    /// Whether one of its menus is open or comes back.
    fn busy(&self) -> bool {
        self.all.is_some() || self.behind.is_some() || !matches!(self.target, Target::Go)
    }
}

/// FAR's `is_service`: a shortcut with no folder, a line of the menu.
pub(super) fn is_service(shortcut: &FolderShortcut) -> bool {
    shortcut.folder.is_empty()
}

/// FAR's `MakeName`: the name with its variables (its `&` the hotkey), else the folder with its
/// variables (`&` shown as it is).
pub(super) fn shown_name(environment: &Environment, shortcut: &FolderShortcut) -> String {
    if shortcut.name.is_empty() {
        super::vmenu_keys::escape(&environment.expand(&shortcut.folder))
    } else {
        environment.expand(&shortcut.name)
    }
}

impl App {
    /// The panels' keys of the shortcuts; true when `key` was one.
    pub(super) fn handle_shortcut_key(&mut self, key: KeyEvent) -> bool {
        let files = self.active_panel().visible && self.shows_files(self.active);
        if let Some(list) = add_digit(&key) {
            // FAR's info panel and a hidden one take no shortcut; the tree gives its folder.
            if files || matches!(self.tree_target(), Some(Some(_))) {
                let shortcut = self.shortcut_of_panel();
                self.shortcuts.lists[list].push(shortcut);
            }
            return true;
        }
        let Some(list) = right_ctrl_digit(&key) else {
            return false;
        };
        let target = if self.active_panel().visible || !self.shows_files(self.active) {
            Target::Go
        } else {
            Target::CommandLine
        };
        self.use_shortcut(list, target);
        true
    }

    /// RCtrl+0..9 in a dialog's edit line: the folder goes into it.
    pub(super) fn shortcut_into_dialog(&mut self, dialog: Dialog, list: usize) {
        self.use_shortcut(list, Target::Dialog(Box::new(dialog)));
    }

    /// The active panel's folder as a new shortcut (FAR's `CreateShortcutFromPanel`); the tree's is the
    /// folder under its cursor (FAR's `TreeList::GetCurDir`).
    pub(super) fn shortcut_of_panel(&self) -> FolderShortcut {
        let folder = match self.tree_target() {
            Some(Some(folder)) => folder.display().to_string(),
            _ => self.active_panel().path.to_string(),
        };
        FolderShortcut {
            name: String::new(),
            folder,
        }
    }

    /// FAR's `Shortcuts::Get`: the one folder of the list is taken at once; with several, or none, the
    /// list is shown to choose from.
    pub(super) fn use_shortcut(&mut self, list: usize, target: Target) {
        let shortcuts = &self.shortcuts.lists[list];
        if let [only] = shortcuts.as_slice()
            && !is_service(only)
        {
            let folder = only.folder.clone();
            return self.shortcut_chosen(&folder, target);
        }
        // A menu of the shortcuts stands open already (a dialog of it is in front): no second one.
        if self.shortcuts.busy() {
            return self.give_back(target);
        }
        self.shortcuts.target = target;
        self.open_shortcut_list(list, false);
    }

    /// The folder chosen, its variables replaced: gone to, or put into the line.
    pub(super) fn shortcut_chosen(&mut self, folder: &str, target: Target) {
        let folder = self.environment.expand(folder);
        match target {
            Target::Go => self.go_to_shortcut(&folder),
            Target::CommandLine => self.command_line.insert_str(&folder),
            Target::Dialog(mut dialog) => {
                if let Some(input) = focused_input(&mut dialog) {
                    input.insert_str(&folder);
                }
                self.overlay = Some(Overlay::Dialog(*dialog));
            }
        }
    }

    /// Nothing chosen: the dialog the folder was for comes back.
    pub(super) fn give_back(&mut self, target: Target) {
        if let Target::Dialog(dialog) = target {
            self.overlay = Some(Overlay::Dialog(*dialog));
        }
    }

    /// FAR's `ExecFolder`: the files panel goes there, on the info panel the other one.
    fn go_to_shortcut(&mut self, folder: &str) {
        let side = if !self.shows_files(self.active) {
            self.active.other()
        } else {
            self.active
        };
        let path = VfsPath::parse(folder);
        if let Err(error) = self.panels[side.index()].change_dir(&path, None) {
            self.show_error("Change directory", &error);
        }
    }
}

#[cfg(test)]
#[path = "shortcuts_tests.rs"]
pub(super) mod tests;
