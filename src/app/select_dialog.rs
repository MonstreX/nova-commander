//! FAR's Select / Deselect dialog (`FileList::SelectFiles`): the mask (the last one, `*.*` at first)
//! with Ok, Filter and Cancel. A mask with errors says so and the dialog comes back; Filter opens the
//! Select area's filters and, once that menu closes, selects by them (folders as the filters take them).

use std::time::SystemTime;

use super::App;
use super::command::Side;
use super::dialog::{Dialog, FormAction};
use super::filters::Area;
use super::form::{Control, EditHistory, Form, Item};
use super::line_input::LineInput;

/// The dialog's items.
const MASK: usize = 0;
const OK: usize = 2;
const FILTER: usize = 3;

/// FAR's `filemasks::ErrorMessage`.
const MASK_ERRORS: &str = "File-mask string contains errors";

impl App {
    /// The menu's Select and Unselect group: the active panel's, when it shows files.
    pub(super) fn ask_group_of_active(&mut self, on: bool) {
        if self.active_panel().visible && self.shows_files(self.active) {
            self.ask_select_group(self.active, on);
        }
    }

    /// FAR's Select / Deselect dialog with the last mask (`*.*` at first).
    pub(super) fn ask_select_group(&mut self, side: Side, on: bool) {
        let button = |label: &str, default| Control::Button {
            label: label.to_string(),
            default,
        };
        let mask = Control::Edit(LineInput::with_text(&self.remembered.last_mask));
        let items = vec![
            (Item::new(5, 2, mask).wide(45)).with_history(EditHistory::new("Masks")),
            Item::new(0, 3, Control::Separator),
            Item::new(0, 4, button("Ok", true)),
            Item::new(0, 4, button("&Filter", false)),
            Item::new(0, 4, button("Cancel", false)),
        ];
        let title = if on { "Select" } else { "Deselect" };
        self.show_dialog(Dialog::Form {
            form: Form::new(title, 55, 7, items),
            action: FormAction::SelectGroup { side, on },
        });
    }

    /// The dialog closed: Ok selects by its mask, Filter by the Select area's filters after their menu.
    pub(super) fn finish_select_form(
        &mut self,
        form: &Form,
        button: Option<usize>,
        (side, on): (Side, bool),
    ) {
        if button == Some(FILTER) {
            self.filters.then_select = Some((side, on));
            return self.open_area_filters_menu(Area::Select);
        }
        if button != Some(OK) {
            return;
        }
        let text = form.edit_text(MASK).to_string();
        if self.file_mask(&text).is_none() {
            // FAR's loop: the warning, then the dialog again as it was.
            self.filters.dialog =
                Some((Box::new(form.clone()), FormAction::SelectGroup { side, on }));
            self.show_dialog(Dialog::Message {
                title: "Warning".to_string(),
                lines: vec![MASK_ERRORS.to_string()],
                error: true,
            });
            return;
        }
        self.select_group(side, on, &text);
    }

    pub(super) fn select_group(&mut self, side: Side, on: bool, text: &str) {
        let Some(mask) = self.file_mask(text) else {
            return;
        };
        if !text.starts_with('"') {
            self.remembered.last_mask = text.to_string();
        }
        self.panels[side.index()].select_by_mask(&mask, on);
    }

    /// Select group's Filter: what the Select area's filters take, folders too when they take them.
    pub(super) fn select_by_filter(&mut self, side: Side, on: bool) {
        let filter = self.area_filter(Area::Select);
        let now = SystemTime::now();
        self.panels[side.index()].select_where(on, |entry| filter.takes(entry, now));
    }
}

#[cfg(test)]
#[path = "select_dialog_tests.rs"]
mod tests;
