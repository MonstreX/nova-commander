//! Reading the registry: a string value.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY, KEY_QUERY_VALUE, RRF_RT_REG_SZ, RegCloseKey, RegGetValueW, RegOpenKeyExW,
};

use super::wide;

/// A `REG_SZ` value (`value` empty: the key's default one).
pub(super) fn string(key: HKEY, subkey: &str, value: &str) -> Option<String> {
    let subkey = wide(subkey);
    let value = wide(value);
    let mut size: u32 = 0;
    // SAFETY: both names are NUL-terminated; no data buffer is passed, only its size is asked.
    let status = unsafe {
        RegGetValueW(
            key,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut size,
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }
    let mut data = vec![0u16; usize::try_from(size).ok()?.div_ceil(2)];
    // SAFETY: both names are NUL-terminated; `data` holds `size` bytes; no type is asked back.
    let status = unsafe {
        RegGetValueW(
            key,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            data.as_mut_ptr().cast(),
            &raw mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let end = data
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(data.len());
    OsString::from_wide(&data[..end]).into_string().ok()
}

/// Whether key `subkey` is there.
pub(super) fn key_exists(key: HKEY, subkey: &str) -> bool {
    let subkey = wide(subkey);
    let mut opened: HKEY = std::ptr::null_mut();
    // SAFETY: `subkey` is NUL-terminated; `opened` receives the key, closed below.
    let status =
        unsafe { RegOpenKeyExW(key, subkey.as_ptr(), 0, KEY_QUERY_VALUE, &raw mut opened) };
    if status != ERROR_SUCCESS {
        return false;
    }
    // SAFETY: `opened` is the key just opened.
    unsafe { RegCloseKey(opened) };
    true
}
