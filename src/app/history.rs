//! The histories (`settings::history`): the commands given (Ctrl+E / Ctrl+X, Alt+F8), the folders the
//! panels were in (Alt+F12), the files viewed and edited (Alt+F11), and what is kept between runs. Each
//! is saved only while its switch in System settings is on; with it off it is of this run only.

use std::path::{Path, PathBuf};

use super::dialog::Dialog;
use super::history_menu::HistoryMenu;
use super::{App, Side};
use crate::settings::history::{Entry, Histories, Kind, List, Writer, load};
use crate::vfs::VfsPath;

/// What the app keeps of the histories.
#[derive(Debug, Default)]
pub(super) struct HistoryState {
    pub(super) lists: Histories,
    /// Saves the lines given; none until the program opens the file (not in most tests).
    pub(super) writer: Option<Writer>,
    /// Ctrl+E / Ctrl+X: the command on the line.
    pub(super) index: Option<usize>,
    /// The menu a dialog of it stands over.
    pub(super) behind: Option<HistoryMenu>,
    /// Each panel's folder when last looked at.
    seen: [Option<VfsPath>; 2],
    /// A folder gone to with Shift+Enter in the folders history: not noted when the panel gets there.
    pub(super) untold: [Option<VfsPath>; 2],
}

impl App {
    /// Whether `list` is saved between runs (FAR's System settings).
    fn saves(&self, list: List) -> bool {
        match list {
            List::Commands => self.prefs.save_history,
            List::Folders => self.prefs.save_folders_history,
            List::Views => self.prefs.save_view_history,
        }
    }

    pub(super) fn writer_for(&self, list: List) -> Option<&Writer> {
        self.history.writer.as_ref().filter(|_| self.saves(list))
    }

    /// FAR's `AddToHistory`: a line given again moves to the end, whatever its case, and keeps its lock.
    pub(super) fn add_to_history(&mut self, list: List, entry: Entry) {
        if let Some(writer) = self.writer_for(list) {
            writer.add(list, &entry);
        }
        self.history.lists.add(list, entry);
        self.refresh_open_history(list);
    }

    /// A command given, with the folder it was given in.
    pub(super) fn push_history(&mut self, command: &str) {
        let entry = Entry {
            folder: self.active_folder().display().to_string(),
            ..Entry::new(command, Kind::Plain)
        };
        self.add_to_history(List::Commands, entry);
        self.history.index = None;
    }

    /// A file opened in the viewer or the editor.
    pub(super) fn remember_file(&mut self, path: &Path, kind: Kind) {
        self.add_to_history(List::Views, Entry::new(&path.display().to_string(), kind));
    }

    /// FAR notes the folder a panel leaves and the one it comes to (`FileList::ChangeDir`).
    pub(super) fn remember_folders(&mut self) {
        for side in [Side::Left, Side::Right] {
            let panel = &self.panels[side.index()];
            // A quiet visit that never came (the folder failed): the next one is noted again.
            if (self.history.untold[side.index()].as_ref())
                .is_some_and(|quiet| !panel.is_reading() && *quiet != panel.path)
            {
                self.history.untold[side.index()] = None;
            }
            let path = &self.panels[side.index()].path;
            if self.history.seen[side.index()].as_ref() == Some(path) {
                continue;
            }
            let path = path.clone();
            let left = self.history.seen[side.index()].replace(path.clone());
            if self.history.untold[side.index()]
                .take()
                .is_some_and(|quiet| quiet == path)
            {
                continue;
            }
            if let Some(left) = left {
                self.add_to_history(List::Folders, Entry::new(&left.to_string(), Kind::Plain));
                self.add_to_history(List::Folders, Entry::new(&path.to_string(), Kind::Plain));
            }
        }
    }

    /// The commands given, oldest first.
    pub(super) fn commands(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.history
            .lists
            .commands
            .iter()
            .map(|entry| entry.text.as_str())
    }

    /// The histories of earlier runs, and saving from now on. A file that cannot be read is left alone:
    /// the history of this run is not saved over it.
    pub fn open_history(&mut self, path: PathBuf) {
        match load(&path) {
            Ok(saved) => {
                // FAR's history with saving off is of this run only.
                for list in List::ALL {
                    if self.saves(list) {
                        self.history
                            .lists
                            .list_mut(list)
                            .clone_from(saved.list(list));
                    }
                }
                self.history.writer = Some(Writer::start(path));
            }
            Err(error) => self.show_error("Command history", &error),
        }
    }

    /// The program ends: the active panel's folder is noted (FAR's `ControlObject::close`), and
    /// everything is on the disk first.
    pub fn close_history(&mut self) {
        if self.history.writer.is_some() {
            let folder = self.active_panel().path.to_string();
            self.add_to_history(List::Folders, Entry::new(&folder, Kind::Plain));
        }
        if let Some(writer) = self.history.writer.take() {
            writer.finish();
        }
    }

    pub(super) fn report_history_failure(&mut self) {
        if self.screen_busy() {
            return;
        }
        let failure = (self.history.writer.as_ref()).and_then(Writer::take_failure);
        if let Some(failure) = failure {
            self.show_dialog(Dialog::error("Command history", vec![failure]));
        }
    }

    /// Steps through command history: -1 older, +1 newer.
    pub(super) fn recall_history(&mut self, step: isize) {
        let count = self.history.lists.commands.len();
        if count == 0 {
            return;
        }
        let last = count - 1;
        let index = match (self.history.index, step < 0) {
            (None, true) => Some(last),
            (None, false) => None,
            (Some(index), true) => Some(index.saturating_sub(1)),
            (Some(index), false) if index < last => Some(index + 1),
            (Some(_), false) => None,
        };
        self.history.index = index;
        let text = index
            .and_then(|i| self.history.lists.commands.get(i))
            .map(|entry| entry.text.clone())
            .unwrap_or_default();
        self.command_line.set(&text);
    }

    /// Takes `line` out of the history, here and on the disk.
    pub(super) fn forget_history_line(&mut self, line: &str) {
        let gone: Vec<Entry> = (self.history.lists.commands.iter())
            .filter(|known| known.text == line)
            .cloned()
            .collect();
        self.history
            .lists
            .commands
            .retain(|known| known.text != line);
        self.history.index = None;
        if let Some(writer) = self.writer_for(List::Commands) {
            for entry in &gone {
                writer.remove(List::Commands, entry);
            }
        }
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
