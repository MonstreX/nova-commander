//! F4 on a folder shortcut: FAR's dialog of `EditItemImpl`, "Folder shortcuts" with its title and its
//! path. A shortcut with no path is a line of its menu (`--` with no title either). A path that is not
//! there is asked about ("Save this shortcut?"), except in a list opened from Commands > Folder
//! shortcuts (FAR's raw mode); the folder is looked for beside the UI.

use super::App;
use super::dialog::{ConfirmAction, Dialog, FormAction};
use super::form::{Control, EditHistory, Form, Item};
use super::line_input::LineInput;
use super::shortcuts::SEPARATOR;
use crate::settings::FolderShortcut;
use crate::vfs::VfsPath;

const TITLE: &str = "Folder shortcuts";
const NAME: usize = 1;
const PATH: usize = 3;
/// FAR's `AddEditField(.., 50)`.
const EDIT_WIDTH: u16 = 50;

fn edit(y: u16, text: &str) -> Item {
    let mut item = Item::new(5, y, Control::Edit(LineInput::with_text(text)));
    item.width = EDIT_WIDTH;
    item
}

fn button(label: &str, default: bool) -> Item {
    Item::new(
        0,
        7,
        Control::Button {
            label: label.to_string(),
            default,
        },
    )
}

/// FAR's dialog (`DialogBuilder` with two text and edit pairs and OK, Cancel).
fn shortcut_form(shortcut: &FolderShortcut) -> Form {
    let items = vec![
        Item::new(5, 2, Control::Text("Title:".to_string())),
        edit(3, &shortcut.name).with_history(EditHistory::new("FS_Name")),
        Item::new(5, 4, Control::Text("Path:".to_string())),
        edit(5, &shortcut.folder).with_history(EditHistory::new("FS_Path")),
        Item::new(0, 6, Control::Separator),
        button("OK", true),
        button("Cancel", false),
    ];
    Form::new(TITLE, EDIT_WIDTH + 10, 10, items)
}

impl App {
    /// F4: the dialog of shortcut `at` of `list`; the menu it was opened from is behind it.
    pub(super) fn edit_shortcut(&mut self, list: usize, at: usize, raw: bool) {
        let Some(shortcut) = self.shortcuts.lists[list].get(at) else {
            return;
        };
        self.show_dialog(Dialog::Form {
            form: shortcut_form(shortcut),
            action: FormAction::FolderShortcut { list, at, raw },
        });
    }

    pub(super) fn finish_shortcut(
        &mut self,
        form: &Form,
        button: Option<usize>,
        (list, at, raw): (usize, usize, bool),
    ) {
        if button != Some(form.items.len() - 2) {
            return;
        }
        let mut shortcut = FolderShortcut {
            name: form.edit_text(NAME).to_string(),
            folder: form.edit_text(PATH).to_string(),
        };
        if self.shortcuts.lists[list].get(at) == Some(&shortcut) {
            return;
        }
        if shortcut.folder.is_empty() {
            if shortcut.name.is_empty() {
                shortcut.name = SEPARATOR.to_string();
            }
            return self.save_shortcut(list, at, shortcut);
        }
        if raw {
            return self.save_shortcut(list, at, shortcut);
        }
        let place = VfsPath::parse(&self.environment.expand(&shortcut.folder));
        let Some(place) = place.as_local().map(std::path::Path::to_path_buf) else {
            return self.save_shortcut(list, at, shortcut);
        };
        self.wait_for(Vec::new(), move || {
            let found = std::fs::metadata(&place);
            Box::new(move |app: &mut App| match found {
                Ok(_) => app.save_shortcut(list, at, shortcut),
                Err(error) => app.ask_save_shortcut(list, at, shortcut, &error),
            })
        });
        self.label_waiting("Looking for the folder…");
    }

    /// FAR's warning with the system's words, the path and the question.
    fn ask_save_shortcut(
        &mut self,
        list: usize,
        at: usize,
        shortcut: FolderShortcut,
        error: &std::io::Error,
    ) {
        let lines = vec![
            error.to_string(),
            shortcut.folder.clone(),
            "Save this shortcut?".to_string(),
        ];
        self.show_dialog(Dialog::Confirm {
            title: "Error".to_string(),
            lines,
            buttons: vec!["Yes", "No"],
            focused: 0,
            danger: true,
            action: ConfirmAction::SaveShortcut {
                list,
                at,
                shortcut: Box::new(shortcut),
            },
        });
    }

    pub(super) fn save_shortcut(&mut self, list: usize, at: usize, shortcut: FolderShortcut) {
        if let Some(known) = self.shortcuts.lists[list].get_mut(at) {
            *known = shortcut;
        }
    }
}

#[cfg(test)]
#[path = "shortcuts_edit_tests.rs"]
mod tests;
