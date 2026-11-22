//! The program's panels as a plugin reads and changes them (API 3, FAR's `PanelControl`): [`Host::panel`]
//! and [`Host::panel_items`] read either panel; the rest changes it, its command line, or opens the viewer
//! or the editor. Each blocks the plugin's thread until the program answers; a folder set or read again
//! is answered once it is read, as FAR's calls are.

use std::path::Path;
use std::time::SystemTime;

use crate::Host;
use crate::api::{self, NcPanelAction, NcPanelItem, NcPanelState, NcStr};
use crate::host::flag;

/// Which panel (FAR's `PANEL_ACTIVE`, `PANEL_PASSIVE`; either side by its place).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Active,
    Passive,
    Left,
    Right,
}

impl Which {
    pub(crate) fn code(self) -> u32 {
        match self {
            Self::Active => api::WHICH_ACTIVE,
            Self::Passive => api::WHICH_PASSIVE,
            Self::Left => api::WHICH_LEFT,
            Self::Right => api::WHICH_RIGHT,
        }
    }
}

/// What the panel's side shows (FAR's `PTYPE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelKind {
    #[default]
    Files,
    Tree,
    QuickView,
    Info,
}

/// A panel as it is (FAR's `PanelInfo`, `FCTL_GETPANELDIRECTORY`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelState {
    pub left: bool,
    pub active: bool,
    pub visible: bool,
    /// The plugin whose panel it is; none for a disk's.
    pub plugin: Option<String>,
    /// A plugin's panel whose items are files on a disk (FAR's `PFLAGS_REALNAMES`).
    pub real_names: bool,
    pub show_hidden: bool,
    pub reverse_sort: bool,
    pub selected_first: bool,
    /// The panel the plugin's call running is for: the drive menu's that chose its item.
    pub call_target: bool,
    /// Its folder is still being read.
    pub reading: bool,
    pub kind: PanelKind,
    /// A disk's path, or a plugin's own path (`/` its root).
    pub folder: String,
    /// The disk's folder under the panel (FAR's `GetCurDir`).
    pub disk_folder: String,
    /// Its items, `..` first where it has one.
    pub items: usize,
    /// The selected ones, or 1 for the current item when none is (FAR's `SelectedItemsNumber`).
    pub selected: usize,
    pub current: usize,
    pub top: usize,
    /// The ABI's `SORT_*`.
    pub sort: u32,
    pub view_mode: u32,
    /// The view mode's columns as the modes name them ("N,S,D"; FAR's `FCTL_GETCOLUMNTYPES`).
    pub columns: String,
    /// Their widths as drawn now ("20,10,8"; FAR's `FCTL_GETCOLUMNWIDTHS`).
    pub widths: String,
}

/// One item of a panel (FAR's `PluginPanelItem`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelItem {
    pub name: String,
    pub selected: bool,
    pub folder: bool,
    /// The `..` at the top.
    pub parent: bool,
    /// The file's attributes as the system gives them (Windows' `FILE_ATTRIBUTE_*`).
    pub attributes: u32,
    pub size: u64,
    pub allocated: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub changed: Option<SystemTime>,
    pub links: u32,
    /// Its file on a disk, whole; none for a plugin's item that is no file.
    pub path: Option<String>,
    pub description: String,
    pub owner: String,
}

impl Host {
    /// The panel `which` as it is now; none while the plugin starts.
    pub fn panel(&self, which: Which) -> Option<PanelState> {
        let table = self.table();
        let mut raw = NcPanelState::EMPTY;
        // SAFETY: as every call of the host's; its texts are copied before the next call.
        unsafe {
            if (table.panel_state)(table.context, which.code(), &raw mut raw) != 0 {
                return None;
            }
            Some(state_of(&raw))
        }
    }

    /// The panel's items, `..` first where it has one (FAR's `FCTL_GETPANELITEM`).
    pub fn panel_items(&self, which: Which) -> Vec<PanelItem> {
        self.items(which, 0)
    }

    /// The selected items in the panel's order, or the current one when none is and it is no `..`
    /// (FAR's `FCTL_GETSELECTEDPANELITEM`).
    pub fn selected_items(&self, which: Which) -> Vec<PanelItem> {
        self.items(which, api::ITEMS_SELECTED)
    }

    fn items(&self, which: Which, flags: u32) -> Vec<PanelItem> {
        let table = self.table();
        // SAFETY: as every call; the first asks how many, the second (the same items) reads them.
        unsafe {
            let count = (table.panel_items)(
                table.context,
                which.code(),
                flags,
                0,
                std::ptr::null_mut(),
                0,
            );
            let Ok(count) = usize::try_from(count) else {
                return Vec::new();
            };
            let mut raw = vec![NcPanelItem::EMPTY; count];
            let read = (table.panel_items)(
                table.context,
                which.code(),
                flags,
                0,
                raw.as_mut_ptr(),
                count,
            );
            if read < 0 {
                return Vec::new();
            }
            raw.iter().map(|item| item_of(item)).collect()
        }
    }

