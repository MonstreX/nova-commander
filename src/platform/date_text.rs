//! Local dates and times as FAR shows them (port of `time_to_string`, `far/datetime.cpp`), in the
//! user's regional format.

use std::time::SystemTime;

use super::locale::{
    DateFormat, DateOrder, LocalTime, date_format, local_time, month_name, number_format,
};

/// How a date and a time are written (FAR's `time_to_string` arguments).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DateStyle {
    /// A four-digit year.
    pub full_year: bool,
    /// 5 is "hh:mm", 6 "hh:mma" (12 hours), 8 "hh:mm:ss", more the parts of a second.
    pub time_length: usize,
    /// FAR's `COLFLAGS_BRIEF`: the day and month only, the year instead of the time for a date more
    /// than six months ago.
    pub brief: bool,
    /// FAR's `COLFLAGS_MONTH`: the month's short name.
    pub month: bool,
}

/// Date and time of `time` in the user's time zone and regional format.
/// `full_year` gives a four-digit year; `time_length` 5 is "hh:mm", 8 is "hh:mm:ss".
pub fn date_time(time: SystemTime, full_year: bool, time_length: usize) -> (String, String) {
    let style = DateStyle {
        full_year,
        time_length,
        ..DateStyle::default()
    };
    styled_date_time(time, style, time)
}

/// [`date_time`] in a `style`; `now` tells a recent date for the brief one.
pub fn styled_date_time(time: SystemTime, style: DateStyle, now: SystemTime) -> (String, String) {
    local_time(time).map_or_else(Default::default, |local| {
        styled_local(local, date_format(), style, time, now)
    })
}

fn styled_local(
    local: LocalTime,
    format: DateFormat,
    style: DateStyle,
    time: SystemTime,
    now: SystemTime,
) -> (String, String) {
    let (date, time_text) = if style.month {
        (
            named_month_date(local, format, style),
            clock(local, format, style, time),
        )
    } else {
        local_date_time(local, format, style, time)
    };
    if !style.brief {
        return (date, time_text);
    }
    let date: String = date.chars().take(if style.month { 6 } else { 5 }).collect();
    if is_recent(time, now) {
        (date, time_text)
    } else {
        (date, format!("{:>5}", local.year))
    }
}

/// FAR's `is_recent_date`: within six months before now.
fn is_recent(time: SystemTime, now: SystemTime) -> bool {
    /// FAR's `std::chrono::months{6}`.
    const SIX_MONTHS: std::time::Duration = std::time::Duration::from_secs(15_778_476);
    now.duration_since(time).is_ok_and(|ago| ago < SIX_MONTHS)
}

/// The order of the parts: a brief date in the year-first order is month-first (FAR's `CurDateFormat`).
fn order(format: DateFormat, style: DateStyle) -> DateOrder {
    if style.brief && format.order == DateOrder::YearMonthDay {
        DateOrder::MonthDayYear
    } else {
        format.order
    }
}

fn year(time: LocalTime, style: DateStyle) -> u16 {
    if style.full_year {
        time.year
    } else {
        time.year % 100
    }
}

/// FAR's date with `{1:3.3}`: the month's name in three columns.
fn named_month_date(time: LocalTime, format: DateFormat, style: DateStyle) -> String {
    let name: String = month_name(time.month).chars().take(3).collect();
    let (day, year) = (time.day, year(time, style));
    match order(format, style) {
        DateOrder::YearMonthDay => format!("{year:02} {name:<3} {day:>2}"),
        DateOrder::DayMonthYear => format!("{day:>2} {name:<3} {year:02}"),
        DateOrder::MonthDayYear => format!("{name:<3} {day:>2} {year:02}"),
    }
}

/// FAR's time: "hh:mm", with `a` / `p` on twelve hours for 6, else "hh:mm:ss.fffffff" cut to its length.
fn clock(time: LocalTime, format: DateFormat, style: DateStyle, stamp: SystemTime) -> String {
    let separator = format.time_separator;
    let length = style.time_length;
    if length < 7 {
        let (hour, letter) = if length == 6 {
            let letter = if time.hour < 12 { "a" } else { "p" };
            let hour = match time.hour % 12 {
                0 => 12,
                hour => hour,
            };
            (hour, letter)
        } else {
            (time.hour, "")
        };
        return format!("{hour:02}{separator}{:02}{letter}", time.minute);
    }
    let ticks = stamp
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos() / 100);
    let decimal = number_format().decimal_separator;
    let full = format!(
        "{:02}{separator}{:02}{separator}{:02}{decimal}{ticks:07}",
        time.hour, time.minute, time.second
    );
    full.chars().take(length).collect()
}

