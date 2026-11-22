//! The four times of the "Attributes" dialog as FAR shows and reads them (`far/datetime.cpp`:
//! `time_to_string` with a time 16 long and a full year, `parse_time`, `merge_time`): the date in the
//! user's order with a year of up to five digits, the time to a ten-millionth of a second, in local time
//! or in UTC. A part left blank keeps the original's; all blank leaves the time as it is.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::platform::files::FileTimes;
use crate::platform::locale::{self, DateOrder, LocalTime};

/// 100-nanosecond ticks in a second.
const TICKS: i128 = 10_000_000;
const SECONDS_A_DAY: i128 = 86_400;

/// The user's date order and separators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeFormat {
    pub order: DateOrder,
    pub date_separator: char,
    pub time_separator: char,
    pub decimal_separator: char,
}

impl TimeFormat {
    pub fn current() -> Self {
        let date = locale::date_format();
        Self {
            order: date.order,
            date_separator: date.date_separator,
            time_separator: date.time_separator,
            decimal_separator: locale::number_format().decimal_separator,
        }
    }

    /// FAR's date mask: a year of five places, the first of them may be blank.
    pub fn date_mask(self) -> String {
        let s = self.date_separator;
        match self.order {
            DateOrder::YearMonthDay => format!("N9999{s}99{s}99"),
            DateOrder::DayMonthYear | DateOrder::MonthDayYear => format!("99{s}99{s}9999N"),
        }
    }

    pub fn time_mask(self) -> String {
        let (t, d) = (self.time_separator, self.decimal_separator);
        format!("99{t}99{t}99{d}9999999")
    }

    /// The title over the dates (FAR's `MSetAttrDateTitle*`).
    pub fn date_title(self) -> String {
        let s = self.date_separator;
        match self.order {
            DateOrder::YearMonthDay => format!("YYYYY{s}MM{s}DD"),
            DateOrder::DayMonthYear => format!("DD{s}MM{s}YYYYY"),
            DateOrder::MonthDayYear => format!("MM{s}DD{s}YYYYY"),
        }
    }

    pub fn time_title(self) -> String {
        let t = self.time_separator;
        format!("hh{t}mm{t}ss")
    }
}

/// A point of the calendar to a tick; a part the user left blank is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Parts {
    year: Option<u32>,
    month: Option<u32>,
    day: Option<u32>,
    hour: Option<u32>,
    minute: Option<u32>,
    second: Option<u32>,
    tick: Option<u32>,
}

/// A point of the calendar to a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Civil {
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    tick: u32,
}

/// The dialog's four times of a file: write, creation, access, change.
pub fn row_times(times: &FileTimes) -> [Option<SystemTime>; 4] {
    [times.modified, times.created, times.accessed, times.changed]
}

/// The date and the time of `time` for the fields; blank for `None` or a time that cannot be shown.
pub fn show(time: Option<SystemTime>, utc: bool, format: TimeFormat) -> (String, String) {
    match time.and_then(|time| civil(time, utc)) {
        Some(civil) => show_civil(civil, format),
        None => (String::new(), String::new()),
    }
}

fn show_civil(civil: Civil, format: TimeFormat) -> (String, String) {
    let Civil {
        year,
        month,
        day,
        hour,
        minute,
        second,
        tick,
    } = civil;
    let (s, t, d) = (
        format.date_separator,
        format.time_separator,
        format.decimal_separator,
    );
    let date = match format.order {
        DateOrder::YearMonthDay => format!("{year:>5}{s}{month:02}{s}{day:02}"),
        DateOrder::DayMonthYear => format!("{day:02}{s}{month:02}{s}{year:>2}"),
        DateOrder::MonthDayYear => format!("{month:02}{s}{day:02}{s}{year:>2}"),
    };
    let time = format!("{hour:02}{t}{minute:02}{t}{second:02}{d}{tick:07}");
    (date, time)
}

/// The time the fields give: blank parts are the original's (FAR's `merge_time`); `None` when they make
/// no point in time (a 13th month, hour 25).
pub fn parse(
    (date, time): (&str, &str),
    original: Option<SystemTime>,
    utc: bool,
    format: TimeFormat,
) -> Option<SystemTime> {
    let parts = parts(date, time, format.order);
    if parts == Parts::default() {
        return original;
    }
    let base = original
        .and_then(|time| civil(time, utc))
        .unwrap_or_default();
    let merged = Civil {
        year: parts.year.map_or(base.year, full_year),
        month: parts.month.unwrap_or(base.month),
        day: parts.day.unwrap_or(base.day),
        hour: parts.hour.unwrap_or(base.hour),
        minute: parts.minute.unwrap_or(base.minute),
        second: parts.second.unwrap_or(base.second),
        tick: parts.tick.unwrap_or(base.tick),
    };
    to_time(merged, utc)
}

