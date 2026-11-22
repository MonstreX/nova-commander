//! The code page menu's settings (FAR's `CodePages.Favorites` with `CPST_FAVORITE`, `CodePages.Names`,
//! `CPMenuMode`): the favourite code pages, the user's names of code pages, whether the menu hides
//! the code pages that are neither standard nor favourite. Find file's marks are its own
//! (`find.codePages`, FAR's `CPST_FIND`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CodePageSettings {
    /// The favourites, by their numbers in order.
    #[serde(rename = "codePages.favorites")]
    pub favorites: Vec<u32>,
    /// The user's names, by the code pages' numbers (as text: the flat keys' maps are keyed so).
    #[serde(rename = "codePages.names")]
    pub names: BTreeMap<String, String>,
    /// FAR's `CPMenuMode` (Ctrl+H in the menu): only the standard and the favourite code pages.
    #[serde(rename = "codePages.menuMode")]
    pub menu_mode: bool,
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;

    #[test]
    fn the_code_page_settings_are_flat_keys() {
        let read: Settings = serde_json::from_str(
            r#"{"codePages.favorites": [866, 1251], "codePages.names": {"866": "DOS"}}"#,
        )
        .unwrap();
        assert_eq!(read.code_pages.favorites, [866, 1251]);
        assert_eq!(
            read.code_pages.names.get("866").map(String::as_str),
            Some("DOS")
        );
        assert!(!read.code_pages.menu_mode);
        let written = serde_json::to_value(&read).unwrap();
        assert_eq!(written["codePages.names"]["866"], "DOS");
    }
}
