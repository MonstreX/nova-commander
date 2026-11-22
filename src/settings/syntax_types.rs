//! The syntax type menu's settings (FarColorer's `favorite` and `hotkey` parameters of a type, kept in
//! its user's `hrcsettings`): the favourite types and the types' hotkeys, by the types' names.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyntaxTypeSettings {
    /// The favourites (Ins in the menu), in the order they were added.
    #[serde(rename = "syntax.favorites")]
    pub favorites: Vec<String>,
    /// Each type's hotkey (F4 in the menu): a letter or a digit.
    #[serde(rename = "syntax.hotkeys")]
    pub hotkeys: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;

    #[test]
    fn the_syntax_type_settings_are_flat_keys() {
        let read: Settings = serde_json::from_str(
            r#"{"syntax.favorites": ["rust", "c"], "syntax.hotkeys": {"rust": "R"}}"#,
        )
        .unwrap();
        assert_eq!(read.syntax_types.favorites, ["rust", "c"]);
        assert_eq!(
            read.syntax_types.hotkeys.get("rust").map(String::as_str),
            Some("R")
        );
        let written = serde_json::to_value(&read).unwrap();
        assert_eq!(written["syntax.favorites"][1], "c");
    }
}
