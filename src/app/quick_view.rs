//! Ctrl+Q: FAR's quick view panel (`qview.cpp`, `filepanels.cpp`). The passive panel shows, in place of
//! its files, the item under the other panel's cursor (its folder for `..`; the tree's folder beside a
//! tree): a file's name and the system's name of its type, its contents with the view core (2c step 3b,
//! ADR-0015); a folder's folders, files, bytes and the space they take, counted on a thread of its own
//! (`jobs::folder_info`, ADR-0015: FAR counts behind a modal window), and where a link leads. Ctrl+Q
//! again (on either side) brings the files back; Tab goes to it, where F3 and Num5 view the other
//! panel's item, Gray + / - move its cursor, the viewer's keys move a file's contents
//! (`quick_view_text`), and the file keys do nothing.

use std::collections::HashMap;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

use super::App;
use super::associations::AssocMode;
use super::command::Side;
use super::metasymbols::PanelFacts;
use super::quick_view_text::{FileText, viewer_move};
use crate::editor::view::Move;
use crate::jobs::folder_info::{Counted, InfoCount};
use crate::platform;
use crate::vfs::VfsPath;

/// The quick view panel on one side.
#[derive(Debug)]
pub struct QuickView {
    pub side: Side,
    /// What it shows; none until the other panel has an item.
    pub shown: Option<Shown>,
    /// The types' names asked of the system, by extension.
    type_names: HashMap<String, Option<String>>,
}

/// The item shown (FAR's `strCurFileName`).
#[derive(Debug)]
pub struct Shown {
    pub path: VfsPath,
    pub what: Showing,
}

#[derive(Debug)]
pub enum Showing {
    /// The count and what it stood at when last drawn.
    Folder { count: InfoCount, seen: Counted },
    /// The system's name of its type (FAR's `strCurFileType`, on the separator) and its contents.
    File {
        type_name: Option<String>,
        text: Box<FileText>,
    },
}

impl QuickView {
    fn new(side: Side) -> Self {
        Self {
            side,
            shown: None,
            type_names: HashMap::new(),
        }
    }

    /// A panel showing `shown` (tests of the drawing).
    #[cfg(test)]
    pub fn showing(side: Side, shown: Option<Shown>) -> Self {
        Self {
            shown,
            ..Self::new(side)
        }
    }

    /// The system's name of the type of a file named `name`, asked once per extension.
    fn type_name(&mut self, name: &str) -> Option<String> {
        let extension = name.rfind('.').map_or(".", |dot| &name[dot..]);
        let key = extension.to_lowercase();
        let named = self
            .type_names
            .entry(key)
            .or_insert_with(|| platform::files::file_type_name(name));
        named.clone()
    }
}

impl App {
    /// Ctrl+Q (FAR's `FilePanels::ProcessKey`): with the active panel shown, a quick view on either
    /// side becomes files again; otherwise the passive panel becomes the quick view.
    pub(super) fn toggle_quick_view(&mut self) {
        if !self.active_panel().visible {
            return;
        }
        if self.quick_view.is_some() {
            self.quick_view = None;
            return;
        }
        self.open_quick_view(self.active.other());
    }

    /// The menu's "Quick view" of `side`: that side shows it; one on the other side moves.
    pub(super) fn show_quick_view_on(&mut self, side: Side) {
        if self.quick_view_side() != Some(side) {
            self.open_quick_view(side);
        }
    }

    /// FAR's `ChangePanel`: the info panel or the tree there gives way, a hidden panel comes back.
    fn open_quick_view(&mut self, side: Side) {
        self.close_info_panel(side);
        self.close_tree_panel(side);
        self.panels[side.index()].visible = true;
        self.quick_view = Some(QuickView::new(side));
        self.pump_quick_view();
    }

    /// The side the quick view is on.
    pub fn quick_view_side(&self) -> Option<Side> {
        self.quick_view.as_ref().map(|view| view.side)
    }

