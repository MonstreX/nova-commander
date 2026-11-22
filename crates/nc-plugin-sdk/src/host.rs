//! The program as a plugin asks it things: its windows (a message, a menu, an input line), its log and the
//! plugin's folder. Every window blocks the plugin's thread until the user closes it; while the plugin starts
//! (in `new` and the first `info` and `commands`) none is shown and each answers as Esc at once.

use std::path::PathBuf;

use crate::api::{self, NcHost, NcMenu, NcMenuItem, NcStr};

/// The program, as a plugin asks it things.
#[derive(Debug, Clone, Copy)]
pub struct Host {
    pub(crate) table: *const NcHost,
}

/// A message window (FAR's `Message`): its lines centred (or from the left), its buttons, the warning
/// colours. `&` in a button's label marks its hotkey; a label is one line, not empty. A line
/// [`Message::SEPARATOR`] (or [`Message::DOUBLE_SEPARATOR`]) followed by a title is a line across.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Message {
    pub title: String,
    pub lines: Vec<String>,
    pub buttons: Vec<String>,
    pub warning: bool,
    pub left_align: bool,
    /// The system's text of the thread's last error added (FAR's `FMSG_ERRORTYPE`).
    pub error: bool,
    /// The program's button set instead of `buttons`.
    pub button_set: Option<ButtonSet>,
}

/// The program's buttons for a message (FAR's `FMSG_MB_*`); the answer is the index in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonSet {
    Ok,
    OkCancel,
    AbortRetryIgnore,
    YesNo,
    YesNoCancel,
    RetryCancel,
}

impl ButtonSet {
    fn flags(self) -> u32 {
        match self {
            Self::Ok => api::MESSAGE_BUTTONS_OK,
            Self::OkCancel => api::MESSAGE_BUTTONS_OK_CANCEL,
            Self::AbortRetryIgnore => api::MESSAGE_BUTTONS_ABORT_RETRY_IGNORE,
            Self::YesNo => api::MESSAGE_BUTTONS_YES_NO,
            Self::YesNoCancel => api::MESSAGE_BUTTONS_YES_NO_CANCEL,
            Self::RetryCancel => api::MESSAGE_BUTTONS_RETRY_CANCEL,
        }
    }
}

impl Message {
    pub fn new(title: &str, lines: &[&str]) -> Self {
        Self {
            title: title.to_string(),
            lines: lines.iter().map(ToString::to_string).collect(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn buttons(mut self, buttons: &[&str]) -> Self {
        self.buttons = buttons.iter().map(ToString::to_string).collect();
        self
    }

    #[must_use]
    pub fn warning(mut self) -> Self {
        self.warning = true;
        self
    }

    #[must_use]
    pub fn left_align(mut self) -> Self {
        self.left_align = true;
        self
    }

    /// The thread's last system error's text under the lines (FAR's `FMSG_ERRORTYPE`).
    #[must_use]
    pub fn error(mut self) -> Self {
        self.error = true;
        self
    }

    #[must_use]
    pub fn button_set(mut self, set: ButtonSet) -> Self {
        self.button_set = Some(set);
        self
    }

    /// A line starting so is a separator (FAR's `\1`), the rest a title in its middle.
    pub const SEPARATOR: &str = "\u{1}";
    /// A line starting so is a double separator (FAR's `\2`).
    pub const DOUBLE_SEPARATOR: &str = "\u{2}";
}

/// An input line's options (FAR's `FIB_*`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputOptions {
    /// An empty answer taken; without it an empty one is `None`.
    pub enable_empty: bool,
    /// The text shown as stars.
    pub password: bool,
    /// `%NAME%` in the answer replaced by the variable's value.
    pub expand_env: bool,
    /// "OK" and "Cancel" under the line.
    pub buttons: bool,
    /// `&` in the prompt marks a hotkey.
    pub no_ampersand: bool,
    /// With a history, an empty line does not start with its last one (`FIB_NOUSELASTHISTORY`).
    pub no_last_history: bool,
    /// The line is a path, completed as the dialogs' lines are (`FIB_EDITPATH`).
    pub edit_path: bool,
}

/// One item of a [`Menu`]: `&` before its hotkey letter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuItem {
    pub text: String,
    /// A line across the menu instead of an item.
    pub separator: bool,
    pub checked: bool,
    /// Shown grey, the cursor passing over it (FAR's `MIF_DISABLE`).
    pub disabled: bool,
    /// Shown grey, the cursor stopping on it, Enter doing nothing there (`MIF_GRAYED`).
    pub grayed: bool,
    /// Not shown; the other items keep their indexes (`MIF_HIDDEN`).
    pub hidden: bool,
}

impl MenuItem {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            ..Self::default()
        }
    }

    pub fn separator() -> Self {
        Self {
            separator: true,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub fn grayed(mut self, grayed: bool) -> Self {
        self.grayed = grayed;
        self
    }

    #[must_use]
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }
}

