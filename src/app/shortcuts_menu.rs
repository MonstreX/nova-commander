//! The menus of the folder shortcuts as FAR's (`Shortcuts::Select`, `Shortcuts::Configure`):
//! - a list, "Folder shortcuts (n)": its folders, each with a hotkey of its own; Enter takes one, Ins
//!   adds the active panel's folder before the cursor, Del takes the line out, F4 changes it, Ctrl+Up /
//!   Ctrl+Down move it; RCtrl and the list's own digit goes down, another digit shows that list;
//! - Commands > Folder shortcuts: the ten, each line "RCtrl+n │" and its first folder; Enter goes there
//!   (or shows the list), Ins adds to an empty list, Del and F4 change a list of one; any other list is
//!   opened for them.
//!
//! A list opened from the ten is FAR's raw one: its title and separator lines are shown as the lines
//! they are, and Enter, Esc or another list's key go back to the ten.

use std::cell::Cell;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::console_input::right_ctrl_digit;
use super::shortcuts::{LISTS, SEPARATOR, Target, is_service, shown_name};
use super::vmenu_keys;
use super::{App, Overlay};

/// The keys in the ten's bottom frame; a list has [`super::EDIT_LIST_KEYS`].
pub const SHORTCUTS_KEYS: &str = "Ins, Del, F4";

const TITLE: &str = "Folder shortcuts";

/// A line of a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutLine {
    /// With its hotkey marked by `&`.
    pub text: String,
    pub separator: bool,
    /// A list of several folders: `►` after it.
    pub submenu: bool,
}

/// An open menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutMenu {
    /// The list of RCtrl and this digit; `None`: Commands > Folder shortcuts, the ten.
    pub list: Option<usize>,
    /// Opened from the ten (FAR's raw mode).
    pub raw: bool,
    pub title: String,
    pub lines: Vec<ShortcutLine>,
    /// The shortcut of each line in its list; for the ten, the list.
    pub items: Vec<usize>,
    pub selected: usize,
    pub top: Cell<usize>,
    pub rows: Cell<usize>,
}

impl ShortcutMenu {
    fn new(list: Option<usize>, raw: bool) -> Self {
        Self {
            list,
            raw,
            title: String::new(),
            lines: Vec::new(),
            items: Vec::new(),
            selected: 0,
            top: Cell::new(0),
            rows: Cell::new(1),
        }
    }

    /// The keys in its bottom frame.
    pub fn keys(&self) -> &'static str {
        if self.list.is_some() {
            super::EDIT_LIST_KEYS
        } else {
            SHORTCUTS_KEYS
        }
    }

    fn skip(&self) -> Vec<bool> {
        self.lines.iter().map(|line| line.separator).collect()
    }

    /// The shortcut (or list) under the cursor; none on a separator (a list of separators alone leaves
    /// the cursor there).
    fn item(&self) -> Option<usize> {
        let line = self.lines.get(self.selected)?;
        if line.separator {
            return None;
        }
        self.items.get(self.selected).copied()
    }

    fn select_item(&mut self, item: usize) {
        if let Some(line) = self.items.iter().position(|&known| known == item) {
            self.selected = line;
        }
    }

    fn hotkey_line(&self, ch: char) -> Option<usize> {
        let ch = ch.to_lowercase().next()?;
        (self.lines.iter())
            .position(|line| !line.separator && vmenu_keys::marked_hotkey(&line.text) == Some(ch))
    }
}

/// What a key does in a menu.
enum Act {
    Stay,
    Close,
    Choose,
    Insert,
    Remove,
    Change,
    Move(bool),
    OtherList(usize),
}

fn act_of(menu: &mut ShortcutMenu, key: KeyEvent) -> Act {
    if let Some(digit) = right_ctrl_digit(&key) {
        if menu.list == Some(digit) {
            menu.selected = vmenu_keys::step(&menu.skip(), menu.selected, KeyCode::Down, 1);
            return Act::Stay;
        }
        return Act::OtherList(digit);
    }
    let plain = key.modifiers.is_empty();
    let ctrl = key.modifiers == KeyModifiers::CONTROL;
    match key.code {
        KeyCode::Esc | KeyCode::F(10) => Act::Close,
        KeyCode::Enter if plain => Act::Choose,
        KeyCode::Insert if plain => Act::Insert,
        KeyCode::Delete if plain => Act::Remove,
        KeyCode::F(4) if plain => Act::Change,
        KeyCode::Up | KeyCode::Down if ctrl && menu.list.is_some() => {
            Act::Move(key.code == KeyCode::Up)
        }
        KeyCode::Char(ch) if !key.modifiers.intersects(KeyModifiers::CONTROL) => {
            match menu.hotkey_line(ch) {
                Some(line) => {
                    menu.selected = line;
                    Act::Choose
                }
                None => Act::Stay,
            }
        }
        code => {
            menu.selected = vmenu_keys::step(&menu.skip(), menu.selected, code, menu.rows.get());
            Act::Stay
        }
    }
}

