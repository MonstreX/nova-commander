//! Options > settings dialogs as FAR's `DialogBuilder` makes them (`far/config.cpp`): each page is a
//! list of fields, one under another, with OK and Cancel. Only what the program acts on is here;
//! `far-parity.md` says what is left out.

use super::form::{Control, Form, Item, ListLine};
use super::line_input::LineInput;
use super::options_pages::{Field, page};
use super::{App, Dialog, FormAction};
use crate::settings::Settings;

/// A page of Options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionsPage {
    System,
    Panel,
    Tree,
    Interface,
    Dialog,
    Menu,
    CommandLine,
    AutoComplete,
    InfoPanel,
    Confirmations,
    Viewer,
    /// Alt+Shift+F9 in the viewer: its own copy (FAR's `LocalViewerConfig`).
    ViewerLocal,
    /// Alt+Shift+F9 in the editor: FAR's local "Editor" settings.
    EditorLocal,
    Editor,
}

/// What a mouse button does outside a menu (FAR's `VMENUCLICK_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuClick {
    Cancel,
    Apply,
    Ignore,
}

impl MenuClick {
    pub const ALL: [Self; 3] = [Self::Cancel, Self::Apply, Self::Ignore];

    pub fn id(self) -> &'static str {
        match self {
            Self::Cancel => "cancel",
            Self::Apply => "apply",
            Self::Ignore => "ignore",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|click| click.id() == id)
            .unwrap_or(Self::Cancel)
    }
}

/// The switches of Options that live nowhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preferences {
    pub save_history: bool,
    pub save_folders_history: bool,
    pub save_view_history: bool,
    pub update_environment: bool,
    pub clock: bool,
    pub click_outside_dialog: bool,
    pub menu_left: MenuClick,
    pub menu_right: MenuClick,
    pub menu_middle: MenuClick,
    pub completion_list: bool,
    pub confirm_interrupt: bool,
    pub confirm_drag: bool,
    pub confirm_exit: bool,
    pub confirm_connection: bool,
    pub confirm_subst: bool,
    pub confirm_virtual: bool,
    pub confirm_hotplug: bool,
    pub confirm_history_clear: bool,
    pub confirm_reedit: bool,
    /// FAR's `MultiMakeDir`: F7's "Process multiple names".
    pub multi_make_dir: bool,
    pub tree: crate::settings::TreeOptions,
    /// FAR's `PanelCtrlFRule` (`app::name_cases`).
    pub ctrl_f_rule: bool,
    /// FAR's `PgUpChangeDisk`: Ctrl+PgUp at a drive's root opens the drive menu (0 never; 2: a
    /// network drive's share in the network plugin first, as a share's root always does).
    pub ctrl_pg_up_changes_drive: u8,
    /// FAR's `ShowPanelScrollbar`: the files' and the tree's scroll bar on their right frame.
    pub panel_scrollbar: bool,
    /// FAR's `Dialogs.EditHistory` and `ShowMenuScrollbar`.
    pub dialogs: crate::settings::DialogSwitches,
}

