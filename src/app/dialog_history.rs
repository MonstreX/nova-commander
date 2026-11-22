//! The dialogs' edit lines with a history (`form::EditHistory`, FAR's `HISTORYTYPE_DIALOG`): a form
//! that comes up gets the lines of each history it names, newest first, and its empty
//! `DIF_USELASTHISTORY` lines the last one; what its list changed is kept whatever closes it, and a
//! button closing it adds each such line's text, an empty one too (FAR's `CloseDialog` with an exit
//! code, its `AddToHistory` for a dialog). With "History in dialog edit controls" off there is none.

use super::App;
use super::dialog::FormAction;
use super::form::{Form, HistoryChange, HistoryLine};
use super::history_menu::{day_of, system_time};
use crate::platform;
use crate::settings::history::{Entry, Key, Kind};

impl App {
    /// Each history of `form` as it is now.
    pub(super) fn give_histories(&self, form: &mut Form) {
        let on = self.prefs.dialogs.edit_history;
        for item in &mut form.items {
            let Some(history) = item.history.as_deref_mut() else {
                continue;
            };
            history.confirm_clear = self.prefs.confirm_history_clear;
            history.entries = Some(self.dialog_lines(&history.name));
        }
        form.fill_last_history();
        // The option only gives the arrow and the list.
        if !on {
            for history in form
                .items
                .iter_mut()
                .filter_map(|item| item.history.as_deref_mut())
            {
                history.entries = None;
            }
        }
    }

    /// The lines of the history `name`, newest first, each with its day's date.
    fn dialog_lines(&self, name: &str) -> Vec<HistoryLine> {
        let mut day = None;
        let mut date = String::new();
        let entries = self.history.lists.dialog(name).iter().rev();
        (entries.map(|entry| {
            let today = day_of(entry.time);
            if today != day {
                day = today;
                date = match today {
                    Some(_) => platform::locale::date_time(system_time(entry.time), true, 8).0,
                    None => String::new(),
                };
            }
            HistoryLine {
                entry: entry.clone(),
                day: date.clone(),
            }
        }))
        .collect()
    }

    /// Whether button `button` closes the form doing `action` (FAR's `DIF_BTNNOCLOSE` buttons, and a
    /// `DN_CLOSE` that refuses, add no lines).
    pub(super) fn closes_for_good(&self, action: &FormAction, form: &Form, button: usize) -> bool {
        use super::filter_dialog as fd;
        match action {
            FormAction::FindFile => self.find_form_closes(form, button),
            FormAction::Filter(_) => {
                !matches!(button, fd::RESET | fd::CURRENT | fd::BLANK | fd::ATTRIBUTES)
            }
            // A plugin's dialog adds them as it really closes (its handler may keep it, `DN_CLOSE`).
            FormAction::Plugin(super::plugins::PluginForm::Dialog) => false,
            _ => true,
        }
    }

    /// `form` left (`closed`: by a button that closes it): what its lists changed, taken out of it so
    /// that a form shown again does not do them twice, then its lines' texts.
    pub(super) fn keep_form_histories(&mut self, form: &mut Form, closed: bool) {
        for item in &mut form.items {
            let Some(history) = item.history.as_deref_mut() else {
                continue;
            };
            let key = Key::Dialog(history.name.clone());
            for change in std::mem::take(&mut history.changes) {
                self.change_dialog_history(&key, &change);
            }
            if closed && self.prefs.dialogs.edit_history && !item.hidden {
                let text = match &item.control {
                    super::form::Control::Edit(input) => input.text(),
                    _ => continue,
                };
                let entry = Entry::new(text, Kind::Plain);
                if let Some(writer) = &self.history.writer {
                    writer.add(key.clone(), &entry);
                }
                self.history.lists.add(key, entry);
            }
        }
    }

    fn change_dialog_history(&mut self, key: &Key, change: &HistoryChange) {
        let writer = self.history.writer.as_ref();
        let entries = self.history.lists.entries_mut(key);
        match change {
            HistoryChange::Remove(line) => {
                if let Some(writer) = writer {
                    writer.remove(key.clone(), line);
                }
                entries.retain(|entry| !(entry.same(line) && entry.time <= line.time));
            }
            HistoryChange::Lock(line) => {
                if let Some(writer) = writer {
                    writer.set_lock(key.clone(), line);
                }
                let same = entries.iter_mut().filter(|entry| entry.same(line));
                same.for_each(|entry| entry.locked = line.locked);
            }
            HistoryChange::Clear => {
                if let Some(writer) = writer {
                    writer.clear_unlocked(key.clone());
                }
                entries.retain(|entry| entry.locked);
            }
        }
    }
}

#[cfg(test)]
#[path = "dialog_history_tests.rs"]
mod tests;
