//! Find file's settings (Alt+F7), flat keys as the others: what its dialog was left with, FAR's
//! `FileSearchMode`, `FindFolders` and the rest.

use serde::{Deserialize, Serialize};

use super::{FIND_COLUMN_TYPES, FIND_COLUMN_WIDTHS};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FindSettings {
    /// Alt+F7: the masks looked for last (FAR keeps them as the last line of its "Masks" history).
    #[serde(rename = "find.mask")]
    pub mask: String,
    /// allDrives | localDrives | path | root | fromCurrent | current | selected (FAR's
    /// `FileSearchMode`).
    #[serde(rename = "find.area")]
    pub area: String,
    /// FAR's `FindFolders`: "Search for folders".
    #[serde(rename = "find.folders")]
    pub folders: bool,
    /// FAR's `FindSymLinks`: "Search in symbolic links".
    #[serde(rename = "find.symbolicLinks")]
    pub links: bool,
    /// FAR's `FindCodePage`: `auto`, `all` or a code page's number.
    #[serde(rename = "find.codePage")]
    pub code_page: String,
    /// The code pages marked in its list (FAR's `CodePages.Favorites` with `CPST_FIND`).
    #[serde(rename = "find.codePages")]
    pub code_pages: Vec<u32>,
    /// FAR's `FindAlternateStreams`: "Search in alternate streams".
    #[serde(rename = "find.alternateStreams")]
    pub streams: bool,
    /// FAR's `UseFilterInSearch`: "Use filter".
    #[serde(rename = "find.useFilter")]
    pub use_filter: bool,
    /// FAR's `SearchInFirstSize`: a size (`64K`), empty for whole files.
    #[serde(rename = "find.searchInFirst")]
    pub first: String,
    /// FAR's `SearchOutFormat` and `SearchOutFormatWidth`: the results' columns.
    #[serde(rename = "find.columnTypes")]
    pub column_types: String,
    #[serde(rename = "find.columnWidths")]
    pub column_widths: String,
}

impl Default for FindSettings {
    fn default() -> Self {
        Self {
            mask: String::new(),
            area: "fromCurrent".to_string(),
            folders: true,
            links: true,
            streams: false,
            use_filter: false,
            code_page: "auto".to_string(),
            code_pages: Vec::new(),
            first: String::new(),
            column_types: FIND_COLUMN_TYPES.to_string(),
            column_widths: FIND_COLUMN_WIDTHS.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::Settings;

    #[test]
    fn the_find_settings_are_flat_keys() {
        let read: Settings =
            serde_json::from_str(r#"{"find.mask": "*.rs", "find.folders": false}"#).unwrap();
        assert_eq!(read.find.mask, "*.rs");
        assert!(!read.find.folders && read.find.links);
        assert!(!read.other.contains_key("find.mask"));
        let written = serde_json::to_value(&read).unwrap();
        assert_eq!(written["find.area"], "fromCurrent");
    }
}
