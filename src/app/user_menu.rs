//! F2: FAR's user menu (`usermenu.cpp`). The local menu is `UserMenu.ini` in the active panel's folder,
//! else the main menu (`settings\UserMenu.ini`); Shift+F2 goes from one to the other, BS to the menu of a
//! folder above. Items run their commands (`user_menu_run`) or open their submenus (Enter, Right; Left
//! and Esc go back, Shift+F10 closes it all); a letter, a digit or F1..F24 is an item's hotkey. Ins,
//! F4, Del, Ctrl+Up / Ctrl+Down and Alt+F4 change the menu (`user_menu_edit`), saved at once.

use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::metasymbols::{Facts, PanelFacts, expand_label};
use super::vmenu_keys;
use super::{App, MainView, Overlay, Panel, Screen};
use crate::settings::user_menu::{self, Item, MENU_FILE};

/// Which menu is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuPlace {
    /// `UserMenu.ini` of this folder.
    Local(PathBuf),
    Main,
}

/// One line of the level shown, as the menu writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMenuLine {
    /// The hotkey (marked with `&`) and the label, symbols replaced.
    pub text: String,
    pub separator: bool,
    pub submenu: bool,
}

/// An open user menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMenu {
    pub place: MenuPlace,
    pub file: PathBuf,
    pub items: Vec<Item>,
    /// The submenus open: the place of each in the level above.
    pub open: Vec<usize>,
    pub selected: usize,
    pub lines: Vec<UserMenuLine>,
    pub title: String,
    /// The first line shown (FAR's `TopPos`) and the lines shown, kept by the renderer.
    pub top: Cell<usize>,
    pub rows: Cell<usize>,
    /// Where each open submenu's cursor was, to come back to.
    pub cursors: Vec<usize>,
    /// The levels above the one shown as they were when it opened: FAR draws a submenu over them.
    pub under: Vec<UserMenuLevel>,
}

/// A level of the user menu under the submenu shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMenuLevel {
    pub lines: Vec<UserMenuLine>,
    pub title: String,
    pub selected: usize,
    pub top: Cell<usize>,
    pub rows: Cell<usize>,
}

/// What the app keeps of the user menu.
#[derive(Debug, Default)]
pub(super) struct UserMenuState {
    /// `settings\UserMenu.ini`; none in tests that do not set it.
    pub(super) main_file: Option<PathBuf>,
    /// The menu a dialog of it stands over.
    pub(super) behind: Option<UserMenu>,
    /// The commands of an item still to run.
    pub(super) run: Option<super::user_menu_run::MenuRun>,
    /// An item's dialog that comes back (it was wrong, or "Save changes?" was cancelled).
    pub(super) form: Option<Box<(super::form::Form, super::user_menu_edit::ItemEdit)>>,
    /// The menu to read again once the editor of its file (Alt+F4) is closed.
    pub(super) reread: Option<Look>,
    /// The file Alt+F4 made for the editor, taken away if it is still empty.
    pub(super) made_empty: Option<PathBuf>,
    /// The files `!@!` made, and when.
    pub(super) list_files: Vec<(PathBuf, std::time::Instant)>,
    /// Where list files are made; the system's temporary folder when none is set (tests set theirs).
    pub(super) list_dir: Option<PathBuf>,
    /// The command Apply command was last given (FAR's `strPrevCommand`).
    pub(super) apply_last: String,
}

/// Where to look for a menu.
#[derive(Debug, Clone)]
pub(super) enum Look {
    /// F2: this folder's, else the main one.
    Here(PathBuf),
    /// Shift+F2 from the main menu: this folder's or the nearest above, else the main one.
    Nearest(PathBuf),
    /// BS: the nearest above this folder, else the main one.
    Above(PathBuf),
    Main,
    /// Commands > Edit user menu > Local: this folder's, made if it is not there.
    Only(PathBuf),
    /// This file again, as this menu.
    File(MenuPlace, PathBuf),
}

/// FAR's bottom title of the menu.
pub const USER_MENU_KEYS: &str = "Ins, Del, F4, Alt+F4, Ctrl+Up, Ctrl+Down";

/// FAR's `PrepareHotKey`: `F1`..`F24` are function keys.
pub(super) fn function_key(hotkey: &str) -> Option<u8> {
    let number = hotkey.strip_prefix(['F', 'f'])?.parse::<u8>().ok()?;
    (1..=24).contains(&number).then_some(number)
}

