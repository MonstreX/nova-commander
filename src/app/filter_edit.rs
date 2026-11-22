//! Ins, F4 and F5 in the filters menu (FAR's `EditFilters`): a new filter for files only before the line
//! under the cursor, the user's filter under it changed, or a copy of the line's filter (the user's one
//! with no title and no marks, "Folders", a file type's for files of that type) made a new one; each
//! through the filter's dialog (`filter_dialog`). OK puts the filter in its place at once and the menu
//! comes back with it under the cursor; the panels are read again when the menu closes.

use std::time::SystemTime;

use super::dialog::{Dialog, FormAction};
use super::filter_dialog::{self as fd, FILTER_TITLE};
use super::filters_menu::{FilterItem, FiltersMenu};
use super::form::Form;
use super::{App, Overlay};
use crate::jobs::attributes::times::TimeFormat;
use crate::settings::{Filter, FilterMarks};
use crate::vfs::filter::DIRECTORY;

/// Where the filter of the dialog goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTarget {
    /// A new one at this place of the user's filters.
    Insert(usize),
    /// The user's filter at this place.
    Change(usize),
}

/// The filter's dialog and what it keeps between its buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterEdit {
    pub target: FilterTarget,
    /// The filter as it was (its marks stay).
    pub filter: Filter,
    /// Each attribute of the attributes dialog: set, clear or either.
    pub states: Vec<Option<bool>>,
    /// The UTC box as the dates show it.
    pub utc: bool,
    pub format: TimeFormat,
}

/// FAR's new filter: the mask `*`, folders left out.
fn new_filter() -> Filter {
    Filter {
        use_attributes: true,
        attributes_clear: DIRECTORY,
        ..Filter::default()
    }
}

/// After a key in the filter's dialog: the fields its boxes enable, the dates turned with UTC.
pub(super) fn follow(dialog: &mut Dialog) {
    let Dialog::Form {
        form,
        action: FormAction::Filter(edit),
    } = dialog
    else {
        return;
    };
    fd::follow(form);
    let utc = form.is_checked(fd::UTC);
    if utc != edit.utc {
        fd::turn_utc(form, utc, edit.format);
        edit.utc = utc;
    }
}

impl App {
    /// Ins, F4, F5 in the filters menu: the dialog, the menu waiting under it.
    pub(super) fn edit_filter_line(&mut self, menu: FiltersMenu, copy: bool, change: bool) {
        let count = self.filters.list.len();
        let item = menu.lines.get(menu.selected).map(|line| line.item.clone());
        let at = menu.selected.min(count);
        let edit = match (item, change, copy) {
            (Some(FilterItem::User(at)), true, _) => {
                Some((FilterTarget::Change(at), self.filters.list[at].clone()))
            }
            (_, true, _) => None,
            (Some(FilterItem::User(from)), _, true) => Some((
                FilterTarget::Insert(at),
                Filter {
                    title: String::new(),
                    marks: FilterMarks::default(),
                    ..self.filters.list[from].clone()
                },
            )),
            (Some(FilterItem::Folders), _, true) => {
                let folders = Filter {
                    use_mask: false,
                    use_attributes: true,
                    attributes_set: DIRECTORY,
                    ..Filter::default()
                };
                Some((FilterTarget::Insert(at), folders))
            }
            (Some(FilterItem::Type(mask)), _, true) => {
                let kind = Filter {
                    mask,
                    ..new_filter()
                };
                Some((FilterTarget::Insert(at), kind))
            }
            (Some(FilterItem::Separator) | None, _, true) => None,
            (_, _, false) => Some((FilterTarget::Insert(at), new_filter())),
        };
        self.filters.behind = Some(menu);
        let Some((target, filter)) = edit else {
            return;
        };
        let format = TimeFormat::current();
        let edit = FilterEdit {
            target,
            states: fd::attribute_states(&filter),
            utc: false,
            format,
            filter,
        };
        let form = fd::filter_form(&edit.filter, format);
        self.show_filter_form(form, edit);
    }

