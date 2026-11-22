//! The windows a plugin asks the program for (ADR-0016 5b-1), as the program shows them: FAR's `Message`,
//! `Menu` and `InputBox`, and what the user answered.

/// A message window: its lines, its buttons (`&` before a hotkey letter), the warning colours, lines from
/// the left instead of centred.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageRequest {
    pub title: String,
    pub lines: Vec<String>,
    pub buttons: Vec<String>,
    pub warning: bool,
    pub left_align: bool,
    /// The system's text of the plugin's last error (FAR's `FMSG_ERRORTYPE`), wrapped under the lines.
    pub error: Option<String>,
}

/// A line of a plugin's menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuLine {
    /// `&` before the hotkey letter.
    pub text: String,
    pub separator: bool,
    pub checked: bool,
    /// FAR's `MIF_DISABLE`: grey, passed over.
    pub disabled: bool,
    /// `MIF_GRAYED`: grey, the cursor stops, Enter does nothing.
    pub grayed: bool,
    /// `MIF_HIDDEN`: not shown.
    pub hidden: bool,
}

/// A plugin's menu: its lines, the cursor's first line, the keys that close it besides Enter and Esc (as
/// the plugin names them: "F4", "Shift+Del").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuRequest {
    pub title: String,
    pub bottom: String,
    pub lines: Vec<MenuLine>,
    pub selected: usize,
    pub break_keys: Vec<String>,
    pub auto_hotkeys: bool,
    /// FAR's `FMENU_WRAPMODE`.
    pub wrap: bool,
    /// `FMENU_SHOWAMPERSAND`.
    pub show_ampersand: bool,
    /// `FMENU_REVERSEAUTOHIGHLIGHT`.
    pub reverse_auto_hotkeys: bool,
    /// FAR's `X`, `Y`: the menu's left column and top row (none: centred).
    pub x: Option<u16>,
    pub y: Option<u16>,
    /// FAR's `MaxHeight`: the most lines shown (none: as the screen takes).
    pub max_height: Option<usize>,
}

/// An input line under its prompt, starting with `text`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputRequest {
    pub title: String,
    pub prompt: String,
    pub text: String,
    /// FAR's `FIB_ENABLEEMPTY`: an empty answer taken (else it is Esc).
    pub enable_empty: bool,
    /// `FIB_PASSWORD`: stars.
    pub password: bool,
    /// `FIB_EXPANDENV`: `%NAME%` in the answer replaced.
    pub expand_env: bool,
    /// `FIB_BUTTONS`: "OK" and "Cancel".
    pub buttons: bool,
    /// `FIB_NOAMPERSAND`: `&` in the prompt marks a hotkey.
    pub no_ampersand: bool,
    /// FAR's `HistoryName`: the line's history (`DIF_HISTORY`).
    pub history: Option<String>,
    /// `FIB_NOUSELASTHISTORY`: an empty line does not start with the history's last.
    pub no_last_history: bool,
    /// `FIB_EDITPATH`: a path, completed as the dialogs' lines are.
    pub edit_path: bool,
}

/// What a plugin waits for the user to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Message(MessageRequest),
    Menu(MenuRequest),
    Input(InputRequest),
}

/// The user's answer to a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Esc, or the window gone another way.
    Esc,
    /// A message's button.
    Button(usize),
    /// A menu closed by Enter (`key` none) or a break key: the line under the cursor (none in an empty
    /// menu) and the key's index.
    Chosen {
        item: Option<usize>,
        key: Option<usize>,
    },
    /// An input line's text.
    Text(String),
}

/// A dialog item's kind (ADR-0016 §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemKind {
    #[default]
    Text,
    Edit,
    Password,
    Check,
    Radio,
    List,
    Separator,
    Button,
    /// FAR's `DI_FIXEDIT`: through its mask, or a line of its width.
    Fixed,
    /// FAR's `DI_LISTBOX`.
    ListBox,
    /// FAR's `DI_SINGLEBOX` (API 8): a frame, or one row high a line with its title.
    Box,
}

