//! FTP's listings (WinSCP's `TFTPFileSystem` through FileZilla's parser): MLSD's facts (RFC 3659 - `type`,
//! `size`, `modify` in UTC, `UNIX.mode`, `UNIX.owner` / `UNIX.ownername`, `UNIX.group` / `UNIX.groupname`,
//! `perm`), else LIST's lines as Unix's `ls -l` (`scp_listing`, its times the server's wall clock); "." and
//! ".." and `cdir` / `pdir` left out.

use crate::scp_listing::{Listed, seconds_of};
use crate::sftp::RemoteEntry;

/// One MLSD line read; none for the folder itself, its parent and a line not understood.
pub fn mlsd_entry(line: &str) -> Option<RemoteEntry> {
    let (facts, name) = line.split_once(' ')?;
    let name = name.trim_end_matches(['\r', '\n']).to_string();
    if matches!(name.as_str(), "" | "." | "..") {
        return None;
    }
    let mut entry = RemoteEntry {
        name,
        ..RemoteEntry::default()
    };
    let mut rights = None;
    let mut writable = true;
    for fact in facts.split(';').filter(|fact| !fact.is_empty()) {
        let (key, value) = fact.split_once('=')?;
        match key.to_ascii_lowercase().as_str() {
            "type" => match value.to_ascii_lowercase().as_str() {
                "dir" => entry.is_dir = true,
                "cdir" | "pdir" => return None,
                "os.unix=symlink" | "os.unix=slink" => entry.is_link = true,
                other if other.starts_with("os.unix=slink") => entry.is_link = true,
                _ => {}
            },
            "size" | "sizd" => entry.size = value.parse().unwrap_or(0),
            "modify" => entry.modified = modify_of(value),
            "unix.mode" => {
                rights = u32::from_str_radix(value.trim_start_matches('0'), 8)
                    .ok()
                    .or(Some(0))
            }
            "unix.owner" if entry.owner.is_none() => entry.owner = Some(value.to_string()),
            "unix.ownername" => entry.owner = Some(value.to_string()),
            "unix.group" if entry.group.is_none() => entry.group = Some(value.to_string()),
            "unix.groupname" => entry.group = Some(value.to_string()),
            "perm" => writable = value.contains(['w', 'a', 'c', 'm']),
            _ => {}
        }
    }
    let kind = match (entry.is_link, entry.is_dir) {
        (true, _) => 0o120_000,
        (_, true) => 0o040_000,
        _ => 0o100_000,
    };
    // No `UNIX.mode`: the kind kept, the rights as `perm` tells (write or not), a folder searchable.
    let searchable = if entry.is_dir { 0o111 } else { 0 };
    entry.permissions = match rights {
        Some(rights) => Some(kind | (rights & 0o7777)),
        None if !writable => Some(kind | 0o444 | searchable),
        None => Some(kind | 0o644 | searchable),
    };
    Some(entry)
}

/// MLSD's `modify`: `YYYYMMDDHHMMSS[.sss]` in UTC.
pub fn modify_of(value: &str) -> Option<u32> {
    let digits = value.split('.').next()?;
    if digits.len() < 14 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let part = |from: usize, to: usize| digits[from..to].parse::<i64>().ok();
    let date = (part(0, 4)?, part(4, 6)?, part(6, 8)?);
    let time = (part(8, 10)?, part(10, 12)?, part(12, 14)?);
    u32::try_from(seconds_of(date, time)).ok()
}

/// MLSD's lines read.
pub fn mlsd(lines: &[String]) -> Vec<RemoteEntry> {
    lines.iter().filter_map(|line| mlsd_entry(line)).collect()
}

/// LIST's lines read as Unix's `ls -l` (the times as read; the caller puts them in the client's zone).
pub fn list(lines: &[String], today: (i64, i64, i64)) -> Vec<Listed> {
    let text = lines.join("\n");
    crate::scp_listing::listing(&text, today)
}

#[cfg(test)]
#[path = "ftp_listing_tests.rs"]
mod tests;
