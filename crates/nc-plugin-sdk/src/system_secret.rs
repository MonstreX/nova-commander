//! Secrets kept for the user by Windows (DPAPI: `CryptProtectData` / `CryptUnprotectData`, ADR-0023 §3):
//! what one user protected only that user unprotects on that computer, no window ever asked
//! (`CRYPTPROTECT_UI_FORBIDDEN`). `entropy` is the plugin's own: data protected with one is not
//! unprotected with another.

use std::io;

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

/// `data` protected for the current user: the bytes to keep.
pub fn protect(data: &[u8], entropy: &[u8]) -> io::Result<Vec<u8>> {
    let input = blob(data)?;
    let salt = blob(entropy)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: the blobs point at slices that live through the call; no prompt, no description.
    let done = unsafe {
        CryptProtectData(
            &raw const input,
            std::ptr::null(),
            &raw const salt,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output,
        )
    };
    taken(done, output)
}

/// What [`protect`] kept, back: an error when another user, another computer or another `entropy`
/// protected it, or it is not such data.
pub fn unprotect(kept: &[u8], entropy: &[u8]) -> io::Result<Vec<u8>> {
    let input = blob(kept)?;
    let salt = blob(entropy)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: as in `protect`; the description is not asked for.
    let done = unsafe {
        CryptUnprotectData(
            &raw const input,
            std::ptr::null_mut(),
            &raw const salt,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output,
        )
    };
    taken(done, output)
}

/// A blob over `bytes` (read only by the calls: the pointer is cast to the API's mutable one).
fn blob(bytes: &[u8]) -> io::Result<CRYPT_INTEGER_BLOB> {
    let size =
        u32::try_from(bytes.len()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    Ok(CRYPT_INTEGER_BLOB {
        cbData: size,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

/// The call's output copied out and freed; the thread's last error when it failed.
fn taken(done: i32, output: CRYPT_INTEGER_BLOB) -> io::Result<Vec<u8>> {
    if done == 0 {
        return Err(io::Error::last_os_error());
    }
    let length = usize::try_from(output.cbData).unwrap_or(0);
    let bytes = if output.pbData.is_null() || length == 0 {
        Vec::new()
    } else {
        // SAFETY: the system's output of `cbData` bytes, read before it is freed below.
        unsafe { std::slice::from_raw_parts(output.pbData, length) }.to_vec()
    };
    // SAFETY: the output was allocated by the system with LocalAlloc; freed once.
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "system_secret_tests.rs"]
mod tests;
