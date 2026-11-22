//! WinSCP's file masks (`FileMasks.cpp` `TFileMasks`, `Masks::TMask`; `Common.cpp` `TryStrToSize`,
//! `TryRelativeStrToDateTime`, `TryStrToDateTimeStandard`) for the transfer settings' "File mask" and the
//! text mode's masks: masks split by `;` or `,` (a doubled one is the character), those after `|` excluding;
//! a mask ending with `/` (or `\`) a folder's; a path before the name matched against the item's folder;
//! `*`, `?` and `[a-z]` / `[!a]` without regard to case, `*.` a name without an extension; size and time
//! bounds after `>` (above), `<` (below), `>=` and `<=`: a size with `K`, `M` or `G`, a date
//! `yyyy-mm-dd[ hh:nn[:ss]]` (local) or a span back from now (`1D`, `2H`, `30N`, `10S`, `1Y`, `S` after it
//! from the span's start, `today`, `yesterday`). A folder is taken when it or one of its folders matches
//! an including mask of folders (or there is none) and no excluding one matches it. The masks' relative
//! folders (`./`) match nothing: WinSCP's roots for them come with the synchronization.

use nc_plugin_sdk::system::{dos_time_of, dos_time_ticks, now_ticks};

/// 100 ns units between 1601 and 1970.
const UNIX_TICKS: i64 = 11_644_473_600 * 10_000_000;
const TICKS_PER_SECOND: i64 = 10_000_000;

/// What an item is matched with besides its name: its size and its time (seconds since 1970).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Params {
    pub size: u64,
    pub modified: i64,
}

/// A bound of a size or a time: above or below, the value itself taken (`=`) or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Bound {
    value: i64,
    closed: bool,
}

/// A name's part of a mask.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Name {
    Any,
    /// `*.`: a name without a dot, or one ending with it.
    NoExt,
    Regular(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Mask {
    /// The folder's part: `None` any folder.
    folder: Option<String>,
    name: Name,
    /// Above, below.
    size: (Option<Bound>, Option<Bound>),
    modified: (Option<Bound>, Option<Bound>),
}

/// The masks as typed and parsed: files' including and excluding, folders' including and excluding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileMasks {
    text: String,
    masks: [Vec<Mask>; 4],
}

/// WinSCP's `MASK_ERROR`.
pub fn mask_error(near: &str) -> String {
    format!("Mask is invalid near '{near}'.")
}

/// `text` cut at `delimiters` (a doubled one is the character): each piece and the delimiter after it.
fn pieces(text: &str, delimiters: &[char]) -> Vec<(String, Option<char>)> {
    let mut found = Vec::new();
    let mut piece = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if !delimiters.contains(&ch) {
            piece.push(ch);
        } else if chars.peek() == Some(&ch) {
            chars.next();
            piece.push(ch);
        } else {
            found.push((std::mem::take(&mut piece), Some(ch)));
        }
    }
    found.push((piece, None));
    found
}

/// WinSCP's `IsEffectiveFileNameMask` and `CreateMaskMask` with `Ex`.
fn name_of(mask: &str) -> Name {
    match mask {
        "" | "*" | "*.*" => Name::Any,
        "*." => Name::NoExt,
        _ => Name::Regular(mask.to_string()),
    }
}

/// A folder's mask with `/` for `\`, without its trailing `/` (the root's kept).
fn folder_of(part: &str) -> String {
    let unix = part.replace('\\', "/");
    match unix.len() > 1 && unix.ends_with('/') {
        true => unix[..unix.len() - 1].to_string(),
        false => unix,
    }
}

impl Mask {
    fn new() -> Self {
        Self {
            folder: None,
            name: Name::Any,
            size: (None, None),
            modified: (None, None),
        }
    }

    /// The name's part: a folder's mask (ending with a slash), the folder's part before the name.
    fn set_name(&mut self, part: &str) -> bool {
        let directory = part.ends_with(['/', '\\']);
        let part = part.trim_end_matches(['/', '\\']);
        match part.rfind(['/', '\\']) {
            Some(at) => {
                self.folder = Some(folder_of(&part[..=at]));
                self.name = name_of(&part[at + 1..]);
            }
            None => self.name = name_of(part),
        }
        directory
    }

    /// A bound after `>` (`low`) or `<`: a time when the value reads as one, else a size.
    fn set_bound(&mut self, value: &str, low: bool) -> Result<(), ()> {
        let (closed, value) = match value.strip_prefix('=') {
            Some(rest) => (true, rest.trim()),
            None => (false, value),
        };
        let time = match value.parse::<i64>() {
            Ok(_) => relative_time(value, now_ticks()),
            Err(_) => standard_time(value).or_else(|| relative_time(value, now_ticks())),
        };
        let (kept, value) = match time {
            Some(seconds) => (&mut self.modified, seconds),
            None => (&mut self.size, size_of(value).ok_or(())?),
        };
        let slot = if low { &mut kept.0 } else { &mut kept.1 };
        if slot.is_some() {
            return Err(());
        }
        *slot = Some(Bound { value, closed });
        Ok(())
    }