impl Preferences {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            save_history: settings.save_history,
            save_folders_history: settings.save_folders_history,
            save_view_history: settings.save_view_history,
            update_environment: settings.update_environment,
            clock: settings.clock,
            click_outside_dialog: settings.click_outside_dialog,
            menu_left: MenuClick::from_id(&settings.menu_left_click),
            menu_right: MenuClick::from_id(&settings.menu_right_click),
            menu_middle: MenuClick::from_id(&settings.menu_middle_click),
            completion_list: settings.completion_list,
            confirm_interrupt: settings.confirm_interrupt,
            confirm_drag: settings.confirm_drag,
            confirm_exit: settings.confirm_exit,
            confirm_connection: settings.confirm_connection,
            confirm_subst: settings.confirm_subst,
            confirm_virtual: settings.confirm_virtual,
            confirm_hotplug: settings.confirm_hotplug,
            confirm_history_clear: settings.confirm_history_clear,
            confirm_reedit: settings.confirm_reedit,
            multi_make_dir: settings.multi_make_dir,
            tree: settings.tree,
            ctrl_f_rule: settings.panel_switches.ctrl_f_rule,
            ctrl_pg_up_changes_drive: settings.panel_switches.ctrl_pg_up_changes_drive,
            panel_scrollbar: settings.panel_switches.panel_scrollbar,
            dialogs: settings.dialog_switches,
        }
    }

    pub fn write_to(&self, settings: &mut Settings) {
        settings.save_history = self.save_history;
        settings.save_folders_history = self.save_folders_history;
        settings.save_view_history = self.save_view_history;
        settings.update_environment = self.update_environment;
        settings.clock = self.clock;
        settings.click_outside_dialog = self.click_outside_dialog;
        settings.menu_left_click = self.menu_left.id().to_string();
        settings.menu_right_click = self.menu_right.id().to_string();
        settings.menu_middle_click = self.menu_middle.id().to_string();
        settings.completion_list = self.completion_list;
        settings.confirm_interrupt = self.confirm_interrupt;
        settings.confirm_drag = self.confirm_drag;
        settings.confirm_exit = self.confirm_exit;
        settings.confirm_connection = self.confirm_connection;
        settings.confirm_subst = self.confirm_subst;
        settings.confirm_virtual = self.confirm_virtual;
        settings.confirm_hotplug = self.confirm_hotplug;
        settings.confirm_history_clear = self.confirm_history_clear;
        settings.confirm_reedit = self.confirm_reedit;
        settings.multi_make_dir = self.multi_make_dir;
        settings.tree = self.tree;
        let switches = &mut settings.panel_switches;
        switches.ctrl_f_rule = self.ctrl_f_rule;
        switches.ctrl_pg_up_changes_drive = self.ctrl_pg_up_changes_drive;
        switches.panel_scrollbar = self.panel_scrollbar;
        settings.dialog_switches = self.dialogs;
    }
}

/// FAR's `DialogBuilder`: fields from column 5 of row 2, the width of the longest and the margins.
const LEFT: u16 = 5;
const FIRST_ROW: u16 = 2;
const MARGINS: u16 = 10;
const LEAST_WIDTH: u16 = 40;
/// A drop-down list and the number field beside its label (FAR's sizes).
const LIST_WIDTH: u16 = 40;
const NUMBER_WIDTH: u16 = 6;
/// An item indented under the one above it.
const INDENT: u16 = 4;
/// The columns of `[ ] ` before a check box's label.
const CHECK_MARK: u16 = 4;

fn visible(label: &str) -> u16 {
    u16::try_from(label.chars().filter(|&ch| ch != '&').count()).unwrap_or(u16::MAX)
}

fn button(label: &str, default: bool) -> Control {
    Control::Button {
        label: label.to_string(),
        default,
    }
}

/// A field's items at `y`; returns the next free row and the columns it needs.
fn add_field(app: &App, field: &Field, items: &mut Vec<Item>, y: u16) -> (u16, u16) {
    match field {
        Field::Check { label, get, .. } => {
            let check = Control::Check {
                label: (*label).to_string(),
                checked: get(app),
            };
            items.push(Item::new(LEFT, y, check));
            (y + 1, CHECK_MARK + visible(label))
        }
        Field::Tristate { label, get, .. } => {
            let check = Control::Tristate {
                label: (*label).to_string(),
                state: get(app),
                three: true,
            };
            items.push(Item::new(LEFT, y, check));
            (y + 1, CHECK_MARK + visible(label))
        }
        Field::Choice {
            label, lines, get, ..
        } => {
            items.push(Item::new(LEFT, y, Control::Text((*label).to_string())));
            let list = Control::DropDown {
                lines: lines(app).into_iter().map(ListLine::Choice).collect(),
                chosen: get(app),
                open: None,
                caption: None,
                bottom_title: None,
            };
            let mut item = Item::new(LEFT, y + 1, list);
            item.width = LIST_WIDTH;
            items.push(item);
            (y + 2, LIST_WIDTH.max(visible(label)))
        }
        Field::Limit {
            label, text, get, ..
        } => add_limit(app, (label, text, *get), items, y),
        // FAR's `AddIntEditField` with `AddTextBefore`: the label, then the number on its row.
        Field::Number {
            label, width, get, ..
        } => {
            items.push(Item::new(LEFT, y, Control::Text((*label).to_string())));
            let number = Control::Edit(LineInput::with_text(&get(app).to_string()));
            let mut field = Item::new(LEFT + visible(label) + 1, y, number);
            field.width = *width;
            items.push(field);
            (y + 1, visible(label) + 1 + width)
        }
        Field::Text {
            label, width, get, ..
        } => {
            items.push(Item::new(LEFT, y, Control::Text((*label).to_string())));
            let mut field = Item::new(LEFT, y + 1, Control::Edit(LineInput::with_text(&get(app))));
            field.width = *width;
            items.push(field);
            (y + 2, (*width).max(visible(label)))
        }
        // A row apart from the field above it.
        Field::Button { item, .. } => {
            let button = item(LEFT, y + 1);
            let width = match &button.control {
                Control::Button { label, .. } => visible(label) + 4,
                _ => 0,
            };
            items.push(button);
            (y + 2, width)
        }
    }
}

