//! A Windows shortcut's target read from its file (the `[MS-SHLLINK]` format), as FAR's farcmds asks
//! `IShellLink::GetPath` with `SLGP_RAWPATH` (`GetShellLinkPath`): the local base path and its common
//! suffix, or a share's name and the suffix; the Unicode forms when the file has them. Read without the
//! system's COM: a file that is no shortcut, or one with only a shell item list, gives none.

/// The header's size and the class of a shortcut (`00021401-0000-0000-C000-000000000046`).
const HEADER: usize = 0x4C;
const CLASS: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];
const HAS_ID_LIST: u32 = 0x1;
const HAS_LINK_INFO: u32 = 0x2;
const LOCAL_PATH: u32 = 0x1;
const NETWORK_PATH: u32 = 0x2;

fn u16_at(bytes: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(u16::from_le_bytes(
        bytes.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn offset_at(bytes: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u32_at(bytes, at)?).ok()
}

/// The NUL-ended string of the code page at `at`.
fn ansi_at(bytes: &[u8], at: usize) -> Option<String> {
    let rest = bytes.get(at..)?;
    let end = rest.iter().position(|&byte| byte == 0)?;
    Some(nc_plugin_sdk::system::from_code_page(&rest[..end], false))
}

/// The NUL-ended UTF-16 string at `at`.
fn unicode_at(bytes: &[u8], at: usize) -> Option<String> {
    let units: Vec<u16> = (bytes.get(at..)?.chunks_exact(2))
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|&unit| unit != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// The target of the shortcut whose file holds `bytes`; none when it is no shortcut or names no path.
pub fn target(bytes: &[u8]) -> Option<String> {
    if u32_at(bytes, 0)? != 0x4C || bytes.get(4..20)? != CLASS {
        return None;
    }
    let flags = u32_at(bytes, 0x14)?;
    let mut at = HEADER;
    if flags & HAS_ID_LIST != 0 {
        at += 2 + u16_at(bytes, at)?;
    }
    if flags & HAS_LINK_INFO == 0 {
        return None;
    }
    link_info(bytes.get(at..)?)
}

/// The path a link info names (`LinkInfo`): local, or a share's; the Unicode forms when it has them.
fn link_info(info: &[u8]) -> Option<String> {
    let header_size = offset_at(info, 4)?;
    let info_flags = u32_at(info, 8)?;
    let unicode = header_size >= 0x24;
    let string_at = |unicode_offset: usize, ansi_offset: usize| match unicode {
        true => unicode_at(info, offset_at(info, unicode_offset)?),
        false => ansi_at(info, offset_at(info, ansi_offset)?),
    };
    let suffix = string_at(0x20, 0x18)?;
    if info_flags & LOCAL_PATH != 0 {
        let base = string_at(0x1C, 0x10)?;
        return Some(format!("{base}{suffix}"));
    }
    if info_flags & NETWORK_PATH == 0 {
        return None;
    }
    let network = info.get(offset_at(info, 0x14)?..)?;
    let share = ansi_at(network, offset_at(network, 0x08)?)?;
    Some(match suffix.is_empty() {
        true => share,
        false => format!("{share}\\{suffix}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shortcut with no item list and a link info naming `base` (code page) and `suffix`.
    fn shortcut(base: &str, suffix: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; HEADER];
        bytes[0..4].copy_from_slice(&0x4Cu32.to_le_bytes());
        bytes[4..20].copy_from_slice(&CLASS);
        bytes[0x14..0x18].copy_from_slice(&HAS_LINK_INFO.to_le_bytes());
        let header = 0x1Cu32;
        let base_at = header as usize;
        let suffix_at = base_at + base.len() + 1;
        let size = suffix_at + suffix.len() + 1;
        let mut info = Vec::new();
        for value in [
            size as u32,
            header,
            LOCAL_PATH,
            0,
            base_at as u32,
            0,
            suffix_at as u32,
        ] {
            info.extend(value.to_le_bytes());
        }
        info.extend(base.as_bytes());
        info.push(0);
        info.extend(suffix.as_bytes());
        info.push(0);
        bytes.extend(info);
        bytes
    }

    #[test]
    fn a_shortcuts_local_path_is_its_target() {
        assert_eq!(
            target(&shortcut(r"C:\Tools\", "x.exe")).as_deref(),
            Some(r"C:\Tools\x.exe")
        );
        assert_eq!(
            target(&shortcut(r"C:\Data", "")).as_deref(),
            Some(r"C:\Data")
        );
    }

    #[test]
    fn another_file_is_no_shortcut() {
        assert_eq!(target(b"C:\\just\\a\\path"), None);
        assert_eq!(target(&[]), None);
        let mut cut = shortcut(r"C:\x", "");
        cut.truncate(HEADER + 8);
        assert_eq!(target(&cut), None, "cut short");
    }
}