    /// The panel to `folder` (a disk's path, or on a plugin's panel its own `/...`), the cursor on the
    /// item `cursor` when given (FAR's `FCTL_SETPANELDIRECTORY`): true once it is read there.
    pub fn set_folder(&self, which: Which, folder: &str, cursor: Option<&str>) -> bool {
        let names: Vec<NcStr> = cursor.into_iter().map(NcStr::new).collect();
        let mut action = NcPanelAction::new(api::DO_SET_FOLDER);
        action.text = NcStr::new(folder);
        action.names = names.as_ptr();
        action.name_count = names.len();
        self.act(which, &action)
    }

    /// The cursor to the item `index`, the top to `top` (none: as the panel puts it) (FAR's
    /// `FCTL_REDRAWPANEL`).
    pub fn set_cursor(&self, which: Which, index: usize, top: Option<usize>) -> bool {
        let mut action = NcPanelAction::new(api::DO_SET_CURSOR);
        action.number = index;
        action.number2 = top.unwrap_or(usize::MAX);
        self.act(which, &action)
    }

    /// The cursor to the item named `name`.
    pub fn set_cursor_on(&self, which: Which, name: &str) -> bool {
        let mut action = NcPanelAction::new(api::DO_SET_CURSOR);
        action.text = NcStr::new(name);
        action.number2 = usize::MAX;
        self.act(which, &action)
    }

    /// The folder read again, the selection kept or not (FAR's `FCTL_UPDATEPANEL`): true once read.
    pub fn reread(&self, which: Which, keep_selection: bool) -> bool {
        let mut action = NcPanelAction::new(api::DO_REREAD);
        action.flags = flag(keep_selection, api::DO_KEEP_SELECTION);
        self.act(which, &action)
    }

    /// Every item but `..` selected, or (`on` false) unselected.
    pub fn select_all(&self, which: Which, on: bool) -> bool {
        let mut action = NcPanelAction::new(api::DO_SELECT);
        action.flags = flag(!on, api::DO_OFF) | api::DO_ALL;
        self.act(which, &action)
    }

    /// The items named selected, or (`on` false) unselected (FAR's `FCTL_SETSELECTION`).
    pub fn select(&self, which: Which, names: &[String], on: bool) -> bool {
        let names: Vec<NcStr> = names.iter().map(|name| NcStr::new(name)).collect();
        let mut action = NcPanelAction::new(api::DO_SELECT);
        action.flags = flag(!on, api::DO_OFF);
        action.names = names.as_ptr();
        action.name_count = names.len();
        self.act(which, &action)
    }

    /// The sort `sort` (`SORT_*`), the other way or not (FAR's `FCTL_SETSORTMODE`,
    /// `FCTL_SETSORTORDER`).
    pub fn set_sort(&self, which: Which, sort: u32, reverse: bool) -> bool {
        let mut action = NcPanelAction::new(api::DO_SET_SORT);
        action.number = usize::try_from(sort).unwrap_or(0);
        action.flags = flag(reverse, api::DO_REVERSE);
        self.act(which, &action)
    }

    /// A plugin's panel closed, to the disk's `folder` or where it came from (FAR's `FCTL_CLOSEPANEL`).
    pub fn close_panel(&self, which: Which, folder: Option<&str>) -> bool {
        let mut action = NcPanelAction::new(api::DO_CLOSE);
        action.text = NcStr::new(folder.unwrap_or(""));
        self.act(which, &action)
    }

    /// The command line's text (FAR's `FCTL_GETCMDLINE`).
    pub fn command_line(&self) -> String {
        let table = self.table();
        let action = NcPanelAction::new(api::DO_GET_COMMAND_LINE);
        let mut answer = NcStr::EMPTY;
        // SAFETY: as every call; the answer is copied before the next call.
        unsafe {
            let done = (table.panel_do)(
                table.context,
                api::WHICH_ACTIVE,
                &raw const action,
                &raw mut answer,
            );
            if done == 0 {
                answer.as_str().to_string()
            } else {
                String::new()
            }
        }
    }

    /// The command line set to `text` (FAR's `FCTL_SETCMDLINE`).
    pub fn set_command_line(&self, text: &str) -> bool {
        let mut action = NcPanelAction::new(api::DO_SET_COMMAND_LINE);
        action.text = NcStr::new(text);
        self.act(Which::Active, &action)
    }

    /// `text` put into the command line at its cursor (FAR's `FCTL_INSERTCMDLINE`).
    pub fn insert_command_line(&self, text: &str) -> bool {
        let mut action = NcPanelAction::new(api::DO_INSERT_COMMAND_LINE);
        action.text = NcStr::new(text);
        self.act(Which::Active, &action)
    }

    /// `command` run as the command line runs it, Enter pressed on it, in the program's terminal (FAR's
    /// farcmds on the user screen). Returns as it starts; false when it cannot (other work waited for).
    pub fn run_command(&self, command: &str) -> bool {
        let mut action = NcPanelAction::new(api::DO_RUN_COMMAND);
        action.text = NcStr::new(command);
        self.act(Which::Active, &action)
    }

