//! Ctrl+L: FAR's info panel (`far/infolist.cpp`, `far/filepanels.cpp`). The passive panel shows, in place
//! of its files, the computer and the user, the disk of the other panel's folder, memory and power;
//! Ctrl+L again (on either side) brings the files back. Tab goes to it as to a panel; there Ctrl+R reads
//! it again and Ctrl+F12 opens FAR's "Show Info" menu that folds and unfolds its sections. What it shows
//! is read on a thread of its own, again when the other panel goes to another folder, and every few
//! seconds while it is on screen (ours: FAR reads it on each redraw, so memory and free space stay
//! current there too).

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::App;
use super::command::Side;
use super::info_description::{Description, description_file};
use super::popup::{ChooseKey, Popup, PopupAction, PopupAnchor, PopupItem};
use super::{Overlay, sorting};
use crate::vfs::local::{Info, read_info};

/// What a reading found: its folder, the panel's facts and the folder's description file.
type Reading = (PathBuf, Info, Option<PathBuf>);

/// How often what is shown is read again while it is on screen.
const REFRESH: Duration = Duration::from_secs(3);
/// How soon a change of the list beside it reads it again at most.
const BESIDE_AT_MOST: Duration = Duration::from_secs(1);

/// FAR's "Show Info" menu: its items and the sections they fold.
const SECTIONS: [(&str, Section); 5] = [
    ("&Disk", Section::Disk),
    ("&Memory", Section::Memory),
    ("&Description", Section::Description),
    ("Plu&gin panel", Section::Plugin),
    ("&Power status", Section::Power),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Disk,
    Memory,
    Description,
    /// A plugin's lines beside its panel (FAR's `ILSS_PLDESCRIPTION`).
    Plugin,
    Power,
}

/// Which sections are unfolded (FAR's `ShowStatusInfo`); the power section is on by default (owner,
/// 2026-11-09: FAR's `ShowPowerStatus` is off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InfoSections {
    pub disk: bool,
    pub memory: bool,
    pub description: bool,
    pub plugin: bool,
    pub power: bool,
}

impl Default for InfoSections {
    fn default() -> Self {
        Self {
            disk: true,
            memory: true,
            description: true,
            plugin: true,
            power: true,
        }
    }
}

impl InfoSections {
    pub fn shows(&self, section: Section) -> bool {
        match section {
            Section::Disk => self.disk,
            Section::Memory => self.memory,
            Section::Description => self.description,
            Section::Plugin => self.plugin,
            Section::Power => self.power,
        }
    }

    fn set(&mut self, section: Section, shown: bool) {
        match section {
            Section::Disk => self.disk = shown,
            Section::Memory => self.memory = shown,
            Section::Description => self.description = shown,
            Section::Plugin => self.plugin = shown,
            Section::Power => self.power = shown,
        }
    }
}

/// The info panel on one side.
#[derive(Debug)]
pub struct InfoPanel {
    pub side: Side,
    pub sections: InfoSections,
    /// What was read last; `None` until the first reading ends.
    pub info: Option<Info>,
    /// The folder description file shown (none off the disks, as FAR's for a plugin's panel).
    pub description: Option<Description>,
    /// The other panel is on a disk: the description section is there.
    pub describes: bool,
    /// The other panel is a plugin's: its lines for the "Plugin" section (FAR's `InfoLines`).
    pub plugin_lines: Option<std::sync::Arc<[crate::vfs::InfoLine]>>,
    /// The folder `info` is of.
    folder: Option<PathBuf>,
    /// The reading, its thread counted as left on its disk once nobody waits for it (review R25).
    reading: Option<(mpsc::Receiver<Reading>, crate::jobs::disk_threads::Waited)>,
    read_at: Option<Instant>,
    /// The other panel's list as the last reading started (`Panel::entries_version`): a change in its
    /// folder that panel took is read here too (FAR updates the info panel with it).
    beside_version: u64,
}

impl InfoPanel {
    /// A panel showing `info`, or nothing read yet (tests of the drawing).
    #[cfg(test)]
    pub fn showing(side: Side, sections: InfoSections, info: Option<Info>) -> Self {
        Self {
            info,
            ..Self::new(side, sections)
        }
    }