/// FAR's `remove_highlight`: a label without its hotkey marks, `&&` as one `&`.
pub(super) fn without_marks(text: &str) -> String {
    text.replace("&&", "\0").replace('&', "").replace('\0', "&")
}

/// FAR's `FillUserMenu`: the hotkey in a column of four (five with `&`), then the label shown.
fn line_text(item: &Item, label: &str) -> String {
    let label = super::vmenu_keys::escape(label);
    let hotkey = if item.hotkey == "&" {
        "&&".to_string()
    } else {
        item.hotkey.clone()
    };
    let marked = !hotkey.is_empty() && function_key(&hotkey).is_none();
    let width: usize = if hotkey.starts_with('&') { 5 } else { 4 };
    let padding = " ".repeat(width.saturating_sub(hotkey.chars().count()));
    format!("{}{hotkey}{padding}{label}", if marked { "&" } else { "" })
}

/// What the symbols of the panel `panel` speak of.
fn panel_facts(panel: &Panel) -> PanelFacts {
    // A plugin's entry of a real file names that file (FAR's real names, P2.4).
    let named = |entry: &crate::vfs::Entry| {
        (entry.real_path.as_ref())
            .map_or_else(|| entry.name.clone(), |real| real.display().to_string())
    };
    let current = panel
        .current()
        .filter(|entry| !entry.is_parent)
        .map(named)
        .filter(|name| name != "..")
        .unwrap_or_default();
    let mut selected: Vec<String> = (panel.entries.iter())
        .filter(|entry| panel.selected.contains(&entry.name))
        .map(named)
        .collect();
    if selected.is_empty() && !current.is_empty() {
        selected.push(current.clone());
    }
    PanelFacts {
        dir: panel.path.to_string(),
        current,
        selected,
    }
}

/// Finds and reads the menu `look` asks for.
fn find(look: Look, main: Option<&Path>) -> (MenuPlace, PathBuf, io::Result<Vec<Item>>) {
    let at = |dir: &Path| dir.join(MENU_FILE);
    let load = |file: &Path| user_menu::load(file, crate::platform::locale::decode_oem);
    let (from, first) = match &look {
        Look::Here(dir) => (Some(dir.clone()), true),
        Look::Nearest(dir) => (Some(dir.clone()), false),
        Look::Above(dir) => (dir.parent().map(Path::to_path_buf), false),
        Look::Main => (None, true),
        Look::Only(dir) => {
            let file = at(dir);
            let items = load(&file).map(Option::unwrap_or_default);
            return (MenuPlace::Local(dir.clone()), file, items);
        }
        Look::File(place, file) => {
            let items = load(file).map(Option::unwrap_or_default);
            return (place.clone(), file.clone(), items);
        }
    };
    let mut dir = from;
    while let Some(folder) = dir {
        let file = at(&folder);
        match load(&file) {
            Ok(Some(items)) => return (MenuPlace::Local(folder), file, Ok(items)),
            // One that cannot be read is said, not passed over for the main menu.
            Err(error) => return (MenuPlace::Local(folder), file, Err(error)),
            Ok(None) => {}
        }
        dir = if first {
            None
        } else {
            folder.parent().map(Path::to_path_buf)
        };
    }
    let Some(main) = main else {
        return (MenuPlace::Main, PathBuf::new(), Ok(Vec::new()));
    };
    let items = load(main).map(Option::unwrap_or_default);
    (MenuPlace::Main, main.to_path_buf(), items)
}

impl UserMenu {
    /// The items of the level shown.
    pub(super) fn level(&self) -> &[Item] {
        let mut items = &self.items[..];
        for &at in &self.open {
            match items.get(at).and_then(|item| item.submenu.as_deref()) {
                Some(submenu) => items = submenu,
                None => return &[],
            }
        }
        items
    }

    pub(super) fn level_mut(&mut self) -> Option<&mut Vec<Item>> {
        let mut items = &mut self.items;
        for &at in &self.open {
            items = items.get_mut(at)?.submenu.as_mut()?;
        }
        Some(items)
    }

    pub(super) fn current(&self) -> Option<&Item> {
        self.level()
            .get(self.selected)
            .filter(|item| !item.is_separator())
    }
}

impl App {
    /// The facts the menu's symbols speak of.
    pub(super) fn menu_facts(&self) -> Facts {
        Facts {
            active: (self.quick_view_facts(self.active))
                .or_else(|| self.info_panel_facts(self.active))
                .unwrap_or_else(|| panel_facts(self.active_panel())),
            passive: (self.quick_view_facts(self.active.other()))
                .or_else(|| self.info_panel_facts(self.active.other()))
                .unwrap_or_else(|| panel_facts(self.passive_panel())),
            active_is_left: self.active == super::Side::Left,
        }
    }

