//! The texts and paths FAR's farcmds works with (`Mix.cpp`, `OpenCmd.cpp`): environment variables expanded
//! as Windows' `ExpandEnvironmentStrings` does, quotes taken off, a name made whole against the current
//! folder, a file's text read as `loadFile` reads it (its byte order mark, UTF-8, UTF-16, else the code
//! page) up to the first NUL.

use std::path::{Component, Path, PathBuf};

/// `text` with each `%NAME%` of a variable that is set replaced by its value; the others left as they are.
pub fn expand(text: &str) -> String {
    let mut done = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        done.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match (!name.is_empty())
                    .then(|| std::env::var(name).ok())
                    .flatten()
                {
                    Some(value) => {
                        done.push_str(&value);
                        rest = &after[end + 1..];
                    }
                    None => {
                        done.push('%');
                        rest = after;
                    }
                }
            }
            None => {
                done.push('%');
                rest = after;
            }
        }
    }
    done.push_str(rest);
    done
}

/// `text` without its quotes (FAR's `Unquote`: every `"` goes).
pub fn unquote(text: &str) -> String {
    text.replace('"', "")
}

/// Where a path's name starts (FAR's `PointToName`): after the last `\` or `/`, else after a drive's `:`.
pub fn name_start(path: &str) -> usize {
    match path.rfind(['\\', '/']) {
        Some(at) => at + 1,
        None if path.len() >= 2 && path.as_bytes()[1] == b':' => 2,
        None => 0,
    }
}

/// `name` whole against `folder` (FAR's `ConvertPath` with `CPM_FULL`): `.` and `..` worked out.
pub fn full_path(name: &str, folder: &Path) -> PathBuf {
    let joined = folder.join(name);
    let mut done = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if done.file_name().is_some() {
                    done.pop();
                }
            }
            other => done.push(other),
        }
    }
    done
}

/// Whether `bytes` are UTF-8 with letters beyond ASCII (FAR's `IsTextUTF8`, a cut last letter allowed).
fn is_utf8(bytes: &[u8]) -> bool {
    if bytes.is_ascii() {
        return false;
    }
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(error) => error.error_len().is_none() && bytes.len() - error.valid_up_to() < 4,
    }
}

fn utf16(bytes: &[u8], big_endian: bool) -> String {
    let units: Vec<u16> = (bytes.chunks_exact(2))
        .map(|pair| match big_endian {
            true => u16::from_be_bytes([pair[0], pair[1]]),
            false => u16::from_le_bytes([pair[0], pair[1]]),
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Whether `bytes` look like UTF-16 (FAR asks `IsTextUnicode`): even, and most high bytes nothing.
fn looks_utf16(bytes: &[u8], big_endian: bool) -> bool {
    let pairs = bytes.len() / 2;
    if pairs == 0 || !bytes.len().is_multiple_of(2) {
        return false;
    }
    let high = usize::from(!big_endian);
    let zeros = (bytes.chunks_exact(2))
        .filter(|pair| pair[high] == 0)
        .count();
    zeros * 2 > pairs
}

/// `bytes` as text (FAR's `ConvertBuffer`): by their byte order mark, UTF-8, UTF-16, else the code page
/// (the console's for a program's output); up to the first NUL.
pub fn decode(bytes: &[u8], console: bool) -> String {
    let text = if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        utf16(rest, false)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        utf16(rest, true)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else if is_utf8(bytes) {
        String::from_utf8_lossy(bytes).into_owned()
    } else if looks_utf16(bytes, false) {
        utf16(bytes, false)
    } else if looks_utf16(bytes, true) {
        utf16(bytes, true)
    } else {
        nc_plugin_sdk::system::from_code_page(bytes, console)
    };
    match text.find('\0') {
        Some(end) => text[..end].to_string(),
        None => text,
    }
}

/// At most `most` bytes of the file `path` (never the whole of a huge file); none when it is not there or
/// cannot be read.
pub fn read_bytes(path: &Path, most: u64) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(most).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// The text of the file `path`, at most `most` bytes of it, without a mark in the code page (`console`:
/// the console's, as FAR reads a file that holds an object).
pub fn read_text(path: &Path, most: u64, console: bool) -> Option<String> {
    Some(decode(&read_bytes(path, most)?, console))
}

/// The first line of `text` (FAR's `__getContent`: up to the first CR, else LF).
pub fn first_line(text: &str) -> &str {
    let end = text
        .find('\r')
        .or_else(|| text.find('\n'))
        .unwrap_or(text.len());
    &text[..end]
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
