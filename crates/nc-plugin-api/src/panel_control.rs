//! The program's panels as a plugin reads and changes them (API 3, FAR's `PanelControl`): either panel's
//! state and items ([`NcHost::panel_state`](crate::NcHost::panel_state),
//! [`NcHost::panel_items`](crate::NcHost::panel_items)) and what is done to it, its command line, the viewer
//! and the editor ([`NcHost::panel_do`](crate::NcHost::panel_do)).

use crate::NcStr;

/// The active panel (FAR's `PANEL_ACTIVE`).
pub const WHICH_ACTIVE: u32 = 0;
/// The other one (FAR's `PANEL_PASSIVE`).
pub const WHICH_PASSIVE: u32 = 1;
/// The left panel.
pub const WHICH_LEFT: u32 = 2;
/// The right panel.
pub const WHICH_RIGHT: u32 = 3;

/// The left panel (FAR's `PFLAGS_PANELLEFT`).
pub const STATE_LEFT: u32 = 0x1;
/// The active one (`PFLAGS_FOCUS`).
pub const STATE_ACTIVE: u32 = 0x2;
/// Shown (`PFLAGS_VISIBLE`).
pub const STATE_VISIBLE: u32 = 0x4;
/// A plugin's panel (`PFLAGS_PLUGIN`): its folder is the plugin's own path.
pub const STATE_PLUGIN: u32 = 0x8;
/// The items are files on a disk (`PFLAGS_REALNAMES`): always on a disk's panel, on a plugin's when its
/// items are.
pub const STATE_REAL_NAMES: u32 = 0x10;
/// Hidden and system files shown (`PFLAGS_SHOWHIDDEN`).
pub const STATE_SHOW_HIDDEN: u32 = 0x20;
/// Sorted the other way (`PFLAGS_REVERSESORTORDER`).
pub const STATE_REVERSE_SORT: u32 = 0x40;
/// The selected items first (`PFLAGS_SELECTEDFIRST`).
pub const STATE_SELECTED_FIRST: u32 = 0x80;
/// The panel the call running is for: the drive menu's that chose the plugin's item (FAR's
/// `OPEN_LEFTDISKMENU` / `OPEN_RIGHTDISKMENU`), the active one for other calls.
pub const STATE_CALL_TARGET: u32 = 0x100;
/// Its folder is still being read: the items are those shown meanwhile.
pub const STATE_READING: u32 = 0x200;

/// A panel of files (FAR's `PTYPE_FILEPANEL`).
pub const KIND_FILES: u32 = 0;
/// The folder tree (`PTYPE_TREEPANEL`).
pub const KIND_TREE: u32 = 1;
/// The quick view (`PTYPE_QVIEWPANEL`).
pub const KIND_QUICK_VIEW: u32 = 2;
/// The information panel (`PTYPE_INFOPANEL`).
pub const KIND_INFO: u32 = 3;

/// A panel as it is now (FAR's `PanelInfo` and `FCTL_GETPANELDIRECTORY`); texts the host's, valid until
/// the plugin's next call to the host.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcPanelState {
    /// `STATE_*`.
    pub flags: u32,
    /// `KIND_*`: what the panel's side shows (the files under a tree or an info panel are still read).
    pub kind: u32,
    /// The folder: a disk's path, or on a plugin's panel the plugin's own path (`/` its root).
    pub folder: NcStr,
    /// The plugin whose panel it is; empty for a disk's.
    pub plugin: NcStr,
    /// The disk's folder under a plugin's panel (FAR's `GetCurDir`); the folder itself on a disk's.
    pub disk_folder: NcStr,
    /// Its items, `..` first where it has one (FAR's `ItemsNumber`).
    pub items: usize,
    /// The selected ones (FAR's `SelectedItemsNumber`, which counts the current item when none is).
    pub selected: usize,
    /// The item under the cursor and the first one shown.
    pub current: usize,
    pub top: usize,
    /// The sort (`SORT_*` of the plugin's own panels, FAR's `SM_*`) and the view mode (0-9, Ctrl+0..9).
    pub sort: u32,
    pub view_mode: u32,
    /// The view mode's columns as the modes name them ("N,S,D"; FAR's `FCTL_GETCOLUMNTYPES`) and their
    /// widths as drawn now ("20,10,8"; `FCTL_GETCOLUMNWIDTHS`); empty for a panel showing no files.
    pub columns: NcStr,
    pub widths: NcStr,
}

