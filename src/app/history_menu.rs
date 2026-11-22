//! The history menus as FAR's `History::ProcessMenu`: Alt+F8 the commands, Alt+F12 the folders, Alt+F11
//! the files viewed and edited. Oldest first, a line with the date where a day begins, the newest one
//! selected. Ins locks a line (`√`), Shift+Del takes out the one under the cursor, Del all that are not
//! locked, Ctrl+R those whose folder or file is gone; Ctrl+C / Ctrl+Ins copy it. What Enter and its
//! variants do is FAR's (`cmdline.cpp`); a folder or file no longer there is said and the menu stays.
//! Ctrl+Alt+F filters the lines (`menu_filter`, FAR's `VMenu` filter); a date stays over the lines left.

use std::cell::Cell;
use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::dialog::{ConfirmAction, Dialog};
use super::menu_filter::{Filtered, MenuFilter, filter_key};
use super::{App, MainView, Overlay, Screen};
use crate::platform;
use crate::settings::history::{Entry, Kind, List};
use crate::vfs::VfsPath;

/// An open history menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMenu {
    pub list: List,
    pub lines: Vec<HistoryLine>,
    pub selected: usize,
    /// The first line shown (FAR's `TopPos`), kept by the renderer.
    pub top: Cell<usize>,
    /// The lines the renderer shows, for PgUp and PgDn.
    pub rows: Cell<usize>,
    /// Ctrl+Alt+F's filter, while it is on.
    pub filter: Option<MenuFilter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryLine {
    /// Where a day begins: its date (FAR's separator with a title).
    Date(String),
    /// Line `index` of the history, as the menu shows it, and when it was given.
    Entry {
        index: usize,
        text: String,
        locked: bool,
        time: u64,
    },
}

impl HistoryLine {
    fn index(&self) -> Option<usize> {
        match self {
            Self::Date(_) => None,
            Self::Entry { index, .. } => Some(*index),
        }
    }
}

/// FAR's titles (`MHistoryTitle`, `MFolderHistoryTitle`, `MViewHistoryTitle`).
pub fn history_title(list: List) -> &'static str {
    match list {
        List::Commands => "History",
        List::Folders => "Folders history",
        List::Views => "File view and edit history",
    }
}

/// How a line was taken (FAR's `history_return_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Choice {
    Enter,
    /// Run in a new window; go or open without the history noting it.
    ShiftEnter,
    /// Only onto the command line.
    CtrlEnter,
    /// The folder in the passive panel.
    CtrlShiftEnter,
    /// Run as administrator.
    CtrlAltEnter,
    /// F3 and F4 of the view history.
    View,
    Edit,
}

fn choice_of(key: KeyEvent) -> Option<Choice> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Enter => Some(match (ctrl, alt, shift) {
            (true, true, _) => Choice::CtrlAltEnter,
            (true, false, true) => Choice::CtrlShiftEnter,
            (true, false, false) => Choice::CtrlEnter,
            (false, _, true) => Choice::ShiftEnter,
            (false, _, false) => Choice::Enter,
        }),
        _ => None,
    }
}

/// Day, month and year of `time` in local time.
pub(super) fn day_of(time: u64) -> Option<(u16, u16, u16)> {
    let local = platform::locale::local_time(system_time(time))?;
    Some((local.year, local.month, local.day))
}

pub(super) fn system_time(time: u64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(time)
}

/// The menu's text of a line (FAR's `View:`, `Edit:` before a file).
fn line_text(entry: &Entry) -> String {
    match entry.kind {
        Kind::Plain => entry.text.clone(),
        Kind::View => format!("View: {}", entry.text),
        Kind::Edit => format!("Edit: {}", entry.text),
    }
}

/// Where a folder or file of the history is, to look whether it is still there.
fn local_place(list: List, entry: &Entry) -> Option<PathBuf> {
    match list {
        List::Commands => None,
        List::Folders => VfsPath::parse(&entry.text)
            .as_local()
            .map(std::path::Path::to_path_buf),
        List::Views => Some(PathBuf::from(&entry.text)),
    }
}

/// Whether FAR's handler of `list` does anything with `choice` (`cmdline.cpp`: Alt+F8 takes Enter,
/// Shift+Enter, Ctrl+Enter, Ctrl+Alt+Enter; Alt+F12 Enter, Shift+Enter, Ctrl+Shift+Enter, Ctrl+Enter;
/// Alt+F11 Enter, Shift+Enter, F3, F4, Ctrl+Enter).
fn takes(list: List, choice: Choice) -> bool {
    match list {
        List::Commands => !matches!(choice, Choice::CtrlShiftEnter | Choice::View | Choice::Edit),
        List::Folders => !matches!(choice, Choice::CtrlAltEnter | Choice::View | Choice::Edit),
        List::Views => !matches!(choice, Choice::CtrlShiftEnter | Choice::CtrlAltEnter),
    }
}

