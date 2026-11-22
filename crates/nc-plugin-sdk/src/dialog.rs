//! A plugin's dialog (ADR-0016 §11): its items in rows, laid out and drawn by the program as FAR's
//! dialogs are (or at their own coordinates in a dialog given a size), and a handler hearing its events on
//! the plugin's thread while it is up - the dialog opening and drawn, an item changed (each key), the focus
//! moving, a button that does not close pressed, the dialog closing (the handler may keep it open, after
//! showing a message about what is wrong, say), and with [`Dialog::hear_keys`] each key and click before
//! the dialog does it. The handler sees every item's value as the event came and changes items through
//! [`Controls`]; its changes show when it returns. Its `false` stops what the event would do: a closing
//! dialog stays, a key or click is not done.

use crate::api;
use crate::host::flag;

/// What a dialog item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemKind {
    #[default]
    Text,
    Edit,
    /// An edit line showing `*` for each character.
    Password,
    Check,
    Radio,
    /// A drop-down list.
    List,
    Separator,
    Button,
    /// FAR's `DI_FIXEDIT`: a line typed through its mask (`9` a digit, `N` a digit or a space, `#` a
    /// digit, a space or a minus, `A` a letter, `H` a hex digit, `X` any), or of its width.
    Fixed,
    /// FAR's `DI_LISTBOX`: its lines in a box in the dialog, its text the box's title; its state the line
    /// under the cursor. Enter on it closes the dialog with it unless [`DialogItem::no_close`].
    ListBox,
    /// FAR's `DI_SINGLEBOX` in a dialog given a size: a frame at its place, its text the title; one row
    /// high a line across with the title in its middle.
    Box,
}

/// One item of a [`Dialog`]: `&` in a label marks its hotkey.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialogItem {
    pub kind: ItemKind,
    /// A label or an edit line's text.
    pub text: String,
    /// A check box's state (0 off, 1 on, 2 `[?]`), a radio button's (0, 1), a list's chosen line.
    pub state: i32,
    /// An edit line's or a list's columns; 0: as wide as the dialog lets it.
    pub width: u32,
    /// A list's lines.
    pub lines: Vec<String>,
    pub disabled: bool,
    pub hidden: bool,
    /// On the same row as the item before it.
    pub same_row: bool,
    /// A radio button starting a new group.
    pub group: bool,
    /// The button Enter presses anywhere but on another button.
    pub default: bool,
    /// A button the handler hears without the dialog closing.
    pub no_close: bool,
    /// A check box with FAR's third state `[?]`.
    pub three_state: bool,
    /// The item with the focus when the dialog opens.
    pub focus: bool,
    /// An edit line's history (FAR's `DIF_HISTORY`; empty: none).
    pub history: String,
    /// An empty line with a history starts with its last one (`DIF_USELASTHISTORY`).
    pub use_last_history: bool,
    /// A path, completed as the dialogs' lines are (`DIF_EDITPATH`).
    pub edit_path: bool,
    /// An edit line read only (`DIF_READONLY`).
    pub read_only: bool,
    /// A fixed line's mask.
    pub mask: String,
    /// A list box's lines shown (0: as many as it has, up to 8).
    pub height: u32,
    /// FAR's `X1`, `Y1`, `X2`, `Y2` inside a dialog given a size ([`Dialog::size`]).
    pub place: (i32, i32, i32, i32),
}

impl DialogItem {
    fn of(kind: ItemKind, text: &str, state: i32) -> Self {
        Self {
            kind,
            text: text.to_string(),
            state,
            ..Self::default()
        }
    }

    pub fn text(label: &str) -> Self {
        Self::of(ItemKind::Text, label, 0)
    }

    pub fn edit(text: &str) -> Self {
        Self::of(ItemKind::Edit, text, 0)
    }

    pub fn password(text: &str) -> Self {
        Self::of(ItemKind::Password, text, 0)
    }

    pub fn check(label: &str, checked: bool) -> Self {
        Self::of(ItemKind::Check, label, i32::from(checked))
    }

    /// A check box with the third state: `None` is `[?]`.
    pub fn check3(label: &str, state: Option<bool>) -> Self {
        let state = state.map_or(2, i32::from);
        Self {
            three_state: true,
            ..Self::of(ItemKind::Check, label, state)
        }
    }

    pub fn radio(label: &str, checked: bool) -> Self {
        Self::of(ItemKind::Radio, label, i32::from(checked))
    }

    pub fn list(lines: &[&str], chosen: usize) -> Self {
        Self {
            lines: lines.iter().map(ToString::to_string).collect(),
            ..Self::of(ItemKind::List, "", i32::try_from(chosen).unwrap_or(0))
        }
    }

    pub fn separator() -> Self {
        Self::of(ItemKind::Separator, "", 0)
    }

    /// A line typed through `mask`, starting with `text` (FAR's `DI_FIXEDIT` with `DIF_MASKEDIT`).
    pub fn fixed(mask: &str, text: &str) -> Self {
        Self {
            mask: mask.to_string(),
            ..Self::of(ItemKind::Fixed, text, 0)
        }
    }

    /// A list in a box under `title`, the cursor on `cursor`.
    pub fn list_box(title: &str, lines: &[&str], cursor: usize) -> Self {
        Self {
            lines: lines.iter().map(ToString::to_string).collect(),
            ..Self::of(ItemKind::ListBox, title, i32::try_from(cursor).unwrap_or(0))
        }
    }

    pub fn button(label: &str) -> Self {
        Self::of(ItemKind::Button, label, 0)
    }

