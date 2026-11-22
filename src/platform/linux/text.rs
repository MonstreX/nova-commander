//! Local time on Linux (S7.1a): the C library's time zone (`TZ`, else `/etc/localtime`) through
//! `localtime_r` and `mktime`, as every program on the system shows its times (ADR-0029: `rustix` has
//! no time zones). `time_t` is 64 bits, as on every Linux the port builds for (x86-64, AArch64). The
//! zone is the library's to read: `mktime` reads it each time (POSIX), glibc's `localtime_r` once.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::platform::locale::LocalTime;

/// Converts a time stamp to the user's local time (time zone and daylight saving of that date); the
/// second it falls in, as Windows' conversion keeps no fraction either.
pub fn local_time(time: SystemTime) -> Option<LocalTime> {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).ok()?,
        Err(before) => {
            let before = before.duration();
            let whole = i64::try_from(before.as_secs()).ok()?;
            // 0.5 s before 1970 is in 1969's last second.
            whole
                .checked_add(i64::from(before.subsec_nanos() != 0))?
                .checked_neg()?
        }
    };
    local_of(seconds)
}

/// The fields of a `time_t` in the local zone.
fn local_of(seconds: libc::time_t) -> Option<LocalTime> {
    // SAFETY: `tm` is plain data for which all zeroes is valid (a null `tm_zone`).
    let mut fields: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers refer to live locals for the duration of the call; the result points at
    // `fields` or is null.
    let done = unsafe { libc::localtime_r(&raw const seconds, &raw mut fields) };
    if done.is_null() {
        return None;
    }
    Some(LocalTime {
        year: u16::try_from(fields.tm_year.checked_add(1900)?).ok()?,
        month: u16::try_from(fields.tm_mon.checked_add(1)?).ok()?,
        day: u16::try_from(fields.tm_mday).ok()?,
        hour: u16::try_from(fields.tm_hour).ok()?,
        minute: u16::try_from(fields.tm_min).ok()?,
        // A leap second (60) is shown as the 59th, as the user's clock never shows 60.
        second: u16::try_from(fields.tm_sec.min(59)).ok()?,
    })
}

/// The time stamp of a point in the user's local time (its zone and daylight saving of that date);
/// `None` for a point that does not exist (the 30th of February, hour 25, the hour skipped in spring):
/// `mktime` moves such a point elsewhere, which converting back tells.
pub fn from_local_time(local: LocalTime) -> Option<SystemTime> {
    // SAFETY: `tm` is plain data for which all zeroes is valid.
    let mut fields: libc::tm = unsafe { std::mem::zeroed() };
    fields.tm_year = i32::from(local.year) - 1900;
    fields.tm_mon = i32::from(local.month) - 1;
    fields.tm_mday = i32::from(local.day);
    fields.tm_hour = i32::from(local.hour);
    fields.tm_min = i32::from(local.minute);
    fields.tm_sec = i32::from(local.second);
    // Whether daylight saving is in force then is the zone's to say.
    fields.tm_isdst = -1;
    // SAFETY: the pointer refers to a live local; `mktime` reads and normalises it in place.
    let seconds = unsafe { libc::mktime(&raw mut fields) };
    // -1 is also one second before 1970: converting back tells an error from it.
    if local_of(seconds) != Some(local) {
        return None;
    }
    match u64::try_from(seconds) {
        Ok(after) => UNIX_EPOCH.checked_add(Duration::from_secs(after)),
        Err(_) => UNIX_EPOCH.checked_sub(Duration::from_secs(seconds.unsigned_abs())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: u16, month: u16, day: u16, hour: u16, minute: u16, second: u16) -> LocalTime {
        LocalTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }

    #[test]
    fn local_time_goes_there_and_back() {
        for local in [
            at(2026, 11, 22, 18, 46, 5),
            at(1970, 1, 1, 0, 0, 0),
            at(1969, 12, 31, 23, 59, 59),
            at(1901, 3, 4, 5, 6, 7),
            at(2100, 2, 28, 12, 0, 0),
        ] {
            let time = from_local_time(local).unwrap();
            assert_eq!(local_time(time), Some(local), "{local:?}");
        }
    }

    #[test]
    fn a_fraction_of_a_second_falls_in_its_second() {
        let time = from_local_time(at(2026, 11, 22, 18, 46, 5)).unwrap();
        let later = time + Duration::from_millis(999);
        assert_eq!(local_time(later), local_time(time));
        let before_1970 = UNIX_EPOCH - Duration::from_millis(500);
        let second_before = UNIX_EPOCH - Duration::from_secs(1);
        assert_eq!(local_time(before_1970), local_time(second_before));
    }

    #[test]
    fn a_point_that_does_not_exist_has_no_time() {
        assert_eq!(from_local_time(at(2026, 2, 30, 12, 0, 0)), None);
        assert_eq!(from_local_time(at(2026, 1, 1, 25, 0, 0)), None);
        assert_eq!(from_local_time(at(2026, 13, 1, 0, 0, 0)), None);
    }
}