/// One item of a plugin's dialog as it asked for it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialogItem {
    pub kind: ItemKind,
    /// A label (`&` before the hotkey letter) or an edit line's text.
    pub text: String,
    /// A check box's state (0 off, 1 on, 2 `[?]`), a radio button's (0, 1), a list's chosen line.
    pub state: i32,
    /// An edit line's or a list's columns; 0: as wide as the dialog lets it.
    pub width: u16,
    pub lines: Vec<String>,
    pub disabled: bool,
    pub hidden: bool,
    pub same_row: bool,
    pub group: bool,
    pub default: bool,
    pub no_close: bool,
    pub three_state: bool,
    pub focus: bool,
    /// FAR's `DIF_HISTORY` with its name.
    pub history: Option<String>,
    /// `DIF_USELASTHISTORY`.
    pub use_last_history: bool,
    /// `DIF_EDITPATH`.
    pub edit_path: bool,
    /// `DIF_READONLY` (API 5).
    pub read_only: bool,
    /// A fixed line's mask (empty: none).
    pub mask: String,
    /// A list box's lines shown (0: as many as it has, up to 8).
    pub height: u16,
    /// FAR's `X1`, `Y1`, `X2`, `Y2`, read in a placed dialog.
    pub place: (i32, i32, i32, i32),
}

/// A plugin's dialog: its items and whether a handler hears its events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialogRequest {
    pub title: String,
    pub warning: bool,
    pub items: Vec<DialogItem>,
    pub handled: bool,
    /// The handler hears the keys and clicks first (FAR's `DN_CONTROLINPUT`).
    pub keys: bool,
    /// FAR's `X1`, `Y1` on the screen (none: centred).
    pub x: Option<u16>,
    pub y: Option<u16>,
    /// Columns and rows: the items at their own places (none: laid out by the program).
    pub size: Option<(u16, u16)>,
}

/// An item's value: its state and its text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemValue {
    pub state: i32,
    pub text: String,
}

/// What the handler hears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogEventKind {
    Init,
    /// The item's value changed.
    Changed(usize),
    /// A button that does not close the dialog.
    Button(usize),
    /// Closing by a button, or Esc (none).
    Close(Option<usize>),
    /// A key named as the program names keys, in the focused item.
    Key {
        item: Option<usize>,
        key: String,
    },
    /// A click ("MsLClick") at column `x`, row `y` of the dialog, on an item or none.
    Mouse {
        item: Option<usize>,
        key: String,
        x: i32,
        y: i32,
    },
    /// The focus came to the item.
    Focus(usize),
    /// The focus left the item.
    Unfocus(usize),
    /// Drawn anew on a screen of these columns and rows.
    Draw {
        columns: u16,
        rows: u16,
    },
}

/// A change the handler asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Text(usize, String),
    State(usize, i32),
    Enabled(usize, bool),
    Visible(usize, bool),
    Focus(usize),
    /// A list's lines (FAR's `DM_LISTSET`).
    Lines(usize, Vec<String>),
}

impl Change {
    /// The item it changes.
    pub fn item(&self) -> usize {
        match self {
            Self::Text(item, _)
            | Self::State(item, _)
            | Self::Enabled(item, _)
            | Self::Visible(item, _)
            | Self::Focus(item)
            | Self::Lines(item, _) => *item,
        }
    }
}

/// The handler's answer: its changes, whether a closing dialog stays open, whether it took a key or a
/// click.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Handled {
    pub changes: Vec<Change>,
    pub keep_open: bool,
    pub taken: bool,
}

/// What the program tells a plugin's thread waiting in its dialog.
#[derive(Debug)]
pub enum DialogMessage {
    /// An event for the handler with every item's value now; the answer goes to `reply`.
    Event {
        kind: DialogEventKind,
        values: Vec<ItemValue>,
        reply: std::sync::mpsc::Sender<Handled>,
    },
    /// The dialog closed by a button (none: Esc) with every item's last value.
    Closed {
        button: Option<usize>,
        values: Vec<ItemValue>,
    },
}