impl App {
    /// Commands > Folder shortcuts.
    pub(super) fn open_folder_shortcuts(&mut self) {
        let mut menu = ShortcutMenu::new(None, false);
        self.fill_shortcut_menu(&mut menu);
        self.overlay = Some(Overlay::Shortcuts(menu));
    }

    /// The list of RCtrl+`list`.
    pub(super) fn open_shortcut_list(&mut self, list: usize, raw: bool) {
        let mut menu = ShortcutMenu::new(Some(list), raw);
        self.fill_shortcut_menu(&mut menu);
        menu.selected = vmenu_keys::first_from(&menu.skip(), 0);
        self.overlay = Some(Overlay::Shortcuts(menu));
    }

    /// The lines as the lists are now, the cursor kept on a line that can take it.
    pub(super) fn fill_shortcut_menu(&self, menu: &mut ShortcutMenu) {
        menu.lines.clear();
        menu.items.clear();
        match menu.list {
            Some(list) => self.fill_list_lines(menu, list),
            None => self.fill_all_lines(menu),
        }
        let last = menu.lines.len().saturating_sub(1);
        menu.selected = vmenu_keys::first_from(&menu.skip(), menu.selected.min(last));
    }

    /// FAR's `FillMenu`: outside the raw mode a line with no folder is a separator (`--`) or, before
    /// any line, the title; the lines' hotkeys are given (`VMENU_AUTOHIGHLIGHT`).
    fn fill_list_lines(&self, menu: &mut ShortcutMenu, list: usize) {
        menu.title = format!("{TITLE} ({list})");
        for (at, shortcut) in self.shortcuts.lists[list].iter().enumerate() {
            let name = shown_name(&self.environment, shortcut);
            if name.is_empty() {
                continue;
            }
            let service = !menu.raw && is_service(shortcut);
            if service && name != SEPARATOR {
                if menu.lines.is_empty() {
                    menu.title = name;
                }
                continue;
            }
            menu.lines.push(ShortcutLine {
                text: if service { String::new() } else { name },
                separator: service,
                submenu: false,
            });
            menu.items.push(at);
        }
        let mut texts: Vec<String> = menu.lines.iter().map(|line| line.text.clone()).collect();
        vmenu_keys::auto_hotkeys(&mut texts);
        for (line, text) in menu.lines.iter_mut().zip(texts) {
            line.text = text;
        }
    }

    /// FAR's `MakeListName`: "RCtrl+n │" and the list's first folder, or `<none>`.
    fn fill_all_lines(&self, menu: &mut ShortcutMenu) {
        menu.title = TITLE.to_string();
        for (list, shortcuts) in self.shortcuts.lists.iter().enumerate().take(LISTS) {
            let first = (shortcuts.first()).map_or_else(
                || "<none>".to_string(),
                |first| shown_name(&self.environment, first),
            );
            menu.lines.push(ShortcutLine {
                text: format!("RCtrl+&{list} \u{2502} {first}"),
                separator: false,
                submenu: shortcuts.len() > 1,
            });
            menu.items.push(list);
        }
    }

    pub(super) fn handle_shortcut_menu_key(&mut self, mut menu: ShortcutMenu, key: KeyEvent) {
        let act = act_of(&mut menu, key);
        match menu.list {
            Some(list) => self.list_act(menu, list, act),
            None => self.all_act(menu, act),
        }
    }

