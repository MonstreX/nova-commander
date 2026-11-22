//! FAR's filters (`far/filefilter.cpp`): the user's filters, the "Folders" one and the file types' ones
//! kept marked, each with a mark for each area. A panel reads its folder through its area's filter
//! (`vfs::filter`); Ctrl+U swaps the two panels' marks with the panels (FAR's `SwapPanelFilters`).

use std::collections::BTreeSet;
use std::time::{Duration, SystemTime};

use super::{App, Panel, Side};
use crate::settings::{Filter, FilterMarks, Settings, TypeFilter};
use crate::vfs::filter::{AreaFilter, Conditions, DateKind, Dates, Mark, Marked, type_conditions};
use crate::vfs::{Entry, FileMask};

/// Where a filter is used (FAR's `filter_area`): the panels, Find file, Copy / Move / Link, and Select
/// group's Filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    LeftPanel,
    RightPanel,
    FindFile,
    Copy,
    Select,
}

impl Area {
    pub fn of(side: Side) -> Self {
        match side {
            Side::Left => Self::LeftPanel,
            Side::Right => Self::RightPanel,
        }
    }
}

/// The filters as kept, and the menu a dialog of it stands over.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilterState {
    pub list: Vec<Filter>,
    pub folders: FilterMarks,
    /// The file types' filters by their masks (`*.txt`, `*.` for names without an extension).
    pub types: Vec<TypeFilter>,
    pub(super) behind: Option<super::filters_menu::FiltersMenu>,
    /// The copy, move or link dialog under the filters menu its Filter opened.
    pub(super) under: Option<super::dialog::Dialog>,
    /// Select group's Filter: the panel and whether it selects, by the Select area's filter once the
    /// menu closes (FAR's `SelectFiles` with `bUseFilter`).
    pub(super) then_select: Option<(Side, bool)>,
    /// The filter's dialog (or Select's) while FAR's warning about it is in front.
    pub(super) dialog: Option<(Box<super::form::Form>, super::dialog::FormAction)>,
}

impl FilterState {
    pub fn saved(settings: &Settings) -> Self {
        Self {
            list: settings.filters.clone(),
            folders: settings.folders_filter.clone(),
            types: settings.type_filters.clone(),
            behind: None,
            dialog: None,
            under: None,
            then_select: None,
        }
    }

    pub fn write_to(&self, settings: &mut Settings) {
        settings.filters = self.list.clone();
        settings.folders_filter = self.folders.clone();
        settings.type_filters = self.types.clone();
    }
}

pub fn mark_of(marks: &FilterMarks, area: Area) -> Mark {
    let text = match area {
        Area::LeftPanel => &marks.left_panel,
        Area::RightPanel => &marks.right_panel,
        Area::FindFile => &marks.find_file,
        Area::Copy => &marks.copy,
        Area::Select => &marks.select,
    };
    Mark::from_check(text.chars().next())
}

pub fn set_mark(marks: &mut FilterMarks, area: Area, mark: Mark) {
    let text = mark.check().map(String::from).unwrap_or_default();
    match area {
        Area::LeftPanel => marks.left_panel = text,
        Area::RightPanel => marks.right_panel = text,
        Area::FindFile => marks.find_file = text,
        Area::Copy => marks.copy = text,
        Area::Select => marks.select = text,
    }
}

/// Marked anywhere at all.
pub fn marked_anywhere(marks: &FilterMarks) -> bool {
    [
        &marks.left_panel,
        &marks.right_panel,
        &marks.find_file,
        &marks.copy,
        &marks.select,
        &marks.custom,
    ]
    .iter()
    .any(|mark| !mark.is_empty())
}

fn swap_marks(marks: &mut FilterMarks) {
    std::mem::swap(&mut marks.left_panel, &mut marks.right_panel);
}

/// FAR's `enumFDateType` as written in the settings.
pub fn date_kind(text: &str) -> DateKind {
    match text {
        "created" => DateKind::Created,
        "accessed" => DateKind::Accessed,
        "changed" => DateKind::Changed,
        _ => DateKind::Modified,
    }
}

fn dates_of(filter: &Filter) -> Dates {
    let seconds = |value: u64| Duration::from_secs(value);
    if filter.date_relative {
        Dates::Relative {
            after: seconds(filter.date_after),
            before: seconds(filter.date_before),
        }
    } else {
        let point = |value: u64| (value != 0).then(|| SystemTime::UNIX_EPOCH + seconds(value));
        Dates::Absolute {
            after: point(filter.date_after),
            before: point(filter.date_before),
        }
    }
}

