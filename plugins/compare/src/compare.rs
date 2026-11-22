//! The two panels' items compared as FAR's compare does (`CompareDirs`, `CompareFiles`): by name, case not
//! told apart, folders with folders and files with files (one pair a name; another of the same name in a
//! folder that tells case apart has none); a pair compared by size, time (two seconds' precision, a whole
//! number of quarter hours up to 26 hours ignored as a time zone's) and contents; two folders, with
//! subfolders on, by what is in them down to the depth given (the first difference ends that). What has no
//! pair, or differs, is marked; what is the same unmarked. A link to a folder is gone into as FAR does, but
//! a folder already being compared above is not gone into again (FAR goes round a loop until its depth).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::contents::{self, Contents, Ignore};
use crate::options::Options;

/// One item compared, with what a panel's filter looks at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    pub name: String,
    pub folder: bool,
    /// Windows' `FILE_ATTRIBUTE_*`.
    pub attributes: u32,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub changed: Option<SystemTime>,
    /// Its file on a disk; none for a plugin's item.
    pub path: Option<PathBuf>,
}

/// Windows' hidden attribute.
pub const HIDDEN: u32 = 0x2;

/// Which side an item is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Active,
    Passive,
}

/// The work's surroundings: the options, what a subfolder's listing shows, the stop, the progress.
pub struct Run<'a> {
    pub options: &'a Options,
    /// Hidden items in subfolders (FAR's `ShowHidden`).
    pub hidden: bool,
    /// The bytes read at once for contents.
    pub block: usize,
    pub stop: &'a dyn Fn() -> bool,
    /// The two files or folders compared now.
    pub progress: &'a mut dyn FnMut(&Path, &Path),
    /// Which of a subfolder's items its panel's filter lets through.
    pub filter: &'a dyn Fn(Side, &[Item]) -> Vec<bool>,
    /// A file could not be opened.
    pub open_failed: bool,
    pub stopped: bool,
    /// The folders being compared now, as each side's real path: not gone into again.
    ancestors: Vec<(PathBuf, PathBuf)>,
}

/// A time in FAR's units (100 ns from the epoch, before it negative).
fn ticks(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i128::try_from(after.as_nanos() / 100).unwrap_or(i128::MAX),
        Err(before) => -i128::try_from(before.duration().as_nanos() / 100).unwrap_or(i128::MAX),
    }
}

/// FAR's time compare: within the precision, or (time zones ignored) a whole number of quarter hours
/// apart within it, up to 26 hours.
pub fn same_time(a: Option<SystemTime>, b: Option<SystemTime>, options: &Options) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return a.is_none() && b.is_none();
    };
    let precision: i128 = if options.low_precision { 20_000_000 } else { 0 };
    let delta = (ticks(a) - ticks(b)).abs();
    if delta <= precision {
        return true;
    }
    let quarter = i128::from(Duration::from_secs(15 * 60).as_secs()) * 10_000_000;
    if !options.ignore_time_zone || delta > 26 * 4 * quarter + precision {
        return false;
    }
    let rest = delta % quarter;
    rest.min(quarter - rest) <= precision
}

/// The attributes of an entry (Windows' bits; elsewhere hidden for a dot name).
#[cfg(windows)]
fn attributes(meta: &std::fs::Metadata, _name: &str) -> u32 {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes()
}

#[cfg(not(windows))]
fn attributes(_meta: &std::fs::Metadata, name: &str) -> u32 {
    if name.starts_with('.') { HIDDEN } else { 0 }
}

/// A folder's items as its listing gives them (none of `.`, `..`; hidden ones only when shown).
fn listed(folder: &Path, hidden: bool) -> Vec<Item> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    (entries.flatten())
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let bits = attributes(&meta, &name);
            (hidden || bits & HIDDEN == 0).then(|| Item {
                folder: entry.path().is_dir(),
                attributes: bits,
                size: meta.len(),
                modified: meta.modified().ok(),
                created: meta.created().ok(),
                accessed: meta.accessed().ok(),
                changed: None,
                path: Some(entry.path()),
                name,
            })
        })
        .collect()
}

/// The name an item is paired by.
fn key(item: &Item) -> (bool, String) {
    (item.folder, item.name.to_lowercase())
}

