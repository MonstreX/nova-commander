//! The info panel's folder description (FAR's `InfoList::ShowDirDescription`, 2c step 4): the first file
//! in the other panel's folder that the names of "Folder description files" find (FAR's
//! `DirInfo,File_Id.diz,Descript.ion,ReadMe.*,Read.Me`, wildcards as the system matches them), looked for
//! with the panel's other facts on its thread, and shown under the sections by the view core
//! (`quick_view_text`, ADR-0015) to the panel's bottom. On the active info panel the viewer's keys move
//! it, F3 views it, F4 edits it (with none, a new file of the first name without wildcards: FAR's
//! `FFILEEDIT_CANNEWFILE`); the mouse's left button on it is F3, the right one F4.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::App;
use super::dialog::{Dialog, InputAction};
use super::line_input::LineInput;
use super::quick_view_text::{FileText, viewer_move};
use crate::editor::view::Move;
use crate::platform;

/// The description file shown.
#[derive(Debug)]
pub struct Description {
    pub path: PathBuf,
    pub text: FileText,
    /// Where its rows were drawn: left, top, width, height (the drawing writes it, for the mouse).
    pub area: Cell<(u16, u16, u16, u16)>,
}

impl Description {
    pub fn open(path: PathBuf) -> Self {
        Self {
            text: FileText::open(crate::vfs::VfsPath::local(&path)),
            path,
            area: Cell::new((0, 0, 0, 0)),
        }
    }
}

impl super::App {
    /// The menus' symbols of the info panel on `side` (FAR's `InfoList`): its folder is the other panel's
    /// (`GetCurDir`), its name the description file shown (`GetCurName`), and nothing is selected
    /// (`GetSelName` of a panel without files).
    pub(super) fn info_panel_facts(
        &self,
        side: super::Side,
    ) -> Option<super::metasymbols::PanelFacts> {
        let info = self.info_panel.as_ref().filter(|info| info.side == side)?;
        let dir = (self.panels[side.other().index()].local_folder())
            .map_or_else(String::new, |dir| dir.display().to_string());
        let current = (info.description.as_ref())
            .map_or_else(String::new, |shown| shown.path.display().to_string());
        Some(super::metasymbols::PanelFacts {
            dir,
            current,
            selected: Vec::new(),
        })
    }
}

/// The names of `list`: `,` or `;` between them, quotes keeping either in a name (FAR's
/// `enum_tokens_with_quotes`).
pub fn description_names(list: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut name = String::new();
    let mut quoted = false;
    for ch in list.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' | ';' if !quoted => names.push(std::mem::take(&mut name)),
            _ => name.push(ch),
        }
    }
    names.push(name);
    names
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// The first file in `folder` the names of `list` find, in their order (blocking: the info panel's
/// thread asks).
pub fn description_file(folder: &Path, list: &str) -> Option<PathBuf> {
    description_names(list).iter().find_map(|name| {
        // A name with a folder in it finds its file in that folder.
        let pattern = folder.join(name);
        let parent = pattern.parent().unwrap_or(folder).to_path_buf();
        platform::files::first_file_like(folder, name).map(|found| parent.join(found))
    })
}

impl App {
    /// The description file found for the other panel's folder, folded or not (F3 and F4 take it);
    /// none while the other panel is off the disks.
    fn found_description(&self) -> Option<&Description> {
        let info = self.info_panel.as_ref().filter(|info| info.describes)?;
        info.description.as_ref()
    }

    /// The description file, when it is shown (folded: none): the keys, the wheel and the mouse.
    fn description(&self) -> Option<&Description> {
        let info = self.info_panel.as_ref()?;
        info.sections.description.then_some(())?;
        self.found_description()
    }

    /// The info panel has the keys (its own key bar).
    pub fn info_has_keys(&self) -> bool {
        self.info_is_active() && self.active_panel().visible
    }

