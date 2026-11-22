//! Ctrl+T: FAR's tree panel (`far/treelist.cpp`, `far/filepanels.cpp`). A panel shows, in place of its
//! files, every folder of the disk of the other panel's folder (FAR's `GetRoot`). Its worker reads the
//! disk in the background, the most wanted folders first, and keeps it in the disk's file (ADR-0014): the
//! tree is in use at once, says "Reading" while the worker goes on, and fills as folders come. The line
//! of the other panel's folder is marked and the cursor follows that panel (FAR's `SyncDir`; a folder
//! not in the tree yet is read first and the cursor goes there when it is in). Ctrl+T on the tree, or on
//! its other panel, brings the files back; elsewhere the passive panel becomes the tree (an info panel
//! there gives way). On it the cursor keys, Gray + / Gray - (the next or previous folder of the same
//! depth), Enter (the other panel goes there; a folder that is gone leaves the tree) and Ctrl+R (every
//! folder read again); "Auto change folder" makes each move an Enter.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

use super::App;
use super::command::Side;
use crate::jobs::tree::lay_out;
pub use crate::jobs::tree::{TreeItem, make_items};
use crate::jobs::tree_worker::{Keeping, Order, Snapshot, TreeWorker};
use crate::platform::files::same_place;
use crate::vfs::VfsPath;

mod folder_tree;
mod mouse;
mod names;
mod operations;
mod search;
pub(super) mod target_tree;

pub use folder_tree::{FolderTree, Purpose};
pub(in crate::app) use operations::{can_trash, delete_items_of};
pub use target_tree::TreeMode;

/// The tree panel on one side.
#[derive(Debug)]
pub struct TreePanel {
    pub side: Side,
    pub root: PathBuf,
    /// The root first, then its folders in their order.
    pub items: Vec<TreeItem>,
    pub cursor: usize,
    pub top: usize,
    /// Lines of the list, as last drawn.
    pub rows: usize,
    /// The other panel's folder (FAR's `m_WorkDir`).
    pub work: Option<usize>,
    /// The worker is still reading; the folders it knows.
    pub reading: bool,
    pub count: usize,
    worker: Option<TreeWorker>,
    /// The other panel's folder the cursor last went after.
    synced: Option<PathBuf>,
    /// That folder, until it is in the tree.
    waiting: Option<PathBuf>,
    /// An operation ended: the panels' folders and these are to be read again once the tree shows.
    stale: Option<Vec<PathBuf>>,
    /// The tree had the keys at the last tick.
    focused: bool,
}

