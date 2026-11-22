//! Alt+F10: FAR's "Find folder" (`foldtree.cpp`, a modal `TreeList` of `MODALTREE_ACTIVE`): the tree of
//! the active panel's disk in a window, read as the tree panel's is (ADR-0014: from the disk's file at
//! once, the rest in the background, the cursor on the active panel's folder once it is in). Typed
//! text (Alt+letter too) goes to the "Search:" line and takes the cursor to the first folder whose own
//! name starts with it, from the cursor on, the line keeping the last text that found one; Ctrl+Enter /
//! Ctrl+Shift+Enter the next / previous; the cursor keys clear it and move. Enter on a folder that is
//! there closes the window and the active panel goes there (the tree's other panel, a tree being
//! active); one that is gone leaves the tree. F2 / Ctrl+R read the disk again, F5 fills the screen or
//! not, Esc / F10 close it. The mouse: a click moves the cursor, a double click is Enter, a click
//! outside the window is Esc (left) or Enter (right), the wheel scrolls. The copy, move and link
//! dialogs open the same window for their target (`target_tree`).

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::console_input::{Mouse, MouseAction};
use super::super::dialog::Dialog;
use super::super::line_edit;
use super::super::mouse::is_button_event;
use super::super::{App, Cells, LineInput, Overlay, Side};
use super::{TreePanel, root_of};
use crate::jobs::tree_worker::TreeWorker;
use crate::platform;
use crate::platform::files::same_place;
use crate::vfs::VfsPath;

/// What the folder chosen is for.
#[derive(Debug)]
pub enum Purpose {
    /// Alt+F10: the active panel goes there.
    GoTo,
    /// F10 in the copy, move or link dialog (FAR's `DM_CALLTREE`): the dialog waits under the window
    /// and the folder goes to its target line.
    Target(Box<Dialog>),
}

/// The window and its state.
#[derive(Debug)]
pub struct FolderTree {
    pub tree: TreePanel,
    pub purpose: Purpose,
    pub search: LineInput,
    /// The last text that found a folder (FAR's `m_LastName`).
    last: String,
    /// F5: the whole screen.
    pub full_screen: bool,
    /// Where it was drawn last, for the mouse.
    pub area: Option<Cells>,
}

impl FolderTree {
    /// A window on `tree`, read (tests of the drawing).
    #[cfg(test)]
    pub fn showing(tree: TreePanel, search: &str) -> Self {
        Self {
            tree,
            purpose: Purpose::GoTo,
            search: LineInput::with_text(search),
            last: search.to_string(),
            full_screen: false,
            area: None,
        }
    }

    /// Alt+F10's window, not a dialog's (FAR's `m_IsStandalone`: where it stands).
    pub fn standalone(&self) -> bool {
        matches!(self.purpose, Purpose::GoTo)
    }

    /// The search line's row in a window drawn at `area` (FAR's `m_Where.bottom - 2`).
    pub fn search_row(area: &Cells) -> u16 {
        (area.y + area.height).saturating_sub(3)
    }

    /// Typed text: kept when it finds a folder from the cursor on (FAR's `FindPartName(.., FALSE, 1)`),
    /// else the last one that did comes back.
    fn typed(&mut self) {
        let text = self.search.text().to_string();
        if text.is_empty() || self.tree.find(&text, false, true) {
            self.last = text;
        } else {
            self.search = LineInput::with_text(&self.last);
        }
    }
}

impl App {
    /// Alt+F10 (FAR's `KEY_ALTF10` in `cmdline.cpp`): the window on the active panel's folder's disk;
    /// none with the tree turned off or a folder off the disks.
    pub(in crate::app) fn open_folder_tree(&mut self) {
        if self.prefs.tree.turn_off_completely {
            return;
        }
        if let Some(folder) = self.folder_of(self.active) {
            self.open_folder_tree_at(folder, Purpose::GoTo);
        }
    }

    /// The folder of `side`'s panel (FAR's `GetCurDir`): the tree's cursor on a tree shown, else the
    /// files panel's; none off the disks.
    pub(in crate::app) fn folder_of(&self, side: Side) -> Option<PathBuf> {
        let tree = (self.tree_panel.as_ref())
            .filter(|tree| tree.side == side && self.panels[side.index()].visible);
        if let Some(item) = tree.and_then(TreePanel::current) {
            return Some(item.path.clone());
        }
        self.panels[side.index()]
            .path
            .as_local()
            .map(Path::to_path_buf)
    }

