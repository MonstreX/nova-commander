//! FAR's panel sorting (`far/filelist.cpp`: `SortModes`, `list_less`): `..` first, then folders (option),
//! then selected entries (option), then the sort groups (option, by name and extension only), then the
//! mode's layers. The first layer follows the panel's direction; the following layers keep their own
//! (ascending), the last is the order the file system listed.

use std::cmp::Ordering;

use super::Entry;
use crate::platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Name,
    /// Name without the extension, then the extension.
    NameOnly,
    Extension,
    /// Write (modification) time.
    Modified,
    Size,
    /// The file system's order.
    Unsorted,
    Created,
    Accessed,
    /// Change time of data or attributes.
    Changed,
    /// Space on disk.
    Allocated,
    /// The entries' descriptions (FAR's `SM_DESCR`, Ctrl+F10): a provider's (a plugin's `Z`); a disk's
    /// have none yet (its descriptions files are read for the info panel only).
    Description,
}

impl SortMode {
    /// FAR's modes the sort groups apply to: by name, name only and extension.
    pub fn uses_sort_groups(self) -> bool {
        matches!(self, Self::Name | Self::NameOnly | Self::Extension)
    }

    /// FAR's default direction for the mode: newest and largest first.
    pub fn descending_by_default(self) -> bool {
        matches!(
            self,
            Self::Modified
                | Self::Size
                | Self::Created
                | Self::Accessed
                | Self::Changed
                | Self::Allocated
        )
    }

    /// Identifier in settings.json.
    pub fn id(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::NameOnly => "nameonly",
            Self::Extension => "extension",
            Self::Modified => "time",
            Self::Size => "size",
            Self::Unsorted => "unsorted",
            Self::Created => "creation",
            Self::Accessed => "access",
            Self::Changed => "change",
            Self::Allocated => "allocated",
            Self::Description => "description",
        }
    }

    pub fn from_id(id: &str) -> Self {
        ALL_MODES
            .into_iter()
            .find(|mode| mode.id() == id)
            .unwrap_or(Self::Name)
    }

    /// The layers after the first one (FAR's `DefaultLayers`).
    fn tie_breakers(self) -> &'static [SortMode] {
        match self {
            Self::Unsorted => &[],
            Self::Extension => &[Self::NameOnly, Self::Unsorted],
            Self::NameOnly => &[Self::Extension, Self::Unsorted],
            Self::Name => &[Self::Unsorted],
            _ => &[Self::Name, Self::Unsorted],
        }
    }
}

const ALL_MODES: [SortMode; 11] = [
    SortMode::Name,
    SortMode::NameOnly,
    SortMode::Extension,
    SortMode::Modified,
    SortMode::Size,
    SortMode::Unsorted,
    SortMode::Created,
    SortMode::Accessed,
    SortMode::Changed,
    SortMode::Allocated,
    SortMode::Description,
];

/// How a panel is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortOrder {
    pub mode: SortMode,
    /// Reverse of the ascending order (FAR's `m_ReverseSortOrder`).
    pub reverse: bool,
    pub directories_first: bool,
    pub selected_first: bool,
    /// FAR's "Use sort groups" (Shift+F11).
    pub sort_groups: bool,
    /// Names compared without their folders (FAR's `list_less` `IgnorePaths`): a provider's whole paths
    /// while the name column shows names only. Not a setting: the panel's view tells it.
    pub names_only: bool,
}

impl SortOrder {
    /// `mode` in its default direction with FAR's default options.
    pub fn new(mode: SortMode) -> Self {
        Self {
            mode,
            reverse: mode.descending_by_default(),
            directories_first: true,
            selected_first: false,
            sort_groups: false,
            names_only: false,
        }
    }
}

impl Default for SortOrder {
    fn default() -> Self {
        Self::new(SortMode::Name)
    }
}

/// Sorts `entries` as FAR does; `is_selected` tells the selection for "selected first" (tests: the
/// panels sort with their groups, `sort_entries_grouped`).
#[cfg(test)]
pub fn sort_entries(
    entries: &mut [Entry],
    order: &SortOrder,
    is_selected: impl Fn(&Entry) -> bool,
) {
    let taken: Vec<Entry> = entries.iter_mut().map(std::mem::take).collect();
    let sorted = sorted(taken, order, &is_selected, &|_| 0);
    for (place, entry) in entries.iter_mut().zip(sorted) {
        *place = entry;
    }
}

