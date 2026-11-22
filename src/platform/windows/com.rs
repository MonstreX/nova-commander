//! COM on the calling thread for as long as a guard lives: FAR's `initialize(mode::sta)`, for the shell's
//! COM objects (`IFileOperation`, `ShellExecuteExW`'s extensions).

use windows_sys::Win32::Foundation::{S_FALSE, S_OK};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

/// COM for this thread while the guard lives (FAR's `initialize(mode::sta)`); a thread already in another
/// mode keeps it.
pub(super) struct Apartment(bool);

impl Apartment {
    pub(super) fn enter() -> Self {
        // SAFETY: plain call; balanced by `CoUninitialize` in `drop` only when it succeeded.
        let result = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        Self(result == S_OK || result == S_FALSE)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful `CoInitializeEx` of `enter` on this same thread.
            unsafe { CoUninitialize() };
        }
    }
}
