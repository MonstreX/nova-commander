//! The context menu as Windows shows it (FAR's emenu `ShowGuiMenu`): a hidden window of the shell
//! thread's own, topmost at the place, owning the menu so that the items drawn by their objects and the
//! submenus filled as they open get their messages; at the mouse or at the console window's centre. Esc
//! in the program cancels it (`WM_CANCELMODE`). The window's class is registered for the menu only, by
//! the plugin's own module, and taken away after it (the plugin may be unloaded later).

use std::io;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, ScreenToClient};
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleHandleExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, EndMenu, GWLP_USERDATA, GetClientRect,
    GetCursorPos, GetWindowLongPtrW, HMENU, PostMessageW, RegisterClassExW, SetForegroundWindow,
    SetWindowLongPtrW, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, UnregisterClassW,
    WM_CANCELMODE, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_NULL,
    WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use super::GuiAt;
use super::com::ContextMenu;
use super::session::FIRST;

/// The window shown now in the shared place: 0 none, [`CANCELLED`] cancelled before it was shown.
pub(super) const CANCELLED: isize = -1;

/// The menu shown in `shown`'s window cancelled (or not shown at all if it is not there yet).
pub(super) fn cancel(shown: &AtomicIsize) {
    let window = shown.swap(CANCELLED, Ordering::SeqCst);
    if window > 0 {
        // SAFETY: a window of the shell thread's, alive while it is in `shown`; a posted message only.
        unsafe { PostMessageW(window as HWND, WM_CANCELMODE, 0, 0) };
    }
}

/// The mouse's place on the screen.
fn cursor() -> Option<POINT> {
    let mut point = POINT::default();
    // SAFETY: the answer's place lives through it.
    (unsafe { GetCursorPos(&raw mut point) } != 0).then_some(point)
}

