//! One item of an archive as the panel shows it (arclite's `ArcFileInfo` with its properties), whatever
//! its format: its path, sizes, times, Windows attributes, whether it is encrypted; and the file times'
//! units (`FILETIME`'s 100 ns from 1601) the library gives and takes.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `FILE_ATTRIBUTE_DIRECTORY`.
pub const DIRECTORY: u32 = 0x10;

/// 100 ns units from 1601 to 1970.
const TICKS_TO_1970: u64 = 116_444_736_000_000_000;

/// One file or folder the archive holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    /// Its name inside the archive split at `/` and `\` (7-Zip's `GetOsPath` on Windows), no empty parts.
    pub path: Vec<String>,
    pub is_dir: bool,
    pub size: u64,
    /// The packed size (7-Zip's `kpidPackSize`).
    pub packed: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    /// Windows' `FILE_ATTRIBUTE_*` bits.
    pub attributes: u32,
    pub encrypted: bool,
}

/// A name's parts between `/`, `\` and U+F05C (arclite's `is_dir_split`: a `\` some tools map there), the
/// empty ones (a leading, doubled or closing separator) dropped.
pub fn split(name: &str) -> Vec<String> {
    (name.split(['/', '\\', '\u{f05c}']))
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// A moment as 100 ns units since 1601 (UTC), as the system's file times count; none before 1601.
pub fn to_ticks(moment: SystemTime) -> Option<i64> {
    let ticks = match moment.duration_since(UNIX_EPOCH) {
        Ok(after) => i128::from(TICKS_TO_1970) + i128::try_from(after.as_nanos() / 100).ok()?,
        Err(before) => {
            i128::from(TICKS_TO_1970) - i128::try_from(before.duration().as_nanos() / 100).ok()?
        }
    };
    i64::try_from(ticks).ok().filter(|ticks| *ticks >= 0)
}

/// 100 ns units since 1601 (UTC) as a moment; none for 0 (not set).
pub fn from_ticks(ticks: u64) -> Option<SystemTime> {
    if ticks == 0 {
        return None;
    }
    let since = |units: u64| Duration::from_nanos(units.saturating_mul(100));
    if ticks >= TICKS_TO_1970 {
        UNIX_EPOCH.checked_add(since(ticks - TICKS_TO_1970))
    } else {
        UNIX_EPOCH.checked_sub(since(TICKS_TO_1970 - ticks))
    }
}

#[cfg(test)]
#[path = "item_tests.rs"]
mod tests;
