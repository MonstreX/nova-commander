//! A folder's listing over SCP's shell (WinSCP's `TSCPFileSystem::ReadDirectory`, `TRemoteFile::
//! SetListingStr`): `ls -la` lines read as WinSCP reads them - the kind and the rights (a `+`, `@` or `.`
//! after them passed by), the links' count (none on BusyBox), the owner, the group (spaces in it kept),
//! the size (a device's major number passed by), the time (`--full-time`'s `yyyy-mm-dd hh:mm:ss[.ns] +zone`,
//! its other form `ddd mmm dd hh:mm:ss yyyy`, `mmm dd hh:mm` of this year - last year when it would be
//! ahead - or `mmm dd yyyy`, and `dd mmm` ones), the name, a link's target after ` -> `. "." and ".." and
//! the `total` line left out.

use crate::sftp::{Precision, RemoteEntry};

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// A listed entry with its link's target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listed {
    pub entry: RemoteEntry,
    pub link_to: Option<String>,
    /// The time came with its zone (`--full-time`): else it is the server's wall clock, read as UTC.
    pub zoned: bool,
}

/// Days since 1970 of a date (the proleptic Gregorian calendar).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_era_days - 719_468
}

/// Seconds since 1970 of a date and time read as UTC.
pub fn seconds_of(date: (i64, i64, i64), time: (i64, i64, i64)) -> i64 {
    days_from_civil(date.0, date.1, date.2) * 86_400 + time.0 * 3_600 + time.1 * 60 + time.2
}

fn month_of(text: &str) -> Option<i64> {
    let lower = text.to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|month| *month == lower)
        .map(|at| at as i64 + 1)
}

/// The line's columns read one by one, the rest kept as it is.
struct Columns<'a> {
    rest: &'a str,
    /// A zone was read with the time.
    zoned: bool,
}

impl<'a> Columns<'a> {
    /// The next column, the spaces after it passed by (WinSCP's `GETCOL`).
    fn next(&mut self) -> Option<&'a str> {
        let column = self.next_raw()?;
        self.rest = self.rest.trim_start_matches(' ');
        Some(column)
    }

    /// The next column, one space after it passed by (`GETNCOL`: a name may start with spaces).
    fn next_raw(&mut self) -> Option<&'a str> {
        if self.rest.is_empty() {
            return None;
        }
        match self.rest.find(' ') {
            Some(at) => {
                let column = &self.rest[..at];
                self.rest = &self.rest[at + 1..];
                Some(column)
            }
            None => Some(std::mem::take(&mut self.rest)),
        }
    }
}

/// The time's columns read: seconds since 1970 (`now` the date the server's year is taken from), and
/// how exact they are.
fn time_of(columns: &mut Columns, now: (i64, i64, i64)) -> Option<(i64, Precision)> {
    let first = columns.next()?;
    if let Some(date) = iso_date(first) {
        return full_time(columns, date).map(|seconds| (seconds, Precision::Second));
    }
    let (day, month_column) = match first.parse::<i64>() {
        Ok(day) if day > 0 => (Some(day), columns.next()?),
        _ => (None, first),
    };
    let Some(month) = month_of(month_column) else {
        return weekday_time(columns).map(|seconds| (seconds, Precision::Second));
    };
    let day = match day {
        Some(day) => day,
        None => columns.next()?.parse().ok()?,
    };
    if !(1..=31).contains(&day) {
        return None;
    }
    year_or_time(columns.next()?, (month, day), now)
}

/// `--full-time`'s `hh:mm:ss[.ns] [+zone]` after its date.
fn full_time(columns: &mut Columns, date: (i64, i64, i64)) -> Option<i64> {
    let time = columns.next_raw()?;
    let zone = columns.rest.split_once(' ').map(|(zone, _)| zone);
    let offset = zone.and_then(zone_seconds);
    if offset.is_some() {
        columns.next_raw();
        columns.zoned = true;
    }
    Some(seconds_of(date, clock(time)?) - offset.unwrap_or(0))
}

/// `ddd mmm dd hh:mm:ss yyyy` after its weekday.
fn weekday_time(columns: &mut Columns) -> Option<i64> {
    let month = month_of(columns.next()?)?;
    let day = columns.next()?.parse().ok()?;
    let time = clock(columns.next()?)?;
    let year = columns.next_raw()?.parse().ok()?;
    Some(seconds_of((year, month, day), time))
}

/// `hh:mm` of this year (the last one when it would be ahead of `now`), or `yyyy` at midnight.
fn year_or_time(
    column: &str,
    (month, day): (i64, i64),
    now: (i64, i64, i64),
) -> Option<(i64, Precision)> {
    let Some((hour, minute)) = column.split_once(':') else {
        let midnight = seconds_of((column.parse().ok()?, month, day), (0, 0, 0));
        return Some((midnight, Precision::Day));
    };
    let (hour, minute): (i64, i64) = (hour.parse().ok()?, minute.parse().ok()?);
    let ahead = month > now.1 || (month == now.1 && day > now.2);
    let year = if ahead { now.0 - 1 } else { now.0 };
    Some((
        seconds_of((year, month, day), (hour, minute, 0)),
        Precision::Minute,
    ))
}

