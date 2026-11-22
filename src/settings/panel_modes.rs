//! FAR's file panel modes (`far/config.cpp` `InitialModes`, `ViewModes` in its settings): each mode's
//! columns and the columns of its status line written as FAR writes them (types `N,S,D,T`, widths
//! `0,6,0,5`: 0 is the type's default width or a share of what is left, `40%` a part of the panel), its
//! name (empty: the mode's own) and FAR's flags.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// One mode (FAR's `PanelViewSettings`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PanelMode {
    pub name: String,
    pub columns: String,
    pub widths: String,
    pub status_columns: String,
    pub status_widths: String,
    /// FAR's `PVS_FULLSCREEN`: the panel takes the whole screen.
    pub full_screen: bool,
    /// FAR's `PVS_ALIGNEXTENSIONS`: files' extensions in a column of their own.
    pub align_extensions: bool,
    /// FAR's `PVS_FOLDERALIGNEXTENSIONS`: folders' too.
    pub folder_align_extensions: bool,
    /// FAR's `PVS_FOLDERUPPERCASE`.
    pub folder_upper_case: bool,
    /// FAR's `PVS_FILELOWERCASE`.
    pub files_lower_case: bool,
    /// FAR's `PVS_FILEUPPERTOLOWERCASE`: a file's name all in capitals in small letters.
    pub upper_to_lower_case: bool,
}

/// The modes of Ctrl+1 .. Ctrl+5 and Ctrl+0 (FAR's 6 .. 9 come with descriptions, owners and links).
/// What a mode in the file does not say is that mode's FAR default, as FAR reads each value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelModes {
    pub brief: PanelMode,
    pub medium: PanelMode,
    pub full: PanelMode,
    pub wide: PanelMode,
    pub detailed: PanelMode,
    pub alternative: PanelMode,
    /// The user's own modes (FAR's `CustomModes`), after the others.
    pub custom: Vec<PanelMode>,
}

impl Default for PanelModes {
    fn default() -> Self {
        Self::far_defaults()
    }
}

impl<'de> Deserialize<'de> for PanelModes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let given = Value::deserialize(deserializer)?;
        let defaults = Self::far_defaults();
        let mode = |key: &str, default: PanelMode| -> PanelMode {
            let (Some(Value::Object(fields)), Ok(Value::Object(mut merged))) =
                (given.get(key), serde_json::to_value(&default))
            else {
                return default;
            };
            merged.extend(fields.clone());
            serde_json::from_value(Value::Object(merged)).unwrap_or(default)
        };
        Ok(Self {
            brief: mode("brief", defaults.brief.clone()),
            medium: mode("medium", defaults.medium.clone()),
            full: mode("full", defaults.full.clone()),
            wide: mode("wide", defaults.wide.clone()),
            detailed: mode("detailed", defaults.detailed.clone()),
            alternative: mode("alternative", defaults.alternative.clone()),
            custom: (given
                .get("custom")
                .and_then(Value::as_array)
                .into_iter()
                .flatten())
            .filter_map(|mode| serde_json::from_value(mode.clone()).ok())
            .collect(),
        })
    }
}

/// The status line of FAR's Brief, Medium and Wide.
const STATUS_FULL: (&str, &str) = ("NR,S,D,T", "0,6,0,5");
/// The status line of the others.
const STATUS_NAME: (&str, &str) = ("NR", "0");

fn mode(columns: (&str, &str), status: (&str, &str), align: bool, full_screen: bool) -> PanelMode {
    PanelMode {
        columns: columns.0.to_string(),
        widths: columns.1.to_string(),
        status_columns: status.0.to_string(),
        status_widths: status.1.to_string(),
        full_screen,
        align_extensions: align,
        ..PanelMode::default()
    }
}

impl PanelModes {
    /// FAR's `InitialModes`.
    pub fn far_defaults() -> Self {
        Self {
            brief: mode(("N,N,N", "0,0,0"), STATUS_FULL, true, false),
            medium: mode(("N,N", "0,0"), STATUS_FULL, false, false),
            full: mode(("N,S,D,T", "0,6,0,5"), STATUS_NAME, true, false),
            wide: mode(("N,S", "0,6"), STATUS_FULL, false, false),
            detailed: mode(
                ("N,S,P,DM,DC,DA,A", "0,6,6,14,14,14,0"),
                STATUS_NAME,
                true,
                true,
            ),
            alternative: mode(("NM,SC,D", "0,10,0"), STATUS_NAME, true, false),
            custom: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_mode_does_not_say_is_its_far_default() {
        let read: PanelModes =
            serde_json::from_str(r#"{"full": {"columns": "N,S"}, "brief": 7}"#).unwrap();
        let far = PanelModes::far_defaults();
        assert_eq!(read.full.columns, "N,S");
        assert_eq!(read.full.status_columns, far.full.status_columns);
        assert!(read.full.align_extensions);
        assert_eq!(
            read.brief, far.brief,
            "a mode that is not an object is FAR's"
        );
        assert_eq!(read.medium, far.medium);
        assert!(read.custom.is_empty());
        let custom: PanelModes =
            serde_json::from_str(r#"{"custom": [{"name": "Mine", "columns": "N"}, 3]}"#).unwrap();
        assert_eq!(custom.custom.len(), 1, "what is not a mode is left out");
        assert_eq!(
            (
                custom.custom[0].name.as_str(),
                custom.custom[0].widths.as_str()
            ),
            ("Mine", "")
        );
        let bad: PanelModes = serde_json::from_str(r#"{"wide": {"fullScreen": "yes"}}"#).unwrap();
        assert_eq!(
            bad.wide, far.wide,
            "a value of the wrong kind leaves the mode FAR's"
        );
    }
}