impl HistoryMenu {
    fn entry_lines(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.lines.len()).filter(|&at| self.lines[at].index().is_some())
    }

    /// The line of the history under the cursor.
    pub(super) fn current(&self) -> Option<usize> {
        self.lines.get(self.selected).and_then(HistoryLine::index)
    }

    /// One entry up or down, round the ends (FAR's `VMENU_WRAPMODE`).
    fn step(&mut self, down: bool) {
        let count = self.lines.len();
        for offset in 1..=count {
            let at = if down {
                (self.selected + offset) % count
            } else {
                (self.selected + count * 2 - offset) % count
            };
            if self.lines[at].index().is_some() {
                self.selected = at;
                return;
            }
        }
    }

    /// The entry nearest to line `at`, below it first.
    fn select_near(&mut self, at: usize) {
        let below = self.entry_lines().find(|&line| line >= at);
        let above = self.entry_lines().filter(|&line| line < at).last();
        self.selected = below.or(above).unwrap_or(0);
    }

    fn page(&mut self, down: bool) {
        let rows = self.rows.get().max(2) - 1;
        let at = if down {
            self.selected + rows
        } else {
            self.selected.saturating_sub(rows)
        };
        let last = self.lines.len().saturating_sub(1);
        if down {
            let at = at.min(last);
            let above = self.entry_lines().filter(|&line| line <= at).last();
            self.selected = above.unwrap_or(self.selected);
        } else {
            self.select_near(at);
        }
    }
}

impl App {
    /// The menu of `list`, its newest line selected.
    pub(super) fn open_history_menu(&mut self, list: List) {
        let mut menu = HistoryMenu {
            list,
            lines: Vec::new(),
            selected: 0,
            top: Cell::new(0),
            rows: Cell::new(1),
            filter: None,
        };
        self.fill_history_menu(&mut menu);
        menu.select_near(menu.lines.len());
        self.overlay = Some(Overlay::History(menu));
    }

    /// The menu's lines as the history has them now; the cursor stays on its line, or where it was.
    fn fill_history_menu(&self, menu: &mut HistoryMenu) {
        // A line given again has a new time; the text alone is the line (once in a list).
        let was = match menu.lines.get(menu.selected) {
            Some(HistoryLine::Entry { text, .. }) => Some(text.clone()),
            _ => None,
        };
        let mut lines = Vec::new();
        let (mut day, mut date) = (None, None);
        for (index, entry) in self.history.lists.list(menu.list).iter().enumerate() {
            let today = day_of(entry.time);
            if today.is_some() && today != day {
                day = today;
                date = Some(platform::locale::date_time(system_time(entry.time), true, 8).0);
            }
            let text = line_text(entry);
            if !(menu.filter.as_ref()).is_none_or(|filter| filter.keeps(&text)) {
                continue;
            }
            // A day's date over its first line left (FAR's filter hides a separator with no lines).
            lines.extend(date.take().map(HistoryLine::Date));
            lines.push(HistoryLine::Entry {
                index,
                text,
                locked: entry.locked,
                time: entry.time,
            });
        }
        menu.lines = lines;
        let same = |line: &HistoryLine| match (line, &was) {
            (HistoryLine::Entry { text, .. }, Some(was)) => {
                crate::platform::files::fold_case(text) == crate::platform::files::fold_case(was)
            }
            _ => false,
        };
        match menu.lines.iter().position(same) {
            Some(at) => menu.selected = at,
            None => {
                let at = menu.selected;
                menu.select_near(at);
            }
        }
    }

    /// The open menu of `list` follows a change of the history under it.
    pub(super) fn refresh_open_history(&mut self, list: List) {
        let Some(Overlay::History(open)) = &self.overlay else {
            return;
        };
        if open.list == list {
            let mut menu = open.clone();
            self.fill_history_menu(&mut menu);
            self.overlay = Some(Overlay::History(menu));
        }
    }

