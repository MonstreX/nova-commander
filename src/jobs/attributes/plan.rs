//! What "Set" changes on one object, in FAR's order (`process_single_file`).

use std::time::SystemTime;

use super::times::row_times;
use crate::jobs::elevation::Verb;
pub use crate::platform::files::Setting as Change;
use crate::platform::files::{FileProperties, FileTimes, Flag};

/// FAR's `MSetAttr...CannotFor` (`MCopyCannotCreateLink` for a link's target).
pub fn failure(change: &Change) -> &'static str {
    match change {
        Change::LinkTarget(_) => "Cannot create the link",
        Change::Owner(_) => "Cannot set owner for",
        Change::Compressed(_) => "Cannot set attribute COMPRESSED for",
        Change::Encrypted(_) => "Cannot set attribute ENCRYPTED for",
        Change::Sparse(_) => "Cannot set attribute SPARSE for",
        Change::RemoveReparsePoint => "Cannot remove reparse point for",
        Change::Attributes(_) => "Cannot set attributes for",
        Change::Times(_) => "Cannot set file time for",
    }
}

/// Why FAR's "Access denied" wants the rights for a change (`elevation.cpp`): the calls that open the
/// file through the helper ask "to open", the others say what they do.
pub fn verb(change: &Change) -> Verb {
    match change {
        Change::Owner(_) => Verb::SetOwner,
        Change::Encrypted(true) => Verb::Encrypt,
        Change::Encrypted(false) => Verb::Decrypt,
        Change::Attributes(_) => Verb::SetAttributes,
        Change::LinkTarget(_)
        | Change::Compressed(_)
        | Change::Sparse(_)
        | Change::RemoveReparsePoint
        | Change::Times(_) => Verb::Open,
    }
}

/// FAR's `process_single_file` for one object: `wanted` in [`Flag::ALL`]'s order (`None` leaves it),
/// `owner` blank leaves it, `time` gives a line's new time from the object's own.
pub fn changes(
    properties: &FileProperties,
    wanted: &[Option<bool>],
    owner: &str,
    time: impl Fn(usize, Option<SystemTime>) -> Option<SystemTime>,
) -> Vec<Change> {
    let mut changes = Vec::new();
    let current_owner = properties.owner.as_deref().unwrap_or_default();
    if !owner.is_empty() && !owner.eq_ignore_ascii_case(current_owner) {
        changes.push(Change::Owner(owner.to_string()));
    }
    let current = properties.attributes;
    let new = (Flag::ALL.iter().zip(wanted)).fold(current, |new, (flag, state)| match state {
        Some(on) => new.with(*flag, *on),
        None => new,
    });
    let changed = |flag: Flag| new.has(flag) != current.has(flag);
    if changed(Flag::Compressed) {
        changes.push(Change::Compressed(new.has(Flag::Compressed)));
    }
    if changed(Flag::Encrypted) {
        changes.push(Change::Encrypted(new.has(Flag::Encrypted)));
    }
    if changed(Flag::Sparse) && !current.is_directory() {
        changes.push(Change::Sparse(new.has(Flag::Sparse)));
    }
    if changed(Flag::ReparsePoint) && current.has(Flag::ReparsePoint) {
        changes.push(Change::RemoveReparsePoint);
    }
    let own = [
        Flag::Compressed,
        Flag::Encrypted,
        Flag::Sparse,
        Flag::ReparsePoint,
    ];
    if (Flag::ALL.iter()).any(|flag| !own.contains(flag) && changed(*flag)) {
        changes.push(Change::Attributes(new));
    }
    let originals = row_times(&properties.times);
    let mut new_times = [None; 4];
    for (row, original) in originals.iter().enumerate() {
        new_times[row] = time(row, *original).filter(|new| Some(*new) != *original);
    }
    if new_times.iter().any(Option::is_some) {
        let [modified, created, accessed, changed] = new_times;
        changes.push(Change::Times(FileTimes {
            created,
            accessed,
            modified,
            changed,
        }));
    }
    changes
}