    /// F8's label with a description file shown: the code page it would switch to; none without.
    pub fn description_next_page(&self) -> Option<&'static str> {
        self.description()
            .map(|description| description.text.next_code_page())
    }

    /// FAR's `InfoList::ProcessKey` on the active info panel: F3 / Num5 view the description file, F4
    /// edits it, the viewer's keys move it; false for the other keys.
    pub(super) fn description_key(&mut self, key: KeyEvent) -> bool {
        if !self.info_has_keys() {
            return false;
        }
        let plain = key.modifiers.is_empty();
        match key.code {
            KeyCode::F(3) if plain => self.view_description(),
            KeyCode::KeypadBegin if plain || key.modifiers == KeyModifiers::SHIFT => {
                self.view_description();
            }
            KeyCode::F(4) if plain => self.edit_description(),
            _ => {
                let Some(step) = viewer_move(key) else {
                    return false;
                };
                return self.press_description(step, 1);
            }
        }
        true
    }

    /// `step` `times` over on the description shown; false with none.
    fn press_description(&mut self, step: Move, times: usize) -> bool {
        let shown = self
            .info_panel
            .as_mut()
            .filter(|info| info.sections.description && info.describes);
        let Some(description) = shown.and_then(|info| info.description.as_mut()) else {
            return false;
        };
        for _ in 0..times {
            description.text.press(step);
        }
        true
    }

    /// The wheel on the active info panel: its description's rows.
    pub(super) fn info_wheel(&mut self, rows: isize) -> bool {
        if !self.info_has_keys() {
            return false;
        }
        let step = if rows < 0 { Move::Up } else { Move::Down };
        self.press_description(step, rows.unsigned_abs())
    }

    /// FAR's `InfoList::ProcessMouse`: the left button on the description views it, the right one
    /// edits it; true when it was there.
    pub(super) fn description_click(&mut self, column: u16, row: u16, left: bool) -> bool {
        let Some(description) = self.description() else {
            return false;
        };
        let (x, y, width, height) = description.area.get();
        let inside = (x..x + width).contains(&column) && (y..y + height).contains(&row);
        if !inside {
            return false;
        }
        if left {
            self.view_description();
        } else {
            self.edit_description();
        }
        true
    }

    /// F3: the description file in the viewer.
    fn view_description(&mut self) {
        if let Some(path) = self
            .found_description()
            .map(|description| description.path.clone())
        {
            self.view_file(&path, true);
        }
    }

    /// F4: the description file in the editor; with none found (or not yet), the first name without
    /// wildcards in the other panel's folder: the file there if there is one, else a new one made when
    /// saved (FAR's `FFILEEDIT_CANNEWFILE`; never an empty document over a file).
    fn edit_description(&mut self) {
        if let Some(path) = self
            .found_description()
            .map(|description| description.path.clone())
        {
            self.edit_file(&path, true);
            return;
        }
        let Some(info) = &self.info_panel else {
            return;
        };
        let Some(folder) = self.panels[info.side.other().index()].path.as_local() else {
            return;
        };
        let names = description_names(&self.config.description_files);
        let Some(name) = names.iter().find(|name| !name.contains(['*', '?'])) else {
            return;
        };
        let path = folder.join(name);
        // Only a name with nothing at it is made new; anything else (there, or not readable) opens
        // as it is and says what is wrong.
        match std::fs::symlink_metadata(platform::files::exact_path(&path)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                self.edit_file(&path, true);
                return;
            }
        }
        self.open_new_file(&path);
    }

    /// Options > "Folder description files" (FAR's `Options::SetFolderInfoFiles`).
    pub(super) fn ask_description_files(&mut self) {
        self.show_dialog(Dialog::Input {
            title: "Folder description files".to_string(),
            prompt: "Enter file names delimited with commas (wildcards are allowed)".to_string(),
            input: LineInput::with_text(&self.config.description_files),
            action: InputAction::DescriptionFiles,
        });
    }

    /// The names given: the info panel looks again.
    pub(super) fn set_description_files(&mut self, list: &str) {
        self.config.description_files = list.to_string();
        self.reread_info_panel();
    }
}

#[cfg(test)]
#[path = "info_description_tests.rs"]
mod tests;