fn local_date_time(
    time: LocalTime,
    format: DateFormat,
    style: DateStyle,
    stamp: SystemTime,
) -> (String, String) {
    let time_text = clock(time, format, style, stamp);
    let year = year(time, style);
    let separator = format.date_separator;
    let date_text = match order(format, style) {
        DateOrder::DayMonthYear => format!(
            "{:02}{separator}{:02}{separator}{year:02}",
            time.day, time.month
        ),
        DateOrder::MonthDayYear => format!(
            "{:02}{separator}{:02}{separator}{year:02}",
            time.month, time.day
        ),
        DateOrder::YearMonthDay => format!(
            "{year:02}{separator}{:02}{separator}{:02}",
            time.month, time.day
        ),
    };
    (date_text, time_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_date_time(
        time: LocalTime,
        format: DateFormat,
        full_year: bool,
        time_length: usize,
    ) -> (String, String) {
        let style = DateStyle {
            full_year,
            time_length,
            ..DateStyle::default()
        };
        super::local_date_time(time, format, style, SystemTime::UNIX_EPOCH)
    }

    const LEAP_DAY: LocalTime = LocalTime {
        year: 2024,
        month: 2,
        day: 29,
        hour: 9,
        minute: 5,
        second: 7,
    };

    fn format(order: DateOrder, date_separator: char) -> DateFormat {
        DateFormat {
            order,
            date_separator,
            time_separator: ':',
        }
    }

    #[test]
    fn dates_follow_the_regional_order() {
        let dmy = format(DateOrder::DayMonthYear, '.');
        assert_eq!(
            local_date_time(LEAP_DAY, dmy, false, 5),
            ("29.02.24".to_string(), "09:05".to_string())
        );
        let mdy = format(DateOrder::MonthDayYear, '/');
        assert_eq!(local_date_time(LEAP_DAY, mdy, false, 5).0, "02/29/24");
        let ymd = format(DateOrder::YearMonthDay, '-');
        assert_eq!(local_date_time(LEAP_DAY, ymd, true, 5).0, "2024-02-29");
        assert_eq!(local_date_time(LEAP_DAY, dmy, false, 8).1, "09:05:07");
    }

    #[test]
    fn real_time_stamps_produce_both_parts() {
        let (date, time) = date_time(SystemTime::now(), false, 5);
        assert_eq!(date.chars().count(), 8);
        assert_eq!(time.chars().count(), 5);
    }

    #[test]
    fn fars_brief_month_and_twelve_hour_styles() {
        let dmy = format(DateOrder::DayMonthYear, '.');
        let stamp = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_709_197_507);
        let style = |brief, month, time_length| DateStyle {
            brief,
            month,
            time_length,
            ..DateStyle::default()
        };
        let soon = stamp + std::time::Duration::from_secs(86_400);
        let later = stamp + std::time::Duration::from_secs(200 * 86_400);
        let at = |style, now| styled_local(LEAP_DAY, dmy, style, stamp, now);
        assert_eq!(
            at(style(true, false, 5), soon),
            ("29.02".to_string(), "09:05".to_string())
        );
        assert_eq!(
            at(style(true, false, 5), later),
            ("29.02".to_string(), " 2024".to_string()),
            "older than six months: the year instead of the time"
        );
        let name: String = month_name(2).chars().take(3).collect();
        let name = format!("{name:<3}");
        assert_eq!(at(style(false, true, 5), soon).0, format!("29 {name} 24"));
        assert_eq!(at(style(true, true, 5), soon).0, format!("29 {name}"));
        let ymd = format(DateOrder::YearMonthDay, '-');
        let brief_ymd = styled_local(LEAP_DAY, ymd, style(true, false, 5), stamp, soon);
        assert_eq!(
            brief_ymd.0, "02-29",
            "a brief year-first date is month-first"
        );
        assert_eq!(at(style(false, false, 6), soon).1, "09:05a");
        let evening = LocalTime {
            hour: 21,
            ..LEAP_DAY
        };
        let pm = styled_local(evening, dmy, style(false, false, 6), stamp, soon);
        assert_eq!(pm.1, "09:05p");
        let midnight = LocalTime {
            hour: 0,
            ..LEAP_DAY
        };
        assert_eq!(
            styled_local(midnight, dmy, style(false, false, 6), stamp, soon).1,
            "12:05a"
        );
        let decimal = number_format().decimal_separator;
        assert_eq!(
            at(style(false, false, 11), soon).1,
            format!("09:05:07{decimal}00")
        );
    }
}
