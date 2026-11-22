//! The texts of FAR's proclist columns (`Plist::GetFindData`'s custom columns, `ui64toa_width`,
//! `DurationToText`): a number as FAR's `FormatFileSize` fits it into its column (K, M, G... when it does
//! not, by 1024 or by 1000 in small letters for counts), or in the float style ("1.50 M") when the mode
//! asks for it; a time as days, hours, minutes, seconds and its 100 ns ticks; each right-aligned in its
//! column's width.

/// The units' letters (FAR's `UnitSymbol`).
const UNITS: [char; 7] = ['B', 'K', 'M', 'G', 'T', 'P', 'E'];

/// What one of `unit` is: 1024 or 1000 to its power.
fn unit_size(unit: usize, thousand: bool) -> Option<u64> {
    let power = u32::try_from(unit).ok()?;
    if thousand {
        1000u64.checked_pow(power)
    } else {
        1u64.checked_shl(power.checked_mul(10)?)
    }
}

/// `value` in `unit`, rounded half up as FAR does.
fn rounded(value: u64, unit: usize, thousand: bool) -> u64 {
    let Some(size) = unit_size(unit, thousand) else {
        return 0;
    };
    value / size + u64::from(value % size >= size - value % size)
}

fn letter(unit: usize, thousand: bool) -> char {
    if thousand {
        UNITS[unit].to_ascii_lowercase()
    } else {
        UNITS[unit]
    }
}

/// FAR's `FormatFileSize(value, width, flags)` as proclist calls it: `width` 0 the float style
/// (`FFFS_FLOATSIZE`), `thousand` units of 1000 (`FFFS_THOUSAND`); `decimal` the user's separator.
pub fn size(value: u64, width: usize, thousand: bool, decimal: char) -> String {
    if width == 0 {
        return float(value, thousand, decimal);
    }
    let mut text = value.to_string();
    if text.len() <= width {
        return text;
    }
    let room = width.saturating_sub(2);
    let mut unit = 0;
    while unit + 1 < UNITS.len() && text.len() > room {
        let next = rounded(value, unit + 1, thousand);
        if next == 0 {
            break;
        }
        unit += 1;
        text = next.to_string();
    }
    if unit == 0 {
        return text;
    }
    format!("{text} {}", letter(unit, thousand))
}

/// The largest unit `value` has a whole one of.
fn largest(value: u64, thousand: bool) -> usize {
    let mut unit = 0;
    while unit + 1 < UNITS.len() && unit_size(unit + 1, thousand).is_some_and(|size| value >= size)
    {
        unit += 1;
    }
    unit
}

/// FAR's float style: three significant digits and the unit ("1.50 K", "10.1 K", "100 K"); below one
/// unit the number as it is.
fn float(value: u64, thousand: bool, decimal: char) -> String {
    let unit = largest(value, thousand);
    let Some(size) = unit_size(unit, thousand).filter(|_| unit > 0) else {
        return value.to_string();
    };
    let whole = value / size;
    let part = (value % size) as f64 / size as f64;
    let digits_for = |whole: u64| match whole {
        0..10 => 2,
        10..100 => 1,
        _ => 0,
    };
    let digits = digits_for(whole);
    let number = if digits == 0 {
        ((whole as f64 + part).round() as u64).to_string()
    } else {
        let scale = 10u64.pow(digits);
        let fraction = (part * scale as f64).round() as u64;
        // Rounded up to the next whole one: as many digits as that one leaves room for.
        let (whole, fraction, digits) = if fraction == scale {
            (whole + 1, 0, digits_for(whole + 1))
        } else {
            (whole, fraction, digits)
        };
        if digits == 0 {
            whole.to_string()
        } else {
            let digits = digits as usize;
            format!("{whole}{decimal}{fraction:0digits$}")
        }
    };
    format!("{number} {}", letter(unit, thousand))
}

/// FAR's `DurationToText`: `[days ]hh:mm:ss.ttttttt` of a time in 100 ns units.
pub fn duration(ticks: u64) -> String {
    const SECOND: u64 = 10_000_000;
    const MINUTE: u64 = SECOND * 60;
    const HOUR: u64 = MINUTE * 60;
    const DAY: u64 = HOUR * 24;
    let (days, rest) = (ticks / DAY, ticks % DAY);
    let (hours, rest) = (rest / HOUR, rest % HOUR);
    let (minutes, rest) = (rest / MINUTE, rest % MINUTE);
    let (seconds, ticks) = (rest / SECOND, rest % SECOND);
    let time = format!("{hours:02}:{minutes:02}:{seconds:02}.{ticks:07}");
    if days > 0 {
        format!("{days} {time}")
    } else {
        time
    }
}

/// `text` right-aligned in `width` (FAR pads a custom column's text with spaces before it); longer, as it
/// is (the panel cuts it).
pub fn right(text: &str, width: usize) -> String {
    format!("{text:>width$}")
}

#[cfg(test)]
#[path = "columns_tests.rs"]
mod tests;