    /// The keys go to the quick view.
    pub(super) fn quick_view_is_active(&self) -> bool {
        self.quick_view_side() == Some(self.active)
    }

    /// Back to files on `side` (another panel chosen for it, a drive chosen).
    pub(super) fn close_quick_view(&mut self, side: Side) {
        if self.quick_view_side() == Some(side) {
            self.quick_view = None;
        }
    }

    /// Ctrl+U: the quick view goes with its side.
    pub(super) fn swap_quick_view(&mut self) {
        if let Some(view) = &mut self.quick_view {
            view.side = view.side.other();
        }
    }

    /// Ctrl+R on the quick view: a folder counted again (ours: FAR's keeps what it counted).
    pub(super) fn reread_quick_view(&mut self) {
        if let Some(view) = &mut self.quick_view {
            view.shown = None;
        }
        self.pump_quick_view();
    }

    /// What the quick view is to show: the item under the cursor of the panel beside it, its folder
    /// for `..` (FAR's `UpdateViewPanel`), the folder under a tree's cursor; with whether it is a
    /// folder. None beside an info panel or a hidden one.
    pub(super) fn quick_view_target(&self, side: Side) -> Option<(VfsPath, bool)> {
        let other = side.other();
        let panel = &self.panels[other.index()];
        if !panel.visible {
            return None;
        }
        if self.tree_side() == Some(other) {
            let tree = self.tree_panel.as_ref()?;
            return tree
                .current()
                .map(|item| (VfsPath::local(&item.path), true));
        }
        if !self.shows_files(other) {
            return None;
        }
        let entry = panel.current()?;
        if entry.is_parent {
            return Some((panel.path.clone(), true));
        }
        Some((panel.item_path(entry), entry.is_dir))
    }

    /// The quick view after the other panel's cursor and its count; true when it is to be drawn again.
    pub(super) fn pump_quick_view(&mut self) -> bool {
        let Some(side) = self.quick_view_side() else {
            return false;
        };
        let shown = self.panels[side.index()].visible;
        let target = self.quick_view_target(side).filter(|_| shown);
        let Some(view) = &mut self.quick_view else {
            return false;
        };
        let mut changed = false;
        if let Some((path, folder)) = target
            && view.shown.as_ref().is_none_or(|shown| shown.path != path)
        {
            let what = if folder {
                Showing::Folder {
                    count: InfoCount::start(path.clone()),
                    seen: Counted::default(),
                }
            } else {
                let name = path.file_name().unwrap_or_default();
                Showing::File {
                    type_name: view.type_name(&name),
                    text: Box::new(FileText::open(path.clone())),
                }
            };
            view.shown = Some(Shown { path, what });
            changed = true;
        }
        match view.shown.as_mut().map(|shown| &mut shown.what) {
            Some(Showing::Folder { count, seen }) => {
                let now = count.now();
                changed |= now != *seen;
                *seen = now;
            }
            Some(Showing::File { text, .. }) => changed |= text.pump(),
            None => {}
        }
        changed
    }

    /// FAR's `QuickView::ProcessKey` on an active quick view: F3 and Num5 view the other panel's item,
    /// Gray + / - move its cursor; false for the other keys.
    pub(super) fn quick_view_key(&mut self, key: KeyEvent) -> bool {
        if !self.quick_view_is_active() || !self.active_panel().visible {
            return false;
        }
        let other = self.active.other();
        let files_beside = self.panels[other.index()].visible && self.shows_files(other);
        let plain = key.modifiers.is_empty();
        let gray = key.state.contains(KeyEventState::KEYPAD);
        match key.code {
            KeyCode::F(3) if plain => {}
            KeyCode::KeypadBegin if plain || key.modifiers == KeyModifiers::SHIFT => {}
            KeyCode::Char('+' | '-') if plain && gray => {
                if files_beside {
                    let step = if key.code == KeyCode::Char('+') {
                        1
                    } else {
                        -1
                    };
                    self.panels[other.index()].move_cursor(step);
                }
                return true;
            }
            _ => return self.move_text(key),
        }
        if files_beside {
            self.view_beside(other);
        }
        true
    }