/// The client area of `window` (FAR's console window), none when it has none.
fn client(window: usize) -> Option<RECT> {
    let mut rect = RECT::default();
    // SAFETY: any window handle (a wrong one fails); the answer's place lives through it.
    let read = window != 0 && unsafe { GetClientRect(window as HWND, &raw mut rect) } != 0;
    (read && rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
}

/// The centre of `window`'s client area on the screen.
fn centre(window: usize) -> Option<POINT> {
    let rect = client(window)?;
    let mut point = POINT {
        x: (rect.left + rect.right) / 2,
        y: (rect.top + rect.bottom) / 2,
    };
    // SAFETY: the window answered above; the point is converted in place.
    (unsafe { ClientToScreen(window as HWND, &raw mut point) } != 0).then_some(point)
}

/// The cell of the program's screen (`columns` × `rows` in the console `window`) the mouse is over (FAR's
/// emenu `GetCursorXY`); none when it is outside the window or the window does not tell its size.
pub fn mouse_cell(window: usize, columns: u32, rows: u32) -> Option<(u16, u16)> {
    let rect = client(window)?;
    let mut point = cursor()?;
    // SAFETY: the window answered above; the point is converted in place.
    if unsafe { ScreenToClient(window as HWND, &raw mut point) } == 0 {
        return None;
    }
    let inside =
        (rect.left..rect.right).contains(&point.x) && (rect.top..rect.bottom).contains(&point.y);
    if !inside {
        return None;
    }
    let along = |at: i32, size: i32, cells: u32| {
        let cells = i64::from(cells);
        u16::try_from(cells * i64::from(at) / i64::from(size).max(1)).ok()
    };
    Some((
        along(point.x - rect.left, rect.right - rect.left, columns)?,
        along(point.y - rect.top, rect.bottom - rect.top, rows)?,
    ))
}

/// The menu window's messages: the menu's own handed to its object, the rest as any window's.
unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: the window's own data: the menu set right after the window was made, alive until it is
    // destroyed (`track` destroys it before the menu goes); 0 before it is set.
    let menu = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const ContextMenu;
    let menus = matches!(
        message,
        WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
    );
    if menus && !menu.is_null() {
        // SAFETY: as above: the menu is alive while the window is.
        let answer = unsafe { &*menu }.handle(message, wparam, lparam);
        match (message, answer) {
            (WM_MENUCHAR, Some(answer)) if answer != 0 => return answer,
            (WM_INITMENUPOPUP, Some(_)) => return 0,
            (WM_DRAWITEM | WM_MEASUREITEM, Some(_)) => return 1,
            _ => {}
        }
    }
    if message == WM_CANCELMODE {
        // SAFETY: on the thread whose menu is shown.
        unsafe { EndMenu() };
    }
    // SAFETY: the window's other messages as any window's.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// The module this code is in (the plugin's), for the window's class.
fn own_module() -> io::Result<HINSTANCE> {
    let mut module: HINSTANCE = null_mut();
    let flags =
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    let address = window_proc as *const u16;
    // SAFETY: an address in this module; its handle's place lives through it; no reference taken.
    if unsafe { GetModuleHandleExW(flags, address, &raw mut module) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(module)
}

/// The window's class name.
fn class_name() -> Vec<u16> {
    "vc-shell-menu\0".encode_utf16().collect()
}

/// `popup` (the items of `menu`) shown at `at`: the command chosen, none when the menu was closed.
pub(super) fn track(
    menu: &ContextMenu,
    popup: HMENU,
    at: GuiAt,
    shown: &AtomicIsize,
) -> io::Result<Option<u32>> {
    let point = match at {
        GuiAt::Centre(window) => centre(window).or_else(cursor),
        GuiAt::Mouse => cursor(),
    }
    .unwrap_or_default();
    let module = own_module()?;
    let class = class_name();
    let description = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(0),
        lpfnWndProc: Some(window_proc),
        hInstance: module,
        lpszClassName: class.as_ptr(),
        ..WNDCLASSEXW::default()
    };
    // SAFETY: the description and its name live through it.
    let registered = unsafe { RegisterClassExW(&raw const description) } != 0;
    // SAFETY: the thread's last error, just after the call.
    if !registered && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
        return Err(io::Error::last_os_error());
    }
    let chosen = shown_at(menu, popup, point, (module, &class), shown);
    // SAFETY: the class this registered, no window of it left.
    unsafe { UnregisterClassW(class.as_ptr(), module) };
    chosen
}

fn shown_at(
    menu: &ContextMenu,
    popup: HMENU,
    point: POINT,
    (module, class): (HINSTANCE, &[u16]),
    shown: &AtomicIsize,
) -> io::Result<Option<u32>> {
    // SAFETY: the class registered by `track`; a hidden popup of no size, owned by no window.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class.as_ptr(),
            null_mut(),
            WS_POPUP,
            point.x,
            point.y,
            0,
            0,
            null_mut(),
            null_mut(),
            module,
            null_mut(),
        )
    };
    if window.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the window just made; the menu outlives it (destroyed below).
    unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, std::ptr::from_ref(menu) as isize) };
    let chosen =
        match shown.compare_exchange(0, window as isize, Ordering::SeqCst, Ordering::SeqCst) {
            Err(_) => 0,
            // SAFETY: the window owns the menu shown; posted `WM_NULL` after it, as the system asks.
            Ok(_) => unsafe {
                SetForegroundWindow(window);
                let chosen = TrackPopupMenu(
                    popup,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    0,
                    window,
                    std::ptr::null(),
                );
                PostMessageW(window, WM_NULL, 0, 0);
                chosen
            },
        };
    let _ = shown.compare_exchange(window as isize, 0, Ordering::SeqCst, Ordering::SeqCst);
    // SAFETY: the window made above, on this thread.
    unsafe { DestroyWindow(window) };
    let chosen = u32::try_from(chosen).unwrap_or(0);
    Ok((chosen >= FIRST).then_some(chosen))
}