/// How many items `add_field` makes of `field`.
fn items_of(field: &Field) -> usize {
    match field {
        Field::Check { .. } | Field::Tristate { .. } | Field::Button { .. } => 1,
        Field::Choice { .. } | Field::Number { .. } | Field::Text { .. } => 2,
        Field::Limit { .. } => 3,
    }
}

/// A check box and, under it, a label with a number: FAR's "if object count exceeds".
fn add_limit(
    app: &App,
    (label, text, get): (&str, &str, fn(&App) -> usize),
    items: &mut Vec<Item>,
    y: u16,
) -> (u16, u16) {
    let limit = get(app);
    let check = Control::Check {
        label: label.to_string(),
        checked: limit != 0,
    };
    items.push(Item::new(LEFT, y, check));
    let x = LEFT + INDENT;
    items.push(Item::new(x, y + 1, Control::Text(text.to_string())));
    let number = Control::Edit(LineInput::with_text(&limit.to_string()));
    // FAR's `LinkFlags(.., false)`: the number with the box, its label not.
    let field = Item::new(x + visible(text) + 1, y + 1, number).follows(items.len() - 2);
    items.push(field.wide(NUMBER_WIDTH));
    (
        y + 2,
        (CHECK_MARK + visible(label)).max(INDENT + visible(text) + 1 + NUMBER_WIDTH),
    )
}

/// The dialog of `page` with the program's values.
fn form_of(app: &App, options: OptionsPage) -> Form {
    let (title, fields) = page(options);
    let mut items = Vec::new();
    let mut y = FIRST_ROW;
    let mut widest = 0;
    for field in &fields {
        let (next, width) = add_field(app, field, &mut items, y);
        y = next;
        widest = widest.max(width);
    }
    items.push(Item::new(0, y, Control::Separator));
    items.push(Item::new(0, y + 1, button("OK", true)));
    items.push(Item::new(0, y + 1, button("Cancel", false)));
    let width = (widest + MARGINS).max(LEAST_WIDTH);
    Form::new(title, width, y + 4, items)
}

/// Takes the dialog's values. Items are counted as `add_field` made them.
fn apply(app: &mut App, form: &Form, options: OptionsPage) {
    let (_, fields) = page(options);
    let mut item = 0;
    for field in &fields {
        match field {
            Field::Check { set, .. } => {
                set(app, form.is_checked(item));
                item += 1;
            }
            Field::Tristate { set, .. } => {
                set(app, form.tristate(item));
                item += 1;
            }
            Field::Choice { set, .. } => {
                set(app, form.chosen(item + 1));
                item += 2;
            }
            Field::Button { .. } => item += 1,
            Field::Limit { set, .. } => {
                // With the box on, what is no number leaves the value as it was.
                let number = form.edit_text(item + 2).trim().parse().ok();
                match (form.is_checked(item), number) {
                    (false, _) => set(app, 0),
                    (true, Some(number)) => set(app, number),
                    (true, None) => {}
                }
                item += 3;
            }
            Field::Text { set, .. } => {
                set(app, form.edit_text(item + 1).to_string());
                item += 2;
            }
            Field::Number { set, .. } => {
                // What is no number leaves the value as it was.
                if let Ok(number) = form.edit_text(item + 1).trim().parse() {
                    set(app, number);
                }
                item += 2;
            }
        }
    }
}

impl App {
    pub(super) fn ask_options(&mut self, options: OptionsPage) {
        let form = form_of(self, options);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::Options(options),
        });
    }

    /// A settings dialog closed: OK takes its values (the button before Cancel); a button of a page
    /// opens its own dialog.
    pub(super) fn finish_options(
        &mut self,
        form: &Form,
        button: Option<usize>,
        options: OptionsPage,
    ) {
        let Some(button) = button else {
            return;
        };
        if button + 2 == form.items.len() {
            return apply(self, form, options);
        }
        let (_, fields) = page(options);
        let mut item = 0;
        for field in &fields {
            if let Field::Button { press, .. } = field
                && item == button
            {
                return press(self, form);
            }
            item += items_of(field);
        }
    }
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod options_tests;