/// FAR's `parse_time`: the parts at their places in the masks.
fn parts(date: &str, time: &str, order: DateOrder) -> Parts {
    let take = |text: &str, from: usize, length: usize| -> Option<u32> {
        let part: String = text.chars().skip(from).take(length).collect();
        part.trim().parse().ok()
    };
    let (first, second, third) = match order {
        DateOrder::YearMonthDay => (take(date, 0, 5), take(date, 6, 2), take(date, 9, 2)),
        DateOrder::DayMonthYear | DateOrder::MonthDayYear => {
            (take(date, 0, 2), take(date, 3, 2), take(date, 6, 5))
        }
    };
    let (year, month, day) = match order {
        DateOrder::YearMonthDay => (first, second, third),
        DateOrder::DayMonthYear => (third, second, first),
        DateOrder::MonthDayYear => (third, first, second),
    };
    Parts {
        year,
        month,
        day,
        hour: take(time, 0, 2),
        minute: take(time, 3, 2),
        second: take(time, 6, 2),
        tick: take(time, 9, 7),
    }
}

/// FAR's `full_year`: a two-digit year in the window the user's calendar says.
fn full_year(year: u32) -> u32 {
    full_year_up_to(year, locale::two_digit_year_max())
}

fn full_year_up_to(year: u32, max: u32) -> u32 {
    if year >= 100 {
        return year;
    }
    (max / 100 - u32::from(year > max % 100)) * 100 + year
}

/// Ticks since 1970, before it negative.
fn ticks_of(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i128::try_from(after.as_nanos() / 100).unwrap_or(i128::MAX),
        Err(before) => -i128::try_from(before.duration().as_nanos().div_ceil(100)).unwrap_or(0),
    }
}

fn from_ticks(ticks: i128) -> Option<SystemTime> {
    let nanos = |ticks: i128| {
        let seconds = u64::try_from(ticks / TICKS).ok()?;
        let rest = u32::try_from(ticks % TICKS).ok()? * 100;
        Some(Duration::new(seconds, rest))
    };
    if ticks >= 0 {
        UNIX_EPOCH.checked_add(nanos(ticks)?)
    } else {
        UNIX_EPOCH.checked_sub(nanos(-ticks)?)
    }
}

/// `time` on the calendar, in UTC or in the user's local time.
fn civil(time: SystemTime, utc: bool) -> Option<Civil> {
    let ticks = ticks_of(time);
    let tick = u32::try_from(ticks.rem_euclid(TICKS)).ok()?;
    if utc {
        let seconds = ticks.div_euclid(TICKS);
        let (year, month, day) = civil_from_days(seconds.div_euclid(SECONDS_A_DAY));
        let of_day = u32::try_from(seconds.rem_euclid(SECONDS_A_DAY)).ok()?;
        return Some(Civil {
            year: u32::try_from(year).ok()?,
            month,
            day,
            hour: of_day / 3600,
            minute: of_day / 60 % 60,
            second: of_day % 60,
            tick,
        });
    }
    let local = locale::local_time(time)?;
    Some(Civil {
        year: u32::from(local.year),
        month: u32::from(local.month),
        day: u32::from(local.day),
        hour: u32::from(local.hour),
        minute: u32::from(local.minute),
        second: u32::from(local.second),
        tick,
    })
}

fn to_time(civil: Civil, utc: bool) -> Option<SystemTime> {
    // FILETIME's years, as Windows takes them.
    let valid = (1601..=30827).contains(&civil.year)
        && (1..=12).contains(&civil.month)
        && civil.day >= 1
        && civil.day <= days_in_month(civil.year, civil.month)
        && civil.hour < 24
        && civil.minute < 60
        && civil.second < 60
        && i128::from(civil.tick) < TICKS;
    if !valid {
        return None;
    }
    let seconds = if utc {
        let days = days_from_civil(i128::from(civil.year), civil.month, civil.day);
        days * SECONDS_A_DAY + i128::from(civil.hour * 3600 + civil.minute * 60 + civil.second)
    } else {
        let local = LocalTime {
            year: u16::try_from(civil.year).ok()?,
            month: u16::try_from(civil.month).ok()?,
            day: u16::try_from(civil.day).ok()?,
            hour: u16::try_from(civil.hour).ok()?,
            minute: u16::try_from(civil.minute).ok()?,
            second: u16::try_from(civil.second).ok()?,
        };
        ticks_of(locale::from_local_time(local)?).div_euclid(TICKS)
    };
    from_ticks(seconds * TICKS + i128::from(civil.tick))
}

fn days_in_month(year: u32, month: u32) -> u32 {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i128, month: u32, day: u32) -> i128 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let month = i128::from(month);
    let of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i128::from(day) - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_era_days - 719_468
}

