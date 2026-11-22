//! Alt+F7: FAR's "Find file" dialog (`far/findfile.cpp`, `FindFiles::FindFiles`): the masks, the text
//! the files contain, the switches and the search area; Find starts the search (`jobs::find`) in its
//! results dialog (`find::results`), Drive opens the drive menu of the active panel and comes back. What
//! the dialog was given stays for the next one in the session (FAR's `LastFindFileOptions` and shared
//! search parameters); the masks, the area, the code page and its marks, the folder, link and stream
//! switches and Advanced's are saved. Hex looks for the bytes, fuzzy folds the text and the files alike.
//! Background sends a search behind the panels (`find::background`).
//! "Use filter" searches through Find file's filter, Filter opens its filters menu (FAR's
//! `EditFilters(FFT_FINDFILE)`) and the dialog comes back. "Search in archives" (kept for the session
//! only) has the plugins open each file without any window, its insides searched (`jobs::find`); Go to,
//! F3 / F4 on what was found there in `find::archived` and `extracted`.

mod advanced;
mod archived;
mod background;
mod code_pages;
pub(in crate::app) mod dialog;
#[cfg(test)]
mod filter_tests;
pub(super) mod results;

use super::associations::{AssocMode, AssocTarget};
pub use background::search_label;
use code_pages::CodePageChoice;
pub(super) use dialog::follow;
#[cfg(test)]
use dialog::{ADVANCED, FOLDERS, FUZZY, HEX, MASK, RADIO_HEX, STREAMS, TEXT};
use dialog::{AREA, DRIVE, FILTER, FIND};

use std::sync::Arc;
use std::time::SystemTime;

use ratatui::crossterm::event::KeyEvent;

use super::dialog::{ConfirmAction, Dialog, FormAction};
use super::form::{Control, Form, Item, ListLine};
use super::{App, MainView, Overlay, Panel, Screen};
use crate::jobs::find::text::{Pattern, Query};
use crate::jobs::find::{Archives, Drives, Request, Search};
use crate::settings::Settings;
use crate::vfs::{FileMask, FileOpener, VfsPath};
use results::{FindResults, ResultsOutcome};

pub(super) const FIND_TITLE: &str = "Find file";
/// FAR's `AllFilesMask`.
const ALL_FILES: &str = "*";
/// FAR's `FINDAREA`, in the order of its list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    AllDrives,
    LocalDrives,
    Path,
    Root,
    FromCurrent,
    Current,
    Selected,
}

impl Area {
    const ALL: [Self; 7] = [
        Self::AllDrives,
        Self::LocalDrives,
        Self::Path,
        Self::Root,
        Self::FromCurrent,
        Self::Current,
        Self::Selected,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::AllDrives => "allDrives",
            Self::LocalDrives => "localDrives",
            Self::Path => "path",
            Self::Root => "root",
            Self::FromCurrent => "fromCurrent",
            Self::Current => "current",
            Self::Selected => "selected",
        }
    }

    fn from_id(id: &str) -> Self {
        (Self::ALL.into_iter())
            .find(|area| area.id() == id)
            .unwrap_or(Self::FromCurrent)
    }

    /// The line of the list; `root` FAR's "From the root of C:" (`PrepareDriveNameStr`).
    fn label(self, root: &str) -> String {
        match self {
            Self::AllDrives => "In &all non-removable drives".to_string(),
            Self::LocalDrives => "In all &local drives".to_string(),
            Self::Path => "In &PATH folders".to_string(),
            Self::Root => root.to_string(),
            Self::FromCurrent => "From the curre&nt folder".to_string(),
            Self::Current => "The current folder onl&y".to_string(),
            Self::Selected => "&Selected folders".to_string(),
        }
    }
}