/// A menu (FAR's `Menu`): its items, the one under the cursor at first, and the keys that close it besides
/// Enter and Esc ("F4", "Del", "Shift+F4", "Ctrl+Up", "Ctrl+A", ...: modifiers `Ctrl+`, `Alt+`, `Shift+`
/// before a key; a letter alone is the one typed, "A" Shift+A).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    /// In the frame's bottom line.
    pub bottom: String,
    pub items: Vec<MenuItem>,
    pub selected: usize,
    pub break_keys: Vec<String>,
    /// Items without a hotkey of their own get one (FAR's `FMENU_AUTOHIGHLIGHT`).
    pub auto_hotkeys: bool,
    /// Up and Down go round the ends (`FMENU_WRAPMODE`).
    pub wrap: bool,
    /// `&` shown as it is, no hotkey of the items' own (`FMENU_SHOWAMPERSAND`).
    pub show_ampersand: bool,
    /// The automatic hotkeys given from the last item up (`FMENU_REVERSEAUTOHIGHLIGHT`); turns them on
    /// by itself, as in FAR.
    pub reverse_auto_hotkeys: bool,
    /// FAR's `X`, `Y`: the menu's left column and top row on the screen (none: centred).
    pub x: Option<u16>,
    pub y: Option<u16>,
    /// FAR's `MaxHeight`: the most lines shown at once (none: as the screen takes).
    pub max_height: Option<u32>,
}

impl Menu {
    pub fn new(title: &str, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.to_string(),
            items,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn bottom(mut self, bottom: &str) -> Self {
        self.bottom = bottom.to_string();
        self
    }

    #[must_use]
    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn break_keys(mut self, keys: &[&str]) -> Self {
        self.break_keys = keys.iter().map(ToString::to_string).collect();
        self
    }

    #[must_use]
    pub fn auto_hotkeys(mut self) -> Self {
        self.auto_hotkeys = true;
        self
    }

    #[must_use]
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    #[must_use]
    pub fn show_ampersand(mut self) -> Self {
        self.show_ampersand = true;
        self
    }

    #[must_use]
    pub fn reverse_auto_hotkeys(mut self) -> Self {
        self.reverse_auto_hotkeys = true;
        self
    }

    /// At column `x`, row `y` of the screen (FAR's `X`, `Y`).
    #[must_use]
    pub fn at(mut self, x: u16, y: u16) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self
    }

    /// At most `lines` shown at once (FAR's `MaxHeight`).
    #[must_use]
    pub fn max_height(mut self, lines: u32) -> Self {
        self.max_height = Some(lines);
        self
    }
}

/// How a menu was closed (not by Esc): the item under the cursor (none in an empty menu) and the break key
/// pressed (none for Enter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chosen {
    pub item: Option<usize>,
    pub key: Option<usize>,
}

/// `flag` when `on`.
pub(crate) fn flag(on: bool, flag: u32) -> u32 {
    if on { flag } else { 0 }
}

