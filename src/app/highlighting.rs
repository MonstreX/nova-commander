//! FAR's files highlighting (`highlight::configuration`, `GetHiColor`): the groups of the settings with
//! their conditions made (masks with the groups of file masks), looked at in FAR's order for an entry;
//! each group that takes it lays its colours over those before it (an empty colour is transparent),
//! its mark too unless the mark is inherited, and the next groups are looked at only when it says
//! "Continue processing". The panel then lays the result over its own colour of each state.

use std::time::SystemTime;

use super::App;
use super::filters::conditions;
use crate::settings::{HighlightColors, HighlightGroup, HighlightGroups};
use crate::vfs::Entry;
use crate::vfs::filter::Conditions;

/// FAR's sixteen console colours, in their order (`C_BLACK` .. `C_WHITE`): a colour is its index.
pub const COLOR_NAMES: [&str; 16] = [
    "black",
    "blue",
    "green",
    "cyan",
    "red",
    "magenta",
    "brown",
    "lightgray",
    "darkgray",
    "lightblue",
    "lightgreen",
    "lightcyan",
    "lightred",
    "lightmagenta",
    "yellow",
    "white",
];

/// A text and a background colour, each one of the sixteen or transparent (`None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Paint {
    pub fg: Option<u8>,
    pub bg: Option<u8>,
}

impl Paint {
    /// `lightgreen`, `lightgreen on blue`, `on blue`; empty or unknown parts are transparent.
    pub fn parse(text: &str) -> Self {
        let index = |name: &str| {
            let name = name.trim().to_ascii_lowercase();
            (COLOR_NAMES.iter())
                .position(|known| *known == name)
                .and_then(|at| u8::try_from(at).ok())
        };
        let text = text.trim().to_ascii_lowercase();
        let (fg, bg) = match text.strip_prefix("on ") {
            Some(bg) => ("", bg),
            None => text.split_once(" on ").unwrap_or((&text, "")),
        };
        Self {
            fg: index(fg),
            bg: index(bg),
        }
    }

    /// FAR's `colors::merge`: what `top` gives over this.
    pub fn under(self, top: Self) -> Self {
        Self {
            fg: top.fg.or(self.fg),
            bg: top.bg.or(self.bg),
        }
    }
}

/// The states of an entry (FAR's `highlight::color::index`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightState {
    Normal,
    Selected,
    Cursor,
    SelectedCursor,
}

impl HighlightState {
    pub fn of(selected: bool, under_cursor: bool) -> Self {
        match (selected, under_cursor) {
            (false, false) => Self::Normal,
            (true, false) => Self::Selected,
            (false, true) => Self::Cursor,
            (true, true) => Self::SelectedCursor,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// What the groups give an entry: each state's name and mark colours, and the mark.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HighlightElement {
    pub names: [Paint; 4],
    pub marks: [Paint; 4],
    pub mark: String,
}

impl HighlightElement {
    pub fn name(&self, state: HighlightState) -> Paint {
        self.names[state.index()]
    }

    pub fn mark(&self, state: HighlightState) -> Paint {
        self.marks[state.index()]
    }
}

/// A group made ready.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Made {
    conditions: Conditions,
    names: [Paint; 4],
    marks: [Paint; 4],
    mark: String,
    mark_inherit: bool,
    continue_processing: bool,
}

/// FAR's `DEFAULT_SORT_GROUP`: an entry no sort group takes, after the upper groups' and before the
/// lower ones'.
const DEFAULT_SORT_GROUP: usize = 10_000;

/// The sort groups made ready (FAR's `highlight::configuration::GetGroup`): the upper ones, then the
/// lower; the first that takes an entry gives its group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SortGroups {
    upper: Vec<Conditions>,
    lower: Vec<Conditions>,
}

impl SortGroups {
    pub fn make(
        groups: &HighlightGroups,
        mask: &dyn Fn(&str) -> Option<crate::vfs::FileMask>,
    ) -> Self {
        let made = |list: &[HighlightGroup]| {
            (list.iter())
                .map(|group| conditions(&group.filter, mask))
                .collect()
        };
        Self {
            upper: made(&groups.upper),
            lower: made(&groups.lower),
        }
    }

    /// The entry's group: an upper group's place, `DEFAULT_SORT_GROUP` for none, past it a lower one's.
    pub fn group(&self, entry: &Entry, now: SystemTime) -> usize {
        let takes = |conditions: &Conditions| conditions.take(entry, now);
        if let Some(at) = self.upper.iter().position(takes) {
            return at;
        }
        (self.lower.iter().position(takes))
            .map_or(DEFAULT_SORT_GROUP, |at| DEFAULT_SORT_GROUP + 1 + at)
    }
}

/// The groups of all four lists, in FAR's order, and as the settings keep them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Highlighting {
    groups: Vec<Made>,
    pub(super) kept: HighlightGroups,
}

fn paints(colors: &HighlightColors) -> ([Paint; 4], [Paint; 4]) {
    let names = [
        &colors.normal,
        &colors.selected,
        &colors.cursor,
        &colors.selected_cursor,
    ];
    let marks = [
        &colors.mark_normal,
        &colors.mark_selected,
        &colors.mark_cursor,
        &colors.mark_selected_cursor,
    ];
    (
        names.map(|text| Paint::parse(text)),
        marks.map(|text| Paint::parse(text)),
    )
}

impl Highlighting {
    /// The groups with their masks made by `mask` (a mask it cannot make takes nothing).
    pub fn make(
        groups: &HighlightGroups,
        mask: &dyn Fn(&str) -> Option<crate::vfs::FileMask>,
    ) -> Self {
        let lists = [&groups.first, &groups.upper, &groups.lower, &groups.last];
        let made = |group: &HighlightGroup| {
            let (names, marks) = paints(&group.colors);
            Made {
                conditions: conditions(&group.filter, mask),
                names,
                marks,
                mark: group.mark.clone(),
                mark_inherit: group.mark_inherit,
                continue_processing: group.continue_processing,
            }
        };
        Self {
            groups: lists.into_iter().flatten().map(made).collect(),
            kept: groups.clone(),
        }
    }

    /// FAR's `GetHiColor`; `None` when no group takes the entry.
    pub fn element(&self, entry: &Entry, now: SystemTime) -> Option<HighlightElement> {
        let mut element: Option<HighlightElement> = None;
        for group in &self.groups {
            if !group.conditions.take(entry, now) {
                continue;
            }
            let mut found = element.take().unwrap_or_default();
            for state in 0..4 {
                found.names[state] = found.names[state].under(group.names[state]);
                found.marks[state] = found.marks[state].under(group.marks[state]);
            }
            if !group.mark_inherit {
                found.mark.clone_from(&group.mark);
            }
            element = Some(found);
            if !group.continue_processing {
                break;
            }
        }
        element
    }
}

impl App {
    /// The groups made again (after the groups of file masks or the groups changed).
    pub(super) fn make_highlighting(&mut self) {
        let mask = |text: &str| self.file_mask(text);
        let kept = &self.highlighting.kept;
        let (highlighting, sort_groups) = (
            Highlighting::make(kept, &mask),
            SortGroups::make(kept, &mask),
        );
        self.highlighting = highlighting;
        for panel in &mut self.panels {
            panel.set_sort_groups(sort_groups.clone());
        }
    }
}

#[cfg(test)]
#[path = "highlighting_tests.rs"]
mod tests;