    fn new(side: Side, sections: InfoSections) -> Self {
        Self {
            side,
            sections,
            info: None,
            description: None,
            describes: false,
            plugin_lines: None,
            folder: None,
            reading: None,
            read_at: None,
            beside_version: 0,
        }
    }
}

impl App {
    /// Ctrl+L (FAR's `FilePanels::ProcessKey`): with the active panel shown, an info panel on either
    /// side becomes files again; otherwise the passive panel becomes the info panel.
    pub(super) fn toggle_info_panel(&mut self) {
        if !self.active_panel().visible {
            return;
        }
        if self.info_panel.is_some() {
            return self.toggle_info_panel_off();
        }
        self.open_info_panel(self.active.other());
    }

    /// FAR's `AnotherPanel->Show()`: a hidden panel comes back as the info panel.
    fn open_info_panel(&mut self, side: Side) {
        // FAR's `ChangePanel`: the tree there gives way.
        self.close_tree_panel(side);
        self.close_quick_view(side);
        self.panels[side.index()].visible = true;
        let mut info = InfoPanel::new(side, self.remembered.info_sections);
        info.plugin_lines = self.plugin_info_lines(side);
        self.start_info_reading(&mut info);
        self.info_panel = Some(info);
    }

    /// The menu's "Info panel" of `side`: that side shows it (FAR's `ChangePanelToFilled`). There is
    /// one here: on the other side it moves.
    pub(super) fn show_info_panel_on(&mut self, side: Side) {
        if self.info_side() == Some(side) {
            return;
        }
        self.toggle_info_panel_off();
        self.open_info_panel(side);
    }

    /// The side the info panel is on.
    pub fn info_side(&self) -> Option<Side> {
        self.info_panel.as_ref().map(|info| info.side)
    }

    /// The keys go to the info panel (Tab took them there).
    pub(super) fn info_is_active(&self) -> bool {
        self.info_side() == Some(self.active)
    }

    /// Back to files on `side` (a drive chosen for it, as FAR's `ChangeDisk` on an info panel).
    pub(super) fn close_info_panel(&mut self, side: Side) {
        if self.info_side() == Some(side) {
            self.toggle_info_panel_off();
        }
    }

    fn toggle_info_panel_off(&mut self) {
        if let Some(info) = self.info_panel.take() {
            self.remembered.info_sections = info.sections;
        }
    }

    /// Ctrl+U: the info panel goes with its side.
    pub(super) fn swap_info_panel(&mut self) {
        if let Some(info) = &mut self.info_panel {
            info.side = info.side.other();
        }
    }

    /// Ctrl+R on the info panel: read it again now.
    pub(super) fn reread_info_panel(&mut self) {
        if let Some(mut info) = self.info_panel.take() {
            self.start_info_reading(&mut info);
            self.info_panel = Some(info);
        }
    }

    /// The lines of the plugin whose panel is beside the info panel on `side` (FAR's
    /// `GetOpenPanelInfo` of `AnotherPanel`, as its listing read them); none beside a disk's.
    fn plugin_info_lines(&self, side: Side) -> Option<std::sync::Arc<[crate::vfs::InfoLine]>> {
        let panel = &self.panels[side.other().index()];
        panel
            .path
            .provider()
            .map(|_| std::sync::Arc::clone(&panel.look.info_lines))
    }

    /// The folder the info panel describes: the other panel's (FAR's `AnotherPanel->GetCurDir()`).
    fn described_folder(&self, side: Side) -> PathBuf {
        self.panels[side.other().index()]
            .local_folder()
            .map_or_else(crate::platform::process::system_folder, PathBuf::from)
    }

