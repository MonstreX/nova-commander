//! FAR's edit line with a history (`DIF_HISTORY`, `Dialog::SelectFromEditHistory`): the `↓` after it,
//! Ctrl+Down / Alt+Down opens its lines under it, newest first with a date before each day's first
//! (FAR's dialog history menu, `VMENU_COMBOBOX`); Enter or Tab puts the line into the edit, Ins locks
//! it, Shift+Del takes an unlocked one out, Del takes out what is not locked (asking first while
//! "Clear history list" is on), Ctrl+C / Ctrl+Ins copies it. The lines are those the app gave the form
//! when it came up; what the list changed waits in [`EditHistory::changes`] for the app to keep, and
//! the app adds each line's text to its history when a button closes the form (FAR's `CloseDialog`).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::list::move_cursor;
use super::{Control, Form, FormOutcome, Item, ListLine, OpenList};
use crate::app::clipboard::Clipboard;
use crate::settings::history::Entry;

/// What the history of an edit line is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditHistory {
    /// FAR's `strHistory`: the history's name, shared by the dialogs that give it.
    pub name: String,
    /// FAR's `DIF_USELASTHISTORY`: an empty line starts with the last one given.
    pub use_last: bool,
    /// The lines, newest first; `None` until the app gives them, and while the dialogs' histories
    /// are off (no `↓`, no list).
    pub entries: Option<Vec<HistoryLine>>,
    /// The list while it is open.
    pub open: Option<OpenList>,
    /// FAR's `Confirm.HistoryClear`, as the app gave it.
    pub confirm_clear: bool,
    /// Del's question over the list.
    pub question: Option<Box<Form>>,
    /// What the list did to the history, in order.
    pub changes: Vec<HistoryChange>,
}

/// A line of the history and the date of its day as the list shows it (empty: not known).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryLine {
    pub entry: Entry,
    pub day: String,
}

/// A change the list made to its history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryChange {
    /// Shift+Del: the line as it was.
    Remove(Entry),
    /// Ins: the line with its new lock.
    Lock(Entry),
    /// Del: what is not locked.
    Clear,
}

impl EditHistory {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            use_last: false,
            entries: None,
            open: None,
            confirm_clear: true,
            question: None,
            changes: Vec::new(),
        }
    }

    /// FAR's `DIF_USELASTHISTORY`.
    pub fn use_last(mut self) -> Self {
        self.use_last = true;
        self
    }

    /// The `↓` after the line: there are histories, and lines in this one (FAR's
    /// `ItemHasDropDownArrow` asks the history's name and the option only).
    pub fn has_arrow(&self) -> bool {
        self.entries.is_some()
    }

    /// The list's lines: a date before each day's first line, the locked ones marked; and the entry of
    /// each line.
    pub fn lines(&self) -> Shown {
        let mut shown = Shown::default();
        let mut before = "";
        for (index, line) in self.entries.iter().flatten().enumerate() {
            if !line.day.is_empty() && line.day != before {
                shown.lines.push(ListLine::Section(line.day.clone()));
                shown.entries.push(None);
            }
            before = &line.day;
            shown.lines.push(ListLine::Marked {
                label: line.entry.text.clone(),
                checked: line.entry.locked,
            });
            shown.entries.push(Some(index));
        }
        shown
    }
}

/// The lines of an open history list.
#[derive(Debug, Default)]
pub struct Shown {
    pub lines: Vec<ListLine>,
    /// The entry each line is; `None` for a date.
    pub entries: Vec<Option<usize>>,
}

impl Item {
    /// FAR's `DIF_HISTORY` named `history` on an edit line.
    pub fn with_history(mut self, history: EditHistory) -> Self {
        self.history = Some(Box::new(history));
        self
    }
}

impl Form {
    /// The history of item `index`.
    pub fn history(&self, index: usize) -> Option<&EditHistory> {
        self.items.get(index)?.history.as_deref()
    }

    pub fn history_mut(&mut self, index: usize) -> Option<&mut EditHistory> {
        self.items.get_mut(index)?.history.as_deref_mut()
    }

    /// The focused edit line's open history list, if any.
    pub fn open_history(&self) -> Option<&EditHistory> {
        self.history(self.focus)
            .filter(|history| history.open.is_some())
    }

    /// Ctrl+Down / Alt+Down on an edit line with lines in its history (FAR's `ProcessOpenComboBox`:
    /// not a read-only one). Returns whether it opened.
    pub(super) fn open_history_list(&mut self) -> bool {
        let read_only = self
            .items
            .get(self.focus)
            .is_some_and(|item| item.read_only);
        match self.history_mut(self.focus) {
            Some(history)
                if !read_only && history.entries.as_ref().is_some_and(|e| !e.is_empty()) =>
            {
                let lines = history.lines().lines;
                let first = (0..lines.len()).find(|&at| !lines[at].is_separator());
                history.open = Some(OpenList::at(first.unwrap_or(0)));
                true
            }
            _ => false,
        }
    }