impl NcPanelState {
    pub const EMPTY: Self = Self {
        flags: 0,
        kind: KIND_FILES,
        folder: NcStr::EMPTY,
        plugin: NcStr::EMPTY,
        disk_folder: NcStr::EMPTY,
        items: 0,
        selected: 0,
        current: 0,
        top: 0,
        sort: 0,
        view_mode: 0,
        columns: NcStr::EMPTY,
        widths: NcStr::EMPTY,
    };
}

/// The item is selected (FAR's `PPIF_SELECTED`).
pub const ITEM_SELECTED: u32 = 0x1;
/// A folder (`FILE_ATTRIBUTE_DIRECTORY`).
pub const ITEM_FOLDER: u32 = 0x2;
/// The `..` at the top.
pub const ITEM_PARENT: u32 = 0x4;
/// [`NcHost::panel_items`](crate::NcHost::panel_items): the selected items only, or the current one
/// when none is and it is not `..` (FAR's `FCTL_GETSELECTEDPANELITEM`).
pub const ITEMS_SELECTED: u32 = 0x1;

/// One item of a panel (FAR's `PluginPanelItem`); texts the host's, valid until the plugin's next call
/// to the host.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcPanelItem {
    pub name: NcStr,
    /// `ITEM_*`.
    pub flags: u32,
    /// The file's attributes as the system gives them (Windows' `FILE_ATTRIBUTE_*`).
    pub attributes: u32,
    pub size: u64,
    /// Its space on the disk (FAR's `AllocationSize`), 0 when not known.
    pub allocated: u64,
    /// Milliseconds since 1970 as the entries' (`TIME_NONE`: not known).
    pub modified: i64,
    pub created: i64,
    pub accessed: i64,
    /// The last change of its data or attributes (FAR's `ChangeTime`).
    pub changed: i64,
    /// Its hard links, 0 when not read (FAR's `NumberOfLinks`).
    pub links: u32,
    /// Its file on a disk: the whole path (a disk panel's folder and the name, a plugin's real name);
    /// empty for a plugin's item that is none.
    pub path: NcStr,
    /// The description column's text (FAR's `Description`) and the owner's (`Owner`).
    pub description: NcStr,
    pub owner: NcStr,
}

impl NcPanelItem {
    pub const EMPTY: Self = Self {
        name: NcStr::EMPTY,
        flags: 0,
        attributes: 0,
        size: 0,
        allocated: 0,
        modified: crate::TIME_NONE,
        created: crate::TIME_NONE,
        accessed: crate::TIME_NONE,
        changed: crate::TIME_NONE,
        links: 0,
        path: NcStr::EMPTY,
        description: NcStr::EMPTY,
        owner: NcStr::EMPTY,
    };
}