fn iso_date(text: &str) -> Option<(i64, i64, i64)> {
    let mut parts = text.split('-');
    let year = parts.next()?;
    if year.len() != 4 {
        return None;
    }
    let date = (
        year.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(date)
}

fn clock(text: &str) -> Option<(i64, i64, i64)> {
    let text = text.split('.').next()?;
    let mut parts = text.split(':');
    let hour = parts.next()?.parse().ok()?;
    let minute = parts.next()?.parse().ok()?;
    let second = parts.next().map_or(Some(0), |second| second.parse().ok())?;
    Some((hour, minute, second))
}

/// `+0300` as seconds east of UTC.
fn zone_seconds(text: &str) -> Option<i64> {
    let (sign, digits) = match text.as_bytes().first()? {
        b'+' => (1, &text[1..]),
        b'-' => (-1, &text[1..]),
        _ => return None,
    };
    if digits.len() != 4 {
        return None;
    }
    let hours: i64 = digits[..2].parse().ok()?;
    let minutes: i64 = digits[2..].parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

/// The size column, a device's `major,` passed by, the group's other words taken in (WinSCP's loop).
fn group_and_size(columns: &mut Columns) -> Option<(String, u64)> {
    let mut group = columns.next()?.to_string();
    loop {
        let mut column = columns.next()?;
        if group == "?" && column == "?" {
            return Some((group, 0));
        }
        if column.ends_with(',') {
            column = columns.next()?;
        }
        match column.parse::<u64>() {
            Ok(size) => return Some((group, size)),
            Err(_) => {
                group.push(' ');
                group.push_str(column);
            }
        }
    }
}

/// WinSCP's `SetListingStr` of one line; `None` for one it cannot read, "." and "..".
pub fn listed(line: &str, now: (i64, i64, i64)) -> Option<Listed> {
    let line = line.replace('\t', " ");
    let mut chars = line.chars();
    let kind = chars.next()?;
    let rest: String = chars.collect();
    let rights_text: String = rest.chars().take(9).collect();
    let mut rest: String = rest.chars().skip(9).collect();
    for prefix in ["+", "@", ".", " +", " @", " ."] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped.to_string();
            break;
        }
    }
    let mut columns = Columns {
        rest: rest.trim_start_matches(' '),
        zoned: false,
    };
    let first = columns.next()?;
    let owner = match first.parse::<u64>() {
        Ok(_) => columns.next()?,
        Err(_) => first,
    };
    let (group, size) = group_and_size(&mut columns)?;
    let modified = time_of(&mut columns, now);
    let mut name = columns.rest.to_string();
    let link = kind == 'l';
    let mut link_to = None;
    if link {
        let (shown, target) = name.split_once(" -> ")?;
        link_to = Some(target.to_string());
        name = shown.to_string();
    }
    let name = name
        .trim()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    if matches!(name.as_str(), "" | "." | "..") {
        return None;
    }
    let entry = RemoteEntry {
        name,
        size,
        is_dir: kind == 'd',
        is_link: link,
        modified: modified.and_then(|(seconds, _)| u32::try_from(seconds).ok()),
        precision: modified.map_or(Precision::Second, |(_, precision)| precision),
        permissions: Some(
            kind_bits(kind)
                | crate::copy_params::rights_of_text(&plain_rights(&rights_text))
                    .map_or(0, u32::from)
                | special_bits(&rights_text),
        ),
        owner: Some(owner.to_string()),
        group: Some(group),
    };
    Some(Listed {
        entry,
        link_to,
        zoned: columns.zoned,
    })
}

/// The rights' text with the specials read as their X (`s`, `t`: set; `S`, `T`: unset).
fn plain_rights(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            's' | 't' => 'x',
            'S' | 'T' => '-',
            other => other,
        })
        .collect()
}

/// The special bits the rights' text shows.
fn special_bits(text: &str) -> u32 {
    let chars: Vec<char> = text.chars().collect();
    let has = |at: usize, set: char| {
        chars
            .get(at)
            .is_some_and(|ch| ch.eq_ignore_ascii_case(&set))
    };
    u32::from(has(2, 's')) * 0o4000
        + u32::from(has(5, 's')) * 0o2000
        + u32::from(has(8, 't')) * 0o1000
}

/// The kind's bits of `stat`'s mode.
fn kind_bits(kind: char) -> u32 {
    match kind {
        'd' => 0o040_000,
        'l' => 0o120_000,
        _ => 0o100_000,
    }
}

/// A whole listing: the lines read, those it cannot read left out.
pub fn listing(text: &str, now: (i64, i64, i64)) -> Vec<Listed> {
    text.lines()
        .filter(|line| !line.starts_with("total "))
        .filter_map(|line| listed(line, now))
        .collect()
}

#[cfg(test)]
#[path = "scp_listing_tests.rs"]
mod tests;
