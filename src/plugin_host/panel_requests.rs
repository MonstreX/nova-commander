//! What a plugin asks of the program's panels (API 3, FAR's `PanelControl`), as the program reads it: the
//! plugin's thread waits for the answer, the screen does not (ADR-0019). A request for a panel whose
//! folder is being read is answered once it is read, as FAR's calls find the panel read.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// The left and the right panel as the program last saw them: what a plugin reads in a panel's call.
pub type PanelsSeen = Arc<Mutex<[PanelState; 2]>>;

/// Which panel (FAR's `PANEL_ACTIVE` / `PANEL_PASSIVE`, and either side by its place).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Active,
    Passive,
    Left,
    Right,
}

/// What its side shows (FAR's `PTYPE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelKind {
    #[default]
    Files,
    Tree,
    QuickView,
    Info,
}

/// A panel as it is (FAR's `PanelInfo`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelState {
    pub left: bool,
    pub active: bool,
    pub visible: bool,
    /// The plugin whose panel it is; none for a disk's.
    pub plugin: Option<String>,
    pub real_names: bool,
    pub show_hidden: bool,
    pub reverse_sort: bool,
    pub selected_first: bool,
    /// The panel the call running is for.
    pub call_target: bool,
    pub reading: bool,
    pub kind: PanelKind,
    /// A disk's path, or a plugin's own path.
    pub folder: String,
    /// The disk's folder under the panel (FAR's `GetCurDir`).
    pub disk_folder: String,
    pub items: usize,
    pub selected: usize,
    pub current: usize,
    pub top: usize,
    /// FAR's `SM_*` code of the panel's sort.
    pub sort: u32,
    pub view_mode: u32,
    /// The view mode's column types ("N,S,D") and their widths as drawn ("20,10,8").
    pub columns: String,
    pub widths: String,
}

/// One item of a panel (FAR's `PluginPanelItem`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelItem {
    pub name: String,
    pub selected: bool,
    pub folder: bool,
    pub parent: bool,
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

/// What is done (FAR's `FCTL_*`, `Viewer`, `Editor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelAction {
    /// The panel to `folder`, the cursor on `cursor` when given; answered once read.
    SetFolder {
        folder: String,
        cursor: Option<String>,
    },
    /// The cursor to the item `index` or the one named `name`, the top to `top`.
    SetCursor {
        index: usize,
        name: Option<String>,
        top: Option<usize>,
    },
    /// The folder read again; answered once read.
    Reread {
        keep_selection: bool,
    },
    /// The items named (`all`: every one but `..`) selected or not.
    Select {
        names: Vec<String>,
        on: bool,
        all: bool,
    },
    /// FAR's `SM_*` code, the other way or not.
    SetSort {
        sort: u32,
        reverse: bool,
    },
    /// A plugin's panel closed, to the disk's `folder` or back where it came from.
    Close {
        folder: Option<String>,
    },
    GetCommandLine,
    SetCommandLine(String),
    InsertCommandLine(String),
    /// The text run as the command line runs it (FAR's farcmds on the user screen).
    RunCommand(String),
    View {
        path: String,
        title: Option<String>,
        delete_on_close: bool,
    },
    Edit {
        path: String,
        line: usize,
        column: usize,
        delete_on_close: bool,
    },
}

/// A request and its panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelRequest {
    State(Which),
    /// The items, or (`selected`) the selected ones or the current one when none is.
    Items {
        which: Which,
        selected: bool,
    },
    Do(Which, PanelAction),
    /// Keys by their names, done as typed once the plugin's call has returned (FAR's
    /// `ACTL_POSTKEYSEQUENCE`).
    PostKeys(String),
    /// The screen's size (FAR's `ACTL_GETFARRECT`).
    Screen,
    /// Whether each entry passes the panel's filter (FAR's `FFT_PANEL`, `FFCTL_ISFILEINFILTER`).
    Filter {
        which: Which,
        entries: Vec<crate::vfs::Entry>,
    },
    /// The text on the clipboard (FAR's `CopyToClipboard`).
    Clipboard(String),
    /// The plugin's help at a topic (FAR's `ShowHelp`).
    Help(String),
    /// A plugin found, loaded or unloaded, by its name or a path of its (FAR's `PluginsControl`).
    Plugins(PluginsAction, String),
    /// Whether the program asks before a work, by FAR's name of its confirmation (`FSSF_CONFIRMATIONS`).
    Confirmation(String),
    /// Whether a file name matches masks in the program's masks (FAR's `ProcessName`, `PN_CMPNAMELIST`),
    /// answered as [`PanelAnswer::Confirms`]: none for masks with errors.
    MatchMasks {
        masks: String,
        name: String,
    },
    /// A copy done as one of the program's background tasks (API 9's `NcHost::transfer`).
    Transfer(Box<TransferRequest>),
}

/// [`PanelRequest::Transfer`]'s copy: the entries `names` of the plugin's panel `instance`'s `folder`
/// into the disk's folder `disk`, or (`upload`) the disk's entries `names` of `disk` into `folder`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferRequest {
    pub instance: String,
    pub folder: String,
    pub names: Vec<String>,
    pub disk: String,
    pub upload: bool,
    pub moving: bool,
}

/// What [`PanelRequest::Plugins`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginsAction {
    Find,
    Load,
    Unload,
}

/// The program's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelAnswer {
    State(Box<PanelState>),
    Items(Vec<PanelItem>),
    Text(String),
    /// Done (true) or not.
    Done(bool),
    Screen {
        columns: u16,
        rows: u16,
    },
    /// Each entry asked about: whether the filter takes it.
    Matches(Vec<bool>),
    /// Whether the program asks before the work; none: a name it does not know.
    Confirms(Option<bool>),
}
