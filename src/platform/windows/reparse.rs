//! What a reparse point keeps (`far/flink.cpp`: `GetReparsePointInfo`, `ModifyReparsePoint`): the target of
//! a symbolic link, a junction, an NFS link, an App Alias (`APPEXECLINK`), a WSL link (`LX_SYMLINK`) or a
//! container's link (`WCI`); and a new target for a symbolic link or a junction, the only two FAR edits.

use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::OwnedHandle;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
};
use windows_sys::Win32::System::Ioctl::FSCTL_GET_REPARSE_POINT;

use super::device::control;
use super::make_link::{
    HEADER, MAXIMUM_BUFFER, TAG_MOUNT_POINT, junction_names, mount_point_buffer, set_reparse_point,
};

const TAG_SYMLINK: u32 = 0xA000_000C;
const TAG_NFS: u32 = 0x8000_0014;
const TAG_APPEXECLINK: u32 = 0x8000_001B;
const TAG_LX_SYMLINK: u32 = 0xA000_001D;
const TAG_WCI: [u32; 4] = [0x8000_0018, 0x9000_1018, 0xA000_0027, 0xA000_1027];
/// `SYMLINK_FLAG_RELATIVE`.
const RELATIVE: u32 = 1;
/// `NFS_SPECFILE_LNK`.
const NFS_LINK: u64 = 0x014B_4E4C;

/// What a reparse point says of itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReparseText {
    pub tag: u32,
    pub target: String,
    /// A symbolic link or a junction: FAR lets its target be edited.
    pub editable: bool,
}

/// The reparse point's buffer as the file system keeps it, its header included.
fn read_buffer(path: &Path) -> io::Result<Vec<u8>> {
    let file = std::fs::OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut buffer = vec![0u8; MAXIMUM_BUFFER];
    control(
        &OwnedHandle::from(file),
        FSCTL_GET_REPARSE_POINT,
        &[],
        &mut buffer,
    )?;
    let length = usize::from(u16::from_le_bytes([buffer[4], buffer[5]]));
    buffer.truncate((HEADER + length).min(MAXIMUM_BUFFER));
    Ok(buffer)
}

/// What the reparse point at `path` leads to, when FAR can tell (`GetReparsePointInfo`).
pub fn reparse_text(path: &Path) -> Option<ReparseText> {
    parse(&read_buffer(path).ok()?)
}

fn word(data: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(u16::from_le_bytes(
        data.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// A symbolic link's or a junction's names after `skip` bytes of their own (the flags): the print name,
/// or the substitute name when there is none.
fn names(data: &[u8], skip: usize) -> Option<String> {
    let path = data.get(8 + skip..)?;
    let name = |offset: usize, length: usize| Some(utf16(path.get(offset..offset + length)?));
    let (print_offset, print_length) = (word(data, 4)?, word(data, 6)?);
    if print_length > 0 {
        return name(print_offset, print_length);
    }
    name(word(data, 0)?, word(data, 2)?)
}

/// FAR's `GetReparsePointInfo` over a buffer: its tag, then what it leads to.
pub fn parse(buffer: &[u8]) -> Option<ReparseText> {
    let tag = u32::from_le_bytes(buffer.get(..4)?.try_into().ok()?);
    let data = buffer.get(HEADER..)?;
    let (target, editable) = match tag {
        TAG_SYMLINK => (names(data, 4)?, true),
        TAG_MOUNT_POINT => (names(data, 0)?, true),
        TAG_NFS => {
            let kind = u64::from_le_bytes(data.get(..8)?.try_into().ok()?);
            ((kind == NFS_LINK).then(|| utf16(&data[8..]))?, false)
        }
        // The third string of the list is the program (all three versions of the format); the list
        // ends at an empty one (FAR's `enum_substrings`).
        TAG_APPEXECLINK => {
            let strings = utf16(data.get(4..)?);
            let mut list = strings.split('\0').take_while(|text| !text.is_empty());
            (list.nth(2)?.to_string(), false)
        }
        TAG_LX_SYMLINK => (String::from_utf8_lossy(data.get(4..)?).into_owned(), false),
        tag if TAG_WCI.contains(&tag) => {
            // Version, reserved, a GUID, then the name's length and the name.
            let length = word(data, 24)?;
            (utf16(data.get(26..26 + length)?), false)
        }
        _ => return None,
    };
    Some(ReparseText {
        tag,
        target: normalize(target),
        editable,
    })
}

/// FAR's `NormalizeSymlinkName`: `\??\C:\x` is shown as `C:\x`.
fn normalize(target: String) -> String {
    match target.strip_prefix(r"\??\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_string(),
        _ => target,
    }
}

/// A symbolic link's buffer as FAR fills it (`PrepareAndSetREPARSE_DATA_BUFFER`, `FillREPARSE_DATA_BUFFER`:
/// as Windows' own, the print name first and the substitute name after it, no NULs): an absolute target
/// keeps its kernel name as the substitute name, a relative one is relative.
fn symlink_buffer(target: &str) -> Option<Vec<u8>> {
    let print: Vec<u16> = target.encode_utf16().collect();
    let absolute = Path::new(target).is_absolute();
    let substitute = if absolute {
        junction_names(Path::new(target)).0
    } else {
        print.clone()
    };
    let print_length = print.len() * 2;
    let substitute_length = substitute.len() * 2;
    let data_length = 12 + print_length + substitute_length;
    if HEADER + data_length > MAXIMUM_BUFFER {
        return None;
    }
    let word = |value: usize| u16::try_from(value).ok().map(u16::to_le_bytes);
    let mut buffer = TAG_SYMLINK.to_le_bytes().to_vec();
    // The data's length, a reserved word, the substitute name's offset and length, the print name's.
    for value in [
        data_length,
        0,
        print_length,
        substitute_length,
        0,
        print_length,
    ] {
        buffer.extend(word(value)?);
    }
    let flags = if absolute { 0 } else { RELATIVE };
    buffer.extend(flags.to_le_bytes());
    for unit in print.iter().chain(&substitute) {
        buffer.extend(unit.to_le_bytes());
    }
    Some(buffer)
}

/// FAR's `ModifyReparsePoint`: the symbolic link or junction `link` leads to `target` from now on (a
/// junction's relative target is counted from the link's folder). The reparse point is replaced in one
/// call: its kind stays, and nothing is left half done. Anything else is refused.
pub fn modify_link(link: &Path, target: &str) -> io::Result<()> {
    let old = read_buffer(link)?;
    let too_long = || io::Error::new(io::ErrorKind::InvalidInput, "the target's name is too long");
    let buffer = match u32::from_le_bytes([old[0], old[1], old[2], old[3]]) {
        TAG_MOUNT_POINT => {
            let folder = link.parent().unwrap_or(link);
            let (substitute, print) = junction_names(&std::path::absolute(folder.join(target))?);
            mount_point_buffer(&substitute, &print).ok_or_else(too_long)?
        }
        TAG_SYMLINK => symlink_buffer(target).ok_or_else(too_long)?,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "only a symbolic link's or a junction's target can be changed",
            ));
        }
    };
    // As FAR's `SetREPARSE_DATA_BUFFER`: an elevated token has the privilege off until asked for, and
    // `FSCTL_SET_REPARSE_POINT` does not ask as `CreateSymbolicLinkW` does.
    let _privilege = super::owner::Privilege::enable("SeCreateSymbolicLinkPrivilege");
    super::properties::without_read_only(link, || set_reparse_point(link, &buffer))
}

#[cfg(test)]
#[path = "reparse_tests.rs"]
mod tests;
