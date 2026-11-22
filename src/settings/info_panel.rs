//! The info panel's settings: its sections folded or not (FAR's `ShowStatusInfo`; the power section on
//! by default: owner, 2026-11-09) and the names of the folder description files (FAR's
//! `System.FolderInfo`). Flat keys under `infoPanel.` in the settings file.

use serde::{Deserialize, Serialize};

/// FAR's folder description files.
pub const DESCRIPTION_FILES: &str = "DirInfo,File_Id.diz,Descript.ion,ReadMe.*,Read.Me";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InfoPanelSettings {
    #[serde(rename = "infoPanel.disk")]
    pub disk: bool,
    #[serde(rename = "infoPanel.memory")]
    pub memory: bool,
    #[serde(rename = "infoPanel.power")]
    pub power: bool,
    #[serde(rename = "infoPanel.description")]
    pub description: bool,
    /// The "Plugin" section beside a plugin's panel (FAR's `ILSS_PLDESCRIPTION`).
    #[serde(rename = "infoPanel.plugin")]
    pub plugin: bool,
    /// FAR's "Folder description files": names and wildcards, `,` or `;` between.
    #[serde(rename = "infoPanel.descriptionFiles")]
    pub description_files: String,
    /// FAR's `ShowCDInfo`: "Show CD drive parameters".
    #[serde(rename = "infoPanel.showCdInfo")]
    pub show_cd_info: bool,
    /// FAR's `InfoComputerNameFormat` (`platform::system::ComputerNameFormat::id`).
    #[serde(rename = "infoPanel.computerNameFormat")]
    pub computer_name_format: String,
    /// FAR's `InfoUserNameFormat` (`platform::system::UserNameFormat::id`).
    #[serde(rename = "infoPanel.userNameFormat")]
    pub user_name_format: String,
}

impl Default for InfoPanelSettings {
    fn default() -> Self {
        Self {
            disk: true,
            memory: true,
            power: true,
            description: true,
            plugin: true,
            description_files: DESCRIPTION_FILES.to_string(),
            show_cd_info: true,
            computer_name_format: "physicalNetBios".to_string(),
            user_name_format: "logon".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;

    #[test]
    fn the_info_panel_s_keys_stay_flat() {
        let written = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(written["infoPanel.power"], true);
        assert_eq!(
            written["infoPanel.descriptionFiles"],
            "DirInfo,File_Id.diz,Descript.ion,ReadMe.*,Read.Me"
        );
        let read: Settings = serde_json::from_str(
            r#"{"infoPanel.memory": false, "infoPanel.descriptionFiles": "a"}"#,
        )
        .unwrap();
        assert!(!read.info_panel.memory);
        assert!(read.info_panel.disk, "the rest as given by default");
        assert_eq!(read.info_panel.description_files, "a");
    }
}