    /// The window on the tree of `folder`'s disk, the cursor going there once it is in.
    pub(in crate::app) fn open_folder_tree_at(&mut self, folder: PathBuf, purpose: Purpose) {
        let mut tree = TreePanel::new(Side::Left);
        let root = root_of(&folder);
        tree.start(root, self.tree_keeping());
        tree.aim(folder);
        self.overlay = Some(Overlay::FolderTree(Box::new(FolderTree {
            tree,
            purpose,
            search: LineInput::default(),
            last: String::new(),
            full_screen: false,
            area: None,
        })));
    }

    /// The window closed, with the folder chosen or none.
    fn close_folder_tree(&mut self, found: Box<FolderTree>, chosen: Option<&Path>) {
        match found.purpose {
            Purpose::GoTo => {
                if let Some(folder) = chosen {
                    self.go_to_found_folder(folder);
                }
            }
            Purpose::Target(mut dialog) => {
                if let Some(folder) = chosen {
                    super::target_tree::put_target(&mut dialog, folder);
                }
                self.overlay = Some(Overlay::Dialog(*dialog));
            }
        }
    }

    /// What the window's worker sent; true when it is to be drawn again.
    pub(in crate::app) fn pump_folder_tree(&mut self) -> bool {
        let Some(Overlay::FolderTree(found)) = &mut self.overlay else {
            return false;
        };
        let tree = &mut found.tree;
        let Some(snapshot) = tree.worker.as_ref().and_then(TreeWorker::latest) else {
            return false;
        };
        tree.take(snapshot);
        true
    }

    /// FAR's `FolderTree::ProcessKey`.
    pub(in crate::app) fn handle_folder_tree_key(
        &mut self,
        mut found: Box<FolderTree>,
        key: KeyEvent,
    ) {
        // FAR matches the keys exactly: with other modifiers they go to the search line.
        let plain = key.modifiers.is_empty();
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        let ctrl_shift = key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        match key.code {
            KeyCode::Esc | KeyCode::F(10) if plain => return self.close_folder_tree(found, None),
            KeyCode::Enter if ctrl || ctrl_shift => {
                let text = found.search.text().to_string();
                found.tree.find(&text, true, ctrl);
            }
            KeyCode::Enter if plain => return self.folder_tree_enter(found),
            KeyCode::F(5) if plain => found.full_screen = !found.full_screen,
            KeyCode::F(2) if plain => found.tree.reread(),
            KeyCode::Char('r') if ctrl => found.tree.reread(),
            _ => {
                // The cursor keys clear the search line (FAR's `ClearString`, `m_LastName` kept);
                // Gray + / - keep it.
                let cursor_key = !matches!(key.code, KeyCode::Char(_));
                match found.tree.key_move(key) {
                    Some(_) if cursor_key => found.search = LineInput::default(),
                    Some(_) => {}
                    None => self.folder_tree_typed(&mut found, key),
                }
            }
        }
        self.overlay = Some(Overlay::FolderTree(found));
    }

    /// Enter: the folder to go to when it is there; one that is gone leaves the tree (FAR's
    /// `TreeList::ProcessEnter` of a modal tree). Its disk is asked off the screen's thread, the tree
    /// under the wait (ADR-0019, review R22).
    fn folder_tree_enter(&mut self, found: Box<FolderTree>) {
        let folder = found.tree.current().map(|item| item.path.clone());
        self.overlay = Some(Overlay::FolderTree(found));
        let Some(folder) = folder else {
            return;
        };
        self.wait_for(Vec::new(), move || {
            let there = platform::files::exact_path(&folder).is_dir();
            Box::new(move |app: &mut App| {
                if !matches!(app.overlay, Some(Overlay::FolderTree(_))) {
                    return;
                }
                let Some(Overlay::FolderTree(mut found)) = app.overlay.take() else {
                    return;
                };
                if there {
                    return app.close_folder_tree(found, Some(&folder));
                }
                found.tree.forget(&folder);
                app.overlay = Some(Overlay::FolderTree(found));
            })
        });
        self.label_waiting("Reading the folder…");
    }