/// What the dialog keeps between searches, and the dialogs waiting behind another.
#[derive(Default)]
pub(super) struct FindState {
    mask: String,
    text: String,
    /// The hex line as it was written, and whether it is the one shown.
    hex: String,
    hex_mode: bool,
    case_sensitive: bool,
    whole_words: bool,
    fuzzy: bool,
    not_containing: bool,
    /// FAR's "Search in archives", kept for the session only (`LastFindFileOptions`).
    archives: bool,
    folders: bool,
    links: bool,
    streams: bool,
    /// FAR's "Use filter".
    use_filter: bool,
    area: Option<Area>,
    /// "Using code page" and the code pages marked in its list.
    code_page: CodePageChoice,
    marked: Vec<u32>,
    /// Advanced's "Search only in the first", "Column types" and "Column widths".
    first: String,
    column_types: String,
    column_widths: String,
    /// The results while the viewer, the editor or a question is in front of them.
    pub(super) results: Option<Box<FindResults>>,
    /// The searches sent to the background, in the order they went there.
    background: Vec<FindResults>,
    /// The dialog while the drive menu or a warning is in front of it.
    dialog: Option<Form>,
    /// The dialog under Advanced, and Advanced while its warning is in front of it.
    dialog_under_advanced: Option<Form>,
    advanced: Option<Form>,
}

impl FindState {
    pub(super) fn saved(settings: &Settings) -> Self {
        Self {
            mask: settings.find.mask.clone(),
            folders: settings.find.folders,
            links: settings.find.links,
            streams: settings.find.streams,
            use_filter: settings.find.use_filter,
            area: Some(Area::from_id(&settings.find.area)),
            code_page: CodePageChoice::from_id(&settings.find.code_page),
            marked: settings.find.code_pages.clone(),
            first: settings.find.first.clone(),
            column_types: settings.find.column_types.clone(),
            column_widths: settings.find.column_widths.clone(),
            ..Self::default()
        }
    }

    pub(super) fn write_to(&self, settings: &mut Settings) {
        settings.find.mask.clone_from(&self.mask);
        settings.find.area = self.area().id().to_string();
        settings.find.code_page = self.code_page.id();
        settings.find.code_pages.clone_from(&self.marked);
        settings.find.folders = self.folders;
        settings.find.links = self.links;
        settings.find.streams = self.streams;
        settings.find.use_filter = self.use_filter;
        settings.find.first.clone_from(&self.first);
        settings.find.column_types.clone_from(&self.column_types);
        settings.find.column_widths.clone_from(&self.column_widths);
    }

    fn area(&self) -> Area {
        self.area.unwrap_or(Area::FromCurrent)
    }

    /// FAR's "Containing", as text or as hex: none when it is empty.
    fn pattern(&self) -> Option<Pattern> {
        if self.hex_mode {
            let bytes = dialog::parse_hex(&self.hex);
            return (!bytes.is_empty()).then_some(Pattern::Bytes(bytes));
        }
        (!self.text.is_empty()).then(|| {
            Pattern::Text(Query {
                text: self.text.clone(),
                case_sensitive: self.case_sensitive,
                whole_words: self.whole_words,
                fuzzy: self.fuzzy,
                code_pages: self.code_page.code_pages(&self.marked),
            })
        })
    }

    /// What "Searching "..." in" shows of the pattern (FAR's text, or its bytes in hex).
    fn shown_pattern(&self) -> Option<String> {
        match self.pattern()? {
            Pattern::Text(query) => Some(query.text),
            Pattern::Bytes(bytes) => Some(dialog::hex_text(&bytes, "")),
        }
    }
}

/// FAR's `GetSelName` loop of "Selected folders": the selected folders, or the one under the cursor.
fn selected_folders(panel: &Panel) -> Vec<VfsPath> {
    let taken = |name: &str| {
        if panel.selected.is_empty() {
            panel.current().is_some_and(|entry| entry.name == name)
        } else {
            panel.selected.contains(name)
        }
    };
    (panel.entries.iter())
        .filter(|entry| entry.is_dir && !entry.is_parent && taken(&entry.name))
        .map(|entry| panel.path.join(&entry.name))
        .collect()
}

/// The folders of PATH, as FAR's `enum_tokens_with_quotes` splits it.
fn path_folders(path: &str) -> Vec<VfsPath> {
    let mut folders = Vec::new();
    let mut folder = String::new();
    let mut quoted = false;
    for ch in path.chars().chain([';']) {
        match ch {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                let taken = std::mem::take(&mut folder);
                let taken = taken.trim();
                if !taken.is_empty() {
                    folders.push(VfsPath::local(taken));
                }
            }
            ch => folder.push(ch),
        }
    }
    folders
}