    /// FAR's viewer keys on a file's contents.
    fn move_text(&mut self, key: KeyEvent) -> bool {
        viewer_move(key).is_some_and(|step| self.press_text(step, 1))
    }

    /// `step` `times` over on the file shown; false when no file is.
    fn press_text(&mut self, step: Move, times: usize) -> bool {
        let shown = self
            .quick_view
            .as_mut()
            .and_then(|view| view.shown.as_mut());
        let Some(Shown {
            what: Showing::File { text, .. },
            ..
        }) = shown
        else {
            return false;
        };
        for _ in 0..times {
            text.press(step);
        }
        true
    }

    /// The wheel on an active quick view: its file's rows up or down.
    pub(super) fn quick_view_wheel(&mut self, rows: isize) -> bool {
        if !self.quick_view_has_keys() {
            return false;
        }
        let step = if rows < 0 { Move::Up } else { Move::Down };
        self.press_text(step, rows.unsigned_abs())
    }

    /// FAR's `AnotherPanel->ProcessKey(KEY_F3)`, the quick view keeping the keys: a folder counted on
    /// the panel beside it, a file opened by its associations (the quick view's facts are that file's,
    /// FAR's `GetCurName`) or viewed.
    fn view_beside(&mut self, other: Side) {
        self.pump_quick_view();
        let panel = &self.panels[other.index()];
        if panel.current().is_some_and(|entry| entry.is_dir) {
            self.panels[other.index()].count_folder_sizes();
            return;
        }
        let Some(path) = panel.current_path() else {
            return;
        };
        if !self.open_by_association(AssocMode::View) {
            self.view_file(&path, true);
        }
    }

    /// The quick view has the keys (its own key bar).
    pub fn quick_view_has_keys(&self) -> bool {
        self.quick_view_is_active() && self.active_panel().visible
    }

    /// F8's label on a file shown: the code page it would switch to; none without a file.
    pub fn quick_view_next_page(&self) -> Option<&'static str> {
        match &self.quick_view.as_ref()?.shown.as_ref()?.what {
            Showing::File { text, .. } => Some(text.next_code_page()),
            Showing::Folder { .. } => None,
        }
    }

    /// The name of the file shown when the quick view has the keys.
    pub(super) fn quick_view_file(&self) -> Option<String> {
        let view = self
            .quick_view
            .as_ref()
            .filter(|_| self.quick_view_is_active())?;
        match view.shown.as_ref()? {
            Shown {
                path,
                what: Showing::File { .. },
            } => path.file_name(),
            _ => None,
        }
    }

    /// The facts of the menus' symbols on the quick view's side (FAR's `QuickView::GetCurDir`, the
    /// folder of the panel beside it, and `GetCurName`, the item shown); none on another side.
    pub(super) fn quick_view_facts(&self, side: Side) -> Option<PanelFacts> {
        let view = self.quick_view.as_ref().filter(|view| view.side == side)?;
        let other = side.other();
        // FAR's `AnotherPanel->GetCurDir()`: a tree's is the folder under its cursor.
        let dir = match self.tree_panel.as_ref().filter(|tree| tree.side == other) {
            Some(tree) => {
                (tree.current()).map_or_else(String::new, |item| item.path.display().to_string())
            }
            None => self.panels[other.index()].path.to_string(),
        };
        // FAR's `strCurFileName`: the item's whole path.
        let current = (view.shown.as_ref())
            .map(|shown| shown.path.to_string())
            .unwrap_or_default();
        let selected = if current.is_empty() {
            Vec::new()
        } else {
            vec![current.clone()]
        };
        Some(PanelFacts {
            dir,
            current,
            selected,
        })
    }
}

#[cfg(test)]
#[path = "quick_view_tests.rs"]
mod tests;
