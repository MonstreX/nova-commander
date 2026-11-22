//! The user's own shells for the guest (Options > Command line settings > "Shells..."): a list as FAR
//! keeps its editable menus (Ins adds, F4 or Enter changes, Del removes after a question) and a dialog
//! with the name, the program, its arguments and the dialect it speaks. Closing a dialog of it brings
//! back the one it came from: the list, then Command line settings.

use super::dialog::{ConfirmAction, Dialog, FormAction, ListAction};
use super::form::{Control, Form, Item, ListLine, Place};
use super::line_input::LineInput;
use super::options::OptionsPage;
use super::{App, MainView, Overlay, Screen};
use crate::terminal::{Dialect, Shell};

/// What comes back when the dialog in front closes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellsBack {
    /// Command line settings, as they were left.
    Settings,
    /// The list, its cursor on this line.
    List(usize),
    /// The shell's dialog as it was (a name already taken), for the shell at this place or a new one.
    Form(Box<Form>, Option<usize>),
}

/// The shells' dialogs open, and the Command line settings they came from, with what was changed in
/// them.
#[derive(Debug, Default)]
pub struct ShellsState {
    back: Option<ShellsBack>,
    parent: Option<Box<Form>>,
    /// The id of the shell chosen in those settings: it follows a rename.
    chosen: Option<String>,
}

const TITLE: &str = "User shells";
/// The only line of a list with no shells in it: what the list is and how one is added.
pub const NO_SHELLS: &str = "No shells of your own yet: Ins adds one";
/// The list's keys, in its bottom frame.
pub const SHELLS_KEYS: &str = "Ins add, Enter or F4 change, Del delete";
const DIALOG_WIDTH: u16 = 64;
const FIELD_WIDTH: u16 = 54;
/// The items of the shell's dialog.
const NAME: usize = 1;
const PROGRAM: usize = 3;
const ARGUMENTS: usize = 5;
const DIALECT: usize = 7;
const OK: usize = 9;

fn labelled(items: &mut Vec<Item>, y: u16, label: &str, control: Control) {
    items.push(Item::new(5, y, Control::Text(label.to_string())));
    let mut field = Item::new(5, y + 1, control);
    field.width = FIELD_WIDTH;
    items.push(field);
}

fn edit(text: &str) -> Control {
    Control::Edit(LineInput::with_text(text))
}

/// The shell's dialog with `shell`'s values, or empty ones.
fn shell_form(shell: Option<&Shell>) -> Form {
    let mut items = Vec::new();
    labelled(
        &mut items,
        2,
        "&Name:",
        edit(shell.map_or("", Shell::title)),
    );
    let program = shell.map(|shell| shell.program().display().to_string());
    labelled(
        &mut items,
        4,
        "&Program:",
        edit(program.as_deref().unwrap_or("")),
    );
    labelled(
        &mut items,
        6,
        "&Arguments:",
        edit(shell.map_or("", Shell::arguments)),
    );
    let dialect = shell.map_or(Dialect::Cmd, Shell::dialect);
    let types = Control::DropDown {
        lines: (Dialect::ALL.iter())
            .map(|dialect| ListLine::Choice(dialect.title().to_string()))
            .collect(),
        chosen: Dialect::ALL
            .iter()
            .position(|known| *known == dialect)
            .unwrap_or(0),
        open: None,
        caption: None,
        bottom_title: None,
    };
    labelled(&mut items, 8, "&Type:", types);
    items.push(Item::new(0, 10, Control::Separator));
    items.push(Item::new(
        0,
        11,
        Control::Button {
            label: "OK".to_string(),
            default: true,
        },
    ));
    items.push(Item::new(
        0,
        11,
        Control::Button {
            label: "Cancel".to_string(),
            default: false,
        },
    ));
    Form::new("Shell", DIALOG_WIDTH, 14, items)
}

/// The "Shells..." button's item in a settings page.
pub fn shells_button(x: u16, y: u16) -> Item {
    let mut item = Item::new(
        x,
        y,
        Control::Button {
            label: "S&hells...".to_string(),
            default: false,
        },
    );
    item.place = Place::At;
    item
}

/// Command line settings as they were left, the shells to choose from as they are now, the one
/// chosen there kept.
fn refreshed_settings(mut form: Form, shells: &[Shell], chosen_id: &str) -> Form {
    if let Some(Control::DropDown { lines, chosen, .. }) = (form.items.iter_mut())
        .map(|item| &mut item.control)
        .find(|control| matches!(control, Control::DropDown { .. }))
    {
        *chosen = (shells.iter())
            .position(|shell| shell.id() == chosen_id)
            .unwrap_or(0);
        *lines = (shells.iter())
            .map(|shell| ListLine::Choice(shell.title().to_string()))
            .collect();
    }
    form
}

/// The line chosen in the settings' list of shells.
fn chosen_line(form: &Form) -> usize {
    (form.items.iter())
        .find_map(|item| match &item.control {
            Control::DropDown { chosen, .. } => Some(*chosen),
            _ => None,
        })
        .unwrap_or(0)
}

/// A program written in quotes, as Windows writes paths with spaces, is the path inside them.
fn unquoted(program: &str) -> &str {
    program
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(program)
}

impl App {
    /// "Shells..." in Command line settings: the list, and the settings back as they are when it closes.
    pub(super) fn open_user_shells(&mut self, settings: &Form) {
        let shells = crate::terminal::available_shells(&self.user_shells);
        self.shells.chosen =
            (shells.get(chosen_line(settings))).map(|shell| shell.id().to_string());
        self.shells.parent = Some(Box::new(settings.clone()));
        self.show_user_shells(0);
    }

