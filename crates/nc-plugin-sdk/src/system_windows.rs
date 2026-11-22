//! The windows of the processes as FAR's proclist finds and shows them (`EnumWndProc`, `is_alttab_window`,
//! `window_style`): each process's top window (a visible one before a hidden one), its title and styles;
//! switched to with Enter when it is one Alt+Tab shows (FAR's `SetForegroundWindow`, `SW_RESTORE`).

use std::collections::HashMap;

use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOTOWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetLastActivePopup, GetParent,
    GetWindowLongPtrW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SW_RESTORE, SetForegroundWindow, ShowWindowAsync, WS_DISABLED, WS_EX_TOOLWINDOW,
};

/// A window of another process's, by its handle's value (it may be gone by the time it is used: each
/// call then does nothing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Window(pub usize);

impl Window {
    fn handle(self) -> HWND {
        std::ptr::with_exposed_provenance_mut(self.0)
    }

    /// Its title (FAR's `GetWindowText` into `MAX_PATH`).
    pub fn title(self) -> String {
        let mut buffer = [0u16; 260];
        let size = i32::try_from(buffer.len()).unwrap_or(i32::MAX);
        // SAFETY: the buffer holds `size` units; a window gone gives 0.
        let length = unsafe { GetWindowTextW(self.handle(), buffer.as_mut_ptr(), size) };
        let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
        String::from_utf16_lossy(&buffer[..length])
    }

    /// Its style and extended style bits (`GWL_STYLE`, `GWL_EXSTYLE`).
    pub fn styles(self) -> (u32, u32) {
        // SAFETY: no pointers; a window gone gives 0. The styles are 32-bit values.
        let style = unsafe { GetWindowLongPtrW(self.handle(), GWL_STYLE) };
        // SAFETY: as above.
        let extended = unsafe { GetWindowLongPtrW(self.handle(), GWL_EXSTYLE) };
        let low = |bits: isize| u32::try_from(bits & 0xFFFF_FFFF).unwrap_or(0);
        (low(style), low(extended))
    }

    /// Brought to the front, restored when minimized, when Alt+Tab would show it (FAR's Enter); false
    /// for one it would not.
    pub fn switch_to(self) -> bool {
        if !self.alt_tab() {
            return false;
        }
        let window = self.handle();
        // SAFETY: no pointers; a window gone is refused by the system.
        unsafe {
            SetForegroundWindow(window);
            if IsIconic(window) != 0 {
                ShowWindowAsync(window, SW_RESTORE);
            }
        }
        true
    }

    /// FAR's `is_alttab_window` (Raymond Chen's rule): visible, the last active popup of its root owner,
    /// no tool window.
    fn alt_tab(self) -> bool {
        let window = self.handle();
        // SAFETY: no pointers; each call answers for a window gone too.
        unsafe {
            if IsWindowVisible(window) == 0 {
                return false;
            }
            let mut tried = GetAncestor(window, GA_ROOTOWNER);
            let mut walk: HWND = std::ptr::null_mut();
            while tried != walk {
                walk = tried;
                tried = GetLastActivePopup(walk);
                if IsWindowVisible(tried) != 0 {
                    break;
                }
            }
            if walk != window {
                return false;
            }
            let extended = GetWindowLongPtrW(window, GWL_EXSTYLE);
            extended & isize::try_from(WS_EX_TOOLWINDOW).unwrap_or(0) == 0
        }
    }
}

/// Each process's top window: the first one found, a visible one (or a minimized one not disabled)
/// taking its place (FAR's `EnumWndProc`).
pub fn top_windows() -> HashMap<u32, Window> {
    let mut found: HashMap<u32, Window> = HashMap::new();
    // SAFETY: the callback gets the map's address back and runs within this call.
    unsafe { EnumWindows(Some(each), std::ptr::from_mut(&mut found) as LPARAM) };
    found
}

/// One top window seen: kept as its process's when there is none yet or it is visible.
unsafe extern "system" fn each(window: HWND, found: LPARAM) -> i32 {
    // SAFETY: `top_windows` passed its map's address, alive through `EnumWindows`.
    let found = unsafe { &mut *(found as *mut HashMap<u32, Window>) };
    let mut pid = 0u32;
    // SAFETY: the window is the system's; the id goes to a local.
    unsafe { GetWindowThreadProcessId(window, &raw mut pid) };
    // SAFETY: no pointers.
    if !unsafe { GetParent(window) }.is_null() {
        return 1;
    }
    // SAFETY: no pointers.
    let visible = unsafe {
        IsWindowVisible(window) != 0
            || (IsIconic(window) != 0
                && GetWindowLongPtrW(window, GWL_STYLE) & isize::try_from(WS_DISABLED).unwrap_or(0)
                    == 0)
    };
    let value = Window(window as usize);
    match found.get(&pid) {
        Some(_) if !visible => {}
        _ => {
            found.insert(pid, value);
        }
    }
    1
}
