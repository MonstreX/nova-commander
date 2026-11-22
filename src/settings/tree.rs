//! FAR's tree panel options (`Panel.Tree` in its settings): whether there is no tree at all (FAR's
//! `TurnOffCompletely`, true in FAR 3; false here, so the tree is on: owner 2026-11-11), whether moving
//! in it changes the other panel's folder (off, as FAR's), and how many folders a tree has before its
//! file is kept (FAR's `MinTreeCount`, 4). The files are in `TreeCache` next to the settings (owner).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// FAR's least number of folders (the root among them) a tree is kept in a file for.
const MIN_TREE_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TreeOptions {
    pub turn_off_completely: bool,
    /// FAR's "Auto change folder".
    pub auto_change_folder: bool,
    /// FAR's "Minimum number of folders".
    pub min_tree_count: usize,
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self {
            turn_off_completely: false,
            auto_change_folder: false,
            min_tree_count: MIN_TREE_COUNT,
        }
    }
}

/// The trees' files: `TreeCache` next to the settings file.
pub fn tree_cache_dir(settings_path: &Path) -> PathBuf {
    settings_path.with_file_name("TreeCache")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_tree_is_on_unless_the_settings_turn_it_off() {
        let settings: crate::settings::Settings = serde_json::from_str("{}").unwrap();
        assert!(
            !settings.tree.turn_off_completely,
            "on here (owner), off in FAR 3"
        );
        assert!(!settings.tree.auto_change_folder, "FAR's default");
        assert_eq!(settings.tree.min_tree_count, 4, "FAR's default");
        let written = serde_json::to_value(crate::settings::Settings::default()).unwrap();
        assert_eq!(
            written["panels.tree"],
            serde_json::json!({
                "turnOffCompletely": false,
                "autoChangeFolder": false,
                "minTreeCount": 4
            })
        );
    }

    #[test]
    fn the_trees_files_are_next_to_the_settings() {
        let path = std::path::Path::new("x")
            .join("settings")
            .join("settings.json");
        assert_eq!(
            super::tree_cache_dir(&path),
            std::path::Path::new("x").join("settings").join("TreeCache")
        );
    }
}