#[cfg(test)]
thread_local! {
    /// The root the tests' trees have: never a real disk's.
    pub(super) static TEST_ROOT: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// The root of `path`'s disk (FAR's `extract_root_directory`).
pub(super) fn root_of(path: &Path) -> PathBuf {
    #[cfg(test)]
    if let Some(root) = TEST_ROOT.with(|root| root.borrow().clone()) {
        return root;
    }
    path.ancestors().last().unwrap_or(path).to_path_buf()
}

impl TreePanel {
    /// A tree of `folders` under `root`, read (tests of the drawing).
    #[cfg(test)]
    pub fn showing(side: Side, root: &Path, folders: Vec<PathBuf>) -> Self {
        Self {
            root: root.to_path_buf(),
            items: make_items(root, folders),
            ..Self::new(side)
        }
    }

    fn new(side: Side) -> Self {
        Self {
            side,
            root: PathBuf::new(),
            items: Vec::new(),
            cursor: 0,
            top: 0,
            rows: 1,
            work: None,
            reading: false,
            count: 0,
            worker: None,
            synced: None,
            waiting: None,
            stale: None,
            focused: false,
        }
    }

    pub fn current(&self) -> Option<&TreeItem> {
        self.items.get(self.cursor)
    }

    fn send(&self, order: Order) {
        if let Some(worker) = &self.worker {
            worker.send(order);
        }
    }

    /// FAR's `CorrectPosition`: the cursor in the list and on screen.
    fn correct(&mut self) {
        self.cursor = self.cursor.min(self.items.len().saturating_sub(1));
        let rows = self.rows.max(1);
        if self.cursor < self.top {
            self.top = self.cursor;
        } else if self.cursor >= self.top + rows {
            self.top = self.cursor + 1 - rows;
        }
        self.top = self.top.min(self.items.len().saturating_sub(rows));
    }

    /// The rows the list has, as drawn; the cursor kept on screen.
    pub fn set_rows(&mut self, rows: usize) {
        self.rows = rows.max(1);
        self.correct();
    }

    fn position(&self, folder: &Path) -> Option<usize> {
        (self.items.iter()).position(|item| same_place(&item.path, folder))
    }

    /// FAR's `SetDirPosition`: the cursor on `folder`, in the middle of the list; false when not in it.
    fn go_to(&mut self, folder: &Path) -> bool {
        let Some(at) = self.position(folder) else {
            return false;
        };
        self.work = Some(at);
        self.cursor = at;
        // FAR's `m_CurTopFile = m_CurFile - (height - 2) / 2`, the list's rows being height - 4.
        self.top = at.saturating_sub((self.rows + 2) / 2);
        self.correct();
        true
    }

    /// FAR's `GetNextNavPos` / `GetPrevNavPos`: the next or previous line of the cursor's depth.
    fn same_depth(&self, forward: bool) -> usize {
        let depth = self.items.get(self.cursor).map_or(0, |item| item.depth);
        let found = if forward {
            (self.cursor + 1..self.items.len()).find(|&at| self.items[at].depth == depth)
        } else {
            (1..self.cursor)
                .rev()
                .find(|&at| self.items[at].depth == depth)
        };
        found.unwrap_or(self.cursor)
    }

    /// A worker for the tree of `root` (the old one stops).
    fn start(&mut self, root: PathBuf, keeping: Option<Keeping>) {
        self.items = make_items(&root, Vec::new());
        (self.cursor, self.top, self.work) = (0, 0, None);
        self.worker = Some(TreeWorker::start(root.clone(), keeping));
        self.root = root;
        self.reading = true;
    }

    /// What the worker knows in the tree: the cursor on the folder it waits for once it is in, else
    /// where it was (on the nearest folder above when that is gone).
    fn take(&mut self, snapshot: Snapshot) {
        let at = self.current().map(|item| item.path.clone());
        if !snapshot.items.is_empty() {
            self.items = snapshot.items;
        }
        self.reading = snapshot.reading;
        self.count = snapshot.count;
        self.work = (self.synced.clone()).and_then(|synced| self.position(&synced));
        if let Some(waiting) = self.waiting.clone()
            && self.go_to(&waiting)
        {
            self.waiting = None;
            return;
        }
        let place = at.and_then(|at| at.ancestors().find_map(|above| self.position(above)));
        self.cursor = place.unwrap_or(0);
        self.correct();
    }

    /// The cursor goes after `folder` (the other panel's, FAR's `SyncDir`): there when it is in the
    /// tree, else the worker reads it first and the cursor waits for it.
    fn aim(&mut self, folder: PathBuf) {
        self.synced = Some(folder.clone());
        self.send(Order::Follow(folder.clone()));
        self.waiting = (!self.go_to(&folder)).then_some(folder);
    }

    /// A folder that is gone leaves the tree with what is in it; the folder it was in is read again.
    fn forget(&mut self, gone: &Path) {
        let folders: Vec<(usize, PathBuf)> = (self.items.iter().skip(1))
            .filter(|other| !other.path.starts_with(gone))
            .map(|other| (other.depth, other.path.clone()))
            .collect();
        self.items = lay_out(&self.root.clone(), folders.into_iter());
        self.correct();
        let above = gone.parent().map(Path::to_path_buf);
        self.send(Order::Check(above.into_iter().collect()));
    }

    /// The cursor where `to` says, or a page on (`page`); true when it moved. The cursor no longer
    /// waits for the other panel's folder, and the folder under it is read first when its folders are
    /// not known yet.
    fn move_cursor(&mut self, to: impl Fn(&TreePanel) -> usize, page: Option<bool>) -> bool {
        let before = self.cursor;
        match page {
            Some(forward) => self.page(forward),
            None => {
                self.cursor = to(self);
                self.correct();
            }
        }
        self.waiting = None;
        if let Some(item) = self.current() {
            self.send(Order::Focus(item.path.clone()));
        }
        self.cursor != before
    }

    /// Ctrl+R: every folder read again, the tree in use meanwhile.
    fn reread(&mut self) {
        self.send(Order::Reread);
        self.reading = self.worker.is_some();
    }

    /// The cursor keys of FAR's tree (the panel's and Find folder's): Up, Down, Home, End, PgUp, PgDn,
    /// Gray + / - (the next or previous folder of the same depth); whether it moved, none for another
    /// key.
    fn key_move(&mut self, key: KeyEvent) -> Option<bool> {
        let plain = key.modifiers.is_empty();
        let gray = key.state.contains(KeyEventState::KEYPAD);
        let moved = match key.code {
            KeyCode::Char('+') if gray && plain => {
                self.move_cursor(|tree| tree.same_depth(true), None)
            }
            KeyCode::Char('-') if gray && plain => {
                self.move_cursor(|tree| tree.same_depth(false), None)
            }
            KeyCode::Up if plain => self.move_cursor(|tree| tree.cursor.saturating_sub(1), None),
            KeyCode::Down if plain => self.move_cursor(|tree| tree.cursor + 1, None),
            KeyCode::Home if plain => self.move_cursor(|_| 0, None),
            KeyCode::End if plain => self.move_cursor(|tree| tree.items.len(), None),
            KeyCode::PageUp if plain => self.move_cursor(|_| usize::MAX, Some(false)),
            KeyCode::PageDown if plain => self.move_cursor(|_| usize::MAX, Some(true)),
            _ => return None,
        };
        Some(moved)
    }

    /// FAR's PgUp / PgDn: the cursor and the list's top a list's height on.
    fn page(&mut self, forward: bool) {
        let rows = self.rows;
        if forward {
            self.cursor += rows;
            self.top += rows;
        } else {
            self.cursor = self.cursor.saturating_sub(rows);
            self.top = self.top.saturating_sub(rows);
        }
        self.correct();
    }
}

impl App {
    /// The side the tree is on.
    pub fn tree_side(&self) -> Option<Side> {
        self.tree_panel.as_ref().map(|tree| tree.side)
    }

    /// The keys go to the tree.
    pub(super) fn tree_is_active(&self) -> bool {
        self.tree_side() == Some(self.active)
    }

    /// Whether `side` shows its files (not the info panel or the tree).
    pub fn shows_files(&self, side: Side) -> bool {
        self.info_side() != Some(side)
            && self.tree_side() != Some(side)
            && self.quick_view_side() != Some(side)
    }

    /// Ctrl+T (FAR's `FilePanels::ProcessKey`): the tree on the active side, or on the passive one,
    /// becomes files again; otherwise the passive panel becomes the tree.
    pub(super) fn toggle_tree_panel(&mut self) {
        if !self.active_panel().visible || self.prefs.tree.turn_off_completely {
            return;
        }
        let side = if self.tree_is_active() {
            self.active
        } else {
            self.active.other()
        };
        if self.tree_side() == Some(side) {
            self.tree_panel = None;
        } else {
            self.open_tree_panel(side);
        }
    }

    /// The menu's "Tree panel" of `side`: that side shows it; there is one, on the other side it moves.
    pub(super) fn show_tree_panel_on(&mut self, side: Side) {
        if self.tree_side() != Some(side) && !self.prefs.tree.turn_off_completely {
            self.open_tree_panel(side);
        }
    }

    fn open_tree_panel(&mut self, side: Side) {
        self.close_info_panel(side);
        self.close_quick_view(side);
        self.panels[side.index()].visible = true;
        // The worker starts as the tree follows the other panel (`pump_tree_panel`).
        self.tree_panel = Some(TreePanel::new(side));
        self.pump_tree_panel();
    }

    /// Back to files on `side`.
    pub(super) fn close_tree_panel(&mut self, side: Side) {
        if self.tree_side() == Some(side) {
            self.tree_panel = None;
        }
    }

    /// Ctrl+U: the tree goes with its side.
    pub(super) fn swap_tree_panel(&mut self) {
        if let Some(tree) = &mut self.tree_panel {
            tree.side = tree.side.other();
        }
    }

    /// The trees' files and how few folders a tree needs to be kept in one.
    fn tree_keeping(&self) -> Option<Keeping> {
        (self.tasks.tree_cache.clone()).map(|cache| Keeping {
            cache,
            min: self.prefs.tree.min_tree_count,
        })
    }

    /// An operation ended or a folder was made: the panels' folders and `places` (where the operations
    /// went) are read again first; what was done elsewhere shows by the folders' times.
    pub(super) fn tree_files_changed(&mut self, places: Vec<PathBuf>) {
        if let Some(tree) = &mut self.tree_panel {
            tree.stale.get_or_insert_with(Vec::new).extend(places);
        }
    }

    /// The folder of the panel beside the tree (FAR's `GetRootPanel`): a local one only.
    fn tree_folder(&self, side: Side) -> Option<PathBuf> {
        self.panels[side.other().index()]
            .path
            .as_local()
            .map(Path::to_path_buf)
    }

    /// What the worker sent, and the cursor after the other panel's folder (FAR's `SyncDir`): another
    /// disk gets a worker of its own. True when the tree is to be drawn again.
    pub(super) fn pump_tree_panel(&mut self) -> bool {
        let found = self.pump_folder_tree();
        let focused = self.tree_is_active();
        let Some(mut tree) = self.tree_panel.take() else {
            return found;
        };
        // FAR's `OnFocusChange`: the tree losing the keys on a folder that is gone, it leaves.
        if std::mem::replace(&mut tree.focused, focused)
            && !focused
            && let Some(item) = tree.current().filter(|item| !item.path.is_dir())
        {
            let above = item.path.parent().map(Path::to_path_buf);
            tree.send(Order::Check(above.into_iter().collect()));
        }
        let mut changed = false;
        if let Some(mut snapshot) = tree.worker.as_ref().and_then(TreeWorker::latest) {
            if let Some(unsaved) = snapshot.unsaved.take() {
                self.show_dialog(super::Dialog::error(
                    "Error",
                    vec![
                        "Cannot save folders tree to file".to_string(),
                        unsaved.file.display().to_string(),
                        unsaved.error,
                    ],
                ));
            }
            tree.take(snapshot);
            changed = true;
        }
        // FAR's `Update` of a hidden panel waits until it shows.
        if self.panels[tree.side.index()].visible {
            changed |= self.follow_in_tree(&mut tree);
            if let Some(places) = tree.stale.take() {
                let folders = [Side::Left, Side::Right].map(|side| self.tree_folder(side.other()));
                tree.send(Order::Check(
                    folders.into_iter().flatten().chain(places).collect(),
                ));
            }
        }
        self.tree_panel = Some(tree);
        changed | found
    }

    /// The other panel's folder, when it changed: the cursor there, or the worker reads it first and
    /// the cursor waits for it.
    fn follow_in_tree(&self, tree: &mut TreePanel) -> bool {
        let Some(folder) = self.tree_folder(tree.side) else {
            return false;
        };
        if (tree.synced.as_ref()).is_some_and(|synced| same_place(synced, &folder)) {
            return false;
        }
        let root = root_of(&folder);
        if tree.worker.is_none() || !same_place(&root, &tree.root) {
            tree.start(root, self.tree_keeping());
        }
        tree.aim(folder);
        true
    }

    /// FAR's `ProcessEnter`: the other panel goes to the folder under the cursor; one that is gone
    /// leaves the tree with what is in it, and the folder it was in is read again.
    pub(super) fn tree_enter(&mut self) {
        // A mouse hold ending while other work waits (review of 19-4): no second wait.
        if self.is_waiting() {
            return;
        }
        let Some(path) = (self.tree_panel.as_ref())
            .and_then(|tree| tree.current())
            .map(|item| item.path.clone())
        else {
            return;
        };
        // Its disk asked off the screen's thread (ADR-0019, review R22).
        self.wait_for(Vec::new(), move || {
            let there = path.is_dir();
            Box::new(move |app: &mut App| app.tree_enter_on(path, there))
        });
        self.label_waiting("Reading the folder…");
    }

    /// Enter on the tree's `path`, which its disk says is `there` or gone.
    fn tree_enter_on(&mut self, path: PathBuf, there: bool) {
        let Some(tree) = &mut self.tree_panel else {
            return;
        };
        if !there {
            tree.forget(&path);
            return;
        }
        tree.work = Some(tree.cursor);
        tree.synced = Some(path.clone());
        tree.waiting = None;
        let side = tree.side.other();
        if let Err(error) = self.panels[side.index()].change_dir(&VfsPath::local(&path), None) {
            self.show_error("Change directory", &error);
        }
    }

    /// Ctrl+R on the tree: every folder read again, the tree in use meanwhile.
    pub(super) fn reread_tree(&mut self) {
        if let Some(tree) = &mut self.tree_panel {
            tree.reread();
        }
    }

    /// The tree's keys; false for those that go on to the panels' others (the command line's).
    pub(super) fn handle_tree_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        if key.code == KeyCode::Char('t') && ctrl {
            self.toggle_tree_panel();
            return true;
        }
        // FAR's hidden tree takes no key (`if (!IsVisible()) return false`).
        if !self.tree_has_keys() {
            return false;
        }
        if self.tree_operation_key(key) || self.tree_name_key(key) {
            return true;
        }
        let plain = key.modifiers.is_empty();
        match key.code {
            // FAR: with a command typed, Enter runs it.
            KeyCode::Enter if plain && self.command_line.is_empty() => {
                self.tree_enter();
                return true;
            }
            KeyCode::Char('r') if ctrl => {
                self.reread_tree();
                return true;
            }
            _ => {}
        }
        let Some(moved) = (self.tree_panel.as_mut()).and_then(|tree| tree.key_move(key)) else {
            return false;
        };
        if moved && self.prefs.tree.auto_change_folder {
            self.tree_enter();
        }
        true
    }

    /// The cursor where `to` says, or a page on (`page`); true when it moved. The folder under it is
    /// read first when its folders are not known yet.
    pub(super) fn move_tree(
        &mut self,
        to: impl Fn(&TreePanel) -> usize,
        page: Option<bool>,
    ) -> bool {
        (self.tree_panel.as_mut()).is_some_and(|tree| tree.move_cursor(to, page))
    }
}

#[cfg(test)]
#[path = "tree_panel_tests.rs"]
mod tests;