/// A non-negative answer as an index.
pub(crate) fn index(answer: i32) -> Option<usize> {
    usize::try_from(answer).ok()
}

impl Host {
    pub(crate) fn table(&self) -> &NcHost {
        // SAFETY: the host's table lives while the plugin is loaded (the plugin is released first).
        unsafe { &*self.table }
    }

    /// A message with an OK button: true for OK, false for Esc.
    pub fn message(&self, title: &str, lines: &[&str]) -> bool {
        self.show(&Message::new(title, lines).buttons(&["OK"])) == Some(0)
    }

    /// The message window: the index of the button pressed (of `buttons`, or of the program's
    /// `button_set` when it names one), `None` for Esc. With no buttons it stays while the plugin works on
    /// (until its call ends or its next window) and this returns `Some(0)` at once.
    pub fn show(&self, message: &Message) -> Option<usize> {
        let text = message.lines.join("\n");
        let buttons = message.buttons.join("\n");
        let flags = flag(message.warning, api::MESSAGE_WARNING)
            | flag(message.left_align, api::MESSAGE_LEFT_ALIGN)
            | flag(message.error, api::MESSAGE_ERROR)
            | message.button_set.map_or(0, ButtonSet::flags);
        let table = self.table();
        // SAFETY: the host's own function with its own context, on the plugin's thread; the strings live
        // through the call.
        index(unsafe {
            (table.message)(
                table.context,
                flags,
                NcStr::new(&message.title),
                NcStr::new(&text),
                NcStr::new(&buttons),
            )
        })
    }

    /// A window without buttons kept while the plugin works ("Reading the list..."), gone when its call
    /// ends or its next window comes.
    pub fn busy(&self, title: &str, lines: &[&str]) {
        let _ = self.show(&Message::new(title, lines));
    }

    /// The menu: how it was closed, `None` for Esc.
    pub fn menu(&self, menu: &Menu) -> Option<Chosen> {
        let items: Vec<NcMenuItem> = menu.items.iter().map(menu_item).collect();
        let keys: Vec<NcStr> = menu.break_keys.iter().map(|key| NcStr::new(key)).collect();
        let raw = NcMenu {
            size: size_of::<NcMenu>(),
            flags: flag(menu.auto_hotkeys, api::MENU_AUTO_HOTKEYS)
                | flag(menu.wrap, api::MENU_WRAP)
                | flag(menu.show_ampersand, api::MENU_SHOW_AMPERSAND)
                | flag(menu.reverse_auto_hotkeys, api::MENU_REVERSE_AUTO_HOTKEYS),
            title: NcStr::new(&menu.title),
            bottom: NcStr::new(&menu.bottom),
            items: items.as_ptr(),
            count: items.len(),
            selected: menu.selected,
            break_keys: keys.as_ptr(),
            break_count: keys.len(),
            x: menu.x.map_or(-1, i32::from),
            y: menu.y.map_or(-1, i32::from),
            max_height: menu.max_height.unwrap_or(0),
        };
        let mut key = -1;
        let table = self.table();
        // SAFETY: as in `show`; the menu, its items and keys live through the call.
        let item = unsafe { (table.menu)(table.context, &raw const raw, &raw mut key) };
        let chosen = Chosen {
            item: index(item),
            key: index(key),
        };
        (chosen.item.is_some() || chosen.key.is_some()).then_some(chosen)
    }

    /// An input line under `prompt`, starting with `text`: what the user typed, `None` for Esc and for an
    /// empty answer (FAR's `InputBox` without `FIB_ENABLEEMPTY`; [`Host::input_with`] takes one).
    pub fn input(&self, title: &str, prompt: &str, text: &str) -> Option<String> {
        self.input_with(title, prompt, text, InputOptions::default())
    }

