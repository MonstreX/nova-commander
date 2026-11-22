//! A file's version resource as FAR's proclist reads it (`Plist::GetVersionInfo`): the first translation's
//! `FileVersion` and `FileDescription`.

use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};

/// What a file's version resource tells.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileVersion {
    pub version: Option<String>,
    pub description: Option<String>,
}

/// `path`'s version resource; `None` when it has none (FAR skips a `\??\` before the path).
pub fn file_version(path: &Path) -> Option<FileVersion> {
    let block = version_block(path)?;
    let translation = value(&block, r"\VarFileInfo\Translation")?;
    let [a, b, c, d, ..] = translation[..] else {
        return None;
    };
    let language = u16::from_le_bytes([a, b]);
    let page = u16::from_le_bytes([c, d]);
    let prefix = format!(r"\StringFileInfo\{language:04X}{page:04X}\");
    let text =
        |name: &str| value(&block, &format!("{prefix}{name}")).map(|bytes| utf16_text(&bytes));
    Some(FileVersion {
        version: text("FileVersion"),
        description: text("FileDescription"),
    })
}

/// `path`'s file version as its fixed part gives it, the high word first (arclite's
/// `get_module_version`: `dwFileVersionMS << 32 | dwFileVersionLS`); `None` when it has none.
pub fn fixed_version(path: &Path) -> Option<u64> {
    let block = version_block(path)?;
    let fixed = value(&block, r"\")?;
    let word = |at: usize| -> Option<u64> {
        let bytes: [u8; 4] = fixed.get(at..at + 4)?.try_into().ok()?;
        Some(u64::from(u32::from_le_bytes(bytes)))
    };
    Some((word(8)? << 32) | word(12)?)
}

/// `path`'s version resource read whole; `None` when it has none.
fn version_block(path: &Path) -> Option<Vec<u64>> {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(r"\??\").unwrap_or(&text);
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    // SAFETY: the path is NUL-terminated; no handle asked for.
    let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut()) };
    if size == 0 {
        return None;
    }
    let mut block = vec![0u64; usize::try_from(size).ok()?.div_ceil(8)];
    // SAFETY: the block holds `size` bytes, aligned.
    if unsafe { GetFileVersionInfoW(wide.as_ptr(), 0, size, block.as_mut_ptr().cast()) } == 0 {
        return None;
    }
    Some(block)
}

/// The bytes of `name` in the resource; `None` when it has none or an empty one.
fn value(block: &[u64], name: &str) -> Option<Vec<u8>> {
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let mut at: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut length = 0u32;
    // SAFETY: the block is the resource read above; the answer points into it.
    let found = unsafe {
        VerQueryValueW(
            block.as_ptr().cast(),
            wide.as_ptr(),
            &raw mut at,
            &raw mut length,
        )
    };
    if found == 0 || length == 0 || at.is_null() {
        return None;
    }
    let base = block.as_ptr() as usize;
    let start = (at as usize).checked_sub(base)?;
    // A text's length is in characters, the translation's in bytes: read at most what the block holds.
    let bytes = length_in_bytes(name, length);
    // SAFETY: the words are plain integers; their bytes are as long as they are.
    let all = unsafe { std::slice::from_raw_parts(block.as_ptr().cast::<u8>(), block.len() * 8) };
    let end = start.saturating_add(bytes).min(all.len());
    all.get(start..end).map(<[u8]>::to_vec)
}

fn length_in_bytes(name: &str, length: u32) -> usize {
    let length = usize::try_from(length).unwrap_or(0);
    if name.starts_with(r"\StringFileInfo") {
        length * 2
    } else {
        length
    }
}

/// A NUL-ended UTF-16 text from its bytes.
fn utf16_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = (bytes.chunks_exact(2))
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}
