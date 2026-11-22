//! FAR's results dialog of Find file (`FindFilesProcess`, `FindDlgProc`, `AddMenuRecord`): the console
//! less a column and a row on each side; the found objects under a line with their folder each time
//! the folder changes, "Files: n, folders: m" while it searches, the place being searched ("Searching
//! "text" in" it when a text is looked for), then "Search done"; New search, Go to, View, Panel, Stop
//! (Cancel once done). The first find moves the focus to Go to unless the user moved it. Up, Down, PgUp,
//! PgDn, Home, End move in the list whatever button has the focus; F3 / F4 open the found file while
//! the search goes on; Esc asks before it stops the search and closes the dialog once it is over.
//! Background (not FAR's: the owner's) or the key that sends a command to the background sends the
//! search on to a line of the Screens list (F12) while it searches.
//! Panel (`FD_BUTTON_PANEL`) hands the found files to a plugin that takes them (TmpPanel's panel).

use crate::app::associations::AssocMode;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::clipboard::Clipboard;
use crate::app::form::{Control, Form, FormOutcome, Item};
use crate::jobs::find::{Found, Search};
use crate::vfs::VfsPath;

/// FAR's `truncate_right` of the text in "Searching "text" in".
const TEXT_SHOWN: usize = 10;

// The form's items, as FAR's `FD_*` past its list.
const STATUS: usize = 0;
const PERCENT: usize = 1;
const SEPARATOR: usize = 2;
const NEW_SEARCH: usize = 3;
pub(in crate::app) const GO_TO: usize = 4;
const VIEW: usize = 5;
const PANEL: usize = 6;
const BACKGROUND: usize = 7;
const STOP: usize = 8;

/// A line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultLine {
    /// The folder of the found objects under it.
    Folder(VfsPath),
    Separator,
    /// The found object at this place.
    Found(usize),
}

/// What a key asks the application for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultsOutcome {
    Stay,
    Close,
    NewSearch,
    /// FAR's question "Operation has been interrupted".
    AskStop,
    /// Go to: the folder for the panel and the name for its cursor.
    GoTo(VfsPath, Option<String>),
    /// F3 / F4 and their modifiers: the found file, by its association for the key first, or the
    /// program's own viewer / editor at once (`None`: Ctrl+Shift, FAR's `shouldForceInternal`).
    View(VfsPath, Option<AssocMode>),
    Edit(VfsPath, Option<AssocMode>),
    /// The search goes on behind the panels, a line of the Screens list.
    Background,
    /// FAR's "Panel": the found files (and folders, when they were looked for) for a plugin's panel.
    Panel(Vec<crate::vfs::Entry>),
}

pub struct FindResults {
    search: Search,
    pub found: Vec<Found>,
    pub lines: Vec<ResultLine>,
    /// The line under the cursor (never a separator).
    pub cursor: usize,
    /// The first line shown.
    pub top: usize,
    /// How many lines the list shows.
    pub rows: usize,
    pub files: usize,
    pub folders: usize,
    /// FAR's `m_LastDirName`, compared as `VfsPath::same_place` does.
    last_folder: Option<VfsPath>,
    /// The frame, the status line and the buttons.
    pub form: Form,
    /// The search has ended and what it found is shown (FAR's `Finalized`).
    pub finalized: bool,
    /// Stop was pressed: what the search still hands over is dropped (FAR's `stop_and_discard`).
    stopping: bool,
    /// FAR's `FindPositionChanged`: the focus is the user's, finding no longer moves it.
    position_changed: bool,
    /// The text looked for.
    text: Option<String>,
    /// Advanced's column types and widths (FAR's `SearchOutFormat`, `SearchOutFormatWidth`), for the
    /// renderer.
    pub columns: (String, String),
    /// The masks searched for, for the Screens list.
    pub mask: String,
    /// A plugin takes found files (FAR's `AnySetFindList`): "Panel" is on once something is found.
    pub find_list: bool,
    /// Folders go to "Panel" too: they were looked for, not by bytes (FAR's `FindFolders && !Hex`).
    pub panel_folders: bool,
    started: Instant,
    /// Its place in the Screens list's searches while it is in front, to go back to.
    pub place: Option<usize>,
}

fn button(label: &str, default: bool, disabled: bool) -> Item {
    let control = Control::Button {
        label: label.to_string(),
        default,
    };
    Item::new(0, 0, control).disabled(disabled)
}