    fn show_filter_form(&mut self, form: Form, edit: FilterEdit) {
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::Filter(Box::new(edit)),
        });
    }

    /// A button of the filter's dialog (`None`: Esc).
    pub(super) fn finish_filter_form(
        &mut self,
        mut edit: FilterEdit,
        form: &Form,
        button: Option<usize>,
    ) {
        let mut form = form.clone();
        match button {
            Some(fd::OK) => return self.store_filter(edit, form),
            Some(fd::RESET) => {
                fd::reset(&mut form);
                edit.states = fd::reset_states();
            }
            Some(fd::CURRENT) => fd::set_dates(&mut form, Some(SystemTime::now()), edit.format),
            Some(fd::BLANK) => fd::set_dates(&mut form, None, edit.format),
            Some(fd::ATTRIBUTES) => {
                let attributes = fd::attributes_form(&edit.states);
                return self.show_dialog(Dialog::Form {
                    form: attributes,
                    action: FormAction::FilterAttributes {
                        edit: Box::new(edit),
                        main: Box::new(form),
                    },
                });
            }
            _ => return,
        }
        self.show_filter_form(form, edit);
    }

    /// OK or Cancel of the attributes dialog: the filter's dialog again.
    pub(super) fn finish_filter_attributes(
        &mut self,
        mut edit: FilterEdit,
        main: Form,
        form: &Form,
        button: Option<usize>,
    ) {
        if button == Some(fd::FILTER_ATTRIBUTES.len() + 1) {
            edit.states = fd::read_attributes(form);
        }
        self.show_filter_form(main, edit);
    }

    /// OK: the filter in its place, or FAR's warning with the dialog back after it.
    fn store_filter(&mut self, edit: FilterEdit, form: Form) {
        let problem = match fd::read_filter(&form, &edit.filter, &edit.states, edit.format) {
            Err(message) => Some((FILTER_TITLE, message)),
            // FAR's `filemasks::ErrorMessage`, a warning of its own.
            Ok(filter) if filter.use_mask && self.file_mask(&filter.mask).is_none() => {
                Some(("Warning", "File-mask string contains errors"))
            }
            Ok(filter) => {
                self.put_filter(edit.target, filter);
                None
            }
        };
        if let Some((title, message)) = problem {
            self.filters.dialog = Some((Box::new(form), FormAction::Filter(Box::new(edit))));
            self.show_dialog(Dialog::Message {
                title: title.to_string(),
                lines: vec![message.to_string()],
                error: true,
            });
        }
    }

    /// The filter into the list and the menu waiting for it, under its cursor.
    fn put_filter(&mut self, target: FilterTarget, filter: Filter) {
        let at = match target {
            FilterTarget::Change(at) if at < self.filters.list.len() => {
                self.filters.list[at] = filter;
                at
            }
            FilterTarget::Change(_) => return,
            FilterTarget::Insert(at) => {
                let at = at.min(self.filters.list.len());
                self.filters.list.insert(at, filter);
                at
            }
        };
        let Some(mut menu) = self.filters.behind.take() else {
            return;
        };
        let mut lines = std::mem::take(&mut menu.lines);
        if let FilterTarget::Insert(_) = target {
            for line in &mut lines {
                if let FilterItem::User(index) = &mut line.item
                    && *index >= at
                {
                    *index += 1;
                }
            }
        }
        self.fill_filters_menu(&mut menu, Some(&lines));
        menu.selected = at;
        menu.changed = true;
        self.filters.behind = Some(menu);
    }

    /// The filter's dialog back after the warning in front of it.
    pub(super) fn resume_filter_dialog(&mut self) -> bool {
        if self.overlay.is_some() || self.waiting.is_some() {
            return false;
        }
        let Some((form, action)) = self.filters.dialog.take() else {
            return false;
        };
        self.overlay = Some(Overlay::Dialog(Dialog::Form {
            form: *form,
            action,
        }));
        true
    }
}

#[cfg(test)]
#[path = "filter_edit_tests.rs"]
mod tests;