/// The date of a count of days since 1970-01-01.
fn civil_from_days(days: i128) -> (i128, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * of_year + 2) / 153;
    let day = u32::try_from(of_year - (153 * month_index + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    })
    .unwrap_or(1);
    let year = year_of_era + era * 400 + i128::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DMY: TimeFormat = TimeFormat {
        order: DateOrder::DayMonthYear,
        date_separator: '.',
        time_separator: ':',
        decimal_separator: ',',
    };
    const YMD: TimeFormat = TimeFormat {
        order: DateOrder::YearMonthDay,
        date_separator: '-',
        ..DMY
    };

    /// 2024-02-29 09:05:07.1234567 UTC.
    fn leap_day() -> SystemTime {
        let days = days_from_civil(2024, 2, 29);
        let seconds = days * SECONDS_A_DAY + 9 * 3600 + 5 * 60 + 7;
        from_ticks(seconds * TICKS + 1_234_567).unwrap()
    }

    #[test]
    fn fields_follow_fars_masks_and_titles() {
        assert_eq!(DMY.date_mask(), "99.99.9999N");
        assert_eq!(YMD.date_mask(), "N9999-99-99");
        assert_eq!(DMY.time_mask(), "99:99:99,9999999");
        assert_eq!(DMY.date_title(), "DD.MM.YYYYY");
        assert_eq!(YMD.date_title(), "YYYYY-MM-DD");
        assert_eq!(DMY.time_title(), "hh:mm:ss");
    }

    #[test]
    fn a_time_is_shown_to_the_tick_in_utc() {
        let (date, time) = show(Some(leap_day()), true, DMY);
        assert_eq!(
            (date.as_str(), time.as_str()),
            ("29.02.2024", "09:05:07,1234567")
        );
        assert_eq!(show(Some(leap_day()), true, YMD).0, " 2024-02-29");
        assert_eq!(show(None, true, DMY), (String::new(), String::new()));
    }

    #[test]
    fn reading_back_what_is_shown_gives_the_same_time() {
        for utc in [true, false] {
            let (date, time) = show(Some(leap_day()), utc, DMY);
            assert_eq!(
                parse((&date, &time), None, utc, DMY),
                Some(leap_day()),
                "utc {utc}"
            );
        }
        let before_1970 = from_ticks(-(TICKS * 86_400 * 400) - 5).unwrap();
        let (date, time) = show(Some(before_1970), true, YMD);
        assert_eq!(parse((&date, &time), None, true, YMD), Some(before_1970));
    }

    #[test]
    fn blank_parts_keep_the_originals_and_all_blank_keeps_the_time() {
        let original = Some(leap_day());
        let blank = ("  .  .     ", "  :  :  ,       ");
        assert_eq!(parse(blank, original, true, DMY), original);
        assert_eq!(parse(blank, None, true, DMY), None);
        let new_hour = parse(("  .  .     ", "23:  :  ,       "), original, true, DMY);
        assert_eq!(show(new_hour, true, DMY).1, "23:05:07,1234567");
        let leap_year = parse(("  .  .28   ", "  :  :  ,       "), original, true, DMY);
        assert_eq!(show(leap_year, true, DMY).0, "29.02.2028");
        let no_leap_day = parse(("  .  .30   ", "  :  :  ,       "), original, true, DMY);
        assert_eq!(
            no_leap_day, None,
            "the 29th of February 2030 does not exist"
        );
    }

    #[test]
    fn a_point_that_does_not_exist_changes_nothing() {
        let original = Some(leap_day());
        assert_eq!(parse(("31.02.2025 ", ""), original, true, DMY), None);
        assert_eq!(parse(("", "25:00:00,0000000"), original, true, DMY), None);
    }

    #[test]
    fn years_far_ahead_are_kept_and_years_windows_has_not_are_refused() {
        let blank_time = "  :  :  ,       ";
        let far_ahead = parse(("01.01.30000", blank_time), Some(leap_day()), true, DMY);
        assert_eq!(show(far_ahead, true, DMY).0, "01.01.30000");
        assert_eq!(
            parse(("01.01.31000", blank_time), Some(leap_day()), true, DMY),
            None
        );
        assert_eq!(
            parse(("01.01.1500 ", blank_time), Some(leap_day()), true, DMY),
            None
        );
    }

    #[test]
    fn two_digit_years_fall_in_windows_window() {
        assert_eq!(
            (
                full_year_up_to(49, 2049),
                full_year_up_to(50, 2049),
                full_year_up_to(7, 2049)
            ),
            (2049, 1950, 2007)
        );
        assert_eq!(full_year(1999), 1999);
    }

    #[test]
    fn the_two_digit_year_window_is_the_calendars() {
        assert_eq!(full_year_up_to(30, 2029), 1930);
        assert_eq!(full_year_up_to(29, 2029), 2029);
    }

    #[test]
    fn days_and_dates_agree() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(days_from_civil(1601, 1, 1)), (1601, 1, 1));
    }
}