impl<'a> Run<'a> {
    pub fn new(
        options: &'a Options,
        hidden: bool,
        block: usize,
        stop: &'a dyn Fn() -> bool,
        progress: &'a mut dyn FnMut(&Path, &Path),
        filter: &'a dyn Fn(Side, &[Item]) -> Vec<bool>,
    ) -> Self {
        Self {
            options,
            hidden,
            block,
            stop,
            progress,
            filter,
            open_failed: false,
            stopped: false,
            ancestors: Vec::new(),
        }
    }

    /// The items `a` (the active panel's) and `p` (the passive's) compared: each one's mark (`true`:
    /// different or without a pair), and whether no difference was found. `all`: every item marked (else
    /// the first difference ends it, the marks then not whole).
    pub fn compare(
        &mut self,
        a: &[Item],
        p: &[Item],
        all: bool,
        depth: u32,
    ) -> (Vec<bool>, Vec<bool>, bool) {
        let mut marks = (vec![false; a.len()], vec![true; p.len()]);
        let mut paired: std::collections::HashMap<(bool, String), Vec<usize>> =
            std::collections::HashMap::new();
        for (at, item) in p.iter().enumerate().rev() {
            paired.entry(key(item)).or_default().push(at);
        }
        let mut same = true;
        for (at, item) in a.iter().enumerate() {
            if self.stopped || (!same && !all) {
                break;
            }
            if (self.stop)() {
                self.stopped = true;
                break;
            }
            let different = match paired.get_mut(&key(item)).and_then(Vec::pop) {
                Some(other) => {
                    let different = !self.same(item, &p[other], depth);
                    marks.1[other] = different;
                    different
                }
                None => true,
            };
            marks.0[at] = different;
            same &= !different;
        }
        same &= paired.values().all(Vec::is_empty);
        (marks.0, marks.1, same)
    }

    /// Whether two items of one name are the same (FAR's `CompareFiles`).
    fn same(&mut self, a: &Item, p: &Item, depth: u32) -> bool {
        if a.folder {
            return self.same_folders(a, p, depth);
        }
        let options = self.options;
        if options.size && a.size != p.size {
            return false;
        }
        if options.time && !same_time(a.modified, p.modified, options) {
            return false;
        }
        if !options.contents {
            return true;
        }
        let (Some(first), Some(second)) = (&a.path, &p.path) else {
            return true;
        };
        (self.progress)(first, second);
        let ignore = match (options.contents_ignore, options.whitespace) {
            (false, _) => Ignore::Nothing,
            (true, false) => Ignore::NewLines,
            (true, true) => Ignore::Whitespace,
        };
        match contents::compare(first, second, ignore, self.block, self.stop) {
            Contents::Same => true,
            Contents::Different => false,
            Contents::NotOpened => {
                self.open_failed = true;
                false
            }
            Contents::Stopped => {
                self.stopped = true;
                false
            }
        }
    }

    /// Two folders of one name: the same unless subfolders are compared and what is in them differs. A
    /// folder being compared above (a link back to it) is not gone into again.
    fn same_folders(&mut self, a: &Item, p: &Item, depth: u32) -> bool {
        let options = self.options;
        if !options.subfolders || (options.use_max_depth && options.max_depth < depth + 1) {
            return true;
        }
        let (Some(first), Some(second)) = (&a.path, &p.path) else {
            return true;
        };
        let real = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let here = (real(first), real(second));
        let looped = (self.ancestors.iter()).any(|(one, other)| *one == here.0 || *other == here.1);
        if looped {
            return true;
        }
        (self.progress)(&first.join("*"), &second.join("*"));
        let shown = |run: &Self, side, items: Vec<Item>| -> Vec<Item> {
            let passes = (run.filter)(side, &items);
            (items.into_iter().zip(passes))
                .filter_map(|(item, passes)| passes.then_some(item))
                .collect()
        };
        let inside_a = shown(self, Side::Active, listed(first, self.hidden));
        let inside_p = shown(self, Side::Passive, listed(second, self.hidden));
        self.ancestors.push(here);
        let same = self.compare(&inside_a, &inside_p, false, depth + 1).2;
        self.ancestors.pop();
        same
    }
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
