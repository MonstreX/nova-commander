//! EMenu's configuration (FAR's `ReadRegValues`, `Configure`, `CfgDlgProc`): its settings under FAR's
//! names (the delete one renamed: the program's own names), the dialog at FAR's DialogBuilder places (the
//! two single boxes as titled lines: the plugins' dialogs have no boxes), "Show message" off while silent,
//! "Hide if the same" off while nothing is added (FAR turns it on only by the other two choices).

use nc_plugin_sdk::{Closed, Controls, Dialog, DialogEvent, DialogItem, Host};

/// The plugin's title, its items' and its messages'.
pub const TITLE: &str = "EMenu";

/// How the menu shows: FAR's `UseGUI` (`[ ]`, `[x]`, `[?]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gui {
    Text,
    Gui,
    Ask,
}

/// What a text menu's item shows in brackets after it: FAR's `Helptext` (`AS_NONE`, `AS_HELPTEXT`,
/// `AS_VERB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extra {
    Nothing,
    Help,
    Verb,
}

/// The settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// "Show message after execution".
    pub wait: bool,
    pub gui: Gui,
    /// The "delete" command done as F8 (Shift+Del, Alt+Del with those held).
    pub delete_with_f8: bool,
    pub clear_selection: bool,
    /// No message ever.
    pub silent: bool,
    pub extra: Extra,
    /// No extra when it says what the item does.
    pub different_only: bool,
    /// The menus at the mouse (FAR's `GuiPos` 0), else in the window's centre.
    pub at_mouse: bool,
}

impl Default for Options {
    /// FAR's defaults.
    fn default() -> Self {
        Self {
            wait: false,
            gui: Gui::Ask,
            delete_with_f8: false,
            clear_selection: true,
            silent: false,
            extra: Extra::Nothing,
            different_only: false,
            at_mouse: false,
        }
    }
}

const WAIT: &str = "WaitToContinue";
const USE_GUI: &str = "UseGUI";
const DELETE: &str = "DeleteWithF8";
const CLEAR: &str = "ClearSelection";
const SILENT: &str = "Silent";
const HELPTEXT: &str = "Helptext";
const DIFFERENT: &str = "DifferentOnly";
const GUI_POSITION: &str = "GuiPos";

/// The dialog's items by index.
const SHOW_MESSAGE: usize = 0;
const SHOW_GUI: usize = 1;
const DELETE_ITEM: usize = 2;
const CLEAR_ITEM: usize = 3;
const SILENT_ITEM: usize = 4;
const NOTHING_ITEM: usize = 6;
const HELP_ITEM: usize = 7;
const VERB_ITEM: usize = 8;
const DIFFERENT_ITEM: usize = 9;
const MOUSE_ITEM: usize = 11;
/// "Save".
pub const SAVE: usize = 14;

impl Options {
    pub fn load(host: &Host) -> Self {
        let default = Self::default();
        Self {
            wait: host.bool_setting(WAIT, default.wait),
            gui: match host.int_setting(USE_GUI, 2) {
                0 => Gui::Text,
                1 => Gui::Gui,
                _ => Gui::Ask,
            },
            delete_with_f8: host.bool_setting(DELETE, default.delete_with_f8),
            clear_selection: host.bool_setting(CLEAR, default.clear_selection),
            silent: host.bool_setting(SILENT, default.silent),
            extra: match host.int_setting(HELPTEXT, 0) {
                1 => Extra::Help,
                2 => Extra::Verb,
                _ => Extra::Nothing,
            },
            different_only: host.bool_setting(DIFFERENT, default.different_only),
            at_mouse: host.int_setting(GUI_POSITION, 1) == 0,
        }
    }

    pub fn save(&self, host: &Host) {
        host.set_bool(WAIT, self.wait);
        let gui = match self.gui {
            Gui::Text => 0,
            Gui::Gui => 1,
            Gui::Ask => 2,
        };
        host.set_int(USE_GUI, gui);
        host.set_bool(DELETE, self.delete_with_f8);
        host.set_bool(CLEAR, self.clear_selection);
        host.set_bool(SILENT, self.silent);
        let extra = match self.extra {
            Extra::Nothing => 0,
            Extra::Help => 1,
            Extra::Verb => 2,
        };
        host.set_int(HELPTEXT, extra);
        host.set_bool(DIFFERENT, self.different_only);
        host.set_int(GUI_POSITION, i64::from(!self.at_mouse));
    }

