//! FAR's files highlighting and sort groups (`far/hilight.cpp`, `Highlight` in its settings): four lists
//! in FAR's order (the first groups, the upper and the lower sort groups, the last groups), each group a
//! filter's conditions with the colours of its four states (normal, selected, under the cursor, selected
//! under the cursor) for the name and for its mark, the mark, and whether the groups after it are
//! looked at too. A colour is written as the theme files write one: `lightgreen` (the text only),
//! `lightgreen on blue`, `on blue` (the background only); empty is FAR's transparent: the panel's own.

use serde::{Deserialize, Serialize};

use super::Filter;

/// The colours of a group: each state's name and mark.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HighlightColors {
    pub normal: String,
    pub selected: String,
    pub cursor: String,
    pub selected_cursor: String,
    pub mark_normal: String,
    pub mark_selected: String,
    pub mark_cursor: String,
    pub mark_selected_cursor: String,
}

/// One group (FAR's `FileFilterParams` with its `highlight::element`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HighlightGroup {
    /// The conditions; its marks of the filters' areas are not used.
    pub filter: Filter,
    pub colors: HighlightColors,
    /// FAR's mark character(s) before the name.
    pub mark: String,
    /// FAR's "Inherit": the mark of a group before this one stays.
    pub mark_inherit: bool,
    /// FAR's "Continue processing": the groups after it are looked at too.
    pub continue_processing: bool,
}

impl Default for HighlightGroup {
    /// FAR's `highlight::element`: no colours, no mark, the mark inherited.
    fn default() -> Self {
        Self {
            filter: Filter::default(),
            colors: HighlightColors::default(),
            mark: String::new(),
            mark_inherit: true,
            continue_processing: false,
        }
    }
}

/// The four lists, in FAR's order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HighlightGroups {
    pub first: Vec<HighlightGroup>,
    /// FAR's upper sort groups: sorted before the rest, the first one first.
    pub upper: Vec<HighlightGroup>,
    /// FAR's lower sort groups: sorted after the rest.
    pub lower: Vec<HighlightGroup>,
    pub last: Vec<HighlightGroup>,
}

/// Windows' `FILE_ATTRIBUTE_*` bits of FAR's default groups.
const HIDDEN: u32 = 0x2;
const SYSTEM: u32 = 0x4;
const DIRECTORY: u32 = 0x10;

impl HighlightGroups {
    /// FAR's `SetHighlighting`: hidden, system, folders, `<exec>`, `<arc>`, `<temp>`, each with its
    /// name colour normal and under the cursor, the rest transparent, the mark inherited.
    pub fn far_defaults() -> Self {
        let group = |mask: &str, attributes: u32, normal: &str, cursor: &str| HighlightGroup {
            filter: Filter {
                use_mask: !mask.is_empty(),
                mask: if mask.is_empty() { "*" } else { mask }.to_string(),
                use_attributes: attributes != 0,
                attributes_set: attributes,
                ..Filter::default()
            },
            colors: HighlightColors {
                normal: normal.to_string(),
                cursor: cursor.to_string(),
                ..HighlightColors::default()
            },
            mark: String::new(),
            mark_inherit: true,
            continue_processing: false,
        };
        Self {
            first: vec![
                group("", HIDDEN, "cyan", "darkgray"),
                group("", SYSTEM, "cyan", "darkgray"),
                group("", DIRECTORY, "white", "white"),
                group("<exec>", 0, "lightgreen", "lightgreen"),
                group("<arc>", 0, "lightmagenta", "lightmagenta"),
                group("<temp>", 0, "brown", "brown"),
            ],
            ..Self::default()
        }
    }
}
