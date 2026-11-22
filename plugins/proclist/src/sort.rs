//! The panel's sorts (FAR's proclist `ProcessKey` Ctrl+F3..F12 and `Compare`): FAR's numbers for them
//! (`SM_*`, and the plugin's own from `SM_PROCLIST_CUSTOM`), kept as the setting `SortMode`. The program
//! sorts by the modes it has; by the others (a number of the process's, a counter, the owner, the
//! threads) the plugin lists the processes in that order and the panel is set to "unsorted" (FAR's
//! slot `SM_CHTIME`; the program reverses it for the descending order). Ctrl+F12's menu has FAR's
//! items and each counter, per second too where it can be.

use nc_plugin_sdk::api;
use nc_plugin_sdk::{MenuItem, SortBy};

use crate::counters::{self, COUNTERS};
use crate::table::Process;

/// FAR's sort modes (`OPENPANELINFO_SORTMODES`).
pub const UNSORTED: u32 = 1;
pub const NAME: u32 = 2;
pub const EXTENSION: u32 = 3;
pub const MODIFIED: u32 = 4;
pub const CREATED: u32 = 5;
pub const ACCESSED: u32 = 6;
pub const SIZE: u32 = 7;
pub const DESCRIPTION: u32 = 8;
pub const OWNER: u32 = 9;
pub const ALLOCATED: u32 = 10;
pub const THREADS: u32 = 11;
pub const CHANGED: u32 = 15;
/// proclist's own (`SM_PROCLIST_*`).
pub const PID: u32 = 65;
pub const PARENT: u32 = 66;
pub const PRIORITY: u32 = 67;
pub const COUNTER: u32 = 68;
pub const PER_SECOND: u32 = 128;
/// The tree by parent (owner, 2026-11-15), the panel's first sort: past every one of FAR's.
pub const TREE: u32 = 255;

/// A sort of FAR's menu's first part: its label, its mode, its key, whether it starts descending
/// (FAR's `StaticItems`).
pub struct Fixed {
    pub label: &'static str,
    pub mode: u32,
    pub key: Option<&'static str>,
    pub descending: bool,
}

const fn fixed(
    label: &'static str,
    mode: u32,
    key: Option<&'static str>,
    descending: bool,
) -> Fixed {
    Fixed {
        label,
        mode,
        key,
        descending,
    }
}

pub const FIXED: [Fixed; 12] = [
    fixed(
        "&Name                 Ctrl+F3",
        NAME,
        Some("Ctrl+F3"),
        false,
    ),
    fixed(
        "E&xtension            Ctrl+F4",
        EXTENSION,
        Some("Ctrl+F4"),
        false,
    ),
    fixed("&Size                 Ctrl+F6", SIZE, Some("Ctrl+F6"), true),
    fixed(
        "&Unsorted             Ctrl+F7",
        UNSORTED,
        Some("Ctrl+F7"),
        false,
    ),
    fixed(
        "S&tart time           Ctrl+F8",
        CREATED,
        Some("Ctrl+F8"),
        true,
    ),
    fixed(
        "&Descriptions         Ctrl+F10",
        DESCRIPTION,
        Some("Ctrl+F10"),
        false,
    ),
    fixed(
        "&Owner                Ctrl+F11",
        OWNER,
        Some("Ctrl+F11"),
        false,
    ),
    fixed("PID", PID, None, false),
    fixed("Parent PID", PARENT, None, false),
    fixed("Threads", THREADS, None, true),
    fixed("Priority", PRIORITY, None, true),
    fixed("Process tree", TREE, None, false),
];

/// The keys FAR takes and does nothing on (the write and the access times).
pub const IGNORED: [&str; 2] = ["Ctrl+F5", "Ctrl+F9"];

/// The program's sort (`SORT_*`) for a FAR mode it has; none for one the plugin orders itself.
pub fn program_code(mode: u32) -> Option<u32> {
    Some(match mode {
        UNSORTED => api::SORT_UNSORTED,
        NAME => api::SORT_NAME,
        EXTENSION => api::SORT_EXTENSION,
        MODIFIED => api::SORT_MODIFIED,
        CREATED => api::SORT_CREATED,
        ACCESSED => api::SORT_ACCESSED,
        SIZE => api::SORT_SIZE,
        DESCRIPTION => api::SORT_DESCRIPTION,
        ALLOCATED => api::SORT_ALLOCATED,
        CHANGED => api::SORT_CHANGED,
        _ => return None,
    })
}

/// The FAR mode of the program's sort `code`.
pub fn far_mode(code: u32) -> Option<u32> {
    [
        UNSORTED,
        NAME,
        EXTENSION,
        MODIFIED,
        CREATED,
        ACCESSED,
        SIZE,
        DESCRIPTION,
        ALLOCATED,
        CHANGED,
    ]
    .into_iter()
    .find(|mode| program_code(*mode) == Some(code))
}

/// The program's sort the panel is set to for `mode`: its own, or "unsorted" for the plugin's order.
pub fn panel_code(mode: u32) -> u32 {
    program_code(mode).unwrap_or(api::SORT_UNSORTED)
}

/// The saved mode as FAR reads it: a per second one without its per second (`SM_PROCLIST_PERSEC`); the
/// tree as it is.
pub fn loaded(saved: i64) -> u32 {
    let mode = u32::try_from(saved).unwrap_or(UNSORTED);
    if mode != TREE && mode >= PER_SECOND {
        mode & (PER_SECOND - 1)
    } else {
        mode
    }
}