    pub(super) fn handle_history_key(&mut self, mut menu: HistoryMenu, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let shown = menu.entry_lines().count();
        let clipboard = &mut self.clipboard;
        match filter_key(&mut menu.filter, key, shown, || {
            clipboard.text().ok().flatten()
        }) {
            Filtered::No => {}
            Filtered::Kept => return self.overlay = Some(Overlay::History(menu)),
            Filtered::Changed => {
                self.fill_history_menu(&mut menu);
                return self.overlay = Some(Overlay::History(menu));
            }
        }
        // FAR's `VMenu`: with no line left another key closes the menu.
        if shown == 0 && menu.filter.is_some() {
            return;
        }
        if let Some(choice) = choice_of(key) {
            return self.choose_history(menu, choice);
        }
        match key.code {
            KeyCode::Esc | KeyCode::F(10) => return,
            KeyCode::F(3) if menu.list == List::Views => {
                return self.choose_history(menu, Choice::View);
            }
            KeyCode::F(4) if menu.list == List::Views => {
                return self.choose_history(menu, Choice::Edit);
            }
            KeyCode::F(3) if menu.list == List::Commands => {
                return self.show_history_folder(menu);
            }
            KeyCode::Up if !menu.lines.is_empty() => menu.step(false),
            KeyCode::Down if !menu.lines.is_empty() => menu.step(true),
            KeyCode::Home => menu.select_near(0),
            KeyCode::End => menu.select_near(menu.lines.len()),
            KeyCode::PageUp => menu.page(false),
            KeyCode::PageDown => menu.page(true),
            KeyCode::Char('c' | 'C') | KeyCode::Insert if ctrl => {
                return self.copy_history_line(menu);
            }
            KeyCode::Insert if key.modifiers.is_empty() => self.flip_history_lock(&mut menu),
            KeyCode::Delete if shift => self.remove_history_line(&mut menu),
            KeyCode::Delete => return self.ask_clear_history(menu),
            KeyCode::Char('r' | 'R') if ctrl && menu.list != List::Commands => {
                return self.remove_missing_history(menu);
            }
            _ => {}
        }
        self.overlay = Some(Overlay::History(menu));
    }

    fn history_entry(&self, menu: &HistoryMenu) -> Option<Entry> {
        let index = menu.current()?;
        self.history.lists.list(menu.list).get(index).cloned()
    }

    fn copy_history_line(&mut self, menu: HistoryMenu) {
        let copied = self
            .history_entry(&menu)
            .map(|entry| self.clipboard.set_text(&entry.text));
        if let Some(Err(error)) = copied {
            self.history.behind = Some(menu);
            self.show_error("Clipboard", &error);
        } else {
            self.overlay = Some(Overlay::History(menu));
        }
    }

    fn flip_history_lock(&mut self, menu: &mut HistoryMenu) {
        let Some(index) = menu.current() else {
            return;
        };
        let list = menu.list;
        let Some(entry) = self.history.lists.list_mut(list).get_mut(index) else {
            return;
        };
        entry.locked = !entry.locked;
        let entry = entry.clone();
        if let Some(writer) = self.writer_for(list) {
            writer.set_lock(list, &entry);
        }
        self.fill_history_menu(menu);
    }

    /// Shift+Del: the line under the cursor, unless it is locked.
    fn remove_history_line(&mut self, menu: &mut HistoryMenu) {
        let Some(entry) = self.history_entry(menu).filter(|entry| !entry.locked) else {
            return;
        };
        let list = menu.list;
        if let Some(index) = menu.current() {
            self.history.lists.list_mut(list).remove(index);
        }
        if let Some(writer) = self.writer_for(list) {
            writer.remove(list, &entry);
        }
        self.history.index = None;
        self.fill_history_menu(menu);
    }

    /// Del: FAR's `MHistoryClear` question while "Clear history list" is on.
    fn ask_clear_history(&mut self, menu: HistoryMenu) {
        let list = menu.list;
        let has_lines = !menu.lines.is_empty();
        self.history.behind = Some(menu);
        if !has_lines {
            return;
        }
        if !self.prefs.confirm_history_clear {
            return self.clear_history(list);
        }
        self.show_dialog(Dialog::Confirm {
            title: history_title(list).to_string(),
            lines: vec!["All records in the history will be deleted. Continue?".to_string()],
            buttons: vec!["&Clear history", "Cancel"],
            focused: 0,
            danger: true,
            action: ConfirmAction::ClearHistory(list),
        });
    }

    /// What is not locked leaves `list`.
    pub(super) fn clear_history(&mut self, list: List) {
        self.history
            .lists
            .list_mut(list)
            .retain(|entry| entry.locked);
        if let Some(writer) = self.writer_for(list) {
            writer.clear_unlocked(list);
        }
        self.history.index = None;
    }

    /// Ctrl+R: the lines whose folder or file is gone leave, locked ones stay. The disk is looked at
    /// beside the UI.
    fn remove_missing_history(&mut self, menu: HistoryMenu) {
        let list = menu.list;
        let places: Vec<(Entry, PathBuf)> = (self.history.lists.list(list).iter())
            .filter(|entry| !entry.locked)
            .filter_map(|entry| Some((entry.clone(), local_place(list, entry)?)))
            .collect();
        self.overlay = Some(Overlay::History(menu));
        self.wait_for(Vec::new(), move || {
            let gone: Vec<Entry> = (places.into_iter())
                .filter(|(_, place)| !place.exists())
                .map(|(entry, _)| entry)
                .collect();
            Box::new(move |app: &mut App| app.forget_history_lines(list, &gone))
        });
        self.label_waiting("Looking for the files and folders…");
    }