    /// A key for the search line (Alt+letter as the letter, as FAR's).
    fn folder_tree_typed(&mut self, found: &mut FolderTree, mut key: KeyEvent) {
        if let KeyCode::Char(ch) = key.code
            && key.modifiers.contains(KeyModifiers::ALT)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            key = KeyEvent::new(
                KeyCode::Char(ch.to_lowercase().next().unwrap_or(ch)),
                KeyModifiers::NONE,
            );
        }
        let before = found.search.text().to_string();
        let edited = line_edit::handle_key(&mut found.search, key, self.clipboard.as_mut());
        if edited.is_ok_and(|edited| edited) && found.search.text() != before {
            found.typed();
        }
    }

    /// The folder chosen: the active panel goes there; with the tree or the info panel active, the
    /// files panel beside it (FAR's `SetCurDir` of the tree, then its Enter).
    fn go_to_found_folder(&mut self, folder: &Path) {
        let side = if self.shows_files(self.active) {
            self.active
        } else {
            self.active.other()
        };
        let panel = &mut self.panels[side.index()];
        if panel
            .path
            .as_local()
            .is_some_and(|path| same_place(path, folder))
        {
            return;
        }
        if let Err(error) = panel.change_dir(&VfsPath::local(folder), None) {
            self.show_error("Change directory", &error);
        }
    }

    /// FAR's `FolderTree::ProcessMouse`; true when the screen is to be drawn again. Its key bar
    /// takes the mouse first.
    pub(in crate::app) fn folder_tree_mouse(&mut self, mouse: &Mouse) -> bool {
        // F5: the window covers the key bar (FAR hides it).
        let zoomed = matches!(&self.overlay, Some(Overlay::FolderTree(found)) if found.full_screen);
        if !zoomed {
            if let Some(changed) = self.key_bar_release(mouse) {
                return changed;
            }
            if self.key_bar_press(mouse) {
                return true;
            }
        }
        let any = mouse.buttons.left || mouse.buttons.right || mouse.buttons.middle;
        if mouse.action == MouseAction::Moved && !any {
            return false;
        }
        let Some(Overlay::FolderTree(mut found)) = self.overlay.take() else {
            return false;
        };
        let Some(area) = found.area else {
            self.overlay = Some(Overlay::FolderTree(found));
            return false;
        };
        let inside = (area.x..area.x + area.width).contains(&mouse.column)
            && (area.y..area.y + area.height).contains(&mouse.row);
        let press = is_button_event(mouse.action);
        // FAR's `TreeList::ProcessMouse`: a button on a line takes the cursor there.
        if inside
            && (mouse.buttons.left || mouse.buttons.right)
            && mouse.row > area.y
            && mouse.row < FolderTree::search_row(&area).saturating_sub(1)
        {
            let to = found.tree.top + usize::from(mouse.row - area.y - 1);
            found.tree.move_cursor(|_| to, None);
        }
        if let MouseAction::Wheel(notches) = mouse.action {
            let lines = if mouse.modifiers == KeyModifiers::ALT {
                1
            } else {
                platform::input::wheel_scroll_lines()
            };
            let delta = i64::from(notches) * i64::from(lines);
            // FAR's wheel is one of the cursor keys: the search line cleared.
            found.search = LineInput::default();
            found.tree.scroll(isize::try_from(-delta).unwrap_or(0));
        } else if mouse.buttons.left && mouse.action == MouseAction::DoubleClick {
            self.overlay = Some(Overlay::FolderTree(found));
            self.dispatch_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            return true;
        } else if press && !inside {
            // FAR's `Dialogs.MouseButton`: the left button outside is Esc, the right one Enter.
            self.overlay = Some(Overlay::FolderTree(found));
            let code = if mouse.pressed.left {
                KeyCode::Esc
            } else {
                KeyCode::Enter
            };
            if mouse.pressed.left || mouse.pressed.right {
                self.dispatch_key(KeyEvent::new(code, KeyModifiers::NONE));
            }
            return true;
        }
        self.overlay = Some(Overlay::FolderTree(found));
        true
    }
}

#[cfg(test)]
#[path = "folder_tree_tests.rs"]
mod tests;
