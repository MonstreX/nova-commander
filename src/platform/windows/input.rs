//! Keyboard layouts and network names: what FAR reads from virtual-key codes and `WNetGetUniversalName`.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, NO_ERROR};
use windows_sys::Win32::NetworkManagement::WNet::{
    UNIVERSAL_NAME_INFO_LEVEL, UNIVERSAL_NAME_INFOW, WNetGetUniversalNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetDoubleClickTime, GetKeyboardLayout, GetKeyboardLayoutList, HKL, VkKeyScanExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SPI_GETKEYBOARDDELAY, SPI_GETKEYBOARDSPEED,
    SPI_GETWHEELSCROLLCHARS, SPI_GETWHEELSCROLLLINES, SYSTEM_PARAMETERS_INFO_ACTION,
    SystemParametersInfoW,
};

use crate::platform::input::latin_for_virtual_key;

/// More keyboard layouts than anyone installs.
const MAX_LAYOUTS: usize = 64;

/// The key that types `ch` in one of the installed keyboard layouts, as its Latin character, and whether
/// Shift is needed: `'х'` (Russian) is `('[', false)`, `'{'` is `('[', true)`. Keys typed with AltGr or
/// Ctrl are not reported. FAR binds its shortcuts to keys, not characters, so Ctrl+[ works whatever
/// layout is on; crossterm reports the character of the current layout instead.
pub fn key_for_char(ch: char) -> Option<(char, bool)> {
    let unit = u16::try_from(u32::from(ch)).ok()?;
    let keys: Vec<(u8, u8)> = keyboard_layouts()
        .into_iter()
        .filter_map(|layout| {
            // SAFETY: VkKeyScanExW takes a UTF-16 unit and a layout handle by value; an unknown
            // handle only makes it return -1.
            let scan = unsafe { VkKeyScanExW(unit, layout) };
            let [key, shift_state] = u16::from_ne_bytes(scan.to_ne_bytes()).to_le_bytes();
            (scan != -1).then_some((key, shift_state))
        })
        .collect();
    // A character typed with AltGr in any layout is text: AltGr arrives as Ctrl+Alt.
    if keys.iter().any(|(_, shift_state)| *shift_state > 1) {
        return None;
    }
    keys.into_iter().find_map(|(key, shift_state)| {
        latin_for_virtual_key(u16::from(key)).map(|latin| (latin, shift_state == 1))
    })
}

/// Whether a Ctrl+Alt+`ch` press is AltGr typing `ch`: the current layout types `ch` with AltGr.
/// Windows treats Ctrl+Alt as AltGr too, so on such a layout the press types the character.
pub fn is_altgr_text(ch: char) -> bool {
    let Ok(unit) = u16::try_from(u32::from(ch)) else {
        return false;
    };
    // SAFETY: GetForegroundWindow has no arguments; GetWindowThreadProcessId accepts a null window and
    // a null out-pointer; GetKeyboardLayout takes a thread id by value (0 for this thread).
    let layout = unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        GetKeyboardLayout(thread)
    };
    // SAFETY: VkKeyScanExW takes a UTF-16 unit and a layout handle by value.
    let scan = unsafe { VkKeyScanExW(unit, layout) };
    let [_, shift_state] = u16::from_ne_bytes(scan.to_ne_bytes()).to_le_bytes();
    scan != -1 && shift_state & ALTGR == ALTGR
}

/// Ctrl and Alt in `VkKeyScanExW`'s shift state: AltGr.
const ALTGR: u8 = 2 | 4;

fn keyboard_layouts() -> Vec<HKL> {
    let mut layouts: Vec<HKL> = vec![std::ptr::null_mut(); MAX_LAYOUTS];
    let capacity = i32::try_from(layouts.len()).unwrap_or(0);
    // SAFETY: the buffer holds `capacity` handles and stays alive for the call.
    let count = unsafe { GetKeyboardLayoutList(capacity, layouts.as_mut_ptr()) };
    layouts.truncate(usize::try_from(count).unwrap_or(0));
    layouts
}

/// The longest time between the clicks of a double click (FAR's `GetDoubleClickTime`).
pub fn double_click_time() -> Duration {
    // SAFETY: GetDoubleClickTime has no arguments.
    Duration::from_millis(u64::from(unsafe { GetDoubleClickTime() }))
}

/// One number of the system's settings.
fn system_number(action: SYSTEM_PARAMETERS_INFO_ACTION) -> Option<u32> {
    let mut value = 0u32;
    // SAFETY: these actions write one UINT to the pointer, which is a live local.
    let done = unsafe { SystemParametersInfoW(action, 0, (&raw mut value).cast(), 0) };
    (done != 0).then_some(value)
}