/// `sort_entries` with FAR's sort groups: `group` tells an entry's (FAR's `GetGroup`, read once for
/// each entry), the lower first, when the order uses them and its mode is by name or extension.
pub fn sort_entries_grouped(
    entries: &mut Vec<Entry>,
    order: &SortOrder,
    is_selected: impl Fn(&Entry) -> bool,
    group: impl Fn(&Entry) -> usize,
) {
    let grouped = order.sort_groups && order.mode.uses_sort_groups();
    let group = |entry: &Entry| if grouped { group(entry) } else { 0 };
    let taken = std::mem::take(entries);
    *entries = sorted(taken, order, &is_selected, &group);
}

/// What an entry is compared by, worked out once before the sort (FAR compares its stored names in
/// place, `filelist.cpp:662-747`; here each name is put into the system's form once, not at every
/// comparison): its group, whether it is selected, its name, name without the extension and extension.
struct Keyed {
    entry: Entry,
    group: usize,
    selected: bool,
    name: Vec<u16>,
    stem: Vec<u16>,
    extension: Vec<u16>,
    /// Its description, when sorted by it.
    description: Vec<u16>,
}

fn sorted(
    entries: Vec<Entry>,
    order: &SortOrder,
    is_selected: &dyn Fn(&Entry) -> bool,
    group: &dyn Fn(&Entry) -> usize,
) -> Vec<Entry> {
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().collect() };
    let parts = matches!(order.mode, SortMode::NameOnly | SortMode::Extension);
    let mut keyed: Vec<Keyed> = (entries.into_iter())
        .map(|entry| {
            let (stem, extension) = match parts {
                true => {
                    let (stem, extension) = name_ext(&entry);
                    (wide(named(stem, order.names_only)), wide(extension))
                }
                false => (Vec::new(), Vec::new()),
            };
            let description = match order.mode {
                SortMode::Description => wide(description_of(&entry)),
                _ => Vec::new(),
            };
            Keyed {
                description,
                group: group(&entry),
                selected: order.selected_first && is_selected(&entry),
                name: wide(named(&entry.name, order.names_only)),
                stem,
                extension,
                entry,
            }
        })
        .collect();
    keyed.sort_by(|a, b| compare(a, b, order));
    keyed.into_iter().map(|keyed| keyed.entry).collect()
}

fn compare(a: &Keyed, b: &Keyed, order: &SortOrder) -> Ordering {
    let (entry_a, entry_b) = (&a.entry, &b.entry);
    if entry_a.is_parent || entry_b.is_parent {
        return entry_b.is_parent.cmp(&entry_a.is_parent);
    }
    if order.directories_first && entry_a.is_dir != entry_b.is_dir {
        return entry_b.is_dir.cmp(&entry_a.is_dir);
    }
    if a.selected != b.selected {
        return b.selected.cmp(&a.selected);
    }
    // FAR's `list_less`: the groups come before the mode and never turn round with it.
    if a.group != b.group {
        return a.group.cmp(&b.group);
    }
    let first = directed(compare_by(order.mode, a, b), order.reverse);
    order
        .mode
        .tie_breakers()
        .iter()
        .fold(first, |result, layer| {
            result.then_with(|| {
                // A tie-breaker that is the panel's own mode follows its direction too.
                directed(
                    compare_by(*layer, a, b),
                    *layer == order.mode && order.reverse,
                )
            })
        })
}

fn directed(ordering: Ordering, reverse: bool) -> Ordering {
    if reverse {
        ordering.reverse()
    } else {
        ordering
    }
}

fn compare_by(mode: SortMode, a: &Keyed, b: &Keyed) -> Ordering {
    let (entry_a, entry_b) = (&a.entry, &b.entry);
    match mode {
        SortMode::Name => compare_text(&a.name, &b.name),
        SortMode::NameOnly => compare_text(&a.stem, &b.stem),
        SortMode::Extension => compare_text(&a.extension, &b.extension),
        SortMode::Modified => entry_a.modified.cmp(&entry_b.modified),
        SortMode::Size => entry_a.size.cmp(&entry_b.size),
        SortMode::Unsorted => entry_a.position.cmp(&entry_b.position),
        SortMode::Created => entry_a.created.cmp(&entry_b.created),
        SortMode::Accessed => entry_a.accessed.cmp(&entry_b.accessed),
        SortMode::Changed => entry_a.changed.cmp(&entry_b.changed),
        SortMode::Allocated => entry_a.allocated.cmp(&entry_b.allocated),
        SortMode::Description => compare_text(&a.description, &b.description),
    }
}

