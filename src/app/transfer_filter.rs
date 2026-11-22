//! "Use filter" and Filter of the copy, move and link dialogs (FAR's `ID_SC_USEFILTER`,
//! `ID_SC_BTNFILTER`): off at each dialog, none for a single file; Filter, enabled with the box (FAR's
//! `SetAutomation`), opens the filters menu of Copy's own marks with the active panel's file types and
//! the dialog comes back after it. The operation then leaves out what Copy's filter does not take
//! (`jobs::transfer`).

use std::time::SystemTime;

use super::dialog::{Dialog, FormAction};
use super::filters::Area;
use super::form::Form;
use super::transfer_dialog::{FILTER, LINK_FILTER, LINK_USE_FILTER, Operation, USE_FILTER};
use super::{App, Overlay};
use crate::vfs::filter::AreaFilter;

/// The box and the button of `operation`'s dialog.
fn items(operation: Operation) -> (usize, usize) {
    if operation == Operation::Link {
        (LINK_USE_FILTER, LINK_FILTER)
    } else {
        (USE_FILTER, FILTER)
    }
}

/// The dialog as FAR opens it: the box off (disabled for a single file), Filter disabled with it.
pub(super) fn prepare(form: &mut Form, operation: Operation, single_file: bool) {
    let (use_filter, _) = items(operation);
    form.items[use_filter].disabled = single_file;
    follow_form(form, operation);
}

/// After a key in the copy, move or link dialog: Filter enabled with the box.
pub(super) fn follow(dialog: &mut Dialog) {
    if let Dialog::Form {
        form,
        action: FormAction::Transfer { operation, .. },
    } = dialog
    {
        follow_form(form, *operation);
    }
}

fn follow_form(form: &mut Form, operation: Operation) {
    let (use_filter, filter) = items(operation);
    let on = form.is_checked(use_filter) && !form.items[use_filter].disabled;
    form.items[filter].disabled = !on;
}

/// Whether `button` is the dialog's Filter.
pub(super) fn is_filter_button(operation: Operation, button: Option<usize>) -> bool {
    button == Some(items(operation).1)
}

impl App {
    /// Filter: the filters menu of Copy's marks, the dialog under it until it closes.
    pub(super) fn open_copy_filters(&mut self, form: &Form, action: FormAction) {
        self.filters.under = Some(Dialog::Form {
            form: form.clone(),
            action,
        });
        self.open_area_filters_menu(Area::Copy);
    }

    /// The dialog under the filters menu back once the menu is gone.
    pub(super) fn resume_under_filters(&mut self) -> bool {
        if self.overlay.is_some() || self.waiting.is_some() || self.filters.behind.is_some() {
            return false;
        }
        let Some(dialog) = self.filters.under.take() else {
            return false;
        };
        self.overlay = Some(Overlay::Dialog(dialog));
        true
    }

    /// Copy's filter for an operation that uses it.
    pub(super) fn copy_filter(&self, use_filter: bool) -> Option<(AreaFilter, SystemTime)> {
        use_filter.then(|| (self.area_filter(Area::Copy), SystemTime::now()))
    }
}

#[cfg(test)]
#[path = "transfer_filter_tests.rs"]
mod tests;