/// The mode saved as the panel closes (FAR's `FE_CLOSE`): the plugin's own while the panel is
/// "unsorted", else the program's.
pub fn saved(mode: u32, panel: u32) -> u32 {
    if panel == api::SORT_UNSORTED {
        mode
    } else {
        far_mode(panel).unwrap_or(mode)
    }
}

/// The sort the panel opens with (FAR's `StartSortMode`, the order ascending).
pub fn start(mode: u32) -> Option<(SortBy, bool)> {
    let by = match panel_code(mode) {
        api::SORT_NAME => SortBy::Name,
        api::SORT_EXTENSION => SortBy::Extension,
        api::SORT_MODIFIED => SortBy::Modified,
        api::SORT_CREATED => SortBy::Created,
        api::SORT_ACCESSED => SortBy::Accessed,
        api::SORT_SIZE => SortBy::Size,
        api::SORT_DESCRIPTION => SortBy::Description,
        api::SORT_UNSORTED => SortBy::Unsorted,
        _ => return None,
    };
    Some((by, false))
}

/// The number `mode` orders a process by; none for a mode that is not the plugin's.
fn number(process: &Process, mode: u32) -> Option<u64> {
    let counter = |mode: u32, results: bool| {
        let index = usize::try_from(mode.checked_sub(COUNTER)?).ok()?;
        let values = if results {
            &process.results
        } else {
            &process.counters
        };
        values.get(index).copied()
    };
    match mode {
        PID => Some(u64::from(process.pid)),
        PARENT => Some(u64::from(process.parent)),
        PRIORITY => u64::try_from(process.priority).ok().or(Some(0)),
        THREADS => Some(u64::from(process.threads)),
        mode if mode >= PER_SECOND => counter(mode & !PER_SECOND, true),
        mode if mode >= COUNTER => counter(mode, false),
        _ => None,
    }
}

/// `processes` in the plugin's order for `mode` (FAR's `Compare`): `_Total` first, the others by the
/// mode's value, those with the same one as the system listed them. A mode the program sorts by leaves
/// them as they are.
pub fn order(processes: &mut [&Process], mode: u32) {
    order_by(processes, mode, |process| *process);
}

/// The places of `processes` in the plugin's order for `mode` (as `order`).
pub fn places(processes: &[Process], mode: u32) -> Vec<usize> {
    let mut places: Vec<usize> = (0..processes.len()).collect();
    order_by(&mut places, mode, |at| &processes[*at]);
    places
}

fn order_by<'a, T>(items: &mut [T], mode: u32, process: impl Fn(&T) -> &'a Process) {
    // The tree is ordered by its own rows.
    if program_code(mode).is_some() || mode == TREE {
        return;
    }
    if mode == OWNER {
        let owner = |process: &Process| process.owner.as_deref().unwrap_or("").to_lowercase();
        items.sort_by_cached_key(|item| (!process(item).is_total(), owner(process(item))));
        return;
    }
    items.sort_by_key(|item| (!process(item).is_total(), number(process(item), mode)));
}

/// Each counter's menu text and mode, then its per second one where it has one.
fn counted() -> Vec<(String, u32)> {
    let mut counted = Vec::new();
    for (index, counter) in COUNTERS.iter().enumerate() {
        let value = COUNTER + u32::try_from(index).unwrap_or(0);
        counted.push((counter.name.to_string(), value));
        if counters::per_second(index) {
            let text = if index < 3 {
                format!("% {}", counter.name)
            } else {
                format!("{}/s", counter.name)
            };
            counted.push((text, value | PER_SECOND));
        }
    }
    counted
}

/// Ctrl+F12's menu (FAR's `Sort by`): its items, the mode of each (none for the line across), and the
/// item of `mode` checked when the panel is in one of these sorts (`known`).
pub fn menu(mode: u32, known: bool) -> (Vec<MenuItem>, Vec<Option<u32>>) {
    let fixed = FIXED
        .iter()
        .map(|one| Some((one.label.to_string(), one.mode)));
    let line = std::iter::once(None);
    (fixed.chain(line).chain(counted().into_iter().map(Some)))
        .map(|item| match item {
            Some((text, value)) => (
                MenuItem::new(&text).checked(known && value == mode),
                Some(value),
            ),
            None => (MenuItem::separator(), None),
        })
        .unzip()
}

/// Whether the panel's sort is one of the menu's (FAR's `KnownSortingSlot`).
pub fn known(panel: u32) -> bool {
    panel == api::SORT_UNSORTED
        || (far_mode(panel)).is_some_and(|mode| FIXED.iter().any(|one| one.mode == mode))
}

/// The panel's sort and order after `chosen` is picked while `current` is and the panel is `reversed`:
/// the same one again turns the order round, another starts in its own (`descending`). The tree is
/// never turned round: the panel's reversed unsorted order would put the children above their parents.
pub fn picked(current: u32, chosen: u32, reversed: bool, descending: bool) -> (u32, bool) {
    let reverse = if chosen == TREE {
        false
    } else if chosen == current {
        !reversed
    } else {
        descending
    };
    (panel_code(chosen), reverse)
}

/// The fixed sort of a key; none for a key that is none of them.
pub fn of_key(key: &str) -> Option<&'static Fixed> {
    FIXED.iter().find(|one| one.key == Some(key))
}

/// Whether a menu's chosen mode starts descending: a counter always (FAR's `!IsStatic`).
pub fn descending(mode: u32) -> bool {
    (FIXED.iter().find(|one| one.mode == mode)).is_none_or(|one| one.descending)
}

#[cfg(test)]
#[path = "sort_tests.rs"]
mod tests;