    /// The viewer on `path` (FAR's `Viewer` with `VF_NONMODAL`), the file deleted as it closes when
    /// `delete_on_close` (`VF_DELETEONCLOSE`). Returns as the viewer opens; false for a path that is
    /// no exact text (R7 review A8: a sibling with U+FFFD was viewed).
    pub fn view(&self, path: &Path, delete_on_close: bool) -> bool {
        let Some(path) = path.to_str() else {
            return false;
        };
        let mut action = NcPanelAction::new(api::DO_VIEW);
        action.text = NcStr::new(path);
        action.flags = flag(delete_on_close, api::DO_DELETE_ON_CLOSE);
        self.act(Which::Active, &action)
    }

    /// The editor on `path` at `line` and `column` (1-based, 0: the start) (FAR's `Editor`); false for
    /// a path that is no exact text, as [`Host::view`].
    pub fn edit(&self, path: &Path, line: usize, column: usize, delete_on_close: bool) -> bool {
        let Some(path) = path.to_str() else {
            return false;
        };
        let mut action = NcPanelAction::new(api::DO_EDIT);
        action.text = NcStr::new(path);
        action.number = line;
        action.number2 = column;
        action.flags = flag(delete_on_close, api::DO_DELETE_ON_CLOSE);
        self.act(Which::Active, &action)
    }

    fn act(&self, which: Which, action: &NcPanelAction) -> bool {
        let table = self.table();
        // SAFETY: as every call; the action and its texts live through it.
        unsafe { (table.panel_do)(table.context, which.code(), action, std::ptr::null_mut()) == 0 }
    }
}

/// # Safety
/// `raw`'s texts are the host's, alive through the call.
unsafe fn state_of(raw: &NcPanelState) -> PanelState {
    let on = |bit: u32| raw.flags & bit != 0;
    // SAFETY: as the caller promises.
    let (folder, plugin, disk_folder, columns, widths) = unsafe {
        (
            raw.folder.as_str(),
            raw.plugin.as_str(),
            raw.disk_folder.as_str(),
            raw.columns.as_str(),
            raw.widths.as_str(),
        )
    };
    PanelState {
        left: on(api::STATE_LEFT),
        active: on(api::STATE_ACTIVE),
        visible: on(api::STATE_VISIBLE),
        plugin: (on(api::STATE_PLUGIN)).then(|| plugin.to_string()),
        real_names: on(api::STATE_REAL_NAMES),
        show_hidden: on(api::STATE_SHOW_HIDDEN),
        reverse_sort: on(api::STATE_REVERSE_SORT),
        selected_first: on(api::STATE_SELECTED_FIRST),
        call_target: on(api::STATE_CALL_TARGET),
        reading: on(api::STATE_READING),
        kind: match raw.kind {
            api::KIND_TREE => PanelKind::Tree,
            api::KIND_QUICK_VIEW => PanelKind::QuickView,
            api::KIND_INFO => PanelKind::Info,
            _ => PanelKind::Files,
        },
        folder: folder.to_string(),
        disk_folder: disk_folder.to_string(),
        items: raw.items,
        selected: raw.selected,
        current: raw.current,
        top: raw.top,
        sort: raw.sort,
        view_mode: raw.view_mode,
        columns: columns.to_string(),
        widths: widths.to_string(),
    }
}

/// # Safety
/// As [`state_of`].
unsafe fn item_of(raw: &NcPanelItem) -> PanelItem {
    let on = |bit: u32| raw.flags & bit != 0;
    // SAFETY: as the caller promises.
    let (name, path, description, owner) = unsafe {
        (
            raw.name.as_str(),
            raw.path.as_str(),
            raw.description.as_str(),
            raw.owner.as_str(),
        )
    };
    PanelItem {
        name: name.to_string(),
        selected: on(api::ITEM_SELECTED),
        folder: on(api::ITEM_FOLDER),
        parent: on(api::ITEM_PARENT),
        attributes: raw.attributes,
        size: raw.size,
        allocated: raw.allocated,
        modified: time(raw.modified),
        created: time(raw.created),
        accessed: time(raw.accessed),
        changed: time(raw.changed),
        links: raw.links,
        path: (!path.is_empty()).then(|| path.to_string()),
        description: description.to_string(),
        owner: owner.to_string(),
    }
}

/// The ABI's milliseconds from 1970 (before it negative).
pub(crate) fn time(millis: i64) -> Option<SystemTime> {
    if millis == api::TIME_NONE {
        return None;
    }
    let distance = std::time::Duration::from_millis(millis.unsigned_abs());
    if millis < 0 {
        std::time::UNIX_EPOCH.checked_sub(distance)
    } else {
        std::time::UNIX_EPOCH.checked_add(distance)
    }
}

#[cfg(test)]
#[path = "panels_tests.rs"]
mod tests;