/// A filter's conditions, its mask read by `mask`.
pub fn conditions(filter: &Filter, mask: &dyn Fn(&str) -> Option<FileMask>) -> Conditions {
    let size = crate::vfs::filter::parse_size;
    let made = filter.use_mask.then(|| mask(&filter.mask));
    Conditions {
        mask: made.clone().flatten(),
        broken_mask: made.is_some_and(|made| made.is_none()),
        size: filter
            .use_size
            .then(|| (size(&filter.size_above), size(&filter.size_below))),
        date: filter
            .use_date
            .then(|| (date_kind(&filter.date_type), dates_of(filter))),
        attributes: filter
            .use_attributes
            .then_some((filter.attributes_set, filter.attributes_clear)),
        hard_links: filter.use_hard_links,
    }
}

/// FAR's type mask of a file (`name_ext`): `*.ext` (`*.gitignore` for `.gitignore`), `*.` without a
/// dot; none for folders.
pub fn type_mask(entry: &Entry) -> Option<String> {
    if entry.is_dir || entry.is_parent {
        return None;
    }
    Some(match entry.name.rsplit_once('.') {
        Some((_, extension)) => format!("*.{extension}"),
        None => "*.".to_string(),
    })
}

impl App {
    /// The filter of `area`, its masks with the groups.
    pub(super) fn area_filter(&self, area: Area) -> AreaFilter {
        let mask = |text: &str| self.file_mask(text);
        let marked = |marks: &FilterMarks, conditions: Conditions| Marked {
            mark: mark_of(marks, area),
            conditions,
        };
        AreaFilter {
            filters: (self.filters.list.iter())
                .map(|filter| marked(&filter.marks, conditions(filter, &mask)))
                .collect(),
            folders: mark_of(&self.filters.folders, area),
            types: (self.filters.types.iter())
                .filter_map(|kind| {
                    // FAR quotes a type with `,` or `;` in it.
                    let quoted = if kind
                        .mask
                        .get(2..)
                        .is_some_and(|rest| rest.contains([',', ';']))
                    {
                        format!("\"{}\"", kind.mask)
                    } else {
                        kind.mask.clone()
                    };
                    // With the groups, as FAR expands them in every mask (`<name>` in a type).
                    let mask = self.file_mask(&quoted)?;
                    Some(marked(&kind.marks, type_conditions(mask)))
                })
                .collect(),
            select: area == Area::Select,
        }
    }

    /// The panels read again through their filters, when those changed.
    pub(super) fn apply_filters(&mut self) {
        for side in [Side::Left, Side::Right] {
            let filter = self.area_filter(Area::of(side));
            self.panels[side.index()].set_filter(filter);
        }
    }

    /// Ctrl+U: the marks of the panels go with them.
    pub(super) fn swap_filter_marks(&mut self) {
        for filter in &mut self.filters.list {
            swap_marks(&mut filter.marks);
        }
        swap_marks(&mut self.filters.folders);
        for kind in &mut self.filters.types {
            swap_marks(&mut kind.marks);
        }
    }
}

impl Panel {
    /// A new filter reads the folder again, keeping the cursor and the selection.
    pub(super) fn set_filter(&mut self, filter: AreaFilter) {
        if self.filter != filter {
            self.filter = filter;
            self.reload();
        }
    }

    /// FAR's `*` beside the sort mode.
    pub fn filter_is_on(&self) -> bool {
        self.filter.is_on()
    }

    /// The entries the filter takes; the types of the files it hides are noted for its menu.
    pub(super) fn apply_filter(&mut self, entries: &mut Vec<Entry>) {
        self.filtered_types.clear();
        if !self.filter.is_on() {
            return;
        }
        let now = SystemTime::now();
        let mut hidden = BTreeSet::new();
        entries.retain(|entry| {
            let keep = entry.is_parent || self.filter.takes(entry, now);
            if !keep && let Some(mask) = type_mask(entry) {
                hidden.insert(mask);
            }
            keep
        });
        self.filtered_types = hidden;
    }
}

#[cfg(test)]
#[path = "filters_tests.rs"]
mod tests;
