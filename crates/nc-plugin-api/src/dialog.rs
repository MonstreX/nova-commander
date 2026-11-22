//! A plugin's dialog in the ABI (ADR-0016 §11, FAR's `DialogInit` / `DialogRun`): its items, their
//! values, the handler's events and the changes it asks for.

use core::ffi::c_void;

use crate::NcStr;

/// A dialog in the warning colours.
pub const DIALOG_WARNING: u32 = 1;
/// The handler hears each key and mouse click before the dialog does ([`EVENT_KEY`], [`EVENT_MOUSE`];
/// FAR's `DN_CONTROLINPUT`), the dialog waiting for its answer.
pub const DIALOG_KEYS: u32 = 2;

/// A dialog item's kinds ([`NcDialogItem::kind`]).
pub const ITEM_TEXT: u32 = 0;
/// An edit line: its text in [`NcDialogItem::text`].
pub const ITEM_EDIT: u32 = 1;
/// An edit line showing `*` for each character.
pub const ITEM_PASSWORD: u32 = 2;
/// A check box: its state 0 off, 1 on, 2 FAR's `[?]` (with [`ITEM_THREE_STATE`]).
pub const ITEM_CHECK: u32 = 3;
/// A radio button: its state 0 or 1; [`ITEM_GROUP`] starts a new group.
pub const ITEM_RADIO: u32 = 4;
/// A drop-down list of [`NcDialogItem::lines`]: its state the line chosen.
pub const ITEM_LIST: u32 = 5;
pub const ITEM_SEPARATOR: u32 = 6;
pub const ITEM_BUTTON: u32 = 7;
/// FAR's `DI_FIXEDIT`: a line typed through [`NcDialogItem::mask`] (FAR's `DIF_MASKEDIT`: `9` a digit,
/// `N` a digit or a space, `#` a digit, a space or a minus, `A` a letter, `H` a hex digit, `X` any
/// character; the others stand where they are); without a mask a line of the item's width.
pub const ITEM_FIXED: u32 = 8;
/// FAR's `DI_LISTBOX`: [`NcDialogItem::lines`] in a box in the dialog, [`NcDialogItem::text`] its
/// title; its state the line under the cursor. Enter on it closes the dialog with it (as a button) unless
/// [`ITEM_NO_CLOSE`].
pub const ITEM_LIST_BOX: u32 = 9;
/// FAR's `DI_SINGLEBOX` (API 8): a frame from `x1`, `y1` to `x2`, `y2` in a dialog given a size, its
/// text the title in its top's middle; one row high (`y1 == y2`) a line across with the title in it.
/// It takes no focus and has no value.
pub const ITEM_BOX: u32 = 10;

/// A dialog item's flags ([`NcDialogItem::flags`]).
pub const ITEM_DISABLED: u32 = 1;
pub const ITEM_HIDDEN: u32 = 2;
/// On the same row as the item before it.
pub const ITEM_SAME_ROW: u32 = 4;
/// A radio button starting a new group.
pub const ITEM_GROUP: u32 = 8;
/// The button Enter presses anywhere but on another button.
pub const ITEM_DEFAULT: u32 = 16;
/// A button that does not close the dialog (FAR's `DIF_BTNNOCLOSE`): the handler hears it; a list box
/// Enter does not close (`DIF_LISTNOCLOSE`).
pub const ITEM_NO_CLOSE: u32 = 32;
/// A check box with FAR's third state.
pub const ITEM_THREE_STATE: u32 = 64;
/// The item with the focus when the dialog opens.
pub const ITEM_FOCUS: u32 = 128;
/// An empty edit line with a [`NcDialogItem::history`] starts with its last line (FAR's
/// `DIF_USELASTHISTORY`).
pub const ITEM_USE_LAST_HISTORY: u32 = 256;
/// An edit line of a path (FAR's `DIF_EDITPATH`): completed as the dialogs' lines are.
pub const ITEM_EDIT_PATH: u32 = 512;
/// An edit line read only (FAR's `DIF_READONLY`): its text selected and scrolled, never changed (API 5).
pub const ITEM_READ_ONLY: u32 = 1024;

/// One item of a dialog.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcDialogItem {
    /// `ITEM_*` kinds.
    pub kind: u32,
    /// `ITEM_*` flags.
    pub flags: u32,
    /// A label (`&` before the hotkey letter), an edit line's text, a list box's title.
    pub text: NcStr,
    /// A check box's or radio button's state, a list's chosen line.
    pub state: i32,
    /// An edit line's or a list's columns; 0: as wide as the dialog lets it.
    pub width: u32,
    /// A list's lines.
    pub lines: *const NcStr,
    pub line_count: usize,
    /// An edit line's history (FAR's `DIF_HISTORY` with its name; empty: none): the lines given before
    /// under the same name, Ctrl+Down opening them; a button closing the dialog adds the line's text.
    pub history: NcStr,
    /// An [`ITEM_FIXED`] line's mask (empty: none).
    pub mask: NcStr,
    /// A list box's lines shown (0: as many as it has, up to 8); in a [placed](NcDialog::width) dialog
    /// its coordinates say.
    pub height: u32,
    /// FAR's `X1`, `Y1`, `X2`, `Y2` inside the dialog, read in a placed dialog only: the item's first
    /// column and row, its last column (an edit line's, a list's; 0: as `width` says) and a list box's
    /// last row. `x1` -1: a text or separator centred.
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