    fn forget_history_lines(&mut self, list: List, gone: &[Entry]) {
        for entry in gone {
            let entries = self.history.lists.list_mut(list);
            entries.retain(|known| !(known.same(entry) && known.time == entry.time));
            if let Some(writer) = self.writer_for(list) {
                writer.remove(list, entry);
            }
        }
        if let Some(Overlay::History(menu)) = &mut self.overlay {
            let mut menu = menu.clone();
            self.fill_history_menu(&mut menu);
            self.overlay = Some(Overlay::History(menu));
        }
    }

    /// F3 of the commands: where the command was given (FAR's `MHistoryInfoTitle`).
    fn show_history_folder(&mut self, menu: HistoryMenu) {
        let Some(entry) = self.history_entry(&menu) else {
            self.overlay = Some(Overlay::History(menu));
            return;
        };
        self.history.behind = Some(menu);
        self.show_dialog(Dialog::message(
            "Information",
            vec!["Folder:".to_string(), entry.folder],
        ));
    }

    /// A line taken: a folder or file is looked for first, beside the UI, as FAR does.
    fn choose_history(&mut self, menu: HistoryMenu, choice: Choice) {
        let Some(entry) = self.history_entry(&menu) else {
            return;
        };
        // What FAR's handlers do nothing with closes the menu, and no more.
        if !takes(menu.list, choice) {
            return;
        }
        let place = local_place(menu.list, &entry).filter(|_| choice != Choice::CtrlEnter);
        let Some(place) = place else {
            return self.use_history_line(menu.list, entry, choice);
        };
        self.overlay = Some(Overlay::History(menu));
        self.wait_for(Vec::new(), move || {
            let there = place.exists();
            Box::new(move |app: &mut App| {
                let Some(Overlay::History(menu)) = app.overlay.take() else {
                    return;
                };
                if there {
                    app.use_history_line(menu.list, entry, choice);
                } else {
                    let title = history_title(menu.list);
                    app.history.behind = Some(menu);
                    let error = std::io::Error::from(std::io::ErrorKind::NotFound);
                    app.show_dialog(Dialog::error(title, vec![error.to_string(), entry.text]));
                }
            })
        });
        self.label_waiting("Looking for the file or folder…");
    }

    fn use_history_line(&mut self, list: List, entry: Entry, choice: Choice) {
        if choice == Choice::CtrlEnter {
            self.command_line.set(&entry.text);
            return;
        }
        match list {
            List::Commands => {
                self.command_line.set(&entry.text);
                match choice {
                    Choice::Enter => self.run_command_line(),
                    Choice::ShiftEnter => self.run_in_new_window(),
                    Choice::CtrlAltEnter => self.run_as_administrator(),
                    _ => {}
                }
            }
            List::Folders => self.go_to_history_folder(&entry.text, choice),
            List::Views => {
                let path = PathBuf::from(&entry.text);
                let remember = choice != Choice::ShiftEnter;
                let edit = match choice {
                    Choice::View => false,
                    Choice::Edit => true,
                    _ => entry.kind == Kind::Edit,
                };
                if edit {
                    self.edit_file(&path, remember);
                } else {
                    self.view_file(&path, remember);
                }
            }
        }
    }

    /// Enter in the active panel, Ctrl+Shift+Enter in the passive one; Shift+Enter leaves no trace in
    /// the history.
    fn go_to_history_folder(&mut self, text: &str, choice: Choice) {
        let side = match choice {
            Choice::Enter | Choice::ShiftEnter => self.active,
            Choice::CtrlShiftEnter => self.active.other(),
            _ => return,
        };
        let path = VfsPath::parse(text);
        if choice == Choice::ShiftEnter && self.panels[side.index()].path != path {
            self.history.untold[side.index()] = Some(path.clone());
        }
        if let Err(error) = self.panels[side.index()].change_dir(&path, None) {
            self.history.untold[side.index()] = None;
            self.show_error("Change directory", &error);
        }
    }

    /// The menu a dialog of it stood over comes back once the dialog is gone; true when it did.
    pub(super) fn resume_history_menu(&mut self) -> bool {
        if self.overlay.is_some() || self.waiting.is_some() {
            return false;
        }
        let Some(mut menu) = self.history.behind.take() else {
            return false;
        };
        if !matches!(self.screen, Screen::Panels) || self.main_view != MainView::Panels {
            return false;
        }
        self.fill_history_menu(&mut menu);
        self.overlay = Some(Overlay::History(menu));
        true
    }
}

#[cfg(test)]
#[path = "history_menu_tests.rs"]
mod tests;