    /// A key while the focused line's history list is open; `None` when none is.
    pub(super) fn history_key(
        &mut self,
        key: KeyEvent,
        clipboard: &mut dyn Clipboard,
    ) -> Option<FormOutcome> {
        let focus = self.focus;
        let history = self
            .history_mut(focus)
            .filter(|history| history.open.is_some())?;
        if let Some(question) = &mut history.question {
            match question.handle_key(key, clipboard) {
                FormOutcome::Stay => {}
                // The question's first button (its item 1).
                FormOutcome::Button(1) => {
                    history.question = None;
                    clear(history);
                }
                FormOutcome::Button(_) | FormOutcome::Cancel => history.question = None,
            }
            return Some(FormOutcome::Stay);
        }
        let shown = history.lines();
        let open = history.open.as_mut()?;
        let current = shown.entries.get(open.cursor).copied().flatten();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let plain = key.modifiers.is_empty();
        match key.code {
            KeyCode::Esc => history.open = None,
            KeyCode::Enter | KeyCode::Tab => {
                history.open = None;
                let line = current.and_then(|at| history.entries.as_ref()?.get(at));
                if let Some(text) = line.map(|line| line.entry.text.clone()) {
                    self.set_edit_text(focus, &text);
                }
            }
            KeyCode::Insert | KeyCode::Char('c' | 'C') if ctrl => {
                if let Some(line) = current.and_then(|at| history.entries.as_ref()?.get(at)) {
                    // A clipboard another program holds only loses this copy (as the edit's).
                    let _ = clipboard.set_text(&line.entry.text);
                }
            }
            KeyCode::Insert if plain => {
                if let Some(line) = current.and_then(|at| history.entries.as_mut()?.get_mut(at)) {
                    line.entry.locked = !line.entry.locked;
                    history
                        .changes
                        .push(HistoryChange::Lock(line.entry.clone()));
                }
            }
            KeyCode::Delete if key.modifiers == KeyModifiers::SHIFT => remove(history, current),
            KeyCode::Delete if plain && history.confirm_clear => {
                history.question = Some(clear_question());
            }
            KeyCode::Delete if plain => clear(history),
            _ => move_cursor(&shown.lines, open, key),
        }
        Some(FormOutcome::Stay)
    }
}

/// Shift+Del: FAR takes out an unlocked line; the cursor stays where it was, the list closes empty.
fn remove(history: &mut EditHistory, current: Option<usize>) {
    let Some(entries) = history.entries.as_mut() else {
        return;
    };
    let Some(at) = current.filter(|at| entries.get(*at).is_some_and(|line| !line.entry.locked))
    else {
        return;
    };
    history
        .changes
        .push(HistoryChange::Remove(entries.remove(at).entry));
    settle(history);
}

/// Del: what is not locked goes; FAR's list comes back with the rest, or closes empty.
fn clear(history: &mut EditHistory) {
    if let Some(entries) = history.entries.as_mut() {
        entries.retain(|line| line.entry.locked);
    }
    history.changes.push(HistoryChange::Clear);
    settle(history);
}

/// The cursor on a line that is there after one went, the list closed when none is left.
fn settle(history: &mut EditHistory) {
    let lines = history.lines().lines;
    let Some(open) = history.open.as_mut() else {
        return;
    };
    let at = open.cursor.min(lines.len().saturating_sub(1));
    // FAR's `VMenu` keeps the selection on a line that takes it: the one there, else the one before.
    let found = (at..lines.len())
        .find(|&line| !lines[line].is_separator())
        .or_else(|| (0..at).rev().find(|&line| !lines[line].is_separator()));
    match found {
        None => history.open = None,
        Some(line) => open.cursor = line,
    }
}

/// FAR's `MHistoryClear` (a warning).
fn clear_question() -> Box<Form> {
    let text = "All records in the history will be deleted. Continue?";
    let width = u16::try_from(text.chars().count()).unwrap_or(60) + 10;
    let items = vec![
        Item::new(5, 2, Control::Label(text.to_string())),
        Item::new(
            0,
            3,
            Control::Button {
                label: "&Clear history".to_string(),
                default: true,
            },
        ),
        Item::new(
            0,
            3,
            Control::Button {
                label: "Cancel".to_string(),
                default: false,
            },
        ),
    ];
    Box::new(Form::new("History", width, 6, items).warning())
}

/// Puts the last line given into each empty `DIF_USELASTHISTORY` line (FAR's `ProcessLastHistory`).
pub(super) fn fill_last(item: &mut Item) {
    let Some(history) = item.history.as_deref() else {
        return;
    };
    let last = (history.use_last)
        .then(|| {
            history
                .entries
                .as_ref()?
                .first()
                .map(|line| line.entry.text.clone())
        })
        .flatten();
    if let (Some(last), Control::Edit(input)) = (last, &mut item.control)
        && input.text().is_empty()
    {
        *input = super::LineInput::with_text(&last);
    }
}

#[cfg(test)]
#[path = "form_history_tests.rs"]
mod tests;
