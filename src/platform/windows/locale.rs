//! Locale: local time, the user's date and number formats, the order of names.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::Globalization::{
    CAL_GREGORIAN, CAL_ITWODIGITYEARMAX, CAL_RETURN_NUMBER, CP_OEMCP, CSTR_GREATER_THAN,
    CSTR_LESS_THAN, CompareStringEx, GetCalendarInfoW, GetLocaleInfoEx, LINGUISTIC_IGNORECASE,
    LOCALE_IDATE, LOCALE_SABBREVMONTHNAME1, LOCALE_SDATE, LOCALE_SDECIMAL, LOCALE_SGROUPING,
    LOCALE_STHOUSAND, LOCALE_STIME, LOCALE_USER_DEFAULT, MultiByteToWideChar, SORT_DIGITSASNUMBERS,
    SORT_STRINGSORT,
};
use windows_sys::Win32::System::Time::{
    FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime,
    TzSpecificLocalTimeToSystemTime,
};

use crate::platform::locale::{DateFormat, DateOrder, DigitGroups, LocalTime, NumberFormat};

/// 100-nanosecond intervals between 1601-01-01 (FILETIME epoch) and 1970-01-01.
const FILETIME_UNIX_EPOCH: u64 = 116_444_736_000_000_000;

/// A FILETIME value (100 ns since 1601-01-01) as a time stamp; `None` for 0 (not set).
pub(super) fn from_file_time(ticks: i64) -> Option<SystemTime> {
    let ticks = u64::try_from(ticks).ok().filter(|ticks| *ticks != 0)?;
    // Whole seconds and the rest: nanoseconds in a u64 end in 2554, FILETIME in 30828.
    let nanos = |ticks: u64| {
        let rest = u32::try_from(ticks % 10_000_000).unwrap_or(0) * 100;
        std::time::Duration::new(ticks / 10_000_000, rest)
    };
    if ticks >= FILETIME_UNIX_EPOCH {
        UNIX_EPOCH.checked_add(nanos(ticks - FILETIME_UNIX_EPOCH))
    } else {
        UNIX_EPOCH.checked_sub(nanos(FILETIME_UNIX_EPOCH - ticks))
    }
}

/// A time stamp as a FILETIME value; `None` when it does not fit one.
pub(super) fn to_file_time(time: SystemTime) -> Option<i64> {
    let ticks = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => {
            FILETIME_UNIX_EPOCH.checked_add(u64::try_from(after.as_nanos() / 100).ok()?)?
        }
        Err(before) => {
            let before = u64::try_from(before.duration().as_nanos().div_ceil(100)).ok()?;
            FILETIME_UNIX_EPOCH.checked_sub(before)?
        }
    };
    i64::try_from(ticks).ok()
}

/// Converts a time stamp to the user's local time (time zone and daylight saving of that date).
pub fn local_time(time: SystemTime) -> Option<LocalTime> {
    // FILETIME counts from 1601, so times before 1970 are representable too.
    let ticks = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => {
            FILETIME_UNIX_EPOCH.checked_add(u64::try_from(after.as_nanos() / 100).ok()?)?
        }
        Err(before) => {
            let before = u64::try_from(before.duration().as_nanos().div_ceil(100)).ok()?;
            FILETIME_UNIX_EPOCH.checked_sub(before)?
        }
    };
    let file_time = FILETIME {
        dwLowDateTime: (ticks & 0xFFFF_FFFF) as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = empty_system_time();
    let mut local = empty_system_time();
    // SAFETY: both pointers refer to live, properly aligned locals for the duration of the call.
    let converted = unsafe { FileTimeToSystemTime(&file_time, &mut utc) } != 0;
    if !converted {
        return None;
    }
    // SAFETY: a null time zone means "the current one"; `utc` and `local` are live locals.
    let converted =
        unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) } != 0;
    converted.then_some(LocalTime {
        year: local.wYear,
        month: local.wMonth,
        day: local.wDay,
        hour: local.wHour,
        minute: local.wMinute,
        second: local.wSecond,
    })
}

