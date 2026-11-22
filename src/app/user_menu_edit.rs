//! Changing the user menu as FAR's `UserMenu::EditMenu` and its keys: Ins asks whether a command or a
//! menu comes and inserts it before the cursor, F4 (and Shift+F4) changes the item under it, Del takes it
//! out after a question, Ctrl+Up / Ctrl+Down move it, Alt+F4 opens the menu's file in the editor. Each
//! change is saved at once; an empty menu takes its file away.

use super::dialog::{ConfirmAction, Dialog, FormAction};
use super::form::{Control, Form, Item as FormItem};
use super::line_input::LineInput;
use super::user_menu::{Look, UserMenu, function_key};
use super::{App, Overlay};
use crate::settings::user_menu::{self, Item};

const TITLE: &str = "User menu";
/// FAR's dialog: 76 columns, commands in ten lines.
const DIALOG_WIDTH: u16 = 76;
const FIELD_WIDTH: u16 = 66;
const COMMAND_LINES: u16 = 10;
const HOTKEY: usize = 1;
const LABEL: usize = 3;
/// The first command line's item.
const FIRST_COMMAND: usize = 6;

/// What a dialog of the item is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemEdit {
    /// Ins: a new item before the cursor.
    pub insert: bool,
    pub submenu: bool,
    /// The commands as the dialog began, to ask "Save changes?" when they were changed.
    pub commands: Vec<String>,
}

fn text(x: u16, y: u16, label: &str) -> FormItem {
    FormItem::new(x, y, Control::Text(label.to_string()))
}

fn edit(y: u16, value: &str, width: u16) -> FormItem {
    let mut item = FormItem::new(5, y, Control::Edit(LineInput::with_text(value)));
    item.width = width;
    item
}

fn button(label: &str, default: bool) -> Control {
    Control::Button {
        label: label.to_string(),
        default,
    }
}

/// FAR's "Edit user menu" or "Edit submenu label" with `item`'s values.
fn item_form(item: &Item, submenu: bool) -> Form {
    let mut items = vec![
        text(5, 2, "&Hot key:"),
        edit(3, &item.hotkey, 3),
        text(5, 4, "&Label:"),
        edit(5, &item.label, FIELD_WIDTH),
    ];
    let mut y = 6;
    if !submenu {
        items.push(FormItem::new(0, 6, Control::Separator));
        items.push(text(5, 7, "&Commands:"));
        for line in 0..COMMAND_LINES {
            let command = item
                .commands
                .get(usize::from(line))
                .map_or("", String::as_str);
            items.push(edit(8 + line, command, FIELD_WIDTH));
        }
        y = 8 + COMMAND_LINES;
    }
    items.push(FormItem::new(0, y, Control::Separator));
    items.push(FormItem::new(0, y + 1, button("OK", true)));
    items.push(FormItem::new(0, y + 1, button("Cancel", false)));
    let title = if submenu {
        "Edit submenu label"
    } else {
        "Edit user menu"
    };
    Form::new(title, DIALOG_WIDTH, y + 4, items)
}

/// The commands the dialog holds, up to the last line written (FAR keeps empty lines between).
fn commands_of(form: &Form, edit: &ItemEdit) -> Vec<String> {
    if edit.submenu {
        return Vec::new();
    }
    let lines: Vec<String> = (0..usize::from(COMMAND_LINES))
        .map(|line| form.edit_text(FIRST_COMMAND + line).to_string())
        .collect();
    let used = lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map_or(0, |last| last + 1);
    lines[..used].to_vec()
}

/// FAR's `EditMenuDlgProc` on OK: what is wrong with the item, if anything.
fn problem_of(hotkey: &str, label: &str) -> Option<&'static str> {
    if hotkey == "--" {
        return None;
    }
    if label.is_empty() {
        return Some("Invalid format for UserMenu Label");
    }
    let chars = hotkey.chars().count();
    (chars > 1 && function_key(hotkey).is_none()).then_some("Invalid format for Hot Key")
}

