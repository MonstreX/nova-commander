//! Options > File panel modes as FAR's `Options::SetFilePanelModes`: the modes in FAR's order (Brief ..
//! Detailed, Alternative full; the user's own after a separator), each by its name or FAR's; Enter or F4
//! opens the mode's dialog, Ins a new mode's (FAR's `AddViewSettings`: before the user's mode under the
//! cursor, else first of them), Del takes a user's mode away at once, Ctrl+Enter shows the mode in the
//! active panel and Ctrl+Shift+Enter in the passive one, the list staying. The dialog: the name, the
//! columns and widths of the panel and of the status line as FAR writes them, FAR's six flags, OK, Reset
//! (FAR's own mode back; a user's mode has none) and Cancel.

use super::dialog::{Dialog, FormAction};
use super::edit_list::{EditList, EditListKind};
use super::form::{Control, Form, Item};
use super::line_input::LineInput;
use super::panel_modes::far_default;
use super::{App, ViewMode};
use crate::settings::PanelMode;

pub(super) const TITLE: &str = "Edit panel modes";
pub(super) const KEYS: &str = "Ins, Del, F4, Ctrl+Enter, Ctrl+Shift+Enter";

/// FAR's menu order of its modes (`RealModeToDisplay`; its 6 .. 9 come later) and their names.
pub(super) const PREDEFINED: [(ViewMode, &str); 6] = [
    (ViewMode::Brief, "&Brief"),
    (ViewMode::Medium, "&Medium"),
    (ViewMode::Full, "&Full"),
    (ViewMode::Wide, "&Wide"),
    (ViewMode::Detailed, "Detai&led"),
    (ViewMode::AlternativeFull, "&Alternative full"),
];

/// The item of the separator line.
pub(super) const SEPARATOR_ITEM: usize = usize::MAX;

const NAME: usize = 1;
const TYPES: usize = 3;
const WIDTHS: usize = 5;
const STATUS_TYPES: usize = 7;
const STATUS_WIDTHS: usize = 9;
/// The six flags' boxes, in FAR's order.
const FLAGS: usize = 11;
const OK: usize = 18;
const RESET: usize = 19;

/// The mode of a line's item: one of FAR's six, then the user's.
pub(super) fn mode_of_item(item: usize) -> Option<ViewMode> {
    match item {
        SEPARATOR_ITEM => None,
        at if at < PREDEFINED.len() => Some(PREDEFINED[at].0),
        at => Some(ViewMode::Custom(at - PREDEFINED.len())),
    }
}

/// A line's item of a mode.
fn item_of_mode(mode: ViewMode) -> usize {
    match mode {
        ViewMode::Custom(at) => PREDEFINED.len() + at,
        mode => (PREDEFINED.iter())
            .position(|(known, _)| *known == mode)
            .unwrap_or(0),
    }
}

/// What a mode's dialog changes: the mode (none: a new one) and the mode under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeEdit {
    pub mode: Option<ViewMode>,
    pub base: ViewMode,
}

fn edit(x: u16, y: u16, width: u16, text: &str) -> Item {
    let mut item = Item::new(x, y, Control::Edit(LineInput::with_text(text)));
    item.width = width;
    item
}

fn check(y: u16, label: &str, checked: bool) -> Item {
    let label = label.to_string();
    Item::new(5, y, Control::Check { label, checked })
}

fn button(label: &str, default: bool) -> Item {
    let label = label.to_string();
    Item::new(0, 16, Control::Button { label, default })
}

