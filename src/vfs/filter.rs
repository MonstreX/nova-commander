//! FAR's file filters (`far/filefilter.cpp` `multifilter::FileInFilterEx`, `far/filefilterparams.cpp`
//! `FileFilterParams::FileInFilter`): the user's filters, the "Folders" one and the filters of the panel's
//! file types, each marked for an area (a panel, Find file, Copy, Select) as `+` (include), `-`
//! (exclude), `I` or `X` (the same, strong: they stop the filters after them). The first filter that
//! takes an entry decides, a strong one over the weak ones before it; an entry none takes is shown
//! unless some filter includes, and a folder none takes is shown unless a filter includes folders.

use std::time::{Duration, SystemTime};

use super::{Entry, FileMask};

/// FAR's `FILE_ATTRIBUTE_DIRECTORY`, as the filters keep it.
pub const DIRECTORY: u32 = 0x0000_0010;

/// How a filter is marked for an area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mark {
    #[default]
    None,
    Include,
    Exclude,
    StrongInclude,
    StrongExclude,
}

impl Mark {
    /// The menu's check character (FAR's `GetCheck`).
    pub fn check(self) -> Option<char> {
        match self {
            Self::None => None,
            Self::Include => Some('+'),
            Self::Exclude => Some('-'),
            Self::StrongInclude => Some('I'),
            Self::StrongExclude => Some('X'),
        }
    }

    pub fn from_check(check: Option<char>) -> Self {
        match check {
            Some('+') => Self::Include,
            Some('-') => Self::Exclude,
            Some('I') => Self::StrongInclude,
            Some('X') => Self::StrongExclude,
            _ => Self::None,
        }
    }

    fn include(self) -> bool {
        matches!(self, Self::Include | Self::StrongInclude)
    }

    fn strong(self) -> bool {
        matches!(self, Self::StrongInclude | Self::StrongExclude)
    }
}

/// Which time of an entry a date condition looks at (FAR's `enumFDateType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateKind {
    #[default]
    Modified,
    Created,
    Accessed,
    Changed,
}

/// The bounds of a date condition; zero is no bound (FAR's `filter_dates`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dates {
    /// So long before now.
    Relative { after: Duration, before: Duration },
    Absolute {
        after: Option<SystemTime>,
        before: Option<SystemTime>,
    },
}

/// The conditions of a filter, its masks read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Conditions {
    pub mask: Option<FileMask>,
    /// Bytes at least and at most.
    pub size: Option<(Option<u64>, Option<u64>)>,
    pub date: Option<(DateKind, Dates)>,
    /// FAR's `FILE_ATTRIBUTE_*` bits that must be set and that must be clear.
    pub attributes: Option<(u32, u32)>,
    /// More than one hard link (the number is not known yet: no entry takes it).
    pub hard_links: bool,
    /// A mask was asked for and could not be made (an unknown group): nothing is taken (FAR's empty
    /// `filemasks`, whose `check` fails).
    pub broken_mask: bool,
}

impl Conditions {
    /// FAR's `FileInFilter`: whether every condition used takes the entry.
    pub fn take(&self, entry: &Entry, now: SystemTime) -> bool {
        if self.broken_mask {
            return false;
        }
        if let Some((set, clear)) = self.attributes {
            let bits = bits_of(entry);
            if bits & set != set || bits & clear != 0 {
                return false;
            }
        }
        if let Some((above, below)) = self.size
            && (above.is_some_and(|above| entry.size < above)
                || below.is_some_and(|below| entry.size > below))
        {
            return false;
        }
        if let Some((kind, dates)) = self.date
            && !dates_take(kind, dates, entry, now)
        {
            return false;
        }
        if self
            .mask
            .as_ref()
            .is_some_and(|mask| !mask.matches(&entry.name))
        {
            return false;
        }
        !self.hard_links
    }
}

/// The entry's attributes as FAR's bits.
fn bits_of(entry: &Entry) -> u32 {
    let mut bits = entry.attributes.windows_flags();
    if entry.is_dir {
        bits |= DIRECTORY;
    }
    bits
}

fn dates_take(kind: DateKind, dates: Dates, entry: &Entry, now: SystemTime) -> bool {
    let time = match kind {
        DateKind::Modified => entry.modified,
        DateKind::Created => entry.created,
        DateKind::Accessed => entry.accessed,
        DateKind::Changed => entry.changed,
    };
    let time = time.unwrap_or(SystemTime::UNIX_EPOCH);
    let (after, before) = match dates {
        Dates::Relative { after, before } => (
            (!after.is_zero()).then(|| now.checked_sub(after)).flatten(),
            (!before.is_zero())
                .then(|| now.checked_sub(before))
                .flatten(),
        ),
        Dates::Absolute { after, before } => (after, before),
    };
    !(after.is_some_and(|after| time < after) || before.is_some_and(|before| time > before))
}

/// A filter with its mark for the area looked at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marked {
    pub mark: Mark,
    pub conditions: Conditions,
}