    /// F2.
    pub(super) fn open_user_menu(&mut self) {
        let dir = self.active_panel().path.as_local().map(Path::to_path_buf);
        self.look_for_menu(dir.map_or(Look::Main, Look::Here));
    }

    /// Reads the menu `look` asks for, beside the UI, and shows it.
    pub(super) fn look_for_menu(&mut self, look: Look) {
        let main = self.user_menu.main_file.clone();
        self.wait_for(Vec::new(), move || {
            let (place, file, items) = find(look, main.as_deref());
            Box::new(move |app: &mut App| match items {
                Ok(items) => app.show_user_menu(place, file, items),
                Err(error) => app.show_error("User menu", &error),
            })
        });
        self.label_waiting("Reading the user menu…");
    }

    fn show_user_menu(&mut self, place: MenuPlace, file: PathBuf, items: Vec<Item>) {
        let mut menu = UserMenu {
            place,
            file,
            items,
            open: Vec::new(),
            selected: 0,
            lines: Vec::new(),
            title: String::new(),
            top: Cell::new(0),
            rows: Cell::new(1),
            cursors: Vec::new(),
            under: Vec::new(),
        };
        self.fill_user_menu(&mut menu);
        menu.selected = vmenu_keys::first_from(&separators(&menu), 0);
        self.overlay = Some(Overlay::UserMenu(menu));
    }

    /// The lines and the title of the level shown, as the panels are now.
    pub(super) fn fill_user_menu(&self, menu: &mut UserMenu) {
        let facts = self.menu_facts();
        menu.lines = (menu.level().iter())
            .map(|item| UserMenuLine {
                // A separator's label is shown as it is written.
                text: if item.is_separator() {
                    item.label.clone()
                } else {
                    line_text(item, &self.shown_label(&item.label, &facts))
                },
                separator: item.is_separator(),
                submenu: item.submenu.is_some(),
            })
            .collect();
        let mut title = match menu.place {
            MenuPlace::Local(_) => "Local menu".to_string(),
            MenuPlace::Main => "Main menu".to_string(),
        };
        let mut items = &menu.items[..];
        for &at in &menu.open {
            let Some(item) = items.get(at) else { break };
            let label = without_marks(&self.shown_label(&item.label, &facts));
            title = format!("{title} \u{bb} {label}");
            items = item.submenu.as_deref().unwrap_or(&[]);
        }
        menu.title = title;
        let count = menu.lines.len();
        let at = menu.selected.min(count.saturating_sub(1));
        menu.selected = vmenu_keys::first_from(&separators(menu), at);
    }

    /// FAR's label: special symbols replaced, then the environment's variables.
    fn shown_label(&self, label: &str, facts: &Facts) -> String {
        let label = expand_label(label, facts);
        crate::platform::process::expand_variables(&label, &|name| self.environment.get(name))
    }

    pub(super) fn handle_user_menu_key(&mut self, mut menu: UserMenu, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let skip = separators(&menu);
        // FAR's `FuncPos` first: an item's F-key hotkey wins over F4 and F10.
        if let KeyCode::F(number) = key.code
            && key.modifiers.is_empty()
            && let Some(at) =
                (menu.level().iter()).position(|item| function_key(&item.hotkey) == Some(number))
        {
            menu.selected = at;
            return self.choose_menu_item(menu);
        }
        match key.code {
            KeyCode::F(10) if shift => return,
            KeyCode::F(2) if shift => return self.switch_user_menu(&menu),
            KeyCode::F(4) if alt => return self.edit_menu_file(menu),
            KeyCode::F(4) => return self.edit_user_menu_item(menu, false),
            KeyCode::Insert => return self.edit_user_menu_item(menu, true),
            KeyCode::Delete => return self.ask_remove_menu_item(menu),
            KeyCode::Up | KeyCode::Down if ctrl => {
                return self.move_menu_item(menu, key.code == KeyCode::Up);
            }
            KeyCode::Backspace => {
                if let MenuPlace::Local(dir) = &menu.place {
                    return self.look_for_menu(Look::Above(dir.clone()));
                }
            }
            KeyCode::Esc | KeyCode::Left | KeyCode::F(10) => {
                if !self.close_submenu(&mut menu) && key.code != KeyCode::Left {
                    return;
                }
            }
            KeyCode::Right if menu.current().is_some_and(|item| item.submenu.is_some()) => {
                self.open_submenu(&mut menu);
            }
            KeyCode::Enter => return self.choose_menu_item(menu),
            KeyCode::Char(' ') => {}
            KeyCode::Char(ch) if !ctrl => {
                let ch = ch.to_lowercase().next().unwrap_or(ch);
                if let Some(at) = (menu.level().iter()).position(|item| {
                    !item.is_separator()
                        && function_key(&item.hotkey).is_none()
                        && item.hotkey.to_lowercase().chars().eq(std::iter::once(ch))
                }) {
                    menu.selected = at;
                    return self.choose_menu_item(menu);
                }
            }
            _ => {
                menu.selected = vmenu_keys::step(&skip, menu.selected, key.code, menu.rows.get());
            }
        }
        self.overlay = Some(Overlay::UserMenu(menu));
    }