/// FAR's `ModeDlg`: the box at 3..72 by 1..17 in a dialog of 76 by 19.
fn mode_form(title: &str, mode: &PanelMode, reset: bool) -> Form {
    let flags = [
        ("&Fullscreen view", mode.full_screen),
        ("Ali&gn file extensions", mode.align_extensions),
        ("Align folder e&xtensions", mode.folder_align_extensions),
        ("Show folders in &uppercase", mode.folder_upper_case),
        ("Show files in &lowercase", mode.files_lower_case),
        (
            "Show uppercase file names in lower&case",
            mode.upper_to_lower_case,
        ),
    ];
    let mut items = vec![
        Item::new(5, 2, Control::Text("&Name".to_string())),
        edit(5, 3, 66, &mode.name),
        Item::new(5, 4, Control::Text("Column &types".to_string())),
        edit(5, 5, 31, &mode.columns),
        Item::new(5, 6, Control::Text("Column &widths".to_string())),
        edit(5, 7, 31, &mode.widths),
        Item::new(
            38,
            4,
            Control::Text("St&atus line column types".to_string()),
        ),
        edit(38, 5, 33, &mode.status_columns),
        Item::new(
            38,
            6,
            Control::Text("Status l&ine column widths".to_string()),
        ),
        edit(38, 7, 33, &mode.status_widths),
        Item::new(0, 8, Control::Separator),
    ];
    items.extend((flags.iter().zip(9..)).map(|((label, on), y)| check(y, label, *on)));
    items.extend([
        Item::new(0, 15, Control::Separator),
        button("OK", true),
        button("&Reset", false).disabled(!reset),
        button("Cancel", false),
    ]);
    Form::new(title, 76, 19, items)
}

/// The mode the dialog holds.
fn read_mode(form: &Form) -> PanelMode {
    let flag = |at: usize| form.is_checked(FLAGS + at);
    PanelMode {
        name: form.edit_text(NAME).to_string(),
        columns: form.edit_text(TYPES).to_string(),
        widths: form.edit_text(WIDTHS).to_string(),
        status_columns: form.edit_text(STATUS_TYPES).to_string(),
        status_widths: form.edit_text(STATUS_WIDTHS).to_string(),
        full_screen: flag(0),
        align_extensions: flag(1),
        folder_align_extensions: flag(2),
        folder_upper_case: flag(3),
        files_lower_case: flag(4),
        upper_to_lower_case: flag(5),
    }
}

impl App {
    /// Options > File panel modes: the cursor on the active panel's mode.
    pub(super) fn open_panel_modes(&mut self) {
        let files = self.shows_files(self.active);
        let mode = self.panels[self.active.index()].view_mode;
        let selected = if files {
            let item = item_of_mode(mode);
            if item < PREDEFINED.len() {
                item
            } else {
                item + 1
            }
        } else {
            0
        };
        self.show_edit_list(EditListKind::PanelModes, selected);
    }

    /// The lines (a mode's name, else FAR's), their items and where the separator is.
    pub(super) fn panel_mode_lines(&self) -> (Vec<String>, Vec<usize>, Vec<usize>) {
        let modes = &self.panel_modes;
        let name = |mode: ViewMode, own: &str| {
            let kept = modes.kept_mode(mode).map_or("", |kept| kept.name.as_str());
            if kept.is_empty() { own } else { kept }.to_string()
        };
        let mut lines: Vec<String> = PREDEFINED
            .iter()
            .map(|(mode, own)| name(*mode, own))
            .collect();
        let mut items: Vec<usize> = (0..PREDEFINED.len()).collect();
        let mut separators = Vec::new();
        if modes.custom_count() > 0 {
            separators.push(lines.len());
            lines.push(String::new());
            items.push(SEPARATOR_ITEM);
        }
        for at in 0..modes.custom_count() {
            lines.push(name(ViewMode::Custom(at), ""));
            items.push(PREDEFINED.len() + at);
        }
        (lines, items, separators)
    }

