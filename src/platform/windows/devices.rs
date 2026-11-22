//! Volumes arriving and leaving, as FAR's `wm_listener.cpp`: a hidden window on a thread of its own
//! takes the system's `WM_DEVICECHANGE` broadcasts for volumes (a stick plugged in or pulled, a disc put
//! in or taken out) and keeps the roots they name until the app takes them. The same window hears
//! `WM_SETTINGCHANGE` for the regional settings and for the environment.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once, OnceLock};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DBT_DEVICEARRIVAL, DBT_DEVICEREMOVECOMPLETE, DBT_DEVTYP_VOLUME,
    DEV_BROADCAST_HDR, DEV_BROADCAST_VOLUME, DefWindowProcW, DispatchMessageW, GetMessageW, MSG,
    RegisterClassW, WM_DEVICECHANGE, WM_SETTINGCHANGE, WNDCLASSW,
};

/// The roots named since the app last looked.
static CHANGED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LISTENER: Once = Once::new();
/// The system's environment changed since the app last looked (FAR's `update_environment`).
static ENVIRONMENT_CHANGED: AtomicBool = AtomicBool::new(false);
/// The hidden window, once made (tests look for it).
static WINDOW: OnceLock<usize> = OnceLock::new();

/// The roots of the volumes that arrived or left since the last call (`D:\`); the first call starts
/// listening.
pub fn take_device_changes() -> Vec<String> {
    LISTENER.call_once(|| {
        // Without the listener the menu is read again on Ctrl+R only, as before.
        let _ = std::thread::Builder::new()
            .name("device changes".to_string())
            .spawn(listen);
    });
    CHANGED
        .lock()
        .map(|mut changed| std::mem::take(&mut *changed))
        .unwrap_or_default()
}

/// Whether the system's environment variables changed since the last call (a `WM_SETTINGCHANGE` with
/// `Environment`, as the system's dialog sends it); heard once `take_device_changes` started listening.
pub fn take_environment_change() -> bool {
    ENVIRONMENT_CHANGED.swap(false, Ordering::Relaxed)
}

/// The roots of the drive letters in a volume broadcast's unit mask (bit 0 is `A:`).
pub(super) fn roots_of(unit_mask: u32) -> Vec<String> {
    (0..26u8)
        .filter(|bit| unit_mask & (1 << bit) != 0)
        .map(|bit| format!("{}:\\", char::from(b'A' + bit)))
        .collect()
}

