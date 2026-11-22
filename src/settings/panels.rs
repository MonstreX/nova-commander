//! Switches of the panels kept apart from the rest of the settings (flat keys as the others): FAR's
//! `Panel.CtrlFRule`, `PgUpChangeDisk`, `ShowPanelScrollbar` and what each side shows (`Panel.*.Type`).

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelSwitches {
    /// FAR's `Panel.CtrlFRule` (no dialog): Ctrl+F and the full names take the mode's cases.
    #[serde(rename = "panels.ctrlFRule")]
    pub ctrl_f_rule: bool,
    /// FAR's `PgUpChangeDisk`: "Use Ctrl+PgUp to change drive" (Interface settings), 0 off, 1 on, 2 (its
    /// third state) a network drive's share in the network plugin first; `true` and `false` read as 1
    /// and 0 (the settings before P6.6c).
    #[serde(
        rename = "interface.ctrlPgUpChangesDrive",
        deserialize_with = "pg_up_change_disk"
    )]
    pub ctrl_pg_up_changes_drive: u8,
    /// FAR's `ShowPanelScrollbar`: "Show scrollbar" (Panel settings), off by default.
    #[serde(rename = "panels.scrollbar")]
    pub panel_scrollbar: bool,
    /// FAR's `Panel.Left.Type` / `Panel.Right.Type`: files | tree | info | quickView.
    #[serde(rename = "panels.leftType")]
    pub left_type: String,
    #[serde(rename = "panels.rightType")]
    pub right_type: String,
}

impl Default for PanelSwitches {
    fn default() -> Self {
        Self {
            ctrl_f_rule: false,
            ctrl_pg_up_changes_drive: 1,
            panel_scrollbar: false,
            left_type: "files".to_string(),
            right_type: "files".to_string(),
        }
    }
}

/// `PgUpChangeDisk` as a number (any written by hand, kept within 0 to 2) or as the switch it was.
fn pg_up_change_disk<'de, D: Deserializer<'de>>(given: D) -> Result<u8, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Given {
        Switch(bool),
        Number(f64),
    }
    Ok(match Given::deserialize(given)? {
        Given::Switch(on) => u8::from(on),
        Given::Number(number) if number >= 2.0 => 2,
        Given::Number(number) if number >= 1.0 => 1,
        Given::Number(_) => 0,
    })
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;

    #[test]
    fn the_switches_are_flat_keys_and_missing_ones_take_fars_defaults() {
        let read: Settings =
            serde_json::from_str(r#"{"panels.scrollbar": true, "panels.rightType": "tree"}"#)
                .unwrap();
        let switches = &read.panel_switches;
        assert!(switches.panel_scrollbar);
        assert_eq!(switches.ctrl_pg_up_changes_drive, 1, "FAR's default");
        assert_eq!(
            (switches.left_type.as_str(), switches.right_type.as_str()),
            ("files", "tree")
        );
        assert!(
            !read.other.contains_key("panels.scrollbar"),
            "not among the unknown keys"
        );
        let written = serde_json::to_value(&read).unwrap();
        assert_eq!(written["interface.ctrlPgUpChangesDrive"], 1);
    }

    /// FAR's three values of `PgUpChangeDisk`; the switch the settings kept before read as 0 or 1, any
    /// number kept within 0 to 2 (never the whole file refused).
    #[test]
    fn ctrl_pg_up_takes_three_values_and_the_old_switch() {
        let value = |text: &str| {
            let json = format!(r#"{{"interface.ctrlPgUpChangesDrive": {text}}}"#);
            let read: Settings = serde_json::from_str(&json).unwrap();
            read.panel_switches.ctrl_pg_up_changes_drive
        };
        assert_eq!(value("false"), 0);
        assert_eq!(value("true"), 1);
        assert_eq!(value("2"), 2);
        assert_eq!(value("7"), 2);
        assert_eq!(value("300"), 2, "past a byte: still read");
        assert_eq!(value("-1"), 0);
        assert_eq!(value("1.0"), 1);
    }
}