    /// Shift+F2: the main menu from a local one, the nearest local one from the main.
    fn switch_user_menu(&mut self, menu: &UserMenu) {
        let here = self.active_panel().path.as_local().map(Path::to_path_buf);
        match (&menu.place, here) {
            (MenuPlace::Main, Some(dir)) => self.look_for_menu(Look::Nearest(dir)),
            _ => self.look_for_menu(Look::Main),
        }
    }

    fn open_submenu(&mut self, menu: &mut UserMenu) {
        menu.under.push(UserMenuLevel {
            lines: menu.lines.clone(),
            title: menu.title.clone(),
            selected: menu.selected,
            top: menu.top.clone(),
            rows: menu.rows.clone(),
        });
        menu.open.push(menu.selected);
        menu.cursors.push(menu.selected);
        menu.selected = 0;
        menu.top.set(0);
        self.fill_user_menu(menu);
        menu.selected = vmenu_keys::first_from(&separators(menu), 0);
    }

    /// Esc and Left in a submenu: the level above; false at the top.
    fn close_submenu(&mut self, menu: &mut UserMenu) -> bool {
        if menu.open.pop().is_none() {
            return false;
        }
        menu.under.pop();
        menu.selected = menu.cursors.pop().unwrap_or(0);
        menu.top.set(0);
        self.fill_user_menu(menu);
        true
    }

    /// Enter on an item: its submenu opens, or its commands run.
    fn choose_menu_item(&mut self, mut menu: UserMenu) {
        let Some(item) = menu.current().cloned() else {
            self.overlay = Some(Overlay::UserMenu(menu));
            return;
        };
        if item.submenu.is_some() {
            self.open_submenu(&mut menu);
            self.overlay = Some(Overlay::UserMenu(menu));
            return;
        }
        let label = without_marks(&self.shown_label(&item.label, &self.menu_facts()));
        self.run_menu_commands(&label, &item.commands);
    }

    /// The menu a dialog of it stood over comes back once the dialog is gone; true when it did.
    pub(super) fn resume_user_menu(&mut self) -> bool {
        if self.overlay.is_some() || self.waiting.is_some() {
            return false;
        }
        if !matches!(self.screen, Screen::Panels) || self.main_view != MainView::Panels {
            return false;
        }
        let Some(mut menu) = self.user_menu.behind.take() else {
            return false;
        };
        self.fill_user_menu(&mut menu);
        self.overlay = Some(Overlay::UserMenu(menu));
        true
    }

    /// Where the main menu is kept (`settings\UserMenu.ini`).
    pub fn set_main_menu_file(&mut self, file: PathBuf) {
        self.user_menu.main_file = Some(file);
    }

    /// Commands > Edit user menu: FAR's question, the main menu or this folder's.
    pub(super) fn ask_user_menu_type(&mut self) {
        self.show_dialog(super::Dialog::Confirm {
            title: "User menu".to_string(),
            lines: vec!["Choose user menu type to edit".to_string()],
            buttons: vec!["&Main", "&Local", "Cancel"],
            focused: 0,
            danger: false,
            action: super::dialog::ConfirmAction::ChooseUserMenu,
        });
    }

    pub(super) fn open_chosen_user_menu(&mut self, local: bool) {
        let dir = self.active_panel().path.as_local().map(Path::to_path_buf);
        match dir.filter(|_| local) {
            Some(dir) => self.look_for_menu(Look::Only(dir)),
            None => self.look_for_menu(Look::Main),
        }
    }
}

/// Which lines of the level shown the cursor passes over.
pub(super) fn separators(menu: &UserMenu) -> Vec<bool> {
    menu.lines.iter().map(|line| line.separator).collect()
}

#[cfg(test)]
#[path = "user_menu_tests.rs"]
mod tests;