    /// Enter or F4 (`mode`), Ins (none): the mode's dialog; `base` is the mode under the cursor.
    pub(super) fn edit_panel_mode(&mut self, mode: Option<ViewMode>, base: ViewMode) {
        let shown = mode
            .and_then(|mode| self.panel_modes.kept_mode(mode))
            .cloned();
        let title = match mode {
            Some(mode) => {
                let at = item_of_mode(mode);
                let own = PREDEFINED.get(at).map_or("", |(_, own)| *own);
                let kept = shown.as_ref().map_or("", |kept| kept.name.as_str());
                let name = if kept.is_empty() { own } else { kept };
                // FAR's `remove_highlight`: `&&` is an `&`, a single one marks the hotkey.
                name.split("&&")
                    .map(|part| part.replace('&', ""))
                    .collect::<Vec<_>>()
                    .join("&")
            }
            None => String::new(),
        };
        // FAR enables Reset by the line under the cursor, a new mode's too.
        let reset = !matches!(base, ViewMode::Custom(_));
        let form = mode_form(&title, &shown.unwrap_or_default(), reset);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::PanelMode(ModeEdit { mode, base }),
        });
    }

    /// OK: the mode as the dialog says; Reset: FAR's own mode of the line under the cursor.
    pub(super) fn finish_panel_mode(&mut self, form: &Form, button: Option<usize>, edit: ModeEdit) {
        let value = match button {
            Some(OK) => read_mode(form),
            Some(RESET) => far_default(edit.base),
            _ => return,
        };
        match edit.mode {
            Some(mode) => self.panel_modes.set(mode, value),
            None => {
                let at = match edit.base {
                    ViewMode::Custom(at) => at,
                    _ => 0,
                };
                // The panels' user modes from that place on move one down with theirs (also the
                // one a panel on a plugin's goes back to).
                for panel in &mut self.panels {
                    panel.set_view_mode(after_add(panel.view_mode, at));
                    for entered in panel.plugin_stack_mut() {
                        entered.view = after_add(entered.view, at);
                    }
                }
                self.panel_modes.add(at, value);
            }
        }
    }

    /// Del on a user's mode: it goes at once; a panel showing it shows the mode before it.
    pub(super) fn delete_panel_mode(&mut self, mut list: EditList) {
        let Some(ViewMode::Custom(at)) = list.item().and_then(mode_of_item) else {
            self.overlay = Some(super::Overlay::EditList(list));
            return;
        };
        for panel in &mut self.panels {
            panel.set_view_mode(after_remove(panel.view_mode, at));
            for entered in panel.plugin_stack_mut() {
                entered.view = after_remove(entered.view, at);
            }
        }
        self.panel_modes.remove(at);
        // FAR's cursor goes to the line before, past the separator.
        list.selected = list.selected.saturating_sub(1);
        self.fill_edit_list(&mut list);
        if list.separators.contains(&list.selected) {
            list.selected -= 1;
        }
        self.overlay = Some(super::Overlay::EditList(list));
    }

    /// Ctrl+Enter, Ctrl+Shift+Enter: the mode under the cursor in the active or the passive panel (FAR's
    /// `SetViewMode` of the panel: an info panel only keeps it for its files panel).
    pub(super) fn show_panel_mode(&mut self, list: &EditList, passive: bool) {
        if let Some(mode) = list.item().and_then(mode_of_item) {
            let side = if passive {
                self.active.other()
            } else {
                self.active
            };
            self.panels[side.index()].set_view_mode(mode);
        }
    }
}

/// `mode` once a user's mode is put at `at`: the user's from there on one down.
fn after_add(mode: ViewMode, at: usize) -> ViewMode {
    match mode {
        ViewMode::Custom(shown) if shown >= at => ViewMode::Custom(shown + 1),
        other => other,
    }
}

/// `mode` once the user's mode at `at` goes: those after it one up, itself the one before it (the first:
/// Alternative full).
fn after_remove(mode: ViewMode, at: usize) -> ViewMode {
    match mode {
        ViewMode::Custom(0) if at == 0 => ViewMode::AlternativeFull,
        ViewMode::Custom(shown) if shown >= at => ViewMode::Custom(shown - 1),
        other => other,
    }
}

#[cfg(test)]
#[path = "panel_modes_menu_tests.rs"]
mod tests;