impl App {
    /// Alt+F7, Commands > Find file.
    pub(super) fn open_find_file(&mut self) {
        // A dialog still waiting for a slow drive gives way to this one.
        self.find.dialog = None;
        let mut form = self.find.form(&self.root_area_label(), &self.code_pages);
        let real_names = self.command_panel().has_real_names();
        (self.find).set_plugin_mode(&mut form, self.finds_in_plugin(), real_names);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::FindFile,
        });
    }

    /// The search starts on a plugin's panel (FAR's `PluginMode`): it walks the plugin's folders.
    fn finds_in_plugin(&self) -> bool {
        self.command_panel().path.provider().is_some()
    }

    /// FAR's `PrepareDriveNameStr`: "From the root of C:", of the folder commands run in.
    fn root_area_label(&self) -> String {
        let path = &self.command_panel().path;
        let root = path.as_local().map(|_| path.root().to_string());
        match root
            .as_deref()
            .map(|root| root.trim_end_matches(['\\', '/']))
        {
            Some(root) if !root.is_empty() => format!("From the &root of {root}"),
            _ => "From the &root folder".to_string(),
        }
    }

    /// FAR's Drive, Filter and Advanced work inside the dialog; Find with a wrong mask keeps it open.
    pub(super) fn find_form_closes(&self, form: &Form, button: usize) -> bool {
        match button {
            DRIVE | FILTER | dialog::ADVANCED => false,
            FIND => self.file_mask(form.edit_text(dialog::MASK)).is_some(),
            _ => true,
        }
    }

    pub(super) fn finish_find_form(&mut self, form: &Form, button: Option<usize>) {
        if button == Some(DRIVE) {
            // FAR's `ChangeDisk` of the panel the search starts from.
            self.find.dialog = Some(form.clone());
            self.open_drive_menu(self.command_side());
            return;
        }
        if button == Some(dialog::ADVANCED) {
            return self.open_find_advanced(form);
        }
        if button == Some(FILTER) {
            // The filters menu of Find file's marks; the dialog comes back after it.
            self.find.dialog = Some(form.clone());
            return self.open_area_filters_menu(super::filters::Area::FindFile);
        }
        // FAR writes a mark at once, Cancel or not.
        self.find.read_marks(form);
        if button != Some(FIND) {
            return;
        }
        self.find.read(form);
        let Some(mask) = self.file_mask(&self.find.mask) else {
            // FAR's `filemasks::ErrorMessage`; the dialog stays.
            self.find.dialog = Some(form.clone());
            self.show_dialog(Dialog::Message {
                title: "Warning".to_string(),
                lines: vec!["File-mask string contains errors".to_string()],
                error: true,
            });
            return;
        };
        self.start_find(mask);
    }

    /// FAR's `FindFilesProcess`: the search and its results dialog.
    fn start_find(&mut self, mask: FileMask) {
        let request = self.find_request(mask);
        let mut title = format!("{FIND_TITLE}: {}", self.find.mask);
        if self.find.use_filter {
            title.push_str(" (using filter)");
        }
        let text = self.find.shown_pattern();
        // FAR's `FindFolders && !Hex`: the hex box, even with nothing written in it.
        let panel_folders = request.folders && !self.find.hex_mode;
        let mut results = FindResults::new(Search::start(request), title, text);
        // FAR's Panel button is gone on a plugin's panel without real names (`OPIF_REALNAMES`).
        let real_names = self.command_panel().has_real_names();
        results.find_list = real_names && !self.plugins.host.find_list_plugins().is_empty();
        results.panel_folders = panel_folders;
        results.columns = (
            self.find.column_types.clone(),
            self.find.column_widths.clone(),
        );
        results.mask.clone_from(&self.find.mask);
        self.overlay = Some(Overlay::FindResults(Box::new(results)));
    }

    /// FAR's `DoPrepareFileList`: where the chosen area is.
    fn find_request(&self, mask: FileMask) -> Request {
        let panel = self.command_panel();
        let folder = panel.path.clone();
        let plugin = self.finds_in_plugin();
        let (roots, drives, recurse) = match self.find.area() {
            // FAR's `DoPreparePluginList`: the drives, PATH and the root are the plugin's root.
            Area::AllDrives | Area::LocalDrives | Area::Path | Area::Root if plugin => {
                (vec![folder.root()], None, true)
            }
            Area::AllDrives => (Vec::new(), Some(Drives { network: true }), true),
            Area::LocalDrives => (Vec::new(), Some(Drives { network: false }), true),
            Area::Path => {
                let path = self.environment.get("PATH").unwrap_or_default();
                (path_folders(&path), None, false)
            }
            Area::Root => (vec![folder.root()], None, true),
            Area::FromCurrent => (vec![folder], None, true),
            Area::Current => (vec![folder], None, false),
            Area::Selected => (selected_folders(panel), None, true),
        };
        Request {
            roots,
            drives,
            recurse,
            mask,
            folders: self.find.folders,
            links: self.find.links && !plugin,
            pattern: self.find.pattern(),
            not_containing: self.find.not_containing,
            first: advanced::first_bytes(&self.find.first).unwrap_or(0),
            streams: self.find.streams && !plugin,
            filter: (self.find.use_filter).then(|| {
                (
                    self.area_filter(super::filters::Area::FindFile),
                    SystemTime::now(),
                )
            }),
            archives: (self.find.archives).then(|| Archives(self.archive_opener(None))),
        }
    }

    /// FAR's `OpenFilePlugin` with `OFP_SEARCH`: the plugins that open files (only `plugin`, when named),
    /// each with its masks as the program reads them, in their order.
    pub(super) fn archive_opener(&self, plugin: Option<&str>) -> Arc<dyn FileOpener> {
        let hosted = &self.plugins.host.plugins;
        let takers = (self.plugins_with_masks().into_iter())
            .filter(|(index, _)| {
                plugin.is_none_or(|name| hosted[*index].name.eq_ignore_ascii_case(name))
            })
            .collect();
        self.plugins.host.file_opener(takers)
    }

    /// The dialog back once the drive menu or the warning in front of it is gone, its root line
    /// for the drive the panel is on now: after the panel has read the drive chosen.
    pub(super) fn resume_find_dialog(&mut self) -> bool {
        if self.resume_find_advanced() {
            return true;
        }
        let reading = self.command_panel().is_reading();
        if self.overlay.is_some() || self.waiting.is_some() || reading {
            return false;
        }
        let Some(mut form) = self.find.dialog.take() else {
            return false;
        };
        let root = self.root_area_label();
        if let Some(Item {
            control: Control::DropDown { lines, .. },
            ..
        }) = form.items.get_mut(AREA)
            && let Some(line) = lines.get_mut(3)
        {
            *line = ListLine::Choice(root);
        }
        // FAR's `DN_*` on a drive changed: the areas and boxes of the panel's kind now.
        let real_names = self.command_panel().has_real_names();
        (self.find).set_plugin_mode(&mut form, self.finds_in_plugin(), real_names);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::FindFile,
        });
        true
    }
}