impl App {
    /// Ins (`insert`): FAR's question, then the dialog; F4: the dialog of the item under the cursor.
    pub(super) fn edit_user_menu_item(&mut self, menu: UserMenu, insert: bool) {
        if insert {
            self.user_menu.behind = Some(menu);
            self.show_dialog(Dialog::Confirm {
                title: TITLE.to_string(),
                lines: vec!["Do you wish to insert a new command or a new menu?".to_string()],
                buttons: vec!["Insert command", "Insert menu"],
                focused: 0,
                danger: false,
                action: ConfirmAction::InsertMenuItem,
            });
            return;
        }
        let Some(item) = menu.current().cloned() else {
            self.overlay = Some(Overlay::UserMenu(menu));
            return;
        };
        let submenu = item.submenu.is_some();
        self.user_menu.behind = Some(menu);
        self.show_item_form(&item, submenu, false);
    }

    /// The question of Ins answered: a command (`submenu` false) or a menu.
    pub(super) fn insert_menu_item(&mut self, submenu: bool) {
        self.show_item_form(&Item::default(), submenu, true);
    }

    fn show_item_form(&mut self, item: &Item, submenu: bool, insert: bool) {
        let edit = ItemEdit {
            insert,
            submenu,
            commands: item.commands.clone(),
        };
        self.show_dialog(Dialog::Form {
            form: item_form(item, submenu),
            action: FormAction::MenuItem(Box::new(edit)),
        });
    }

    /// The item's dialog closed: OK keeps it unless it is wrong; leaving with changed commands asks
    /// FAR's "Save changes?".
    pub(super) fn finish_menu_item(&mut self, form: &Form, button: Option<usize>, edit: ItemEdit) {
        let ok = button == Some(form.items.len() - 2);
        if !ok {
            // The dialog holds ten lines: a longer item is compared by those.
            let shown = usize::from(COMMAND_LINES).min(edit.commands.len());
            let mut before = edit.commands[..shown].to_vec();
            while before.last().is_some_and(String::is_empty) {
                before.pop();
            }
            if commands_of(form, &edit) != before {
                self.user_menu.form = Some(Box::new((form.clone(), edit)));
                self.show_dialog(Dialog::Confirm {
                    title: TITLE.to_string(),
                    lines: vec!["Save changes?".to_string()],
                    buttons: vec!["&Yes", "&No", "&Cancel"],
                    focused: 0,
                    danger: true,
                    action: ConfirmAction::SaveMenuItem,
                });
            }
            return;
        }
        let hotkey = form.edit_text(HOTKEY).to_string();
        let label = form.edit_text(LABEL).to_string();
        if let Some(problem) = problem_of(&hotkey, &label) {
            self.user_menu.form = Some(Box::new((form.clone(), edit)));
            self.show_dialog(Dialog::error(TITLE, vec![problem.to_string()]));
            return;
        }
        let Some(mut menu) = self.user_menu.behind.take() else {
            return;
        };
        let commands = commands_of(form, &edit);
        let selected = menu.selected;
        if let Some(level) = menu.level_mut() {
            let at = selected.min(level.len());
            if edit.insert {
                let submenu = edit.submenu.then(Vec::new);
                level.insert(
                    at,
                    Item {
                        hotkey,
                        label,
                        commands,
                        submenu,
                    },
                );
            } else if let Some(item) = level.get_mut(at) {
                item.hotkey = hotkey;
                item.label = label;
                if !edit.submenu {
                    item.commands = commands;
                }
            }
        }
        self.save_user_menu(menu);
    }

    /// "Save changes?" answered: Yes keeps the item as OK would, No leaves it, Cancel goes back to it.
    pub(super) fn answer_save_menu_item(&mut self, button: usize) {
        let Some(stashed) = self.user_menu.form.take() else {
            return;
        };
        let (form, edit) = *stashed;
        match button {
            0 => {
                let ok = form.items.len() - 2;
                self.finish_menu_item(&form, Some(ok), edit);
            }
            1 => {}
            _ => self.user_menu.form = Some(Box::new((form, edit))),
        }
    }

    /// Del: FAR's question about the item under the cursor.
    pub(super) fn ask_remove_menu_item(&mut self, menu: UserMenu) {
        let Some(item) = menu.current().cloned() else {
            self.overlay = Some(Overlay::UserMenu(menu));
            return;
        };
        let what = if item.submenu.is_some() {
            "Do you wish to delete the submenu"
        } else {
            "Do you wish to delete the menu item"
        };
        self.user_menu.behind = Some(menu);
        self.show_dialog(Dialog::Confirm {
            title: TITLE.to_string(),
            lines: vec![what.to_string(), format!("\"{}\"", item.label)],
            buttons: vec!["&Delete", "Cancel"],
            focused: 0,
            danger: true,
            action: ConfirmAction::RemoveMenuItem,
        });
    }