impl FindResults {
    pub fn new(search: Search, title: String, text: Option<String>) -> Self {
        let items = vec![
            Item::new(5, 0, Control::Label("\u{2026}".to_string())),
            Item::new(0, 0, Control::Label(String::new())).hidden(text.is_none()),
            Item::new(0, 0, Control::Separator),
            button("&New search", true, false),
            button("&Go to", false, true),
            button("&View", false, true),
            button("&Panel", false, true),
            button("&Background", false, false),
            button("&Stop", false, false),
        ];
        let mut form = Form::new(&title, 80, 25, items);
        form.focus = NEW_SEARCH;
        let mut results = Self {
            search,
            found: Vec::new(),
            lines: Vec::new(),
            cursor: 0,
            top: 0,
            rows: 1,
            files: 0,
            folders: 0,
            last_folder: None,
            form,
            finalized: false,
            stopping: false,
            position_changed: false,
            text,
            columns: (
                crate::settings::FIND_COLUMN_TYPES.to_string(),
                crate::settings::FIND_COLUMN_WIDTHS.to_string(),
            ),
            mask: String::new(),
            find_list: false,
            panel_folders: false,
            started: Instant::now(),
            place: None,
        };
        results.fit(80, 25);
        results
    }

    /// FAR's `DlgWidth = ScrX + 1 - 2`, `DlgHeight = ScrY + 1 - 2`, for a console `width` x `height`
    /// (written by the renderer, as a view metric).
    pub fn fit(&mut self, width: u16, height: u16) {
        let (width, height) = (width.saturating_sub(2), height.saturating_sub(2).max(10));
        self.form.width = width;
        self.form.height = height;
        for (index, item) in self.form.items.iter_mut().enumerate() {
            item.y = match index {
                // FAR's status and its percent share a row (`DlgHeight-5`).
                STATUS | PERCENT => height - 5,
                SEPARATOR => height - 4,
                _ => height - 3,
            };
        }
        // FAR's status from column 5 to `DlgWidth-6`, to `DlgWidth-12` with the percent after it.
        let percent = self.text.is_some();
        self.form.items[STATUS].width = width.saturating_sub(if percent { 16 } else { 10 });
        self.form.items[PERCENT].x = width.saturating_sub(9);
        self.rows = usize::from(height - 8);
        self.keep_cursor_shown();
    }

    /// Takes what the search found since the last call; whether anything shown changed.
    pub fn pump(&mut self) -> bool {
        if self.finalized {
            return false;
        }
        let over = self.search.over();
        let found = self.search.take_found();
        let changed = !found.is_empty();
        if !self.stopping {
            for item in found {
                self.add(item);
            }
        }
        if over {
            self.finalize();
            return true;
        }
        let status = self.status_text();
        let shown = self.set_status(status);
        let percent = self.set_percent();
        changed || shown || percent
    }

    /// FAR's `AddMenuRecord`: the folder's line when the folder is another than the last one's.
    fn add(&mut self, item: Found) {
        let folder = &item.folder;
        if !(self.last_folder.as_ref()).is_some_and(|last| last.same_place(folder)) {
            if !self.lines.is_empty() {
                self.lines.push(ResultLine::Separator);
            }
            self.lines.push(ResultLine::Folder(item.folder.clone()));
            self.last_folder = Some(folder.clone());
        }
        if self.files + self.folders == 0 {
            self.cursor = self.lines.len();
            for index in [GO_TO, VIEW] {
                self.form.items[index].disabled = false;
            }
            self.form.items[PANEL].disabled = !self.find_list;
            // FAR's `DN_DRAWDIALOGDONE`: the focus goes to Go to.
            if !self.position_changed {
                self.form.focus = GO_TO;
            }
        }
        if item.entry.is_dir {
            self.folders += 1;
        } else {
            self.files += 1;
        }
        self.lines.push(ResultLine::Found(self.found.len()));
        self.found.push(item);
        self.keep_cursor_shown();
    }

    fn finalize(&mut self) {
        self.finalized = true;
        let done = self.done_text();
        self.set_status(done);
        self.form.items[PERCENT].hidden = true;
        if let Control::Button { label, .. } = &mut self.form.items[STOP].control {
            *label = "&Cancel".to_string();
        }
        self.no_background();
    }

    fn done_text(&self) -> String {
        format!(
            "Search done. Found files: {}, folders: {}",
            self.files, self.folders
        )
    }

    /// The console's title once the search is over (FAR's `SetFarTitle` of `MFindFileDone`).
    pub fn done_title(&self) -> Option<String> {
        (self.finalized && !self.stopping).then(|| self.done_text())
    }

    /// Only a search that goes on goes to the background (owner): not one that is over or stopping.
    fn no_background(&mut self) {
        self.form.items[BACKGROUND].disabled = true;
        if self.form.focus == BACKGROUND {
            self.form.focus = STOP;
        }
    }

    fn goes_on(&self) -> bool {
        !self.finalized && !self.stopping
    }

    /// A search of `*` that found nothing, over or going on, for the tests of the drawing.
    #[cfg(test)]
    pub fn sample(over: bool) -> Self {
        let mut results = Self::new(Search::stalled(Vec::new(), over), "t".into(), None);
        results.mask = "*".to_string();
        results.pump();
        results
    }