    fn list_act(&mut self, mut menu: ShortcutMenu, list: usize, act: Act) {
        match act {
            Act::Stay => {}
            Act::Close => return self.close_shortcut_list(menu.raw),
            Act::Choose => {
                // FAR's menu with nothing to take closes as with Esc.
                let Some(at) = menu.item() else {
                    return self.close_shortcut_list(menu.raw);
                };
                if menu.raw {
                    return self.close_shortcut_list(true);
                }
                let folder = self.shortcuts.lists[list][at].folder.clone();
                let target = std::mem::take(&mut self.shortcuts.target);
                return self.shortcut_chosen(&folder, target);
            }
            Act::OtherList(other) => {
                if menu.raw {
                    return self.close_shortcut_list(true);
                }
                return self.open_shortcut_list(other, false);
            }
            Act::Insert => {
                let at = menu.item().unwrap_or(self.shortcuts.lists[list].len());
                let shortcut = self.shortcut_of_panel();
                self.shortcuts.lists[list].insert(at, shortcut);
                self.fill_shortcut_menu(&mut menu);
                menu.select_item(at);
            }
            Act::Remove => {
                if let Some(at) = menu.item() {
                    self.shortcuts.lists[list].remove(at);
                    self.fill_shortcut_menu(&mut menu);
                }
            }
            Act::Change => {
                if let Some(at) = menu.item() {
                    let raw = menu.raw;
                    self.shortcuts.behind = Some(menu);
                    return self.edit_shortcut(list, at, raw);
                }
            }
            Act::Move(up) => self.move_shortcut(&mut menu, list, up),
        }
        self.overlay = Some(Overlay::Shortcuts(menu));
    }

    /// Ctrl+Up / Ctrl+Down: the shortcut and the one before or after it in the list change places.
    fn move_shortcut(&mut self, menu: &mut ShortcutMenu, list: usize, up: bool) {
        let Some(at) = menu.item() else {
            return;
        };
        let other = if up { at.checked_sub(1) } else { Some(at + 1) };
        let shortcuts = &mut self.shortcuts.lists[list];
        if let Some(other) = other.filter(|&other| other < shortcuts.len()) {
            shortcuts.swap(at, other);
            self.fill_shortcut_menu(menu);
            menu.select_item(other);
        }
    }

    /// A list closed: the ten come back under a raw one; otherwise the dialog the folder was for.
    fn close_shortcut_list(&mut self, raw: bool) {
        if raw && let Some(mut all) = self.shortcuts.all.take() {
            self.fill_shortcut_menu(&mut all);
            self.overlay = Some(Overlay::Shortcuts(all));
            return;
        }
        let target = std::mem::take(&mut self.shortcuts.target);
        self.give_back(target);
    }

    fn all_act(&mut self, mut menu: ShortcutMenu, act: Act) {
        let Some(list) = menu.item() else {
            return;
        };
        let count = self.shortcuts.lists[list].len();
        match act {
            Act::Close => return,
            // FAR's ten take no RCtrl digits: the menu stays.
            Act::OtherList(_) => {}
            Act::Choose => return self.use_shortcut(list, Target::Go),
            Act::Insert if count == 0 => {
                let shortcut = self.shortcut_of_panel();
                self.shortcuts.lists[list].push(shortcut);
                self.fill_shortcut_menu(&mut menu);
            }
            Act::Remove if count == 1 => {
                self.shortcuts.lists[list].clear();
                self.fill_shortcut_menu(&mut menu);
            }
            Act::Change if count == 1 => {
                self.shortcuts.behind = Some(menu);
                return self.edit_shortcut(list, 0, true);
            }
            Act::Insert | Act::Remove | Act::Change if count > 0 => {
                // FAR: a list of several is changed in its own menu.
                self.shortcuts.all = Some(menu);
                return self.open_shortcut_list(list, true);
            }
            _ => {}
        }
        self.overlay = Some(Overlay::Shortcuts(menu));
    }

    /// The menu a dialog of it stood over comes back once the dialog is gone; true when it did.
    pub(super) fn resume_shortcut_menu(&mut self) -> bool {
        if self.overlay.is_some()
            || self.waiting.is_some()
            || !matches!(self.screen, super::Screen::Panels)
            || self.main_view != super::MainView::Panels
        {
            return false;
        }
        let Some(mut menu) = self.shortcuts.behind.take() else {
            return false;
        };
        self.fill_shortcut_menu(&mut menu);
        self.overlay = Some(Overlay::Shortcuts(menu));
        true
    }
}

#[cfg(test)]
#[path = "shortcuts_menu_tests.rs"]
mod tests;