    /// WinSCP's `CreateMask`: the mask and whether it is a folder's; `Err` when it is wrong.
    fn parse(text: &str) -> Result<(Self, bool), ()> {
        let mut mask = Self::new();
        let mut directory = false;
        let mut delimiter = None;
        for (part, next) in pieces(text, &['<', '>']) {
            let part = part.trim();
            match delimiter {
                // A folder's mask takes no size nor time.
                Some(_) if directory => return Err(()),
                Some(found) => mask.set_bound(part, found == '>')?,
                None if !part.is_empty() => directory = mask.set_name(part),
                None => {}
            }
            delimiter = next;
        }
        Ok((mask, directory))
    }
}

/// WinSCP's `TryStrToSize`: digits, then `K`, `M` or `G`.
fn size_of(text: &str) -> Option<i64> {
    let digits = text
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(text.len());
    let size: i64 = text.get(..digits)?.parse().ok()?;
    let shift = match text[digits..].trim().to_ascii_uppercase().as_str() {
        "" => 0,
        "K" => 10,
        "M" => 20,
        "G" => 30,
        _ => return None,
    };
    size.checked_mul(1 << shift)
}

fn seconds_of(ticks: i64) -> i64 {
    (ticks - UNIX_TICKS).div_euclid(TICKS_PER_SECOND)
}

/// A local date and time as seconds since 1970.
fn local_seconds(date: [u16; 3], time: [u16; 3]) -> Option<i64> {
    dos_time_ticks(date, time).map(seconds_of)
}

/// WinSCP's `TryStrToDateTimeStandard`: `yyyy-mm-dd`, then `hh:nn` or `hh:nn:ss`, local.
fn standard_time(text: &str) -> Option<i64> {
    let (date, time) = text.split_once(' ').unwrap_or((text, ""));
    let numbers = |text: &str, separator: char| -> Option<Vec<u16>> {
        text.split(separator)
            .map(|part| part.trim().parse().ok())
            .collect()
    };
    let date = numbers(date, '-')?;
    let [year, month, day] = date[..] else {
        return None;
    };
    let time = match time.trim() {
        "" => vec![0, 0, 0],
        time => numbers(time, ':')?,
    };
    let (hour, minute, second) = match time[..] {
        [hour, minute] => (hour, minute, 0),
        [hour, minute, second] => (hour, minute, second),
        _ => return None,
    };
    local_seconds([year, month, day], [hour, minute, second])
}

/// WinSCP's `TryRelativeStrToDateTime` back from `now` (100 ns units since 1601).
fn relative_time(text: &str, now: i64) -> Option<i64> {
    let text = match text.trim().to_ascii_lowercase().as_str() {
        "today" => "0DS".to_string(),
        "yesterday" => "1DS".to_string(),
        other => other.to_ascii_uppercase(),
    };
    let digits = text
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(text.len());
    let number: i64 = text[..digits].parse().ok()?;
    let unit = text[digits..].trim();
    let (unit, start) = match unit.len() == 2 && unit.ends_with('S') {
        true => (&unit[..1], true),
        false => (unit, false),
    };
    let seconds = match unit {
        "S" => 1,
        "N" => 60,
        "H" => 3_600,
        "D" => 86_400,
        "Y" => return years_back(now, number, start),
        _ => return None,
    };
    let moment = seconds_of(now) - number.checked_mul(seconds)?;
    if !start {
        return Some(moment);
    }
    let (date, [hour, minute, second]) = dos_time_of(moment * TICKS_PER_SECOND + UNIX_TICKS)?;
    let time = match unit {
        "S" => [hour, minute, second],
        "N" => [hour, minute, 0],
        "H" => [hour, 0, 0],
        _ => [0, 0, 0],
    };
    local_seconds(date, time)
}

/// `number` years back from `now`, from the year's start with `start`.
fn years_back(now: i64, number: i64, start: bool) -> Option<i64> {
    let ([year, month, day], time) = dos_time_of(now)?;
    let year = u16::try_from(i64::from(year) - number).ok()?;
    match start {
        true => local_seconds([year, 1, 1], [0, 0, 0]),
        false => local_seconds([year, month, day.min(28)], time),
    }
}

/// `Masks::TMask`: `*`, `?`, `[set]` (`a-z`, `!` first for none of them), without regard to case.
pub fn glob(mask: &str, text: &str) -> bool {
    let mask: Vec<char> = mask.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    glob_at(&mask, &text)
}

