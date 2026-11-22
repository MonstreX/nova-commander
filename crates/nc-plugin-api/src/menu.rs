//! A plugin's menus (FAR's `Menu`): [`NcHost::menu`](crate::NcHost::menu)'s menu, its items and their
//! flags.

use crate::NcStr;

/// A line across the menu instead of an item (FAR's `MIF_SEPARATOR`); its text is not shown.
pub const MENU_ITEM_SEPARATOR: u32 = 1;
/// A check mark before the item (`MIF_CHECKED`).
pub const MENU_ITEM_CHECKED: u32 = 2;
/// Shown grey, the cursor passing over it (`MIF_DISABLE`).
pub const MENU_ITEM_DISABLED: u32 = 4;
/// Shown grey, the cursor stopping on it, Enter doing nothing there (`MIF_GRAYED`).
pub const MENU_ITEM_GRAYED: u32 = 8;
/// Not shown (`MIF_HIDDEN`); the other items keep their indexes.
pub const MENU_ITEM_HIDDEN: u32 = 16;

/// Items without a hotkey of their own get one (FAR's `FMENU_AUTOHIGHLIGHT`).
pub const MENU_AUTO_HOTKEYS: u32 = 1;
/// Up on the first item goes to the last and Down on the last to the first (`FMENU_WRAPMODE`).
pub const MENU_WRAP: u32 = 2;
/// `&` in the items' texts shown as it is, no hotkey of their own (`FMENU_SHOWAMPERSAND`).
pub const MENU_SHOW_AMPERSAND: u32 = 4;
/// The automatic hotkeys given from the last item up (`FMENU_REVERSEAUTOHIGHLIGHT`); by itself it turns
/// them on too (FAR's `apiMenuFn`).
pub const MENU_REVERSE_AUTO_HOTKEYS: u32 = 8;

/// One item of a menu: its text (`&` before the hotkey letter) and `MENU_ITEM_*` flags.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcMenuItem {
    pub text: NcStr,
    pub flags: u32,
}

/// A menu a plugin shows: `count` items at `items`, the cursor on `selected`, and the keys that close it
/// besides Enter and Esc (`break_count` names at `break_keys`: "F4", "Del", "Shift+F4", "Ctrl+Up", ...;
/// the program's keys are names throughout its ABI, not FAR's key codes).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcMenu {
    /// `size_of::<NcMenu>()` of the plugin's build.
    pub size: usize,
    /// `MENU_*` flags.
    pub flags: u32,
    pub title: NcStr,
    /// In the frame's bottom line.
    pub bottom: NcStr,
    pub items: *const NcMenuItem,
    pub count: usize,
    pub selected: usize,
    pub break_keys: *const NcStr,
    pub break_count: usize,
    /// FAR's `X`, `Y`: the menu's left column and top row on the screen (its margin's); -1 centred.
    pub x: i32,
    pub y: i32,
    /// FAR's `MaxHeight`: the most lines shown at once; 0: as many as the screen takes.
    pub max_height: u32,
}