    /// How many objects were found.
    pub fn found_count(&self) -> usize {
        self.files + self.folders
    }

    /// The masks and the text looked for: `*.rs "hello"`.
    pub fn subject(&self) -> String {
        match &self.text {
            Some(text) => format!("{} \"{text}\"", self.mask),
            None => self.mask.clone(),
        }
    }

    /// The time since the search started, for the moving block of the status row.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// FAR's `refresh_status`: the place being searched, cut in its middle to fit.
    fn status_text(&self) -> String {
        let place = self.search.status();
        let text = match &self.text {
            Some(text) => {
                let shown = crate::app::text_cut::truncate_right(text, TEXT_SHOWN);
                format!("Searching \"{shown}\" in {place}")
            }
            None => place,
        };
        // FAR's `truncate_center`.
        crate::app::text_cut::truncate_center(&text, usize::from(self.form.items[STATUS].width))
    }

    /// FAR's `{:3}%` of the file read for the text.
    fn set_percent(&mut self) -> bool {
        let text = format!("{:3}%", self.search.percent());
        let Control::Label(shown) = &mut self.form.items[PERCENT].control else {
            return false;
        };
        if self.text.is_none() || *shown == text {
            return false;
        }
        *shown = text;
        true
    }

    fn set_status(&mut self, text: String) -> bool {
        let Control::Label(shown) = &mut self.form.items[STATUS].control else {
            return false;
        };
        if *shown == text {
            return false;
        }
        *shown = text;
        true
    }

    /// The count on the line above the status while it searches.
    pub fn count_text(&self) -> Option<String> {
        (!self.finalized).then(|| format!("Files: {}, folders: {}", self.files, self.folders))
    }

    /// FAR's question is up: the search waits for the answer.
    pub fn pause(&self, on: bool) {
        if !self.stopping {
            self.search.pause(on);
        }
    }

    /// Whether the search was told to stop, and to wait.
    #[cfg(test)]
    pub fn told(&self) -> (bool, bool) {
        self.search.told()
    }

    /// Stop: the search ends, what it still had is dropped.
    pub fn stop(&mut self) {
        self.stopping = true;
        self.no_background();
        self.search.pause(false);
        self.search.stop();
    }

    pub fn handle_key(&mut self, key: KeyEvent, clipboard: &mut dyn Clipboard) -> ResultsOutcome {
        if let Some(outcome) = self.open_key(key) {
            return outcome;
        }
        if crate::app::detached::is_detach_key(key) {
            return if self.goes_on() {
                ResultsOutcome::Background
            } else {
                ResultsOutcome::Stay
            };
        }
        let plain = key.modifiers.is_empty();
        match key.code {
            KeyCode::Esc | KeyCode::F(10) if plain && !self.finalized => ResultsOutcome::AskStop,
            KeyCode::Esc | KeyCode::F(10) if plain => ResultsOutcome::Close,
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown if plain => {
                self.move_cursor(key.code);
                ResultsOutcome::Stay
            }
            KeyCode::Home | KeyCode::End if plain => {
                self.move_cursor(key.code);
                ResultsOutcome::Stay
            }
            KeyCode::Right | KeyCode::Left if plain => {
                self.step_focus(key.code == KeyCode::Right);
                ResultsOutcome::Stay
            }
            _ => self.form_key(key, clipboard),
        }
    }

    /// F3 and F4 with FAR's modifiers (`GetFiletypeOpenMode`): the found file under the cursor.
    fn open_key(&self, key: KeyEvent) -> Option<ResultsOutcome> {
        let KeyCode::F(number @ (3 | 4)) = key.code else {
            return None;
        };
        let view = number == 3;
        let mode = match key.modifiers {
            KeyModifiers::NONE if view => Some(AssocMode::View),
            KeyModifiers::NONE => Some(AssocMode::Edit),
            KeyModifiers::ALT if view => Some(AssocMode::AltView),
            KeyModifiers::ALT => Some(AssocMode::AltEdit),
            modifiers if modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT => None,
            _ => return None,
        };
        let Some(path) = self.current_file() else {
            return Some(ResultsOutcome::Stay);
        };
        Some(if view {
            ResultsOutcome::View(path, mode)
        } else {
            ResultsOutcome::Edit(path, mode)
        })
    }

    fn form_key(&mut self, key: KeyEvent, clipboard: &mut dyn Clipboard) -> ResultsOutcome {
        let focus = self.form.focus;
        let outcome = self.form.handle_key(key, clipboard);
        if self.form.focus != focus {
            self.position_changed = true;
        }
        match outcome {
            FormOutcome::Stay | FormOutcome::Cancel => ResultsOutcome::Stay,
            FormOutcome::Button(index) => self.press(index),
        }
    }

