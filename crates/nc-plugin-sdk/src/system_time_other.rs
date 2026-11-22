//! Times off Windows: a moment in 100 ns units since 1601 read as UTC (the Linux port takes the local zone
//! and the user's formats: ADR-0017), its date and time written as ISO 8601; the moment now.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds from 1601-01-01 to 1970-01-01.
const EPOCH_1601: i64 = 11_644_473_600;

/// A moment in the local time (here UTC).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// `HH:MM:SS`.
    pub time: String,
}

/// `ticks` (100 ns units since 1601, UTC) as a date and time; `None` before 1970 or past the year 30827.
pub fn local_time(ticks: i64) -> Option<LocalTime> {
    let ([year, month, day], [hour, minute, second]) = dos_time_of(ticks)?;
    Some(LocalTime {
        year,
        month,
        day,
        date: format!("{year:04}-{month:02}-{day:02}"),
        time: format!("{hour:02}:{minute:02}:{second:02}"),
    })
}

/// A date and time as a moment in 100 ns units since 1601, read as UTC; `None` for an impossible date.
pub fn dos_time_ticks(date: [u16; 3], time: [u16; 3]) -> Option<i64> {
    let [year, month, day] = date.map(i64::from);
    let [hour, minute, second] = time.map(i64::from);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    // Days from 1970-01-01 (Howard Hinnant's `days_from_civil`).
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = shifted.div_euclid(400);
    let of_era = shifted - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    let days = era * 146_097 + of_cycle - 719_468;
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    Some((seconds + EPOCH_1601) * 10_000_000)
}

/// The last year Windows' `SYSTEMTIME` converts: past it a moment is none on every system.
const LAST_YEAR: i64 = 30_827;

/// A moment in 100 ns units since 1601 as a date and time, read as UTC (as [`dos_time_ticks`] here);
/// `None` before 1970 or past the year 30827 (as Windows).
pub fn dos_time_of(ticks: i64) -> Option<([u16; 3], [u16; 3])> {
    let seconds = ticks / 10_000_000 - EPOCH_1601;
    if seconds < 0 {
        return None;
    }
    let (days, of_day) = (seconds / 86_400, seconds % 86_400);
    // Howard Hinnant's `civil_from_days`.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let of_era = shifted - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * of_year + 2) / 153;
    let day = of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    if year > LAST_YEAR {
        return None;
    }
    let part = |value: i64| u16::try_from(value).ok();
    Some((
        [part(year)?, part(month)?, part(day)?],
        [
            part(of_day / 3_600)?,
            part(of_day % 3_600 / 60)?,
            part(of_day % 60)?,
        ],
    ))
}

/// The moment now in 100 ns units since 1601 (UTC); a clock before 1970 is 1970.
pub fn now_ticks() -> i64 {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let units = i64::try_from(since.as_nanos() / 100).unwrap_or(i64::MAX);
    units.saturating_add(EPOCH_1601 * 10_000_000)
}

#[cfg(test)]
#[path = "system_time_tests.rs"]
mod tests;