    /// FAR's `InputBox` with its options; `None` for Esc (and an empty answer without `enable_empty`).
    pub fn input_with(
        &self,
        title: &str,
        prompt: &str,
        text: &str,
        options: InputOptions,
    ) -> Option<String> {
        self.input_history(title, prompt, text, "", options)
    }

    /// FAR's `InputBox` with a history (`HistoryName`; empty: none): the lines typed before under the
    /// same name, Ctrl+Down opening them, an empty line starting with the last unless
    /// `no_last_history`.
    pub fn input_history(
        &self,
        title: &str,
        prompt: &str,
        text: &str,
        history: &str,
        options: InputOptions,
    ) -> Option<String> {
        let flags = flag(options.enable_empty, api::INPUT_ENABLE_EMPTY)
            | flag(options.password, api::INPUT_PASSWORD)
            | flag(options.expand_env, api::INPUT_EXPAND_ENV)
            | flag(options.buttons, api::INPUT_BUTTONS)
            | flag(options.no_ampersand, api::INPUT_NO_AMPERSAND)
            | flag(options.no_last_history, api::INPUT_NO_LAST_HISTORY)
            | flag(options.edit_path, api::INPUT_EDIT_PATH);
        let mut answer = NcStr::EMPTY;
        let table = self.table();
        // SAFETY: as in `show`; the host's answer is copied before the next call to the host.
        unsafe {
            let result = (table.input)(
                table.context,
                NcStr::new(title),
                NcStr::new(prompt),
                NcStr::new(text),
                NcStr::new(history),
                flags,
                &raw mut answer,
            );
            (result == 0).then(|| answer.as_str().to_string())
        }
    }

    /// A line in the plugin's log (`settings\Logs\<plugin>.log`). A plugin never prints (`println!`,
    /// `eprintln!`): its stdout is the program's screen.
    pub fn log(&self, text: &str) {
        let table = self.table();
        // SAFETY: as in `show`.
        unsafe { (table.log)(table.context, NcStr::new(text)) }
    }

    /// How far the call running is (ADR-0019 §5): `done` of `total` (0: not known), `item` the one at
    /// hand. The program's window over a call that lasts shows it; it never blocks, so a loop may tell
    /// it at every item.
    pub fn progress(&self, done: u64, total: u64, item: &str) {
        let table = self.table();
        // SAFETY: as in `show`.
        unsafe { (table.progress)(table.context, done, total, NcStr::new(item)) }
    }

    /// Whether the user asked the call running to stop (FAR's `CheckForEsc`): a long loop looks at it
    /// between its items and ends with what it did. A call that does not is let go after a while.
    pub fn cancelled(&self) -> bool {
        let table = self.table();
        // SAFETY: as in `show`.
        unsafe { (table.cancelled)(table.context) != 0 }
    }

    /// The plugin's folder (`plugins\<name>`): its own files are there.
    pub fn folder(&self) -> PathBuf {
        // SAFETY: the host keeps the folder's text while the plugin is loaded.
        PathBuf::from(unsafe { self.table().folder.as_str() })
    }

    /// The plugin's data folder beside the program's settings (`settings\Plugins\<name>`, API 7): what the
    /// user's work with it keeps, which an update of the plugin's own folder leaves alone. It may not exist
    /// yet: the plugin makes it when it writes there.
    pub fn data_folder(&self) -> PathBuf {
        // SAFETY: the host keeps the folder's text while the plugin is loaded.
        PathBuf::from(unsafe { self.table().data_folder.as_str() })
    }
}

fn menu_item(item: &MenuItem) -> NcMenuItem {
    let flags = flag(item.separator, api::MENU_ITEM_SEPARATOR)
        | flag(item.checked, api::MENU_ITEM_CHECKED)
        | flag(item.disabled, api::MENU_ITEM_DISABLED)
        | flag(item.grayed, api::MENU_ITEM_GRAYED)
        | flag(item.hidden, api::MENU_ITEM_HIDDEN);
    NcMenuItem {
        text: NcStr::new(&item.text),
        flags,
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
