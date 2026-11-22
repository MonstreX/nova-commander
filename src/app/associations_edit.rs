//! Commands > File associations as FAR's `EditFileTypes`: the list (each line its description and its
//! masks), and FAR's "Edit file associations" dialog with the masks, the description and six commands,
//! each with its check box. A mask written wrong is said, and the dialog comes back.

use super::App;
use super::associations::{AssocMode, command_of, command_of_mut};
use super::dialog::{ConfirmAction, Dialog, FormAction};
use super::edit_list::EditListKind;
use super::form::{Control, Form, Item};
use super::line_input::LineInput;
use crate::settings::{Association, AssociationCommand};

const TITLE: &str = "File associations";
const MASKS: usize = 1;
const DESCRIPTION: usize = 3;
/// The first command's check box; its edit line follows it.
const FIRST_COMMAND: usize = 5;

fn edit(x: u16, y: u16, text: &str, width: u16) -> Item {
    let mut item = Item::new(x, y, Control::Edit(LineInput::with_text(text)));
    item.width = width;
    item
}

fn button(label: &str, default: bool) -> Control {
    Control::Button {
        label: label.to_string(),
        default,
    }
}

/// FAR's dialog with `association`'s values.
fn association_form(association: &Association) -> Form {
    let mut items = vec![
        Item::new(
            5,
            2,
            Control::Text("A file &mask or several file masks:".to_string()),
        ),
        edit(5, 3, &association.masks, 66),
        Item::new(
            5,
            4,
            Control::Text("&Description of the association:".to_string()),
        ),
        edit(5, 5, &association.description, 66),
        Item::new(0, 6, Control::Separator),
    ];
    for (row, mode) in (0u16..).zip(AssocMode::ALL) {
        let command = command_of(association, mode);
        items.push(Item::new(
            5,
            7 + row * 2,
            Control::Check {
                label: mode.label().to_string(),
                checked: command.on,
            },
        ));
        items.push(edit(9, 8 + row * 2, &command.command, 62));
    }
    items.push(Item::new(0, 19, Control::Separator));
    items.push(Item::new(0, 20, button("OK", true)));
    items.push(Item::new(0, 20, button("Cancel", false)));
    Form::new("Edit file associations", 76, 23, items)
}

/// What the dialog holds.
fn association_of(form: &Form) -> Association {
    let mut association = Association {
        masks: form.edit_text(MASKS).to_string(),
        description: form.edit_text(DESCRIPTION).to_string(),
        ..Association::default()
    };
    for (row, mode) in AssocMode::ALL.into_iter().enumerate() {
        *command_of_mut(&mut association, mode) = AssociationCommand {
            command: form.edit_text(FIRST_COMMAND + row * 2 + 1).to_string(),
            on: form.is_checked(FIRST_COMMAND + row * 2),
        };
    }
    association
}

impl App {
    /// Commands > File associations.
    pub(super) fn open_associations(&mut self) {
        self.show_edit_list(EditListKind::Associations, 0);
    }

    /// FAR's `FillFileTypesMenu`: the description padded to the longest, `│`, the masks.
    pub(super) fn association_lines(&self) -> Vec<String> {
        let widest = (self.config.associations.iter())
            .map(|association| association.description.chars().count())
            .max()
            .unwrap_or(0);
        (self.config.associations.iter())
            .map(|association| {
                let description = &association.description;
                let padding = widest - description.chars().count();
                format!(
                    "{description}{} \u{2502} {}",
                    " ".repeat(padding),
                    association.masks
                )
            })
            .collect()
    }

    /// Ins (a new one before line `at`, all its boxes on) or F4 on line `at`.
    pub(super) fn edit_association(&mut self, at: usize, insert: bool) {
        let association = if insert {
            let on = |command: &str| AssociationCommand {
                command: command.to_string(),
                on: true,
            };
            Association {
                execute: on(""),
                alt_execute: on(""),
                view: on(""),
                alt_view: on(""),
                edit: on(""),
                alt_edit: on(""),
                ..Association::default()
            }
        } else {
            let Some(association) = self.config.associations.get(at) else {
                return;
            };
            association.clone()
        };
        self.show_dialog(Dialog::Form {
            form: association_form(&association),
            action: FormAction::Association { at, insert },
        });
    }

    /// The dialog closed: OK keeps the association unless its masks are wrong.
    pub(super) fn finish_association(
        &mut self,
        form: &Form,
        button: Option<usize>,
        (at, insert): (usize, bool),
    ) {
        if button != Some(form.items.len() - 2) {
            return;
        }
        let association = association_of(form);
        if self.file_mask(&association.masks).is_none() {
            let action = FormAction::Association { at, insert };
            self.edit_lists.form = Some((Box::new(form.clone()), action));
            self.show_dialog(Dialog::error(
                "Warning",
                vec!["File-mask string contains errors".to_string()],
            ));
            return;
        }
        if insert {
            let at = at.min(self.config.associations.len());
            self.config.associations.insert(at, association);
        } else if let Some(known) = self.config.associations.get_mut(at) {
            *known = association;
        }
        if let Some(list) = &mut self.edit_lists.behind {
            list.selected = at;
        }
    }

    /// Del: FAR's question with the association's masks.
    pub(super) fn ask_remove_association(&mut self, at: usize) {
        let Some(association) = self.config.associations.get(at) else {
            return;
        };
        let masks = format!("\"{}\"", association.masks);
        self.show_dialog(Dialog::Confirm {
            title: TITLE.to_string(),
            lines: vec!["Do you wish to delete association for".to_string(), masks],
            buttons: vec!["&Delete", "Cancel"],
            focused: 0,
            danger: true,
            action: ConfirmAction::RemoveAssociation(at),
        });
    }

    pub(super) fn remove_association(&mut self, at: usize) {
        if at < self.config.associations.len() {
            self.config.associations.remove(at);
        }
    }
}

#[cfg(test)]
#[path = "associations_edit_tests.rs"]
mod tests;