    fn press(&mut self, index: usize) -> ResultsOutcome {
        self.position_changed = true;
        match index {
            NEW_SEARCH => ResultsOutcome::NewSearch,
            GO_TO => self.go_to(),
            // FAR's button sends F3.
            VIEW => (self.current_file()).map_or(ResultsOutcome::Stay, |path| {
                ResultsOutcome::View(path, Some(AssocMode::View))
            }),
            // Nothing on a disk (a plugin's panel without real names: FAR's Panel is no button there):
            // nothing handed, no plugin's panel emptied.
            PANEL if self.find_list => match self.panel_entries() {
                found if found.is_empty() => ResultsOutcome::Stay,
                found => ResultsOutcome::Panel(found),
            },
            BACKGROUND if self.goes_on() => ResultsOutcome::Background,
            STOP if !self.finalized => {
                self.stop();
                ResultsOutcome::Stay
            }
            STOP => ResultsOutcome::Close,
            _ => ResultsOutcome::Stay,
        }
    }

    /// FAR's `FD_BUTTON_GOTO`: a found object is shown in its folder; a folder's line, the folder in
    /// the one it is in (a root as it is).
    fn go_to(&self) -> ResultsOutcome {
        let path = match self.lines.get(self.cursor) {
            Some(ResultLine::Folder(folder)) => folder.clone(),
            Some(ResultLine::Found(at)) => self.found[*at].path(),
            _ => return ResultsOutcome::Stay,
        };
        match (path.parent(), path.file_name()) {
            // A stream's file (FAR cuts `:stream` off the name).
            (Some(folder), Some(name)) => {
                let file = name.split(':').next().unwrap_or_default().to_string();
                ResultsOutcome::GoTo(folder, Some(file))
            }
            _ => ResultsOutcome::GoTo(path, None),
        }
    }

    /// FAR's dialog moves among its buttons, Right past Stop to New search (`FindPositionChanged`).
    fn step_focus(&mut self, forward: bool) {
        let buttons: Vec<usize> = (NEW_SEARCH..=STOP)
            .filter(|&index| !self.form.items[index].disabled)
            .collect();
        let Some(at) = buttons.iter().position(|&index| index == self.form.focus) else {
            self.form.focus = NEW_SEARCH;
            return;
        };
        let next = if forward {
            (at + 1) % buttons.len()
        } else {
            (at + buttons.len() - 1) % buttons.len()
        };
        self.form.focus = buttons[next];
        self.position_changed = true;
    }

    fn move_cursor(&mut self, code: KeyCode) {
        if self.lines.is_empty() {
            return;
        }
        let last = self.lines.len() - 1;
        let page = self.rows.saturating_sub(1).max(1);
        let (target, down) = match code {
            KeyCode::Up => (self.cursor.saturating_sub(1), false),
            KeyCode::Down => ((self.cursor + 1).min(last), true),
            KeyCode::PageUp => (self.cursor.saturating_sub(page), false),
            KeyCode::PageDown => ((self.cursor + page).min(last), true),
            KeyCode::Home => (0, true),
            _ => (last, false),
        };
        self.cursor = self.selectable_from(target, down);
        self.keep_cursor_shown();
    }

    /// A click on row `row` of the list: the cursor on its line (not a separator, not past the end);
    /// whether it is there now.
    pub fn click_row(&mut self, row: usize) -> bool {
        let at = self.top + row;
        if self
            .lines
            .get(at)
            .is_none_or(|line| *line == ResultLine::Separator)
        {
            return false;
        }
        self.cursor = at;
        true
    }

    /// The wheel over the dialog: the cursor a line per notch, away from the user up (FAR's list's
    /// `KEY_MSWHEEL_UP` with `MsWheelDelta` 1).
    pub fn wheel(&mut self, notches: i32) {
        let code = if notches > 0 {
            KeyCode::Up
        } else {
            KeyCode::Down
        };
        for _ in 0..notches.unsigned_abs() {
            self.move_cursor(code);
        }
    }

    /// `at`, or the nearest line past a separator, in the direction moved first.
    fn selectable_from(&self, at: usize, down: bool) -> usize {
        let selectable = |index: &usize| self.lines[*index] != ResultLine::Separator;
        let forward = (at..self.lines.len()).find(selectable);
        let backward = (0..=at).rev().find(selectable);
        let first = if down { forward } else { backward };
        first.or(forward).or(backward).unwrap_or(0)
    }

    fn keep_cursor_shown(&mut self) {
        if self.cursor < self.top {
            self.top = self.cursor;
        } else if self.cursor >= self.top + self.rows {
            self.top = self.cursor + 1 - self.rows;
        }
    }
}

#[path = "results_found.rs"]
mod found;

#[cfg(test)]
#[path = "results_tests.rs"]
mod tests;
