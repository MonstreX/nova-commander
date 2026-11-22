//! Switches of the dialogs and menus kept apart from the rest of the settings (flat keys as the
//! others): FAR's `Dialogs.EditHistory` and `ShowMenuScrollbar`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DialogSwitches {
    /// FAR's `Dialogs.EditHistory`: "History in dialog edit controls" (Dialog settings).
    #[serde(rename = "dialogs.editHistory")]
    pub edit_history: bool,
    /// FAR's `ShowMenuScrollbar` (no dialog): a menu longer than its box shows a scroll bar.
    #[serde(rename = "menus.scrollbar")]
    pub menu_scrollbar: bool,
}

impl Default for DialogSwitches {
    fn default() -> Self {
        Self {
            edit_history: true,
            menu_scrollbar: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;

    #[test]
    fn the_switches_are_flat_keys_on_by_default() {
        let read: Settings = serde_json::from_str(r#"{"dialogs.editHistory": false}"#).unwrap();
        assert!(!read.dialog_switches.edit_history && read.dialog_switches.menu_scrollbar);
        assert!(!read.other.contains_key("dialogs.editHistory"));
        let written = serde_json::to_value(&read).unwrap();
        assert_eq!(written["menus.scrollbar"], true);
    }
}