    pub(super) fn remove_menu_item(&mut self) {
        let Some(mut menu) = self.user_menu.behind.take() else {
            return;
        };
        let at = menu.selected;
        if let Some(level) = menu.level_mut().filter(|level| at < level.len()) {
            level.remove(at);
        }
        self.save_user_menu(menu);
    }

    /// Ctrl+Up / Ctrl+Down: the item changes places with the one above or below.
    pub(super) fn move_menu_item(&mut self, mut menu: UserMenu, up: bool) {
        let at = menu.selected;
        let moved = menu.level_mut().and_then(|level| {
            let other = if up { at.checked_sub(1) } else { Some(at + 1) };
            let other = other.filter(|&other| other < level.len() && at < level.len())?;
            level.swap(at, other);
            Some(other)
        });
        let Some(other) = moved else {
            self.overlay = Some(Overlay::UserMenu(menu));
            return;
        };
        menu.selected = other;
        self.save_user_menu(menu);
    }

    /// Saves `menu` and shows it again.
    fn save_user_menu(&mut self, mut menu: UserMenu) {
        self.fill_user_menu(&mut menu);
        self.write_menu_file(menu.clone());
        self.user_menu.behind = Some(menu);
    }

    /// Writes the menu's file beside the UI; a failure is said.
    fn write_menu_file(&mut self, menu: UserMenu) {
        if menu.file.as_os_str().is_empty() {
            return;
        }
        self.wait_for(Vec::new(), move || {
            let saved = user_menu::save(&menu.file, &menu.items);
            Box::new(move |app: &mut App| {
                if let Err(error) = saved {
                    app.show_error(TITLE, &error);
                }
            })
        });
        self.label_waiting("Saving the user menu…");
    }

    /// Alt+F4: the menu's file in the editor; the menu is read again from it afterwards.
    pub(super) fn edit_menu_file(&mut self, menu: UserMenu) {
        let file = menu.file.clone();
        if file.as_os_str().is_empty() {
            self.overlay = Some(Overlay::UserMenu(menu));
            return;
        }
        // The file as it is; one that is not there yet is made empty for the editor, and goes again
        // if it stays so.
        let place = menu.place.clone();
        self.wait_for(Vec::new(), move || {
            let made = if file.exists() {
                Ok(false)
            } else {
                std::fs::create_dir_all(file.parent().unwrap_or(&file))
                    .and_then(|()| {
                        std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&file)
                    })
                    .map(|_| true)
            };
            Box::new(move |app: &mut App| match made {
                Ok(made) => {
                    // Kept before the editor opens: its questions (already loaded) know it is modal.
                    app.user_menu.made_empty = made.then(|| file.clone());
                    app.user_menu.reread = Some(Look::File(place, file.clone()));
                    app.edit_file(&file, false);
                }
                Err(error) => app.show_error(TITLE, &error),
            })
        });
        self.label_waiting("Opening the menu file…");
    }

    /// The menu's file after the editor: the menu read again.
    pub(super) fn reread_user_menu(&mut self) -> bool {
        if self.overlay.is_some()
            || self.waiting.is_some()
            || !matches!(self.screen, super::Screen::Panels)
        {
            return false;
        }
        let Some(look) = self.user_menu.reread.take() else {
            return false;
        };
        if let Some(file) = self.user_menu.made_empty.take()
            && std::fs::metadata(&file).is_ok_and(|data| data.len() == 0)
        {
            // Nothing was written in it: no menu is left behind in the folder.
            let _ = std::fs::remove_file(&file);
        }
        self.look_for_menu(look);
        true
    }

    /// The dialog of an item that was wrong, or the question about it, comes back first.
    pub(super) fn resume_menu_item_form(&mut self) -> bool {
        if self.overlay.is_some() || self.waiting.is_some() {
            return false;
        }
        let Some(stashed) = self.user_menu.form.take() else {
            return false;
        };
        let (form, edit) = *stashed;
        self.overlay = Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::MenuItem(Box::new(edit)),
        }));
        true
    }
}

#[cfg(test)]
#[path = "user_menu_edit_tests.rs"]
mod tests;