/// Everything that filters one area, in FAR's order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AreaFilter {
    /// The user's filters.
    pub filters: Vec<Marked>,
    /// "Folders": takes the folders.
    pub folders: Mark,
    /// The file types' filters (`*.txt`); they take no folder.
    pub types: Vec<Marked>,
    /// Select's area: folders are not shown by default.
    pub select: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Action {
    #[default]
    Ignore,
    Include,
    Exclude,
}

/// FAR's `filter_result` as Find file reads it (`FileInFilterEx`): taken by an including filter, by
/// an excluding one (a folder is not walked into), or by none, while some filter includes or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Include,
    Exclude,
    Ignore { has_include: bool },
}

impl Verdict {
    /// Whether the object is listed (FAR's `filter_result::operator bool`).
    pub fn lists(self) -> bool {
        match self {
            Self::Include => true,
            Self::Exclude => false,
            Self::Ignore { has_include } => !has_include,
        }
    }
}

/// FAR's `filter_result`.
#[derive(Debug, Default)]
struct Outcome {
    action: Action,
    file_include: bool,
    folder_include: bool,
}

impl Outcome {
    /// One filter; false: the filters after it are not looked at.
    fn process(&mut self, filter: &Marked, fast: bool, entry: &Entry, now: SystemTime) -> bool {
        let mark = filter.mark;
        if mark == Mark::None {
            return true;
        }
        if self.action != Action::Ignore && !mark.strong() {
            return true;
        }
        if mark.include() && !(self.file_include && self.folder_include) {
            let (used, clear) = filter
                .conditions
                .attributes
                .map_or((false, 0), |(_, clear)| (true, clear));
            let no_folders = clear & DIRECTORY != 0;
            self.file_include |= !used || no_folders;
            self.folder_include |= !used || !no_folders;
        }
        if fast || !filter.conditions.take(entry, now) {
            return true;
        }
        self.action = if mark.include() {
            Action::Include
        } else {
            Action::Exclude
        };
        !mark.strong()
    }
}

impl AreaFilter {
    /// FAR's `IsEnabledOnPanel`: some filter is marked.
    pub fn is_on(&self) -> bool {
        self.folders != Mark::None
            || (self.filters.iter())
                .chain(&self.types)
                .any(|filter| filter.mark != Mark::None)
    }

    /// FAR's `FileInFilter`: a folder none takes is shown unless a filter includes folders (not in
    /// Select's area).
    pub fn takes(&self, entry: &Entry, now: SystemTime) -> bool {
        let outcome = self.outcome(entry, now);
        let by_default =
            outcome.action == Action::Ignore && !outcome.folder_include && !self.select;
        if entry.is_dir && by_default {
            return true;
        }
        Self::verdict_of(&outcome).lists()
    }

    /// FAR's `FileInFilterEx`, with no folder taken by default.
    pub fn verdict(&self, entry: &Entry, now: SystemTime) -> Verdict {
        Self::verdict_of(&self.outcome(entry, now))
    }

    fn verdict_of(outcome: &Outcome) -> Verdict {
        match outcome.action {
            Action::Include => Verdict::Include,
            Action::Exclude => Verdict::Exclude,
            Action::Ignore => Verdict::Ignore {
                has_include: outcome.file_include || outcome.folder_include,
            },
        }
    }

    /// The user's filters, "Folders", then the types, as FAR's `FileInFilterEx` goes through them.
    fn outcome(&self, entry: &Entry, now: SystemTime) -> Outcome {
        let mut outcome = Outcome::default();
        let folder = entry.is_dir;
        let folders = Marked {
            mark: self.folders,
            conditions: Conditions {
                attributes: Some((DIRECTORY, 0)),
                ..Conditions::default()
            },
        };
        let _ = self
            .filters
            .iter()
            .all(|filter| outcome.process(filter, false, entry, now))
            && outcome.process(&folders, !folder, entry, now)
            && self
                .types
                .iter()
                .all(|filter| outcome.process(filter, folder, entry, now));
        outcome
    }
}

/// A file type's filter (FAR's auto filter): the mask, files only.
pub fn type_conditions(mask: FileMask) -> Conditions {
    Conditions {
        mask: Some(mask),
        attributes: Some((0, DIRECTORY)),
        ..Conditions::default()
    }
}

/// FAR's `CheckFileSizeStringFormat` and `ConvertFileSizeString`: digits, then at most one of B, K, M,
/// G, T, P, E - upper case for the powers of 1024, lower case for those of 1000. `None`: written wrong.
pub fn parse_size(text: &str) -> Option<u64> {
    let digits = text.trim_end_matches(|ch: char| !ch.is_ascii_digit());
    let suffix = &text[digits.len()..];
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) || suffix.len() > 1 {
        return None;
    }
    let number: u64 = digits.parse().ok()?;
    let (base, power): (u64, u32) = match suffix {
        "" | "B" | "b" => (1, 0),
        _ => {
            let at = "KMGTPE".find(suffix.to_ascii_uppercase().as_str())?;
            let base = if suffix.chars().all(char::is_uppercase) {
                1024
            } else {
                1000
            };
            (base, u32::try_from(at).ok()? + 1)
        }
    };
    number.checked_mul(base.checked_pow(power)?)
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
