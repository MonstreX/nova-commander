//! Times as FAR's proclist writes them (`ConvertDate`): a moment in 100 ns units since 1601 (UTC) in the
//! local time, its date and its time in the user's formats (`GetDateFormat`, `GetTimeFormat` with the
//! user's locale and no flags); the moment now.

use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::Globalization::{GetDateFormatW, GetTimeFormatW};
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

/// A moment in the local time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    /// In the user's short date format.
    pub date: String,
    /// In the user's time format.
    pub time: String,
}

/// Windows' `LOCALE_USER_DEFAULT`.
const USER_DEFAULT: u32 = 0x400;

fn file_time(ticks: i64) -> FILETIME {
    let ticks = ticks.cast_unsigned();
    FILETIME {
        dwLowDateTime: u32::try_from(ticks & 0xFFFF_FFFF).unwrap_or(0),
        dwHighDateTime: u32::try_from(ticks >> 32).unwrap_or(0),
    }
}

/// A date or a time of `moment` in the user's format, as `format` writes it.
fn formatted(
    moment: &SYSTEMTIME,
    format: unsafe extern "system" fn(
        u32,
        u32,
        *const SYSTEMTIME,
        *const u16,
        *mut u16,
        i32,
    ) -> i32,
) -> String {
    let mut buffer = [0u16; 128];
    let capacity = i32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: the moment is a valid local time; no format text; the buffer holds `capacity` units.
    let written = unsafe {
        format(
            USER_DEFAULT,
            0,
            moment,
            std::ptr::null(),
            buffer.as_mut_ptr(),
            capacity,
        )
    };
    let length = usize::try_from(written).unwrap_or(0).saturating_sub(1);
    String::from_utf16_lossy(&buffer[..length.min(buffer.len())])
}

/// `ticks` (100 ns units since 1601, UTC) in the local time; `None` for one the system cannot convert.
pub fn local_time(ticks: i64) -> Option<LocalTime> {
    let utc_file = file_time(ticks);
    let mut utc = SYSTEMTIME::default();
    // SAFETY: both structures are locals.
    if unsafe { FileTimeToSystemTime(&raw const utc_file, &raw mut utc) } == 0 {
        return None;
    }
    let mut local = SYSTEMTIME::default();
    // SAFETY: the current time zone (null); both structures are locals.
    if unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &raw const utc, &raw mut local) }
        == 0
    {
        return None;
    }
    Some(LocalTime {
        year: local.wYear,
        month: local.wMonth,
        day: local.wDay,
        date: formatted(&local, GetDateFormatW),
        time: formatted(&local, GetTimeFormatW),
    })
}

/// A local date and time (an archive's MS-DOS time: `year` from 1980, `second` even) as a moment in 100 ns
/// units since 1601 (UTC), as 7-Zip reads it (`DosTime_To_FileTime`, then `LocalFileTimeToFileTime`: the
/// zone's bias now); `None` for a date the system cannot convert.
pub fn dos_time_ticks(date: [u16; 3], time: [u16; 3]) -> Option<i64> {
    use windows_sys::Win32::Storage::FileSystem::LocalFileTimeToFileTime;
    use windows_sys::Win32::System::Time::SystemTimeToFileTime;
    let [year, month, day] = date;
    let [hour, minute, second] = time;
    let local = SYSTEMTIME {
        wYear: year,
        wMonth: month,
        wDay: day,
        wHour: hour,
        wMinute: minute,
        wSecond: second,
        ..SYSTEMTIME::default()
    };
    let mut local_file = FILETIME::default();
    // SAFETY: both structures are locals.
    if unsafe { SystemTimeToFileTime(&raw const local, &raw mut local_file) } == 0 {
        return None;
    }
    let mut utc = FILETIME::default();
    // SAFETY: both structures are locals.
    if unsafe { LocalFileTimeToFileTime(&raw const local_file, &raw mut utc) } == 0 {
        return None;
    }
    Some(((u64::from(utc.dwHighDateTime) << 32) | u64::from(utc.dwLowDateTime)).cast_signed())
}

/// A moment (100 ns units since 1601, UTC) as an archive's local date and time, `[year, month, day]` and
/// `[hour, minute, second]`, as 7-Zip writes an MS-DOS time (`FileTimeToLocalFileTime`: the zone's bias
/// now, then the parts); `None` for one the system cannot convert. [`dos_time_ticks`] reads it back.
pub fn dos_time_of(ticks: i64) -> Option<([u16; 3], [u16; 3])> {
    use windows_sys::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    if ticks < 0 {
        return None;
    }
    let utc = file_time(ticks);
    let mut local_file = FILETIME::default();
    // SAFETY: both structures are locals.
    if unsafe { FileTimeToLocalFileTime(&raw const utc, &raw mut local_file) } == 0 {
        return None;
    }
    let mut local = SYSTEMTIME::default();
    // SAFETY: both structures are locals.
    if unsafe { FileTimeToSystemTime(&raw const local_file, &raw mut local) } == 0 {
        return None;
    }
    Some((
        [local.wYear, local.wMonth, local.wDay],
        [local.wHour, local.wMinute, local.wSecond],
    ))
}

/// The moment now in 100 ns units since 1601 (UTC).
pub fn now_ticks() -> i64 {
    let mut now = FILETIME::default();
    // SAFETY: the structure is a local.
    unsafe { GetSystemTimeAsFileTime(&raw mut now) };
    ((u64::from(now.dwHighDateTime) << 32) | u64::from(now.dwLowDateTime)).cast_signed()
}

#[cfg(test)]
#[path = "system_time_tests.rs"]
mod tests;
