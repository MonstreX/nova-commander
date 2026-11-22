//! What a provider says of a panel showing one of its folders besides the entries (FAR's plugin
//! `OpenPanelInfo`, ADR-0016 §13): its title and its own view modes.

use std::sync::Arc;

/// A panel's look at a folder of a provider; the default (a disk's): the path and the program's modes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelLook {
    /// The panel's title (FAR's `PanelTitle`); `None`: the path.
    pub title: Option<String>,
    /// The provider's own modes (FAR's `PanelModesArray`): the one at `n` for Ctrl+`n`; `None` at a key,
    /// or none at all: the program's mode there. Shared: the drawing reads it for each frame.
    pub modes: Arc<[Option<PanelMode>]>,
    /// The mode the panel opens in (FAR's `StartPanelMode`): Ctrl+`n`'s `n`.
    pub start_mode: Option<usize>,
    /// The sort it opens with and whether descending (FAR's `StartSortMode`, `StartSortOrder`).
    pub start_sort: Option<(super::SortMode, bool)>,
    /// Copying or moving into the panel hands the provider the sources' paths, not their data (FAR's
    /// TmpPanel: references).
    pub takes_paths: bool,
    /// The panel's kind as the copy dialog's target names it (FAR's `Format`); `None`: none given.
    pub format: Option<String>,
    /// The program's modes show names without their folders (FAR's `OPIF_SHOWNAMESONLY`).
    pub names_only: bool,
    /// The program's modes keep long names' ends (FAR's `OPIF_SHOWRIGHTALIGNNAMES`).
    pub right_align_names: bool,
    /// The program's modes keep names' cases (FAR's `OPIF_SHOWPRESERVECASE`).
    pub preserve_case: bool,
    /// Real names still copied out and quick-viewed through the provider (FAR's plugin with its own
    /// `GetFiles`).
    pub own_get: bool,
    /// Real names still deleted through the provider (FAR's plugin with its own `DeleteFiles`).
    pub own_delete: bool,
    /// The provider writes its files out itself (FAR's `GetFilesW`, API 5): F5 / F6 to a disk and F3 / F4
    /// go to it.
    pub gives_files: bool,
    /// F8 hands the entries to the provider, which asks itself (FAR's `DeleteFilesW`, API 5).
    pub deletes: bool,
    /// F7 asks the provider to make a folder, which asks the name itself (FAR's `MakeDirectoryW`).
    pub makes_folders: bool,
    /// A file edited after F4 goes back at each save in the editor (API 8; FAR's NetBox's `EE_SAVE`), not
    /// only once the editor is left.
    pub puts_on_save: bool,
    /// The provider's own keys on the panel with their key bar labels (FAR's `ProcessPanelInput` keys,
    /// `KeyBar`).
    pub keys: Vec<PanelKey>,
    /// What the info panel beside it shows in its "Plugin" section (FAR's `InfoLines`). Shared: the
    /// drawing reads it for each frame.
    pub info_lines: Arc<[InfoLine]>,
}

/// A line of the info panel's "Plugin" section (FAR's `InfoPanelLine`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InfoLine {
    /// At the left; a separator's title.
    pub text: String,
    /// At the right in the info colour; not shown on a separator.
    pub data: String,
    /// A separator with `text` in its middle (FAR's `IPLFLAGS_SEPARATOR`).
    pub separator: bool,
}

/// A key of a provider's panel: its name as the program writes keys ("F7", "Alt+Shift+F2") and its key
/// bar label (empty: the program's stays).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelKey {
    pub key: String,
    pub label: String,
}

impl PanelLook {
    /// The provider's mode at Ctrl+`number`, if it has one.
    pub fn mode(&self, number: usize) -> Option<&PanelMode> {
        self.modes.get(number).and_then(Option::as_ref)
    }
}

/// One of a provider's view modes (FAR's `PanelMode`), in the program's panel-mode strings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelMode {
    /// The column types (`N,S,D,T`; `C0`..`C99` an entry's texts, `Z` its description).
    pub columns: String,
    pub widths: String,
    /// The columns' titles in their order: `None` (or none at all) the program's, an empty one shown so.
    pub titles: Vec<Option<String>>,
    /// The status line's columns; empty: by `detailed_status`.
    pub status_columns: String,
    pub status_widths: String,
    pub full_screen: bool,
    /// The status line shows the name, size, date and time when the mode gives no status columns.
    pub detailed_status: bool,
    pub align_extensions: bool,
    /// Names' cases as the program's mode at that key says; off: as given.
    pub case_conversion: bool,
}