    /// The list of the user's shells, the cursor on line `cursor`.
    pub(super) fn show_user_shells(&mut self, cursor: usize) {
        let mut items: Vec<String> = (self.user_shells.iter())
            .map(|shell| shell.title().to_string())
            .collect();
        // Enter or F4 on it adds one too; Del does nothing.
        if items.is_empty() {
            items.push(NO_SHELLS.to_string());
        }
        let cursor = cursor.min(items.len().saturating_sub(1));
        self.shells.back = Some(ShellsBack::Settings);
        self.show_dialog(Dialog::List {
            title: TITLE.to_string(),
            items,
            cursor,
            top: Default::default(),
            action: ListAction::UserShells,
        });
    }

    /// Enter or F4 on line `index`; Ins (an index past the end): a new shell.
    pub(super) fn edit_user_shell(&mut self, index: usize) {
        let place = (index < self.user_shells.len()).then_some(index);
        let form = shell_form(place.and_then(|index| self.user_shells.get(index)));
        self.shells.back = Some(ShellsBack::List(index));
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::UserShell(place),
        });
    }

    /// Del on line `index`: FAR's question first.
    pub(super) fn ask_remove_user_shell(&mut self, index: usize) {
        let Some(shell) = self.user_shells.get(index) else {
            return;
        };
        let name = shell.title().to_string();
        self.shells.back = Some(ShellsBack::List(index));
        self.show_dialog(Dialog::Confirm {
            title: "Delete shell".to_string(),
            lines: vec!["Do you wish to delete the shell".to_string(), name],
            buttons: vec!["&Delete", "Cancel"],
            focused: 0,
            danger: true,
            action: ConfirmAction::RemoveUserShell(index),
        });
    }

    pub(super) fn remove_user_shell(&mut self, index: usize) {
        if index >= self.user_shells.len() {
            return;
        }
        let gone = self.user_shells.remove(index);
        if gone.id() == self.shell.id() {
            self.choose_guest_shell(Shell::default_shell());
        }
        if self.shells.chosen.as_deref() == Some(gone.id()) {
            self.shells.chosen = None;
        }
    }

    /// The shell's dialog closed: OK keeps what it says, unless a name is missing or taken.
    pub(super) fn finish_user_shell(
        &mut self,
        form: &Form,
        button: Option<usize>,
        place: Option<usize>,
    ) {
        if button != Some(OK) {
            return;
        }
        let name = form.edit_text(NAME).trim().to_string();
        let program = unquoted(form.edit_text(PROGRAM).trim()).trim().to_string();
        let dialect = Dialect::ALL
            .get(form.chosen(DIALECT))
            .copied()
            .unwrap_or(Dialect::Cmd);
        let same = |shell: &Shell| shell.title().to_lowercase() == name.to_lowercase();
        // The system's names too, installed or not: two lines alike in the lists could not be told apart.
        let taken = (self.user_shells.iter().enumerate())
            .any(|(at, shell)| Some(at) != place && same(shell))
            || (crate::platform::process::shells().iter())
                .any(|kind| kind.title().to_lowercase() == name.to_lowercase());
        let problem = if name.is_empty() || program.is_empty() {
            Some("The shell needs a name and a program.")
        } else if taken {
            Some("There is a shell of this name already.")
        } else {
            None
        };
        if let Some(problem) = problem {
            self.shells.back = Some(ShellsBack::Form(Box::new(form.clone()), place));
            self.show_dialog(Dialog::error(TITLE, vec![problem.to_string()]));
            return;
        }
        let shell = Shell::user(&name, &program, form.edit_text(ARGUMENTS).trim(), dialect);
        let at = match place {
            Some(at) => {
                let old = std::mem::replace(&mut self.user_shells[at], shell.clone());
                if self.shells.chosen.as_deref() == Some(old.id()) {
                    self.shells.chosen = Some(shell.id().to_string());
                }
                if old.id() == self.shell.id() && old != shell {
                    self.choose_guest_shell(shell);
                }
                at
            }
            None => {
                self.user_shells.push(shell);
                self.user_shells.len() - 1
            }
        };
        self.shells.back = Some(ShellsBack::List(at));
    }

    /// The dialog a closed one of the shells came from; true when one came back.
    pub(super) fn resume_user_shells(&mut self) -> bool {
        if self.overlay.is_some() || self.waiting.is_some() {
            return false;
        }
        let Some(back) = self.shells.back.take() else {
            return false;
        };
        if !matches!(self.screen, Screen::Panels) || self.main_view != MainView::Panels {
            self.shells.parent = None;
            return false;
        }
        match back {
            ShellsBack::Settings => match self.shells.parent.take() {
                Some(form) => {
                    let shells = crate::terminal::available_shells(&self.user_shells);
                    let default = Shell::default_shell();
                    let chosen = self.shells.chosen.take();
                    let chosen = chosen.as_deref().unwrap_or(default.id());
                    self.overlay = Some(Overlay::Dialog(Dialog::Form {
                        form: refreshed_settings(*form, &shells, chosen),
                        action: FormAction::Options(OptionsPage::CommandLine),
                    }));
                }
                None => self.ask_options(OptionsPage::CommandLine),
            },
            ShellsBack::List(cursor) => self.show_user_shells(cursor),
            ShellsBack::Form(form, place) => {
                self.shells.back = Some(ShellsBack::List(place.unwrap_or(self.user_shells.len())));
                self.overlay = Some(Overlay::Dialog(Dialog::Form {
                    form: *form,
                    action: FormAction::UserShell(place),
                }));
            }
        }
        true
    }
}

#[cfg(test)]
#[path = "user_shells_tests.rs"]
mod tests;