/// The time stamp of a point in the user's local time (its zone and daylight saving of that date);
/// `None` for a point that does not exist (the 30th of February, hour 25).
pub fn from_local_time(local: LocalTime) -> Option<SystemTime> {
    let given = SYSTEMTIME {
        wYear: local.year,
        wMonth: local.month,
        wDayOfWeek: 0,
        wDay: local.day,
        wHour: local.hour,
        wMinute: local.minute,
        wSecond: local.second,
        wMilliseconds: 0,
    };
    let mut utc = empty_system_time();
    // SAFETY: a null time zone means "the current one"; `given` and `utc` are live locals.
    let converted = unsafe {
        TzSpecificLocalTimeToSystemTime(std::ptr::null(), &raw const given, &raw mut utc)
    };
    if converted == 0 {
        return None;
    }
    let mut file_time = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: both pointers refer to live locals; the call checks the fields of `utc`.
    if unsafe { SystemTimeToFileTime(&raw const utc, &raw mut file_time) } == 0 {
        return None;
    }
    let ticks = (u64::from(file_time.dwHighDateTime) << 32) | u64::from(file_time.dwLowDateTime);
    from_file_time(i64::try_from(ticks).ok()?)
}

fn empty_system_time() -> SYSTEMTIME {
    SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    }
}

/// The user's regional settings as last read (FAR's `locale`, read again after `WM_SETTINGCHANGE`).
#[derive(Clone, Copy)]
struct Regional {
    date: DateFormat,
    number: NumberFormat,
    groups: DigitGroups,
}

static REGIONAL: Mutex<Option<Regional>> = Mutex::new(None);
/// The short names of the months as last read.
static MONTHS: Mutex<Option<Vec<String>>> = Mutex::new(None);
/// The settings changed since the app last looked.
static CHANGED: AtomicBool = AtomicBool::new(false);

fn read_regional() -> Regional {
    Regional {
        date: read_date_format(),
        number: read_number_format(),
        groups: read_digit_groups(),
    }
}

fn regional() -> Regional {
    match REGIONAL.lock() {
        Ok(mut kept) => *kept.get_or_insert_with(read_regional),
        Err(_) => read_regional(),
    }
}

/// The system says the regional settings changed (FAR's `update_intl`).
pub(super) fn regional_settings_changed() {
    if let Ok(mut kept) = REGIONAL.lock() {
        *kept = None;
    }
    if let Ok(mut kept) = MONTHS.lock() {
        *kept = None;
    }
    CHANGED.store(true, Ordering::SeqCst);
}

/// Whether the regional settings changed since the last call: what is on screen shows the old ones.
pub fn take_locale_change() -> bool {
    CHANGED.swap(false, Ordering::SeqCst)
}

/// The short name of `month` (1 .. 12) in the user's language (FAR's `LOCALE_SABBREVMONTHNAME`),
/// English when the system says none.
pub fn month_name(month: u16) -> String {
    const ENGLISH: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let read = || {
        (LOCALE_SABBREVMONTHNAME1..)
            .zip(ENGLISH)
            .map(|(kind, english)| locale_info(kind).unwrap_or_else(|| english.to_string()))
            .collect::<Vec<String>>()
    };
    let at = usize::from(month.clamp(1, 12)) - 1;
    match MONTHS.lock() {
        Ok(mut kept) => kept.get_or_insert_with(read)[at].clone(),
        Err(_) => read()[at].clone(),
    }
}

/// The user's regional date order and separators.
pub fn date_format() -> DateFormat {
    regional().date
}

fn read_date_format() -> DateFormat {
    let fallback = DateFormat::default();
    let order = match locale_info(LOCALE_IDATE).as_deref() {
        Some("0") => DateOrder::MonthDayYear,
        Some("1") => DateOrder::DayMonthYear,
        Some("2") => DateOrder::YearMonthDay,
        _ => fallback.order,
    };
    let first_char = |value: Option<String>, default: char| {
        value
            .and_then(|text| text.chars().next())
            .unwrap_or(default)
    };
    DateFormat {
        order,
        date_separator: first_char(locale_info(LOCALE_SDATE), fallback.date_separator),
        time_separator: first_char(locale_info(LOCALE_STIME), fallback.time_separator),
    }
}

/// FAR's default name order (`string_sort::compare`): linguistic for the user's locale, case-insensitive,
/// digits compared as numbers ("file2" before "file10"), signs as symbols ("my-project" before "myapp").
pub fn compare_names(a: &str, b: &str) -> std::cmp::Ordering {
    let wide_a: Vec<u16> = a.encode_utf16().collect();
    let wide_b: Vec<u16> = b.encode_utf16().collect();
    compare_wide_names(&wide_a, &wide_b)
}