/// The panel to the folder `text` (FAR's `FCTL_SETPANELDIRECTORY`): a disk's path leaves a plugin's
/// panels; on a plugin's panel a path of its own (starting with `/`) stays in it; a relative one is the
/// panel's folder's. The call returns once
/// the folder is read (0) or could not be (-1); `names`' first, when given, is the item the cursor goes to.
pub const DO_SET_FOLDER: u32 = 1;
/// The cursor to the item `number` (or, `text` given, the one of that name) and the top to `number2`
/// (`usize::MAX`: as the panel puts it) (FAR's `FCTL_REDRAWPANEL` with its `PanelRedrawInfo`).
pub const DO_SET_CURSOR: u32 = 2;
/// The folder read again (FAR's `FCTL_UPDATEPANEL`); returns once it is. With [`DO_KEEP_SELECTION`] the
/// selection stays (FAR's `Param1` 1).
pub const DO_REREAD: u32 = 3;
/// The items `names` (by name) selected, or with [`DO_OFF`] unselected (FAR's `FCTL_SETSELECTION`
/// between `FCTL_BEGINSELECTION` and `FCTL_ENDSELECTION`); with [`DO_ALL`] every item but `..`.
pub const DO_SELECT: u32 = 4;
/// The sort `number` (`SORT_*`), the other way with [`DO_REVERSE`] (FAR's `FCTL_SETSORTMODE`,
/// `FCTL_SETSORTORDER`).
pub const DO_SET_SORT: u32 = 5;
/// A plugin's panel closed (FAR's `FCTL_CLOSEPANEL`): the panel goes to the disk's folder `text`, or
/// back where it came from when `text` is empty.
pub const DO_CLOSE: u32 = 6;
/// The command line's text: answered in `answer` (FAR's `FCTL_GETCMDLINE`).
pub const DO_GET_COMMAND_LINE: u32 = 7;
/// The command line set to `text` (FAR's `FCTL_SETCMDLINE`).
pub const DO_SET_COMMAND_LINE: u32 = 8;
/// `text` put into the command line at its cursor (FAR's `FCTL_INSERTCMDLINE`).
pub const DO_INSERT_COMMAND_LINE: u32 = 9;
/// The viewer on the file `text`, its title `names`' first when given (FAR's `Viewer` with
/// `VF_NONMODAL`): the call returns as the viewer opens (-1 when the viewer or the editor is up already,
/// or other work is waited for); with [`DO_DELETE_ON_CLOSE`] the file goes when it
/// closes (`VF_DELETEONCLOSE`).
pub const DO_VIEW: u32 = 10;
/// The editor on the file `text` at line `number` and column `number2` (1-based, 0: its start) (FAR's
/// `Editor`, not modal); [`DO_DELETE_ON_CLOSE`] as for the viewer.
pub const DO_EDIT: u32 = 11;

/// `text` run as the command line runs it, Enter pressed on it, in the program's terminal (FAR's
/// farcmds runs it on the user screen, `FCTL_GETUSERSCREEN` / `FCTL_SETUSERSCREEN`): returns as it starts
/// (-1 when it cannot: other work is waited for).
pub const DO_RUN_COMMAND: u32 = 12;

/// [`DO_REREAD`]'s flag: the selection kept.
pub const DO_KEEP_SELECTION: u32 = 0x1;
/// [`DO_SELECT`]'s flag: unselected.
pub const DO_OFF: u32 = 0x2;
/// [`DO_SET_SORT`]'s flag: the other way.
pub const DO_REVERSE: u32 = 0x4;
/// [`DO_VIEW`]'s and [`DO_EDIT`]'s flag: the file deleted as the window closes.
pub const DO_DELETE_ON_CLOSE: u32 = 0x8;
/// [`DO_SELECT`]'s flag: every item, not the names.
pub const DO_ALL: u32 = 0x10;

/// The program's screen ([`NcHost::screen`](crate::NcHost::screen)).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcScreen {
    pub columns: u32,
    pub rows: u32,
    /// The console's window (Windows' `HWND`, for a menu of the system's to belong to); 0: none.
    pub window: usize,
}

/// What [`NcHost::panel_do`](crate::NcHost::panel_do) does: `action` (`DO_*`) with its arguments.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcPanelAction {
    pub action: u32,
    /// `DO_*` flags of the action.
    pub flags: u32,
    pub text: NcStr,
    pub number: usize,
    pub number2: usize,
    /// `name_count` texts.
    pub names: *const NcStr,
    pub name_count: usize,
}

impl NcPanelAction {
    pub const fn new(action: u32) -> Self {
        Self {
            action,
            flags: 0,
            text: NcStr::EMPTY,
            number: 0,
            number2: 0,
            names: core::ptr::null(),
            name_count: 0,
        }
    }
}