    fn start_info_reading(&self, info: &mut InfoPanel) {
        let folder = self.described_folder(info.side);
        info.beside_version = self.panels[info.side.other().index()].entries_version;
        info.describes = self.panels[info.side.other().index()]
            .path
            .as_local()
            .is_some();
        if !info.describes {
            info.description = None;
        }
        let names = info
            .describes
            .then(|| self.config.description_files.clone());
        let options = self.config.info_options;
        let (sender, receiver) = mpsc::channel();
        // A disk with threads left hanging on it gets no more (review R25): what is shown stays.
        let Ok((slot, waited)) =
            crate::jobs::disk_threads::take(&crate::vfs::VfsPath::local(&folder))
        else {
            info.read_at = Some(Instant::now());
            return;
        };
        let spawned = std::thread::Builder::new()
            .name("info panel".to_string())
            .spawn(move || {
                let _slot = slot;
                let read = read_info(&folder, options);
                let found = names.and_then(|names| description_file(&folder, &names));
                let _ = sender.send((folder, read, found));
            });
        // A thread that cannot start leaves what is shown; the next look tries again.
        info.reading = spawned.is_ok().then_some((receiver, waited));
        info.read_at = Some(Instant::now());
    }

    /// A reading that ended, and a new one when the other panel's folder changed or the last is old.
    /// Returns whether the panel is to be drawn again.
    pub(super) fn pump_info_panel(&mut self) -> bool {
        let Some(mut info) = self.info_panel.take() else {
            return false;
        };
        let mut changed = false;
        if let Some((reading, _)) = &info.reading {
            match reading.try_recv() {
                Ok((folder, read, found)) => {
                    info.folder = Some(folder);
                    info.info = Some(read);
                    info.reading = None;
                    // The same file keeps its view (where it is scrolled to, what it read).
                    let same = info.description.as_ref().map(|shown| &shown.path) == found.as_ref();
                    if !same {
                        info.description = found.map(Description::open);
                    }
                    changed = true;
                }
                Err(mpsc::TryRecvError::Disconnected) => info.reading = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(description) = &mut info.description {
            changed |= description.text.pump();
        }
        let lines = self.plugin_info_lines(info.side);
        if info.plugin_lines != lines {
            info.plugin_lines = lines;
            changed = true;
        }
        let shown = self.panels[info.side.index()].visible;
        // After a reading that gave nothing, only the refresh tries again.
        let moved = info
            .folder
            .as_ref()
            .is_some_and(|folder| *folder != self.described_folder(info.side));
        let old = info.read_at.is_none_or(|at| at.elapsed() >= REFRESH);
        // A count of folders' sizes changes the list many times a second: once a second at most.
        let beside = self.panels[info.side.other().index()].entries_version != info.beside_version
            && info.read_at.is_none_or(|at| at.elapsed() >= BESIDE_AT_MOST);
        if shown && info.reading.is_none() && (moved || old || beside) {
            self.start_info_reading(&mut info);
        }
        self.info_panel = Some(info);
        changed
    }

    /// Ctrl+F12 on the info panel: FAR's "Show Info" menu (`InfoList::SelectShowMode`).
    pub(super) fn open_info_sections_menu(&mut self) {
        let Some(info) = &self.info_panel else {
            return;
        };
        let items = SECTIONS
            .iter()
            .map(|&(label, section)| {
                let check = if info.sections.shows(section) {
                    '+'
                } else {
                    '-'
                };
                PopupItem::new(label, "", Some(check))
            })
            .collect();
        self.overlay = Some(Overlay::Popup(Popup {
            title: "Show Info".to_string(),
            bottom_title: String::new(),
            items,
            selected: 0,
            anchor: PopupAnchor::Panel(info.side),
            action: PopupAction::InfoSections,
        }));
    }

    /// FAR's rule: `+` unfolds, `-` folds, Enter and `*` switch.
    pub(super) fn finish_info_sections_menu(&mut self, index: usize, key: ChooseKey) {
        let (Some(info), Some(&(_, section))) = (&mut self.info_panel, SECTIONS.get(index)) else {
            return;
        };
        let shown = sorting::wanted(key, info.sections.shows(section));
        info.sections.set(section, shown);
        self.remembered.info_sections = info.sections;
    }
}

#[cfg(test)]
#[path = "info_panel_tests.rs"]
mod tests;