/// The lines a wheel notch scrolls (FAR's `get_wheel_scroll_lines` with its default 0); a page
/// scroll setting counts as the default 3.
pub fn wheel_scroll_lines() -> u32 {
    system_number(SPI_GETWHEELSCROLLLINES)
        .filter(|lines| (1..1000).contains(lines))
        .unwrap_or(3)
}

/// The characters a tilt of the wheel moves.
pub fn wheel_scroll_chars() -> u32 {
    system_number(SPI_GETWHEELSCROLLCHARS)
        .filter(|chars| (1..1000).contains(chars))
        .unwrap_or(3)
}

/// The keyboard's repeat: the delay before the first repeat and the time between them (FAR's
/// `keyboard_delay` and `keyboard_rate`), which a held mouse button follows too.
pub fn keyboard_repeat() -> (Duration, Duration) {
    let delay = system_number(SPI_GETKEYBOARDDELAY).unwrap_or(1).min(3);
    // 0 is about 2.5 repeats a second, 31 about 30.
    let speed = system_number(SPI_GETKEYBOARDSPEED).unwrap_or(31).min(31);
    let tenths_per_second = 25 + speed * 275 / 31;
    (
        Duration::from_millis(u64::from(delay + 1) * 250),
        Duration::from_millis(u64::from(10_000 / tenths_per_second)),
    )
}

/// FAR's `ConvertNameToUNC`: a path on a mapped network drive as `\\server\share\...`; any other path
/// as it is.
pub fn universal_name(path: &Path) -> PathBuf {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut words = vec![0u64; 128];
    loop {
        let mut size = u32::try_from(words.len() * 8).unwrap_or(u32::MAX);
        // SAFETY: `wide` is NUL-terminated; the buffer is writable for `size` bytes, 8-byte aligned for
        // UNIVERSAL_NAME_INFOW, and both stay alive for the call.
        let result = unsafe {
            WNetGetUniversalNameW(
                wide.as_ptr(),
                UNIVERSAL_NAME_INFO_LEVEL,
                words.as_mut_ptr().cast(),
                &mut size,
            )
        };
        match result {
            NO_ERROR => return read_universal_name(&words).unwrap_or_else(|| path.to_path_buf()),
            ERROR_MORE_DATA if (size as usize) > words.len() * 8 => {
                words.resize((size as usize).div_ceil(8), 0);
            }
            _ => return path.to_path_buf(),
        }
    }
}

fn read_universal_name(words: &[u64]) -> Option<PathBuf> {
    // SAFETY: WNetGetUniversalNameW succeeded, so the buffer starts with a UNIVERSAL_NAME_INFOW whose
    // pointer is either null or a NUL-terminated string inside the same buffer.
    let name = unsafe { (*words.as_ptr().cast::<UNIVERSAL_NAME_INFOW>()).lpUniversalName };
    if name.is_null() {
        return None;
    }
    let range = words.as_ptr_range();
    if (name as usize) < (range.start as usize) {
        return None;
    }
    let buffer_end = range.end.cast::<u16>();
    // The string lies inside the buffer; never read past its end even if the API broke that promise.
    let available = (buffer_end as usize).checked_sub(name as usize)? / 2;
    let mut length = 0;
    loop {
        if length == available {
            return None;
        }
        // SAFETY: `name + length` is inside the buffer (`length < available`) and the buffer is alive.
        if unsafe { *name.add(length) } == 0 {
            break;
        }
        length += 1;
    }
    // SAFETY: the `length` units before the NUL were just read and lie inside the buffer.
    let units = unsafe { std::slice::from_raw_parts(name, length) };
    Some(PathBuf::from(OsString::from_wide(units)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_letters_map_to_themselves() {
        assert_eq!(key_for_char('a'), Some(('a', false)));
        assert_eq!(key_for_char('A'), Some(('a', true)));
    }

    #[test]
    fn the_keyboard_repeats_within_the_systems_bounds() {
        let (delay, interval) = keyboard_repeat();
        assert!((250..=1000).contains(&delay.as_millis()), "{delay:?}");
        assert!((33..=400).contains(&interval.as_millis()), "{interval:?}");
        assert!(wheel_scroll_lines() >= 1);
    }

    #[test]
    fn local_paths_have_no_network_name() {
        let temp = std::env::temp_dir();
        assert_eq!(universal_name(&temp), temp);
    }
}