impl App {
    pub(super) fn handle_find_results_key(&mut self, mut results: Box<FindResults>, key: KeyEvent) {
        match results.handle_key(key, self.clipboard.as_mut()) {
            ResultsOutcome::Stay => self.overlay = Some(Overlay::FindResults(results)),
            // The search stops with its dialog.
            ResultsOutcome::Close => {}
            // FAR's `FD_BUTTON_NEW`: the dialog again with what it was given.
            ResultsOutcome::NewSearch => self.open_find_file(),
            ResultsOutcome::AskStop => self.ask_stop_find(results),
            ResultsOutcome::GoTo(folder, name) => self.go_to_found(&folder, name.as_deref()),
            // The results come back once what the key opened is gone (`resume_find_results`).
            ResultsOutcome::View(path, mode) => {
                let files = results.found_files();
                self.find.results = Some(results);
                if !self.open_found_by_association(&path, mode) {
                    self.view_found(&path, &files);
                }
            }
            ResultsOutcome::Edit(path, mode) => {
                self.find.results = Some(results);
                if !self.open_found_by_association(&path, mode) {
                    match path.as_local() {
                        Some(file) => self.edit_file(file, true),
                        None => self.copy_found(&path, true),
                    }
                }
            }
            ResultsOutcome::Background => {
                self.send_find_to_background(results);
            }
            // The search stops with its dialog, as FAR's (`FD_BUTTON_PANEL`).
            ResultsOutcome::Panel(found) => self.find_list_to_plugins(found),
        }
    }

