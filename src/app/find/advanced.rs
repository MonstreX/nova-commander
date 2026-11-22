//! Advanced in Find file: FAR's "Find file advanced options" (`FindFiles::AdvancedDialog`): "Search only
//! in the first" so many bytes (FAR's size, `64K`; empty for the whole file) and the results' columns,
//! their types and widths (FAR's `SearchOutFormat`, "D,S,A", and `SearchOutFormatWidth`, "0,0,0"). A
//! size written wrong gets "File size field is incorrectly filled" and the dialog stays; the Find file
//! dialog comes back under it either way.

use super::super::dialog::{Dialog, FormAction};
use super::super::form::{Control, Form, Item};
use super::super::line_input::LineInput;
use super::App;
use crate::vfs::filter::parse_size;

pub(super) const ADVANCED_TITLE: &str = "Find file advanced options";
pub(super) const SEARCH_FIRST: usize = 1;
pub(super) const COLUMN_TYPES: usize = 4;
pub(super) const COLUMN_WIDTHS: usize = 6;
const OK: usize = 8;
/// FAR's edit lines from column 5 to 50.
const EDIT_WIDTH: u16 = 46;

fn edit(y: u16, text: &str) -> Item {
    Item::new(5, y, Control::Edit(LineInput::with_text(text))).wide(EDIT_WIDTH)
}

fn button(label: &str, default: bool) -> Item {
    let control = Control::Button {
        label: label.to_string(),
        default,
    };
    Item::new(0, 10, control)
}

/// FAR's `CheckFileSizeStringFormat`, an empty line taken (the whole file).
pub(super) fn first_bytes(text: &str) -> Option<u64> {
    if text.is_empty() {
        return Some(0);
    }
    parse_size(text)
}

impl App {
    /// Advanced: its dialog over the Find file dialog, which waits under it.
    pub(super) fn open_find_advanced(&mut self, main: &Form) {
        self.find.dialog_under_advanced = Some(main.clone());
        let find = &self.find;
        let items = vec![
            Item::new(
                5,
                2,
                Control::Text("Search only in the &first:".to_string()),
            ),
            edit(3, &find.first),
            Item::new(0, 4, Control::Separator),
            Item::new(5, 5, Control::Text("Column &types".to_string())),
            edit(6, &find.column_types),
            Item::new(5, 7, Control::Text("Column &widths".to_string())),
            edit(8, &find.column_widths),
            Item::new(0, 9, Control::Separator),
            button("OK", true),
            button("Cancel", false),
        ];
        self.show_dialog(Dialog::Form {
            form: Form::new(ADVANCED_TITLE, 52 + 4, 13, items),
            action: FormAction::FindAdvanced,
        });
    }

    pub(in crate::app) fn finish_find_advanced(&mut self, form: &Form, button: Option<usize>) {
        if button == Some(OK) {
            let first = form.edit_text(SEARCH_FIRST).trim();
            if first_bytes(first).is_none() {
                // FAR's `AdvancedDlgProc`: the warning, and the dialog stays.
                self.find.advanced = Some(form.clone());
                self.show_dialog(Dialog::error(
                    ADVANCED_TITLE,
                    vec!["File size field is incorrectly filled".to_string()],
                ));
                return;
            }
            self.find.first = first.to_string();
            // FAR's `SetSearchColumns`: no types keeps the columns there were.
            let types = form.edit_text(COLUMN_TYPES).trim();
            if !types.is_empty() {
                self.find.column_types = types.to_string();
                self.find.column_widths = form.edit_text(COLUMN_WIDTHS).trim().to_string();
            }
        }
        self.find.dialog = self.find.dialog_under_advanced.take();
    }

    /// The Advanced dialog back after its warning.
    pub(super) fn resume_find_advanced(&mut self) -> bool {
        if self.overlay.is_some() {
            return false;
        }
        let Some(form) = self.find.advanced.take() else {
            return false;
        };
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::FindAdvanced,
        });
        true
    }
}
