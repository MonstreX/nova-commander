//! A list that is edited in place, as FAR's lists of associations and of mask groups are (`VMenu2` with
//! its keys in the bottom frame): Ins adds, Enter or F4 changes the line under the cursor, Del takes it
//! out after a question, Esc closes. The associations keep their order (Ctrl+Up / Ctrl+Down move a
//! line); the mask groups are in the order of their names, F7 shows those whose masks take a name and
//! Ctrl+R brings back the default ones; the panel modes have a separator before the user's own and
//! hotkeys (`app::panel_modes_menu`). A dialog of it stands over it and the list comes back when the
//! dialog is gone.

use std::cell::Cell;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::form::Form;
use super::vmenu_keys;
use super::{App, MainView, Overlay, Screen};

/// What a list edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditListKind {
    /// Commands > File associations.
    Associations,
    /// Options > Groups of file masks.
    MaskGroups,
    /// Options > File panel modes.
    PanelModes,
}

/// An open list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditList {
    pub kind: EditListKind,
    pub title: String,
    pub lines: Vec<String>,
    /// The item of each line.
    pub items: Vec<usize>,
    /// The keys, or what F7 found.
    pub bottom: String,
    /// F7's names: only the groups whose masks take them all are shown (none: all are).
    pub found: Vec<String>,
    pub selected: usize,
    pub top: Cell<usize>,
    pub rows: Cell<usize>,
    /// The lines that are separators.
    pub separators: Vec<usize>,
    /// The lines' `&` mark their hotkeys (else they are shown as they are).
    pub hotkeys: bool,
}

/// The keys in the associations' bottom frame (FAR's `KeysToLocalizedText`).
pub const EDIT_LIST_KEYS: &str = "Ins, Del, F4, Ctrl+Up, Ctrl+Down";
/// The keys in the mask groups' bottom frame.
const MASK_GROUP_KEYS: &str = "Ins, Del, F4, F7, Ctrl+R";

fn ctrl_shift() -> KeyModifiers {
    KeyModifiers::CONTROL | KeyModifiers::SHIFT
}

/// The line whose `&` hotkey is `letter`, any case.
fn hotkey_line(list: &EditList, letter: char) -> Option<usize> {
    let wanted = letter.to_lowercase().next()?;
    (list.lines.iter()).position(|line| super::vmenu_keys::marked_hotkey(line) == Some(wanted))
}

/// What the app keeps of the lists.
#[derive(Debug, Default)]
pub(super) struct EditListState {
    /// The list a dialog of it stands over.
    pub(super) behind: Option<EditList>,
    /// A dialog of it that comes back (what it held was wrong).
    pub(super) form: Option<(Box<Form>, super::dialog::FormAction)>,
}

impl EditList {
    /// The item under the cursor.
    pub(super) fn item(&self) -> Option<usize> {
        self.items.get(self.selected).copied()
    }
}

impl App {
    /// The list of `kind` with its lines as they are, the cursor on `selected`.
    pub(super) fn show_edit_list(&mut self, kind: EditListKind, selected: usize) {
        let mut list = EditList {
            kind,
            title: String::new(),
            lines: Vec::new(),
            items: Vec::new(),
            bottom: String::new(),
            found: Vec::new(),
            selected,
            top: Cell::new(0),
            rows: Cell::new(1),
            separators: Vec::new(),
            hotkeys: kind == EditListKind::PanelModes,
        };
        self.fill_edit_list(&mut list);
        self.overlay = Some(Overlay::EditList(list));
    }

    pub(super) fn fill_edit_list(&self, list: &mut EditList) {
        match list.kind {
            EditListKind::Associations => {
                list.title = "File associations".to_string();
                list.lines = self.association_lines();
                list.items = (0..list.lines.len()).collect();
                list.bottom = EDIT_LIST_KEYS.to_string();
            }
            EditListKind::MaskGroups => {
                let (lines, items) = self.mask_group_lines(&list.found);
                (list.lines, list.items) = (lines, items);
                match list.found.last() {
                    // FAR's title becomes the name looked for, its bottom the count.
                    Some(name) => {
                        list.title = name.clone();
                        list.bottom = format!("Total: {}", list.lines.len());
                    }
                    None => {
                        list.title = super::mask_groups::TITLE.to_string();
                        list.bottom = MASK_GROUP_KEYS.to_string();
                    }
                }
            }
            EditListKind::PanelModes => {
                (list.lines, list.items, list.separators) = self.panel_mode_lines();
                list.title = super::panel_modes_menu::TITLE.to_string();
                list.bottom = super::panel_modes_menu::KEYS.to_string();
            }
        }
        list.selected = list.selected.min(list.lines.len().saturating_sub(1));
    }