    /// Esc while it searches: FAR's `ConfirmAbort`, the search waiting for the answer.
    fn ask_stop_find(&mut self, mut results: Box<FindResults>) {
        if !self.prefs.confirm_interrupt {
            results.stop();
            self.overlay = Some(Overlay::FindResults(results));
            return;
        }
        results.pause(true);
        self.find.results = Some(results);
        self.ask_interrupt(ConfirmAction::StopFind);
    }

    pub(super) fn stop_find(&mut self) {
        if let Some(results) = &mut self.find.results {
            results.stop();
        }
    }

    /// FAR's `ProcessLocalFileTypes` in `OpenFile`: the found file's association for the key, its
    /// symbols naming the file by its whole path and run in its folder; false when it has none (or
    /// Ctrl+Shift asked for the program's own viewer or editor). With no external viewer or editor set,
    /// Alt+F3 / Alt+F4 without one open the program's own, as FAR's.
    fn open_found_by_association(&mut self, path: &VfsPath, mode: Option<AssocMode>) -> bool {
        let (Some(mode), Some(file)) = (mode, path.as_local()) else {
            return false;
        };
        let Some(name) = file.file_name() else {
            return false;
        };
        let target = AssocTarget {
            name: name.to_string_lossy().into_owned(),
            current: file.display().to_string(),
            dir: file.parent().map(std::path::Path::to_path_buf),
        };
        self.open_target_by_association(target, mode)
    }

    /// F3 on a found file: the viewer, Gray + / Gray - going through the found files.
    fn view_found(&mut self, path: &VfsPath, files: &[VfsPath]) {
        let Some(local) = path.as_local() else {
            // FAR's plugin's file without a real name: viewed as a copy (`GetFiles`).
            return self.copy_found(path, false);
        };
        let list = files.iter().filter_map(VfsPath::as_local);
        let list = list.map(std::path::Path::to_path_buf).collect();
        self.view_file_listed(local, true, list);
    }

    /// A plugin's file found, copied out and viewed or (`edit`) edited, as F3 and F4 on its panel do.
    fn copy_found(&mut self, path: &VfsPath, edit: bool) {
        if let Some(name) = path.file_name() {
            self.open_copy_of(path.clone(), name, edit);
        }
    }

    /// FAR's Go to after its dialog: the file panel (the other one when the info panel is active)
    /// shown, in the folder (not read again when it is there already, so its selection stays), the
    /// cursor on the name, and active.
    pub(in crate::app) fn go_to_found(&mut self, folder: &VfsPath, name: Option<&str>) {
        if self.go_to_in_archive(folder, name) {
            return;
        }
        let side = self.command_side();
        let panel = &mut self.panels[side.index()];
        panel.visible = true;
        let here = panel.path.same_place(folder);
        if here {
            if let Some(name) = name {
                panel.go_to_name(name);
            }
        } else if let Err(error) = panel.change_dir(folder, name) {
            self.show_error("Change directory", &error);
            return;
        }
        self.active = side;
    }

    /// The results back once the viewer, the editor or the question in front of them is gone.
    pub(super) fn resume_find_results(&mut self) -> bool {
        let free = self.overlay.is_none()
            && self.waiting.is_none()
            && matches!(self.screen, Screen::Panels)
            && self.main_view == MainView::Panels;
        if !free {
            return false;
        }
        let Some(results) = self.find.results.take() else {
            return false;
        };
        results.pause(false);
        self.overlay = Some(Overlay::FindResults(results));
        true
    }

    /// What the search found since the last tick, for its dialog in front or behind; whether the
    /// screen changed.
    pub(super) fn pump_find(&mut self) -> bool {
        if let Some(results) = &mut self.find.results {
            results.pump();
        }
        let shown = match &mut self.overlay {
            Some(Overlay::FindResults(results)) => results.pump(),
            _ => false,
        };
        self.pump_background_finds() | shown
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "archive_tests.rs"]
mod archive_tests;