/// An entry's description (FAR's `DizText`): a provider's `Z` text, empty for none.
fn description_of(entry: &Entry) -> &str {
    entry
        .texts
        .as_ref()
        .map_or("", |texts| texts.description.as_str())
}

/// FAR's `ignore_path_opt`: a name's last part (`PointToName`) when the view shows names only.
fn named(name: &str, names_only: bool) -> &str {
    if names_only {
        name.rsplit(std::path::is_separator).next().unwrap_or(name)
    } else {
        name
    }
}

/// FAR's `name_ext` (`pathmix.cpp`): name and extension split at the last dot after the last separator,
/// the dot belonging to the extension (".gitignore" is all extension). Folders keep the whole name, as
/// with FAR's `SortFolderExt` off.
pub fn name_ext(entry: &Entry) -> (&str, &str) {
    if entry.is_dir {
        return (&entry.name, "");
    }
    split_extension(&entry.name)
}

/// A name split before its extension's dot; a dot in a folder of a whole path is no extension's.
pub fn split_extension(name: &str) -> (&str, &str) {
    // The separators as the system has them (FAR's `path::is_separator`: a backslash on Windows only).
    match name.rfind(|letter| letter == '.' || std::path::is_separator(letter)) {
        Some(at) if name[at..].starts_with('.') => name.split_at(at),
        _ => (name, ""),
    }
}

/// FAR's default string sort (linguistic, case-insensitive, digits as numbers), of names already in the
/// system's form.
fn compare_text(a: &[u16], b: &[u16]) -> Ordering {
    platform::files::compare_wide_names(a, b)
}

