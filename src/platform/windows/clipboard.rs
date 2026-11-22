//! The Windows clipboard: text (FAR's `SetClipboardText` / `GetClipboardText`), the editor's vertical
//! blocks (FAR's `SetClipboardVText` / `GetClipboardVText`) and files for Explorer (FAR's `SetHDROP`,
//! Ctrl+Shift+C / Ctrl+Shift+X on a panel).

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::Duration;

use windows_sys::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows_sys::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT, DROPEFFECT_COPY, DROPEFFECT_MOVE};
use windows_sys::Win32::UI::Shell::DROPFILES;

/// The clipboard is shared: another program may hold it for a moment (FAR tries 5 times, 100 ms apart).
const OPEN_ATTEMPTS: u32 = 5;
const OPEN_DELAY: Duration = Duration::from_millis(100);

/// How long a clipboard call is waited for: reading text another program renders on demand, or emptying
/// the clipboard, waits for that program, which may not answer (review R30).
const CALL_WAIT: Duration = Duration::from_secs(2);

/// `call` on a thread of its own, waited for up to `limit`; past it the thread is let go (its call
/// cannot be broken) and the program that holds the clipboard is said not to answer.
fn bounded<T: Send + 'static>(
    limit: Duration,
    call: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    let (answer, answered) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("clipboard".to_string())
        .spawn(move || answer.send(call()))?;
    answered.recv_timeout(limit).unwrap_or_else(|_| {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "The program holding the clipboard does not answer",
        ))
    })
}

/// The clipboard's text, `None` when it holds no text.
pub fn clipboard_text() -> io::Result<Option<String>> {
    bounded(CALL_WAIT, read_text)
}

fn read_text() -> io::Result<Option<String>> {
    let _open = OpenClipboardGuard::open()?;
    // SAFETY: the clipboard is open; the format is a standard one.
    let handle = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT)) };
    if handle.is_null() {
        return Ok(None);
    }
    let units = read_global(handle)?;
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    Ok(Some(String::from_utf16_lossy(&units[..end])))
}

/// Puts `text` on the clipboard.
pub fn set_clipboard_text(text: &str) -> io::Result<()> {
    let data = vec![text_data(text)];
    bounded(CALL_WAIT, move || put(&data))
}

fn text_data(text: &str) -> (u32, Vec<u8>) {
    let mut units: Vec<u16> = text.encode_utf16().collect();
    units.push(0);
    (u32::from(CF_UNICODETEXT), units_to_bytes(&units))
}

/// FAR's vertical block's mark on the clipboard (`clipboard_format::vertical_block_unicode`), and
/// the marks of Borland's and Microsoft's editors that FAR puts with it and reads too.
const COLUMN_FORMAT: &str = "FAR_VerticalBlock_Unicode";
const BORLAND_FORMAT: &str = "Borland IDE Block Type";
const MSDEV_FORMAT: &str = "MSDEVColumnSelect";
/// Borland's block type of a column block.
const BORLAND_COLUMN: u8 = 2;

/// Puts `text` on the clipboard as a vertical block (FAR's `set_vtext`).
pub fn set_clipboard_column_text(text: &str) -> io::Result<()> {
    let data = vec![
        text_data(text),
        (format_named(COLUMN_FORMAT)?, 0_u32.to_le_bytes().to_vec()),
        (format_named(BORLAND_FORMAT)?, vec![BORLAND_COLUMN]),
        (format_named(MSDEV_FORMAT)?, 0_u32.to_le_bytes().to_vec()),
    ];
    bounded(CALL_WAIT, move || put(&data))
}

/// Whether the clipboard's text is a vertical block (FAR's `get_vtext`: its own mark, Microsoft's,
/// or Borland's block type of a column).
pub fn clipboard_is_column() -> io::Result<bool> {
    bounded(CALL_WAIT, || {
        let (column, msdev) = (format_named(COLUMN_FORMAT)?, format_named(MSDEV_FORMAT)?);
        // SAFETY: IsClipboardFormatAvailable takes a format by value.
        let marked =
            unsafe { IsClipboardFormatAvailable(column) | IsClipboardFormatAvailable(msdev) };
        if marked != 0 {
            return Ok(true);
        }
        let borland = format_named(BORLAND_FORMAT)?;
        let _open = OpenClipboardGuard::open()?;
        // SAFETY: the clipboard is open; the format is a registered one.
        let handle = unsafe { GetClipboardData(borland) };
        if handle.is_null() {
            return Ok(false);
        }
        Ok(first_byte(handle)? == Some(BORLAND_COLUMN))
    })
}

/// The first byte of a clipboard block.
fn first_byte(handle: HANDLE) -> io::Result<Option<u8>> {
    // SAFETY: `handle` is clipboard data the clipboard (open) owns; locking only reads it.
    let source = unsafe { GlobalLock(handle) };
    if source.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the block is locked; a byte is read only when GlobalSize says there is one.
    let byte = unsafe {
        let byte = (GlobalSize(handle) > 0).then(|| *source.cast::<u8>());
        GlobalUnlock(handle);
        byte
    };
    Ok(byte)
}

/// `data` in its formats on the clipboard, emptied first. Its blocks are made before the clipboard is
/// opened: it is held only while they are handed over, however much there is (review R29).
fn put(data: &[(u32, Vec<u8>)]) -> io::Result<()> {
    let mut blocks = Vec::with_capacity(data.len());
    for (format, bytes) in data {
        blocks.push((*format, Block(global_with(bytes)?)));
    }
    let _open = OpenClipboardGuard::open()?;
    empty()?;
    for (format, block) in &mut blocks {
        // SAFETY: the clipboard is open and emptied by this process; the block is a movable global one.
        if unsafe { SetClipboardData(*format, block.0) }.is_null() {
            // The blocks not taken are freed as they are dropped.
            return Err(io::Error::last_os_error());
        }
        // The clipboard owns it from now on.
        block.0 = std::ptr::null_mut();
    }
    Ok(())
}