/// [`compare_names`] of names already in UTF-16 (many names sorted: each encoded once).
pub fn compare_wide_names(wide_a: &[u16], wide_b: &[u16]) -> std::cmp::Ordering {
    let lower = |wide: &[u16]| String::from_utf16_lossy(wide).to_lowercase();
    let (Ok(length_a), Ok(length_b)) = (i32::try_from(wide_a.len()), i32::try_from(wide_b.len()))
    else {
        return wide_a.cmp(wide_b);
    };
    // SAFETY: both buffers live for the call and the lengths are their exact sizes in UTF-16 units; a null
    // locale name means the user default; the optional pointers may be null.
    let result = unsafe {
        CompareStringEx(
            std::ptr::null(),
            // FAR's `compare_natural_base` always adds `SORT_STRINGSORT` (`string_sort.cpp:306`): `-` and
            // `'` sort as the symbols they are, not as nearly nothing (review R7).
            SORT_STRINGSORT | LINGUISTIC_IGNORECASE | SORT_DIGITSASNUMBERS,
            wide_a.as_ptr(),
            length_a,
            wide_b.as_ptr(),
            length_b,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    };
    match result {
        CSTR_LESS_THAN => std::cmp::Ordering::Less,
        CSTR_GREATER_THAN => std::cmp::Ordering::Greater,
        0 => lower(wide_a).cmp(&lower(wide_b)),
        _ => std::cmp::Ordering::Equal,
    }
}

/// The user's regional thousands and decimal separators.
pub fn number_format() -> NumberFormat {
    regional().number
}

fn read_number_format() -> NumberFormat {
    let fallback = NumberFormat::default();
    let first = |kind: u32, default: char| {
        locale_info(kind)
            .and_then(|text| text.chars().next())
            .unwrap_or(default)
    };
    NumberFormat {
        thousands_separator: first(LOCALE_STHOUSAND, fallback.thousands_separator),
        decimal_separator: first(LOCALE_SDECIMAL, fallback.decimal_separator),
    }
}

/// How the user's numbers are grouped.
pub fn digit_groups() -> DigitGroups {
    regional().groups
}

fn read_digit_groups() -> DigitGroups {
    locale_info(LOCALE_SGROUPING)
        .map_or_else(DigitGroups::default, |text| DigitGroups::parse(&text))
}

/// Text in the console's OEM code page (FAR's `real_oem`), as old menu files are written.
pub fn decode_oem(bytes: &[u8]) -> String {
    decode(CP_OEMCP, bytes)
}

/// `bytes` in `code_page`, each wrong one its replacement.
pub(super) fn decode(code_page: u32, bytes: &[u8]) -> String {
    let lossy = || String::from_utf8_lossy(bytes).into_owned();
    let Ok(length) = i32::try_from(bytes.len()) else {
        return lossy();
    };
    if length == 0 {
        return String::new();
    }
    // SAFETY: `bytes` holds `length` bytes; no buffer is passed, only the size is asked.
    let units = unsafe {
        MultiByteToWideChar(
            code_page,
            0,
            bytes.as_ptr(),
            length,
            std::ptr::null_mut(),
            0,
        )
    };
    let Some(size) = usize::try_from(units).ok().filter(|&size| size > 0) else {
        return lossy();
    };
    let mut wide = vec![0u16; size];
    // SAFETY: `bytes` holds `length` bytes, `wide` room for `units` code units.
    let written = unsafe {
        MultiByteToWideChar(
            code_page,
            0,
            bytes.as_ptr(),
            length,
            wide.as_mut_ptr(),
            units,
        )
    };
    let written = usize::try_from(written).unwrap_or(0).min(size);
    String::from_utf16_lossy(&wide[..written])
}

/// The last year a two-digit year can mean (FAR's `full_year`; 2049 when the system does not say).
pub fn two_digit_year_max() -> u32 {
    const DEFAULT: u32 = 2049;
    let mut year = 0u32;
    // SAFETY: with `CAL_RETURN_NUMBER` the text buffer is unused and may be null with length 0; `year`
    // is a live local.
    let done = unsafe {
        GetCalendarInfoW(
            LOCALE_USER_DEFAULT,
            CAL_GREGORIAN,
            CAL_ITWODIGITYEARMAX | CAL_RETURN_NUMBER,
            std::ptr::null_mut(),
            0,
            &raw mut year,
        )
    };
    if done == 0 { DEFAULT } else { year }
}

/// One value of the user's default locale.
fn locale_info(kind: u32) -> Option<String> {
    let mut buffer = [0u16; 32];
    let capacity = i32::try_from(buffer.len()).ok()?;
    // SAFETY: a null locale name means the user default; the buffer is writable for `capacity` UTF-16
    // units, and the function never writes more than that.
    let written = unsafe { GetLocaleInfoEx(std::ptr::null(), kind, buffer.as_mut_ptr(), capacity) };
    let length = usize::try_from(written).ok()?.checked_sub(1)?;
    String::from_utf16(buffer.get(..length)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_calendar_says_a_year_for_two_digits() {
        assert!((99..=9999).contains(&two_digit_year_max()));
    }

    #[test]
    fn after_a_change_the_settings_are_read_again() {
        regional_settings_changed();
        assert_eq!(number_format(), read_number_format());
        assert_eq!(digit_groups(), read_digit_groups());
    }

    #[test]
    fn local_time_is_a_valid_calendar_time() {
        let time = UNIX_EPOCH + Duration::from_secs(1_709_214_300);
        let local = local_time(time).unwrap();
        // 2024-02-29 13:45 UTC is 2024-02-29 or 2024-03-01 in any time zone.
        assert_eq!(local.year, 2024);
        assert!(matches!((local.month, local.day), (2, 28 | 29) | (3, 1)));
        assert_eq!(local.minute % 15, 0);
    }

    #[test]
    fn file_times_convert_both_ways_around_1970() {
        assert_eq!(from_file_time(0), None);
        assert_eq!(from_file_time(FILETIME_UNIX_EPOCH as i64), Some(UNIX_EPOCH));
        let day = 864_000_000_000_i64;
        assert_eq!(
            from_file_time(FILETIME_UNIX_EPOCH as i64 + day),
            Some(UNIX_EPOCH + Duration::from_secs(86_400))
        );
        assert_eq!(
            from_file_time(FILETIME_UNIX_EPOCH as i64 - day),
            Some(UNIX_EPOCH - Duration::from_secs(86_400))
        );
    }

    #[test]
    fn times_before_1970_are_shown_too() {
        // 1969-07-20 20:17 UTC.
        let landing = UNIX_EPOCH - Duration::from_secs(14_182_940);
        let local = local_time(landing).unwrap();
        assert_eq!((local.year, local.month), (1969, 7));
        assert!(matches!(local.day, 20 | 21));
        // Before 1601 FILETIME has no value.
        let too_old = UNIX_EPOCH - Duration::from_secs(400 * 366 * 86_400);
        assert_eq!(local_time(too_old), None);
    }

    #[test]
    fn names_compare_like_far() {
        use std::cmp::Ordering::{Equal, Less};
        assert_eq!(compare_names("file2", "file10"), Less);
        assert_eq!(compare_names("Readme", "readme"), Equal);
        assert_eq!(compare_names("alpha", "Beta"), Less);
        assert_eq!(compare_names("ёлка", "яблоко"), Less);
        // FAR's `SORT_STRINGSORT`: a hyphen is a symbol before letters, not nothing (review R7).
        assert_eq!(compare_names("my-project", "myapp"), Less);
        let wide = |name: &str| name.encode_utf16().collect::<Vec<u16>>();
        assert_eq!(compare_wide_names(&wide("file2"), &wide("file10")), Less);
        assert_eq!(compare_wide_names(&wide("Ёлка"), &wide("ёлка")), Equal);
    }

    #[test]
    fn date_format_has_separators() {
        let format = date_format();
        assert!(!format.date_separator.is_alphanumeric());
        assert!(!format.time_separator.is_alphanumeric());
    }

    #[test]
    fn the_oem_code_page_reads_ascii_as_it_is_and_a_byte_as_one_character() {
        assert_eq!(decode_oem(b"Menu: x"), "Menu: x");
        assert_eq!(decode_oem(&[0xc4]).chars().count(), 1);
        assert_eq!(decode_oem(&[]), "");
    }
}