fn glob_at(mask: &[char], text: &[char]) -> bool {
    match mask.split_first() {
        None => text.is_empty(),
        Some(('*', rest)) => (0..=text.len()).any(|skip| glob_at(rest, &text[skip..])),
        Some(('?', rest)) => !text.is_empty() && glob_at(rest, &text[1..]),
        Some(('[', rest)) => match (rest.iter().position(|&ch| ch == ']'), text.first()) {
            (Some(end), Some(&ch)) => {
                in_set(&rest[..end], ch) && glob_at(&rest[end + 1..], &text[1..])
            }
            (None, Some(&'[')) => glob_at(rest, &text[1..]),
            _ => false,
        },
        Some((&ch, rest)) => text.first() == Some(&ch) && glob_at(rest, &text[1..]),
    }
}

/// A `[set]`'s characters and ranges, `!` first for none of them.
fn in_set(set: &[char], ch: char) -> bool {
    let (negated, set) = match set.split_first() {
        Some(('!', rest)) => (true, rest),
        _ => (false, set),
    };
    let mut found = false;
    let mut at = 0;
    while at < set.len() {
        if at + 2 < set.len() && set[at + 1] == '-' {
            found |= (set[at]..=set[at + 2]).contains(&ch);
            at += 3;
        } else {
            found |= set[at] == ch;
            at += 1;
        }
    }
    found != negated
}

fn bound_holds(bound: Option<Bound>, value: Option<i64>, low: bool) -> bool {
    match (bound, value) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(bound), Some(value)) => match (low, bound.closed) {
            (true, true) => value >= bound.value,
            (true, false) => value > bound.value,
            (false, true) => value <= bound.value,
            (false, false) => value < bound.value,
        },
    }
}

impl Mask {
    fn matches(&self, name: &str, folder: &str, params: Option<Params>) -> bool {
        let folder_matches = self.folder.as_ref().is_none_or(|mask| glob(mask, folder));
        let name_matches = match &self.name {
            Name::Any => true,
            Name::NoExt => !name.contains('.') || name.ends_with('.'),
            Name::Regular(mask) => glob(mask, name),
        };
        let size = params.map(|params| i64::try_from(params.size).unwrap_or(i64::MAX));
        let modified = params.map(|params| params.modified);
        folder_matches
            && name_matches
            && bound_holds(self.size.0, size, true)
            && bound_holds(self.size.1, size, false)
            && bound_holds(self.modified.0, modified, true)
            && bound_holds(self.modified.1, modified, false)
    }
}

/// The item's name and its folder (`/`-separated, without its trailing `/` but the root's).
fn split_path(path: &str, local: bool) -> (String, String) {
    let path = if local {
        path.replace('\\', "/")
    } else {
        path.to_string()
    };
    match path.rfind('/') {
        Some(0) => (path[1..].to_string(), "/".to_string()),
        Some(at) => (path[at + 1..].to_string(), path[..at].to_string()),
        None => (path, String::new()),
    }
}

/// WinSCP's `MatchesMasks`: one of `masks` takes the item, or (`recurse`) a folder's folder.
fn any_matches(
    masks: &[Mask],
    (name, folder): (&str, &str),
    directory: bool,
    recurse: bool,
) -> bool {
    if masks.iter().any(|mask| mask.matches(name, folder, None)) {
        return true;
    }
    if !directory || !recurse || folder.is_empty() || folder == "/" {
        return false;
    }
    let (parent_name, parent) = split_path(folder, false);
    any_matches(masks, (&parent_name, &parent), true, true)
}

impl FileMasks {
    /// WinSCP's `SetStr`: `Err` with WinSCP's message when a mask is wrong.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut masks: [Vec<Mask>; 4] = Default::default();
        let mut include = true;
        for (piece, delimiter) in pieces(text, &[';', ',', '|']) {
            let piece = piece.trim();
            if !piece.is_empty() {
                let (mask, directory) = Mask::parse(piece).map_err(|()| mask_error(piece))?;
                masks[usize::from(directory) * 2 + usize::from(!include)].push(mask);
            }
            if delimiter == Some('|') {
                if !include {
                    return Err(mask_error(text));
                }
                include = false;
            }
        }
        Ok(Self {
            text: text.to_string(),
            masks,
        })
    }

    /// As typed.
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.masks.iter().all(Vec::is_empty)
    }

    /// WinSCP's `Matches`: the item `path` (a disk's when `local`), a folder or a file with `params`.
    pub fn matches(
        &self,
        path: &str,
        local: bool,
        directory: bool,
        params: Option<Params>,
    ) -> bool {
        let (name, folder) = split_path(path, local);
        let base = usize::from(directory) * 2;
        let (include, exclude) = (&self.masks[base], &self.masks[base + 1]);
        let take = |masks: &[Mask], recurse: bool| match params {
            Some(_) if !directory => masks
                .iter()
                .any(|mask| mask.matches(&name, &folder, params)),
            _ => any_matches(masks, (&name, &folder), directory, recurse),
        };
        (include.is_empty() || take(include, true)) && !take(exclude, false)
    }
}

#[cfg(test)]
#[path = "file_masks_tests.rs"]
mod tests;
