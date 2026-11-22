//! FAR's TmpPanel list files (`TmpPanel.cpp` `ReadFileLines`, `TmpMix.cpp`): a file of paths, a line each,
//! read in its encoding - UTF-16 or UTF-8 by their byte order marks, UTF-8 without one when its bytes are
//! UTF-8 with more than ASCII (`IsTextUTF8`), UTF-16 when they look like it (`IsTextUnicode`'s statistics,
//! here by where the zero bytes are), else the console's OEM code page; lines split at CR or LF, the empty
//! ones dropped, nothing trimmed, quotes kept, no comments. A line is `%VAR%`-expanded, its `|label|`
//! dropped, and named from the program's folder when it is relative (`GetFileInfoAndValidate`). The file
//! named on the command line is looked for there, in the plugin's folder, then the program's and `%PATH%`
//! (`FindListFile`). A reversed (big-endian) byte order mark is read as UTF-16BE: FAR passes it to
//! `MultiByteToWideChar`, which reads nothing of it.

use std::path::{Path, PathBuf};

/// The lines of a list file's `bytes`; `oem` reads the OEM code page.
pub fn lines(bytes: &[u8], oem: &dyn Fn(&[u8]) -> String) -> Vec<String> {
    let text = match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ if is_text_utf8(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        _ if looks_utf16(bytes) => utf16(bytes, u16::from_le_bytes),
        _ => return split(bytes, |line| oem(line)),
    };
    (text.split(['\r', '\n']))
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Single-byte lines split at CR or LF before they are read (FAR's way for its code pages).
fn split(bytes: &[u8], read: impl Fn(&[u8]) -> String) -> Vec<String> {
    (bytes.split(|byte| *byte == b'\r' || *byte == b'\n'))
        .filter(|line| !line.is_empty())
        .map(read)
        .filter(|line| !line.is_empty())
        .collect()
}

fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let units: Vec<u16> = (bytes.as_chunks::<2>().0.iter())
        .map(|pair| unit([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// FAR's `IsTextUTF8` (`TmpMix.cpp`): valid UTF-8 (a character cut at the very end let be) with a byte
/// above ASCII.
pub fn is_text_utf8(bytes: &[u8]) -> bool {
    let (mut ascii, mut octets, mut last) = (true, 0usize, 0usize);
    for (at, &byte) in bytes.iter().enumerate() {
        if byte & 0x80 != 0 {
            ascii = false;
        }
        if octets > 0 {
            if byte & 0xC0 != 0x80 {
                return false;
            }
            octets -= 1;
            continue;
        }
        last = at;
        if byte & 0x80 != 0 {
            let leading = byte.leading_ones() as usize;
            if leading < 2 {
                return false;
            }
            octets = leading - 1;
        }
    }
    (octets == 0 || bytes.len() - last < 4) && !ascii
}

/// UTF-16LE without a byte order mark, as `IsTextUnicode`'s statistics take it: an even length, zero
/// bytes in most of the characters' high halves and few in their low ones.
fn looks_utf16(bytes: &[u8]) -> bool {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return false;
    }
    let pairs = bytes.len() / 2;
    let high = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .filter(|pair| pair[1] == 0)
        .count();
    let low = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .filter(|pair| pair[0] == 0)
        .count();
    high * 2 > pairs && low * 10 < pairs
}

/// FAR's `ExpandEnvStrs` (Windows' `ExpandEnvironmentStrings`): `%NAME%` of a variable set is its value;
/// another is left as it is.
pub fn expand_env(text: &str) -> String {
    expand_with(text, &|name| std::env::var(name).ok())
}

/// [`expand_env`] with the variables `lookup` knows.
pub fn expand_with(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        match (!name.is_empty()).then(|| lookup(name)).flatten() {
            Some(value) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                // Not a variable: the `%` stays, the next one may start one.
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// FAR's `ParseParam`: `|label|rest` as the label and the rest without its leading spaces; none without
/// both bars.
pub fn parse_param(line: &str) -> Option<(&str, &str)> {
    let param = line.strip_prefix('|')?;
    let (label, tail) = param.split_once('|')?;
    Some((label, tail.trim_start()))
}

/// FAR's `GetFullPath` (`ConvertPath(CPM_FULL)`): `name` from `folder` when it is relative, `.` and `..`
/// gone.
pub fn full_path(name: &str, folder: Option<&Path>) -> PathBuf {
    let joined = match folder {
        Some(folder) => folder.join(name),
        None => PathBuf::from(name),
    };
    std::path::absolute(&joined).unwrap_or(joined)
}

/// FAR's devices a line may name (`GetFileInfoAndValidate`): `\\.\X:`, `\\.\PhysicalDriveN`, `\\.\cdromN`.
pub fn is_device(name: &str) -> bool {
    let Some(tail) = name.strip_prefix(r"\\.\") else {
        return false;
    };
    let mut chars = tail.chars();
    if let (Some(letter), Some(':'), None) = (chars.next(), chars.next(), chars.next())
        && letter.is_alphabetic()
    {
        return true;
    }
    ["PhysicalDrive", "cdrom"].iter().any(|device| {
        tail.get(..device.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(device))
            && tail.len() > device.len()
            && tail[device.len()..].chars().all(|c| c.is_ascii_digit())
    })
}

/// What a list's line puts on the panel (FAR's `PutOneFile` with `GetFileInfoAndValidate`): its whole
/// path when it is there, a device as it is written, with `any` the line as written (expanded, without
/// its label) for what is not there; none for an empty one or, without `any`, one not there.
pub fn item(line: &str, folder: Option<&Path>, any: bool) -> Option<PathBuf> {
    let expanded = expand_env(line);
    let name = match parse_param(&expanded) {
        Some((_, tail)) if !tail.is_empty() => tail,
        _ => &expanded,
    };
    if name.is_empty() {
        return None;
    }
    if is_device(name) {
        return Some(PathBuf::from(name));
    }
    let full = full_path(name, folder);
    if std::fs::symlink_metadata(&full).is_ok() {
        return Some(full);
    }
    any.then(|| PathBuf::from(name))
}

/// FAR's `FindListFile`: `name` from the program's folder if it is there, else in the plugin's folder,
/// the program's (`%FARHOME%`) and each of `%PATH%`'s.
pub fn find(name: &str, folder: Option<&Path>, plugin: &Path) -> Option<PathBuf> {
    let full = full_path(name, folder);
    if full.exists() {
        return Some(full);
    }
    let home = plugin
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf);
    let path = std::env::var_os("PATH").unwrap_or_default();
    let places = std::iter::once(plugin.to_path_buf())
        .chain(home)
        .chain(std::env::split_paths(&path));
    for place in places.filter(|place| !place.as_os_str().is_empty()) {
        let found = place.join(name);
        if found.is_file() {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
#[path = "listfile_tests.rs"]
mod tests;
