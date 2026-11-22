use super::*;

#[test]
fn now_is_a_local_date_and_time() {
    let now = local_time(now_ticks()).expect("now converts");
    assert!(now.year >= 2024, "{now:?}");
    assert!((1..=12).contains(&now.month) && (1..=31).contains(&now.day));
    assert!(now.date.contains(&now.day.to_string()), "{now:?}");
    assert!(!now.time.is_empty(), "{now:?}");
}

/// 2000-06-15 12:00 UTC: the same day in every time zone within 12 hours of it.
#[test]
fn a_moment_is_its_local_day() {
    let ticks = (961_070_400 + 11_644_473_600) * 10_000_000;
    let moment = local_time(ticks).expect("converts");
    assert_eq!((moment.year, moment.month, moment.day), (2000, 6, 15));
    assert!(moment.date.contains("15"), "{moment:?}");
}

/// A DOS time 2000-06-15 12:00:00 local: within a day of that moment in UTC, its local day the same; an
/// impossible date none.
#[test]
fn a_dos_time_is_its_local_moment() {
    let ticks = dos_time_ticks([2000, 6, 15], [12, 0, 0]).expect("converts");
    let noon_utc = (961_070_400 + 11_644_473_600) * 10_000_000_i64;
    let hours = (ticks - noon_utc).abs() / 36_000_000_000;
    assert!(hours <= 14, "{hours} hours from UTC noon");
    let back = local_time(ticks).expect("converts");
    assert_eq!((back.year, back.month, back.day), (2000, 6, 15));
    assert_eq!(dos_time_ticks([2000, 13, 40], [0, 0, 0]), None);
}

/// A moment written as an MS-DOS time reads back as that moment: the zone's bias both ways.
#[test]
fn a_dos_time_written_reads_back() {
    let ticks = (961_070_400 + 11_644_473_600) * 10_000_000_i64;
    let (date, time) = dos_time_of(ticks).expect("converts");
    assert_eq!(dos_time_ticks(date, time), Some(ticks));
    assert_eq!((date[0], date[1]), (2000, 6));
}

/// The parts of a moment: its seconds whatever the zone (whole minutes off UTC), a part of a second
/// dropped, every part in its range; a moment the system cannot convert none.
#[test]
fn a_moments_parts_are_in_their_ranges() {
    let base = (961_070_400 + 11_644_473_600) * 10_000_000_i64;
    let at = base + 37 * 10_000_000 + 9_999_999;
    let (date, time) = dos_time_of(at).expect("converts");
    assert_eq!(time[2], 37, "the seconds, the part of a second dropped");
    assert!((1..=12).contains(&date[1]) && (1..=31).contains(&date[2]));
    assert!(time[0] < 24 && time[1] < 60);
    let (_, minute_before) = dos_time_of(at - 38 * 10_000_000).expect("converts");
    assert_eq!(minute_before[2], 59);
    assert_eq!(dos_time_of(i64::MAX), None, "past what the system converts");
    assert_eq!(dos_time_of(-1), None, "before 1601");
}