#[cfg(test)]
#[path = "sort_paths_tests.rs"]
mod paths_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn file(name: &str, size: u64, position: usize) -> Entry {
        Entry {
            name: name.to_string(),
            size,
            position,
            ..Entry::default()
        }
    }

    fn dir(name: &str, position: usize) -> Entry {
        Entry {
            is_dir: true,
            ..file(name, 0, position)
        }
    }

    fn sorted(mut entries: Vec<Entry>, order: SortOrder) -> Vec<String> {
        sort_entries(&mut entries, &order, |entry| entry.name.starts_with('*'));
        entries.into_iter().map(|entry| entry.name).collect()
    }

    #[test]
    fn name_sort_puts_parent_and_folders_first() {
        let parent = Entry {
            name: "..".into(),
            is_dir: true,
            is_parent: true,
            ..Entry::default()
        };
        let entries = vec![
            file("b.txt", 1, 0),
            dir("Zeta", 1),
            file("A.txt", 1, 2),
            parent,
            dir("alpha", 3),
        ];
        assert_eq!(
            sorted(entries, SortOrder::default()),
            ["..", "alpha", "Zeta", "A.txt", "b.txt"]
        );
    }

    #[test]
    fn same_mode_twice_reverses_and_folders_stay_first() {
        let entries = vec![
            file("a", 1, 0),
            file("c", 1, 1),
            dir("d", 2),
            file("b", 1, 3),
        ];
        let order = SortOrder {
            reverse: true,
            ..SortOrder::default()
        };
        assert_eq!(sorted(entries, order), ["d", "c", "b", "a"]);
    }

    #[test]
    fn size_and_time_default_to_descending_with_name_as_tie_breaker() {
        assert!(SortMode::Size.descending_by_default());
        assert!(!SortMode::Extension.descending_by_default());
        let entries = vec![file("b", 5, 0), file("a", 5, 1), file("big", 50, 2)];
        assert_eq!(
            sorted(entries, SortOrder::new(SortMode::Size)),
            ["big", "a", "b"]
        );

        let at = |seconds| Some(UNIX_EPOCH + Duration::from_secs(seconds));
        let old = Entry {
            modified: at(10),
            ..file("old", 0, 0)
        };
        let new = Entry {
            modified: at(20),
            ..file("new", 0, 1)
        };
        assert_eq!(
            sorted(vec![old, new], SortOrder::new(SortMode::Modified)),
            ["new", "old"]
        );
    }

    #[test]
    fn extension_then_name_only_and_unsorted_keeps_file_system_order() {
        let entries = vec![
            file("z.rs", 1, 0),
            file("a.toml", 1, 1),
            file("b.rs", 1, 2),
            file("noext", 1, 3),
        ];
        assert_eq!(
            sorted(entries.clone(), SortOrder::new(SortMode::Extension)),
            ["noext", "b.rs", "z.rs", "a.toml"]
        );
        assert_eq!(
            sorted(entries.clone(), SortOrder::new(SortMode::NameOnly)),
            ["a.toml", "b.rs", "noext", "z.rs"]
        );
        assert_eq!(
            sorted(entries, SortOrder::new(SortMode::Unsorted)),
            ["z.rs", "a.toml", "b.rs", "noext"]
        );
    }

    #[test]
    fn names_sort_like_far_with_numbers_and_dot_files() {
        let entries = vec![
            file("file10", 1, 0),
            file("File2", 1, 1),
            file("file1", 1, 2),
        ];
        assert_eq!(
            sorted(entries, SortOrder::default()),
            ["file1", "File2", "file10"]
        );
        // ".gitignore" is all extension in FAR: it sorts among the extensions under Ctrl+F4.
        let entries = vec![
            file("b.txt", 1, 0),
            file(".gitignore", 1, 1),
            file("a", 1, 2),
        ];
        assert_eq!(
            sorted(entries, SortOrder::new(SortMode::Extension)),
            ["a", ".gitignore", "b.txt"]
        );
    }

    #[test]
    fn options_change_grouping() {
        let entries = vec![file("b", 1, 0), dir("a-dir", 1), file("*marked", 1, 2)];
        let no_dirs_first = SortOrder {
            directories_first: false,
            ..SortOrder::default()
        };
        assert_eq!(
            sorted(entries.clone(), no_dirs_first),
            ["*marked", "a-dir", "b"]
        );
        let selected_first = SortOrder {
            selected_first: true,
            ..SortOrder::default()
        };
        assert_eq!(sorted(entries, selected_first), ["a-dir", "*marked", "b"]);
    }

    #[test]
    fn ids_round_trip() {
        for mode in ALL_MODES {
            assert_eq!(SortMode::from_id(mode.id()), mode);
        }
        assert_eq!(SortMode::from_id("bogus"), SortMode::Name);
    }

    #[test]
    fn sort_groups_come_after_folders_and_selection_and_keep_their_order_when_reversed() {
        let group = |entry: &Entry| match entry.name.rsplit('.').next() {
            Some("md") => 0,
            Some("txt") => 10_001,
            _ => 10_000,
        };
        let grouped = |order: SortOrder| {
            let mut entries = vec![
                file("a.txt", 0, 0),
                file("b.md", 0, 1),
                file("*c.txt", 0, 2),
                dir("d.txt", 3),
                file("z.log", 0, 4),
            ];
            sort_entries_grouped(
                &mut entries,
                &order,
                |entry| entry.name.starts_with('*'),
                group,
            );
            entries
                .into_iter()
                .map(|entry| entry.name)
                .collect::<Vec<_>>()
        };
        let order = SortOrder {
            sort_groups: true,
            selected_first: true,
            ..SortOrder::default()
        };
        assert_eq!(
            grouped(order),
            ["d.txt", "*c.txt", "b.md", "z.log", "a.txt"]
        );
        let reversed = SortOrder {
            reverse: true,
            ..order
        };
        assert_eq!(
            grouped(reversed),
            ["d.txt", "*c.txt", "b.md", "z.log", "a.txt"]
        );
        let by_extension = SortOrder {
            mode: SortMode::Extension,
            selected_first: false,
            ..order
        };
        assert_eq!(
            grouped(by_extension),
            ["d.txt", "b.md", "z.log", "*c.txt", "a.txt"]
        );
        let off = SortOrder {
            sort_groups: false,
            ..order
        };
        assert_eq!(grouped(off), ["d.txt", "*c.txt", "a.txt", "b.md", "z.log"]);
        let unsorted = SortOrder {
            mode: SortMode::Unsorted,
            ..order
        };
        assert_eq!(
            grouped(unsorted),
            ["d.txt", "*c.txt", "a.txt", "b.md", "z.log"],
            "only by name and extension"
        );
    }

    #[test]
    fn by_description_the_empty_ones_first_and_names_break_ties() {
        let described = |name: &str, description: &str, position: usize| Entry {
            texts: crate::vfs::EntryTexts::of(Vec::new(), description.to_string()),
            ..file(name, 0, position)
        };
        let entries = vec![
            described("c.txt", "beta", 0),
            described("b.txt", "", 1),
            described("a.txt", "Alpha 10", 2),
            described("d.txt", "alpha 9", 3),
        ];
        let order = SortOrder::new(SortMode::Description);
        assert!(!order.reverse, "ascending, as FAR's");
        assert_eq!(
            sorted(entries, order),
            ["b.txt", "d.txt", "a.txt", "c.txt"],
            "case folded, digits as numbers"
        );
        assert_eq!(SortMode::from_id("description"), SortMode::Description);
    }
}