    pub(super) fn handle_edit_list_key(&mut self, mut list: EditList, key: KeyEvent) {
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        // FAR's list takes these keys plain only.
        let plain = key.modifiers.is_empty();
        let any = !list.lines.is_empty();
        let groups = list.kind == EditListKind::MaskGroups;
        let modes = list.kind == EditListKind::PanelModes;
        match key.code {
            // FAR: these keys first show all the groups again.
            KeyCode::Esc | KeyCode::F(10) | KeyCode::Enter if !list.found.is_empty() => {
                list.found.clear();
                self.fill_edit_list(&mut list);
            }
            KeyCode::Esc | KeyCode::F(10) => return,
            KeyCode::Insert if plain => return self.edit_list_item(list, true),
            KeyCode::Enter | KeyCode::F(4) if any && plain => {
                return self.edit_list_item(list, false);
            }
            KeyCode::Delete if any && plain => return self.ask_remove_list_item(list),
            KeyCode::F(7) if groups && plain => {
                self.edit_lists.behind = Some(list);
                return self.ask_find_mask_group();
            }
            KeyCode::Char('r') if groups && ctrl => {
                self.edit_lists.behind = Some(list);
                return self.ask_restore_mask_groups();
            }
            KeyCode::Enter if modes && (ctrl || key.modifiers == ctrl_shift()) => {
                self.show_panel_mode(&list, key.modifiers != KeyModifiers::CONTROL);
            }
            KeyCode::Char(letter)
                if list.hotkeys && (plain || key.modifiers == KeyModifiers::SHIFT) =>
            {
                if let Some(at) = hotkey_line(&list, letter) {
                    list.selected = at;
                    return self.edit_list_item(list, false);
                }
            }
            KeyCode::Up | KeyCode::Down if ctrl && !groups && !modes => {
                self.move_list_item(&mut list, key);
            }
            code => {
                let skip: Vec<bool> = (0..list.lines.len())
                    .map(|line| list.separators.contains(&line))
                    .collect();
                list.selected = vmenu_keys::step(&skip, list.selected, code, list.rows.get());
            }
        }
        self.overlay = Some(Overlay::EditList(list));
    }

    /// Ctrl+Up / Ctrl+Down: the line and the one before or after it change places.
    fn move_list_item(&mut self, list: &mut EditList, key: KeyEvent) {
        let other = if key.code == KeyCode::Up {
            list.selected.checked_sub(1)
        } else {
            Some(list.selected + 1)
        };
        if let Some(other) = other.filter(|&other| other < list.lines.len()) {
            self.config.associations.swap(list.selected, other);
            list.selected = other;
            self.fill_edit_list(list);
        }
    }

    fn edit_list_item(&mut self, list: EditList, insert: bool) {
        let (kind, at, item) = (list.kind, list.selected, list.item());
        self.edit_lists.behind = Some(list);
        match kind {
            EditListKind::Associations => self.edit_association(at, insert),
            EditListKind::MaskGroups => self.edit_mask_group(item.filter(|_| !insert)),
            EditListKind::PanelModes => {
                let base = item.and_then(super::panel_modes_menu::mode_of_item);
                let base = base.unwrap_or_default();
                self.edit_panel_mode((!insert).then_some(base), base);
            }
        }
    }

    fn ask_remove_list_item(&mut self, list: EditList) {
        let (at, item) = (list.selected, list.item());
        match list.kind {
            // Its user's mode goes at once, with no question.
            EditListKind::PanelModes => self.delete_panel_mode(list),
            EditListKind::Associations => {
                self.edit_lists.behind = Some(list);
                self.ask_remove_association(at);
            }
            EditListKind::MaskGroups => {
                self.edit_lists.behind = Some(list);
                if let Some(item) = item {
                    self.ask_remove_mask_group(item);
                }
            }
        }
    }

    /// The dialog that comes back, else the list a dialog stood over; true when one came back.
    pub(super) fn resume_edit_list(&mut self) -> bool {
        if self.overlay.is_some()
            || self.waiting.is_some()
            || !matches!(self.screen, Screen::Panels)
            || self.main_view != MainView::Panels
        {
            return false;
        }
        if let Some((form, action)) = self.edit_lists.form.take() {
            self.overlay = Some(Overlay::Dialog(super::Dialog::Form {
                form: *form,
                action,
            }));
            return true;
        }
        let Some(mut list) = self.edit_lists.behind.take() else {
            return false;
        };
        self.fill_edit_list(&mut list);
        self.overlay = Some(Overlay::EditList(list));
        true
    }
}