/// The hidden window and its message loop, for the life of the process.
fn listen() {
    let class: Vec<u16> = "NovaCommanderDeviceListener\0".encode_utf16().collect();
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        lpszClassName: class.as_ptr(),
        ..WNDCLASSW::default()
    };
    // SAFETY: the class structure and its name outlive the call (the name lives as long as this
    // thread, which is the window's).
    if unsafe { RegisterClassW(&raw const window_class) } == 0 {
        return;
    }
    // SAFETY: the class was registered above; a top-level window that is never shown (FAR's way: a
    // message-only window gets no broadcasts). The null parent, menu, instance and parameter are allowed.
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            std::ptr::null(),
            0,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    if window.is_null() {
        return;
    }
    let _ = WINDOW.set(window as usize);
    let mut message = MSG::default();
    // SAFETY: `message` is a valid out structure; the loop ends when the process does.
    while unsafe { GetMessageW(&raw mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        // SAFETY: `message` was just filled by GetMessageW.
        unsafe { DispatchMessageW(&raw const message) };
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_SETTINGCHANGE {
        // SAFETY: the system's own `lparam` of a WM_SETTINGCHANGE: null or a NUL-terminated string.
        if unsafe { names_area(lparam, "intl") } {
            super::locale::regional_settings_changed();
        // SAFETY: as above.
        } else if unsafe { names_area(lparam, "Environment") } {
            ENVIRONMENT_CHANGED.store(true, Ordering::Relaxed);
        }
    }
    if message == WM_DEVICECHANGE {
        // SAFETY: the system's own arguments of a WM_DEVICECHANGE.
        let roots = unsafe { volume_roots(wparam, lparam) };
        if !roots.is_empty()
            && let Ok(mut changed) = CHANGED.lock()
        {
            changed.extend(roots);
        }
    }
    // SAFETY: the arguments are the ones the system passed to this window procedure.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// Whether a WM_SETTINGCHANGE is about `area` (FAR's `wm_listener`: `intl` the regional settings,
/// `Environment` the environment variables): its text is that.
///
/// # Safety
/// `lparam` is null or points to a NUL-terminated UTF-16 string.
unsafe fn names_area(lparam: LPARAM, area: &str) -> bool {
    let text = lparam as *const u16;
    if text.is_null() {
        return false;
    }
    let wanted: Vec<u16> = area.encode_utf16().chain(Some(0)).collect();
    for (offset, unit) in wanted.iter().enumerate() {
        // SAFETY: the units before `offset` matched and none was the NUL, so this one is still
        // inside the string.
        if unsafe { *text.add(offset) } != *unit {
            return false;
        }
    }
    true
}

/// The roots a WM_DEVICECHANGE names: a volume that arrived or left; none for other events.
///
/// # Safety
/// `lparam` is what the system passes with `wparam`: for an arrival or a removal, null or a broadcast
/// header that starts the structure of its device type.
unsafe fn volume_roots(wparam: WPARAM, lparam: LPARAM) -> Vec<String> {
    let event = u32::try_from(wparam).unwrap_or(0);
    if !(event == DBT_DEVICEARRIVAL || event == DBT_DEVICEREMOVECOMPLETE) || lparam == 0 {
        return Vec::new();
    }
    // SAFETY: the caller's promise: a broadcast header.
    let header = unsafe { &*(lparam as *const DEV_BROADCAST_HDR) };
    let size = usize::try_from(header.dbch_size).unwrap_or(0);
    if header.dbch_devicetype != DBT_DEVTYP_VOLUME || size < size_of::<DEV_BROADCAST_VOLUME>() {
        return Vec::new();
    }
    // SAFETY: the header says it is a volume broadcast of the whole structure's size.
    let volume = unsafe { &*(lparam as *const DEV_BROADCAST_VOLUME) };
    roots_of(volume.dbcv_unitmask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_mask_names_its_drives() {
        assert_eq!(roots_of(0), Vec::<String>::new());
        assert_eq!(roots_of(1 | 1 << 3 | 1 << 25), ["A:\\", "D:\\", "Z:\\"]);
    }

    #[test]
    fn a_volume_broadcast_names_its_roots_and_other_events_none() {
        let volume = DEV_BROADCAST_VOLUME {
            dbcv_size: u32::try_from(size_of::<DEV_BROADCAST_VOLUME>()).unwrap(),
            dbcv_devicetype: DBT_DEVTYP_VOLUME,
            dbcv_unitmask: 1 << 16,
            ..DEV_BROADCAST_VOLUME::default()
        };
        let lparam = (&raw const volume) as LPARAM;
        let event = |event: u32| WPARAM::try_from(event).unwrap();
        // SAFETY: a volume broadcast that outlives the calls.
        unsafe {
            assert_eq!(volume_roots(event(DBT_DEVICEARRIVAL), lparam), ["Q:\\"]);
            assert_eq!(
                volume_roots(event(DBT_DEVICEREMOVECOMPLETE), lparam),
                ["Q:\\"]
            );
            assert!(
                volume_roots(event(0x0007), lparam).is_empty(),
                "DBT_DEVNODES_CHANGED"
            );
            assert!(volume_roots(event(DBT_DEVICEARRIVAL), 0).is_empty());
        }
    }

    #[test]
    fn only_the_text_intl_names_the_regional_settings_and_environment_the_variables() {
        let text = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
        for (given, intl, environment) in [
            ("intl", true, false),
            ("intlx", false, false),
            ("int", false, false),
            ("Environment", false, true),
            ("Environments", false, false),
            ("", false, false),
        ] {
            let units = text(given);
            let lparam = units.as_ptr() as LPARAM;
            // SAFETY: a NUL-terminated string that outlives the calls.
            let named = unsafe {
                (
                    names_area(lparam, "intl"),
                    names_area(lparam, "Environment"),
                )
            };
            assert_eq!(named, (intl, environment), "{given}");
        }
        // SAFETY: null is allowed.
        assert!(!unsafe { names_area(0, "intl") });
    }

    #[test]
    fn the_hidden_window_is_made_on_the_first_look() {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        take_device_changes();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let window = loop {
            if let Some(window) = WINDOW.get() {
                break *window;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the listener's window"
            );
            std::thread::yield_now();
        };
        let mut process = 0;
        // SAFETY: a window handle this process made; `process` is a valid out value.
        unsafe { GetWindowThreadProcessId(window as HWND, &raw mut process) };
        assert_eq!(process, std::process::id(), "this program's own window");
    }
}