    /// FAR's configuration dialog, 52 x 21: the boxes from row 2 at column 5, the boxes' contents at 7.
    pub fn dialog(&self) -> Dialog {
        let check = |label: &str, on: bool, y: i32| {
            let x2 = 5 + i32::try_from(label.len()).unwrap_or(0) + 4;
            DialogItem::check(label, on).at(5, y, x2, y)
        };
        let radio = |label: &str, on: bool, y: i32| {
            let x2 = 7 + i32::try_from(label.len()).unwrap_or(0) + 4;
            DialogItem::radio(label, on).at(7, y, x2, y)
        };
        let titled = |title: &str, y: i32| DialogItem {
            text: title.to_string(),
            ..DialogItem::separator().at(5, y, 46, y)
        };
        let gui = match self.gui {
            Gui::Text => Some(false),
            Gui::Gui => Some(true),
            Gui::Ask => None,
        };
        let nothing = self.extra == Extra::Nothing;
        let items = vec![
            check("Show &message after execution", self.wait, 2).disabled(self.silent),
            DialogItem::check3("Display &GUI menu", gui).at(5, 3, 26, 3),
            check("&Delete using F8", self.delete_with_f8, 4),
            check(
                "&Clear selection if command executed",
                self.clear_selection,
                5,
            ),
            check("&Silent mode (never show messages)", self.silent, 6),
            titled("Additional information in text menu", 7),
            radio("&None", nothing, 8).group(),
            radio("Show &helptext", self.extra == Extra::Help, 9),
            radio("Show c&ommands", self.extra == Extra::Verb, 10),
            DialogItem::check("H&ide if the same", self.different_only)
                .at(7, 11, 28, 11)
                .disabled(nothing),
            titled("Menu position", 13),
            radio("&At the mouse cursor", self.at_mouse, 14).group(),
            radio("In the center of the &window", !self.at_mouse, 15),
            DialogItem::separator().at(-1, 17, -1, 17),
            DialogItem::button("Save")
                .default_button()
                .at(16, 18, 23, 18),
            DialogItem::button("Cancel").at(25, 18, 34, 18),
        ];
        Dialog::new(TITLE, items).size(52, 21)
    }

    /// The dialog's live rules: "Show message" off while silent, "Hide if the same" off with no extra.
    pub fn handle(event: &DialogEvent, controls: &mut Controls) -> bool {
        if let DialogEvent::Changed(item) = event {
            match *item {
                SILENT_ITEM => {
                    let silent = controls.value(SILENT_ITEM).checked();
                    controls.enable(SHOW_MESSAGE, !silent);
                }
                NOTHING_ITEM | HELP_ITEM | VERB_ITEM => {
                    let nothing = controls.value(NOTHING_ITEM).checked();
                    controls.enable(DIFFERENT_ITEM, !nothing);
                }
                _ => {}
            }
        }
        true
    }

    /// The settings the dialog closed by "Save" gives.
    pub fn from_dialog(closed: &Closed) -> Option<Self> {
        if closed.button != Some(SAVE) {
            return None;
        }
        let state = |item: usize| closed.values.get(item).map_or(0, |value| value.state);
        let on = |item: usize| state(item) == 1;
        Some(Self {
            wait: on(SHOW_MESSAGE),
            gui: match state(SHOW_GUI) {
                0 => Gui::Text,
                1 => Gui::Gui,
                _ => Gui::Ask,
            },
            delete_with_f8: on(DELETE_ITEM),
            clear_selection: on(CLEAR_ITEM),
            silent: on(SILENT_ITEM),
            extra: match (on(HELP_ITEM), on(VERB_ITEM)) {
                (true, _) => Extra::Help,
                (_, true) => Extra::Verb,
                _ => Extra::Nothing,
            },
            different_only: on(DIFFERENT_ITEM),
            at_mouse: on(MOUSE_ITEM),
        })
    }
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