/// A dialog's events ([`NcDialogEvent::kind`]).
pub const EVENT_INIT: u32 = 0;
/// An item's value changed: a text, a check, a radio button, a list's choice; told for each key.
pub const EVENT_CHANGED: u32 = 1;
/// A button with [`ITEM_NO_CLOSE`] was pressed.
pub const EVENT_BUTTON: u32 = 2;
/// The dialog is closing by a button (`item`) or Esc (-1); the handler's 1 keeps it open.
pub const EVENT_CLOSE: u32 = 3;
/// A key in the dialog ([`DIALOG_KEYS`]): its name in `key` ("F4", "Ctrl+Enter", "a"), `item` the
/// focused one. The handler's 1 takes it: the dialog does nothing with it.
pub const EVENT_KEY: u32 = 4;
/// A mouse click in the dialog ([`DIALOG_KEYS`]): `key` "MsLClick", "MsRClick" or "MsM1Click" (after
/// "Ctrl+", "Alt+", "Shift+"; "MsLDblClick" for a double one), `x` and `y` in the dialog, `item` the one
/// under it (-1: none). The handler's 1 takes it.
pub const EVENT_MOUSE: u32 = 5;
/// The focus came to `item` (FAR's `DN_GOTFOCUS`).
pub const EVENT_FOCUS: u32 = 6;
/// The focus left `item` (FAR's `DN_KILLFOCUS`); the handler may move it back.
pub const EVENT_UNFOCUS: u32 = 7;
/// The dialog is drawn anew (FAR's `DN_DRAWDIALOG`, `DN_RESIZECONSOLE`): shown, back in front, or the
/// screen's size changed (`x` its columns, `y` its rows).
pub const EVENT_DRAW: u32 = 8;

/// What the handler hears: the event, its item (-1 none) and every item's value now.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcDialogEvent {
    pub kind: u32,
    pub item: i32,
    pub values: *const NcDialogValue,
    pub count: usize,
    /// [`EVENT_KEY`]'s and [`EVENT_MOUSE`]'s key.
    pub key: NcStr,
    /// [`EVENT_MOUSE`]'s place, [`EVENT_DRAW`]'s screen.
    pub x: i32,
    pub y: i32,
}

/// An item's value: its state (as [`NcDialogItem::state`]) and its text.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcDialogValue {
    pub state: i32,
    pub text: NcStr,
}

/// What a change does ([`NcDialogChange::what`]).
pub const CHANGE_TEXT: u32 = 0;
pub const CHANGE_STATE: u32 = 1;
pub const CHANGE_ENABLE: u32 = 2;
pub const CHANGE_DISABLE: u32 = 3;
pub const CHANGE_SHOW: u32 = 4;
pub const CHANGE_HIDE: u32 = 5;
pub const CHANGE_FOCUS: u32 = 6;
/// A list's or list box's lines replaced by `line_count` at `lines` (FAR's `DM_LISTSET`); its chosen
/// line kept where it still is, else the last.
pub const CHANGE_LINES: u32 = 7;

/// A change of item `item` asked by the handler: `CHANGE_*`, with its text, state or lines.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcDialogChange {
    pub item: usize,
    pub what: u32,
    pub state: i32,
    pub text: NcStr,
    pub lines: *const NcStr,
    pub line_count: usize,
}

/// A dialog's handler: called on the plugin's thread with its `state`, the `handle` for
/// [`NcHost::dialog_change`](crate::NcHost::dialog_change) and the event. For [`EVENT_CLOSE`] 1 keeps
/// the dialog open, for [`EVENT_KEY`] and [`EVENT_MOUSE`] it takes the input; otherwise 0.
pub type DialogHandler = unsafe extern "C" fn(
    state: *mut c_void,
    handle: *mut c_void,
    event: *const NcDialogEvent,
) -> i32;

/// A dialog a plugin shows: `count` items at `items`, its handler (none: no events) with its state.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcDialog {
    /// `size_of::<NcDialog>()` of the plugin's build.
    pub size: usize,
    /// `DIALOG_*` flags.
    pub flags: u32,
    pub title: NcStr,
    pub items: *const NcDialogItem,
    pub count: usize,
    pub handler: Option<DialogHandler>,
    pub handler_state: *mut c_void,
    /// FAR's `X1`, `Y1`: the dialog's left column and top row on the screen; -1 centred.
    pub x: i32,
    pub y: i32,
    /// The dialog's columns and rows with its margin (FAR's `X2 - X1 + 1`): both given, the dialog is
    /// placed, its items at their coordinates; 0: the program lays it out.
    pub width: u32,
    pub height: u32,
}