/// A block of global memory for the clipboard, freed unless the clipboard took it.
struct Block(HGLOBAL);

impl Drop for Block {
    fn drop(&mut self) {
        if !self.0.is_null() {
            free(self.0);
        }
    }
}

/// Puts files on the clipboard as Explorer does for Copy (`cut` false) or Cut (`cut` true); Paste in
/// Explorer then copies or moves them.
pub fn set_clipboard_files(paths: &[PathBuf], cut: bool) -> io::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let header = DROPFILES {
        pFiles: u32::try_from(size_of::<DROPFILES>()).unwrap_or(u32::MAX),
        pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
        fNC: 1,
        fWide: 1,
    };
    // SAFETY: DROPFILES is plain data; the slice covers exactly the value and lives as long as it.
    let header_bytes = unsafe {
        std::slice::from_raw_parts((&raw const header).cast::<u8>(), size_of::<DROPFILES>())
    };
    let mut files = header_bytes.to_vec();
    files.extend(units_to_bytes(&file_list(paths)));
    let effect = if cut {
        DROPEFFECT_MOVE
    } else {
        DROPEFFECT_COPY
    };
    let effect_format = preferred_drop_effect()?;
    let data = vec![
        (u32::from(CF_HDROP), files),
        (effect_format, effect.to_le_bytes().to_vec()),
    ];
    bounded(CALL_WAIT, move || put(&data))
}

/// The names, each ending with NUL, and one more NUL after the last (the DROPFILES list).
fn file_list(paths: &[PathBuf]) -> Vec<u16> {
    let mut units = Vec::new();
    for path in paths {
        units.extend(path.as_os_str().encode_wide());
        units.push(0);
    }
    units.push(0);
    units
}

fn units_to_bytes(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|unit| unit.to_le_bytes()).collect()
}

/// The "Preferred DropEffect" format Explorer reads to tell Cut from Copy.
fn preferred_drop_effect() -> io::Result<u32> {
    format_named("Preferred DropEffect")
}

/// The clipboard format registered under `name`.
fn format_named(name: &str) -> io::Result<u32> {
    let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `name` is NUL-terminated and alive for the call.
    let format = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
    if format == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(format)
}

/// Movable global memory holding `bytes`, as the clipboard wants it.
fn global_with(bytes: &[u8]) -> io::Result<HGLOBAL> {
    // SAFETY: GlobalAlloc takes flags and a size by value.
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) };
    if memory.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `memory` is a movable block just allocated.
    let target = unsafe { GlobalLock(memory) };
    if target.is_null() {
        let error = io::Error::last_os_error();
        free(memory);
        return Err(error);
    }
    // SAFETY: the locked block is at least `bytes.len()` bytes and does not overlap `bytes`.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
        GlobalUnlock(memory);
    }
    Ok(memory)
}

/// The UTF-16 units of a clipboard block.
fn read_global(handle: HANDLE) -> io::Result<Vec<u16>> {
    // SAFETY: `handle` is clipboard data the clipboard (open) owns; locking only reads it.
    let source = unsafe { GlobalLock(handle) };
    if source.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the block is locked; GlobalSize reports its size, so the slice stays inside it.
    let units = unsafe {
        let size = GlobalSize(handle) / 2;
        let units = std::slice::from_raw_parts(source.cast::<u16>(), size).to_vec();
        GlobalUnlock(handle);
        units
    };
    Ok(units)
}

fn empty() -> io::Result<()> {
    // SAFETY: the clipboard is open (the caller holds the guard).
    if unsafe { EmptyClipboard() } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn free(memory: HGLOBAL) {
    // SAFETY: `memory` came from GlobalAlloc and nobody else owns it.
    unsafe { GlobalFree(memory) };
}

/// The clipboard open for this process until dropped.
struct OpenClipboardGuard;

impl OpenClipboardGuard {
    fn open() -> io::Result<Self> {
        let mut attempt = 1;
        loop {
            // As FAR: the console window owns it (a null owner makes SetClipboardData fail, the
            // documentation says). SAFETY: GetConsoleWindow has no arguments; OpenClipboard takes
            // a window handle or null; the clipboard is closed again on drop.
            if unsafe { OpenClipboard(GetConsoleWindow()) } != 0 {
                return Ok(Self);
            }
            if attempt == OPEN_ATTEMPTS {
                return Err(io::Error::last_os_error());
            }
            attempt += 1;
            std::thread::sleep(OPEN_DELAY);
        }
    }
}

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: the clipboard was opened by this guard.
        unsafe { CloseClipboard() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_that_does_not_come_back_is_let_go() {
        let (_never, hangs) = std::sync::mpsc::channel::<()>();
        let started = std::time::Instant::now();
        let error = bounded(Duration::from_millis(50), move || {
            let _ = hangs.recv();
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5), "not waited for");
        assert_eq!(bounded(CALL_WAIT, || Ok(3)).unwrap(), 3, "one that answers");
    }

    #[test]
    fn the_file_list_ends_with_two_nuls() {
        let list = file_list(&[PathBuf::from("C:\\a"), PathBuf::from("D:\\b c")]);
        let text = String::from_utf16(&list).unwrap();
        assert_eq!(text, "C:\\a\0D:\\b c\0\0");
    }
}