    /// A single frame titled `title` (FAR's `DI_SINGLEBOX`), placed by [`DialogItem::at`].
    pub fn single_box(title: &str) -> Self {
        Self::of(ItemKind::Box, title, 0)
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    #[must_use]
    pub fn same_row(mut self) -> Self {
        self.same_row = true;
        self
    }

    #[must_use]
    pub fn group(mut self) -> Self {
        self.group = true;
        self
    }

    /// The button Enter presses anywhere but on another button.
    #[must_use]
    pub fn default_button(mut self) -> Self {
        self.default = true;
        self
    }

    #[must_use]
    pub fn no_close(mut self) -> Self {
        self.no_close = true;
        self
    }

    #[must_use]
    pub fn width(mut self, width: u32) -> Self {
        self.width = width;
        self
    }

    #[must_use]
    pub fn focus(mut self) -> Self {
        self.focus = true;
        self
    }

    /// An edit line's history `name`; `use_last`: an empty line starts with its last one.
    #[must_use]
    pub fn history(mut self, name: &str, use_last: bool) -> Self {
        self.history = name.to_string();
        self.use_last_history = use_last;
        self
    }

    #[must_use]
    pub fn edit_path(mut self) -> Self {
        self.edit_path = true;
        self
    }

    /// An edit line that cannot be changed (FAR's `DIF_READONLY`).
    #[must_use]
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// A list box's lines shown.
    #[must_use]
    pub fn height(mut self, rows: u32) -> Self {
        self.height = rows;
        self
    }

    /// At FAR's `X1`, `Y1`, `X2`, `Y2` in a dialog given a size.
    #[must_use]
    pub fn at(mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> Self {
        self.place = (x1, y1, x2, y2);
        self
    }

    fn kind_code(&self) -> u32 {
        match self.kind {
            ItemKind::Text => api::ITEM_TEXT,
            ItemKind::Edit => api::ITEM_EDIT,
            ItemKind::Password => api::ITEM_PASSWORD,
            ItemKind::Check => api::ITEM_CHECK,
            ItemKind::Radio => api::ITEM_RADIO,
            ItemKind::List => api::ITEM_LIST,
            ItemKind::Separator => api::ITEM_SEPARATOR,
            ItemKind::Button => api::ITEM_BUTTON,
            ItemKind::Fixed => api::ITEM_FIXED,
            ItemKind::ListBox => api::ITEM_LIST_BOX,
            ItemKind::Box => api::ITEM_BOX,
        }
    }

    fn flags(&self) -> u32 {
        flag(self.disabled, api::ITEM_DISABLED)
            | flag(self.hidden, api::ITEM_HIDDEN)
            | flag(self.same_row, api::ITEM_SAME_ROW)
            | flag(self.group, api::ITEM_GROUP)
            | flag(self.default, api::ITEM_DEFAULT)
            | flag(self.no_close, api::ITEM_NO_CLOSE)
            | flag(self.three_state, api::ITEM_THREE_STATE)
            | flag(self.focus, api::ITEM_FOCUS)
            | flag(self.use_last_history, api::ITEM_USE_LAST_HISTORY)
            | flag(self.edit_path, api::ITEM_EDIT_PATH)
            | flag(self.read_only, api::ITEM_READ_ONLY)
    }
}

/// A dialog: its title, its items in their order, the warning colours.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dialog {
    pub title: String,
    pub items: Vec<DialogItem>,
    pub warning: bool,
    /// The handler hears each key and click first ([`DialogEvent::Key`], [`DialogEvent::Mouse`]).
    pub keys: bool,
    /// FAR's `X1`, `Y1` on the screen (none: centred).
    pub x: Option<u16>,
    pub y: Option<u16>,
    /// Its columns and rows: given, the items stand at their own places ([`DialogItem::at`]).
    pub size: Option<(u32, u32)>,
}

impl Dialog {
    pub fn new(title: &str, items: Vec<DialogItem>) -> Self {
        Self {
            title: title.to_string(),
            items,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn warning(mut self) -> Self {
        self.warning = true;
        self
    }

    /// The handler hears each key and click before the dialog does it (FAR's `DN_CONTROLINPUT`).
    #[must_use]
    pub fn hear_keys(mut self) -> Self {
        self.keys = true;
        self
    }

    /// At column `x`, row `y` of the screen.
    #[must_use]
    pub fn at(mut self, x: u16, y: u16) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self
    }

    /// `width` x `height` with its margin, the items at their own places.
    #[must_use]
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = Some((width, height));
        self
    }
}

/// What the handler hears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogEvent {
    /// The dialog is up.
    Init,
    /// The item's value changed.
    Changed(usize),
    /// A button that does not close the dialog was pressed.
    Button(usize),
    /// The dialog is closing by this button, or Esc (`None`): the handler's `false` keeps it open.
    Close(Option<usize>),
    /// A key ("F4", "Ctrl+Enter", "a") in the focused item: the handler's `false` takes it.
    Key { item: Option<usize>, key: String },
    /// A click ("MsLClick", "MsRClick", "MsM1Click", "MsLDblClick") at column `x`, row `y` of the
    /// dialog, on an item or none: the handler's `false` takes it.
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
    /// The dialog is drawn anew, on a screen of these columns and rows.
    Draw { columns: i32, rows: i32 },
}

/// An item's value: its state (as [`DialogItem::state`]) and its text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Value {
    pub state: i32,
    pub text: String,
}

impl Value {
    pub fn checked(&self) -> bool {
        self.state == 1
    }
}

/// How a dialog ended: the button that closed it (`None`: Esc, or the dialog gone otherwise) and every
/// item's last value (none when it went otherwise).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Closed {
    pub button: Option<usize>,
    pub values: Vec<Value>,
}

#[path = "dialog_run.rs"]
mod run;

pub use run::Controls;

#[cfg(test)]
#[path = "dialog_tests.rs"]
mod tests;
