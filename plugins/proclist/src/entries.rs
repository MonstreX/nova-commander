//! The panel's entries (FAR's `GetList` and `Plist::GetFindData`): one for each process, named by its
//! key, its size the working set and the page file, every time its start; `_Total` hidden, a 32-bit
//! process marked virtual (FAR's `FILE_ATTRIBUTE_VIRTUAL` for another bitness than the system's). The
//! mode's custom columns are written as FAR writes them, each right-aligned in the width it is drawn
//! with; one not drawn (width 0) stays empty, so the next keep their places.

use nc_plugin_sdk::FileEntry;

use crate::columns::{duration, right, size};
use crate::counters::{COUNTERS, Kind};
use crate::modes::{Described, Source, Translated};
use crate::table::{DEFAULT_BITS, Process};

/// Windows' `FILE_ATTRIBUTE_VIRTUAL`.
const VIRTUAL: u32 = 0x1_0000;

/// How the entries are shown: the mode, the widths its columns are drawn with, the user's decimal
/// separator.
pub struct View<'a> {
    pub mode: &'a Translated,
    pub widths: &'a [usize],
    pub decimal: char,
}

/// A process's number for a column; none for one it has not (`_Total`'s but its threads).
fn number(process: &Process, source: Source) -> Option<u64> {
    if process.is_total() {
        return (source == Source::Threads).then_some(u64::from(process.threads));
    }
    let value = match source {
        Source::Priority => i64::from(process.priority),
        Source::Pid => i64::from(process.pid),
        Source::Parent => i64::from(process.parent),
        Source::Threads => i64::from(process.threads),
        Source::Bits => i64::from(process.bits?),
        Source::Gdi => i64::from(process.gdi),
        Source::User => i64::from(process.user),
        Source::Owner | Source::Friendly | Source::Counter { .. } => return None,
    };
    u64::try_from(value).ok()
}

/// A counter's text (FAR's `GetFindData`'s `Format performance counters`).
fn counter_text(process: &Process, source: Source, width: usize, decimal: char) -> String {
    let Source::Counter {
        index,
        per_second,
        thousand,
        float,
    } = source
    else {
        return String::new();
    };
    let Some(counter) = COUNTERS.get(index) else {
        return String::new();
    };
    let room = if float { 0 } else { width };
    match counter.kind {
        Kind::Duration if per_second => process.results[index].to_string(),
        Kind::Duration => duration(process.counters[index]),
        Kind::Bytes => size(process.counters[index], room, thousand, decimal),
        Kind::Number => {
            let value = if per_second {
                process.results[index]
            } else {
                process.counters[index]
            };
            size(value, room, true, decimal)
        }
    }
}

/// The custom column's text for `process` in `width`.
pub fn column(process: &Process, source: Source, width: usize, decimal: char) -> String {
    if width == 0 {
        return String::new();
    }
    if source == Source::Owner {
        return process.owner.clone().unwrap_or_default();
    }
    let text = match number(process, source) {
        Some(value) => value.to_string(),
        None => counter_text(process, source, width, decimal),
    };
    right(&text, width)
}

/// The panel's entry of `process`, `description` its `Z`, `friendly` its `XN`.
pub fn entry(
    process: &Process,
    view: &View,
    description: Option<String>,
    friendly: Option<&str>,
) -> FileEntry {
    let started = process.started();
    let columns = (view.mode.custom.iter())
        .map(|(source, place)| {
            let width = view.widths.get(*place).copied().unwrap_or(0);
            match source {
                // Left as it is, as FAR's name: the program cuts it to the column.
                Source::Friendly => friendly.unwrap_or(&process.name).to_string(),
                _ => column(process, *source, width, view.decimal),
            }
        })
        .collect();
    let other_bits = process.bits.is_some_and(|bits| bits != DEFAULT_BITS);
    FileEntry {
        name: process.key(),
        size: process.size(),
        modified: started,
        created: started,
        accessed: started,
        changed: started,
        hidden: process.is_total(),
        attributes: if other_bits { VIRTUAL } else { 0 },
        columns,
        description: description.unwrap_or_default(),
        ..FileEntry::default()
    }
}

/// What the description shows of `process` that the table has (the window's title and the file's
/// description are the caller's).
pub fn described(process: &Process, kind: Described) -> Option<String> {
    match kind {
        Described::Path => process.path.clone(),
        Described::CommandLine => process.command_line.clone(),
        Described::Window | Described::FileDescription => None,
    }
}

/// The widths the panel draws its columns with ("20,10,8"); unreadable ones 0.
pub fn widths(drawn: &str) -> Vec<usize> {
    (drawn.split(','))
        .map(|width| width.trim().parse().unwrap_or(0))
        .collect()
}

#[cfg(test)]
#[path = "entries_tests.rs"]
mod tests;
