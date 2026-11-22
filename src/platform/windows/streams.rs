//! A file's alternate data streams (FAR's `os::fs::enum_streams`, `FindFirstStreamW`): their names and
//! sizes, the file's own unnamed stream left out.

use std::path::Path;

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::{
    FindClose, FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard, WIN32_FIND_STREAM_DATA,
};

/// The named streams of `path` (`a.txt:notes` is `notes`) with their sizes; none when it has none or
/// cannot be asked.
pub fn alternate_streams(path: &Path) -> Vec<(String, u64)> {
    let wide = super::wide(&super::exact::exact_path(path).to_string_lossy());
    // SAFETY: WIN32_FIND_STREAM_DATA is plain data; all zeros is a valid value.
    let mut data: WIN32_FIND_STREAM_DATA = unsafe { std::mem::zeroed() };
    // SAFETY: `wide` is NUL-terminated, `data` a writable WIN32_FIND_STREAM_DATA.
    let find = unsafe {
        FindFirstStreamW(
            wide.as_ptr(),
            FindStreamInfoStandard,
            (&raw mut data).cast(),
            0,
        )
    };
    if find == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut streams = Vec::new();
    loop {
        if let Some(name) = stream_name(&data.cStreamName) {
            streams.push((name, u64::try_from(data.StreamSize).unwrap_or(0)));
        }
        // SAFETY: `find` is the handle FindFirstStreamW gave, `data` a writable WIN32_FIND_STREAM_DATA.
        if unsafe { FindNextStreamW(find, (&raw mut data).cast()) } == 0 {
            break;
        }
    }
    // SAFETY: `find` is the handle FindFirstStreamW gave, closed once.
    unsafe { FindClose(find) };
    streams
}

/// `:notes:$DATA` is `notes`; the unnamed `::$DATA` none (FAR splits at the second `:`).
fn stream_name(units: &[u16]) -> Option<String> {
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    let full = String::from_utf16_lossy(&units[..end]);
    let rest = full.strip_prefix(':')?;
    let name = rest.split(':').next().unwrap_or_default();
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    #[test]
    fn a_streams_name_is_between_the_colons() {
        assert_eq!(
            stream_name(&units(":notes:$DATA")).as_deref(),
            Some("notes")
        );
        assert_eq!(stream_name(&units("::$DATA")), None);
    }

    #[test]
    fn a_files_named_streams_are_listed() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("a.txt");
        std::fs::write(&file, "main").unwrap();
        let stream = format!("{}:notes", file.display());
        std::fs::write(stream, "12345").unwrap();
        assert_eq!(alternate_streams(&file), [("notes".to_string(), 5)]);
        assert!(alternate_streams(&temp.path().join("none")).is_empty());
    }
}
