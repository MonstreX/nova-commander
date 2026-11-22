//! Code pages (FAR's `far/codepage.cpp`): the ones the system has (`InstalledCodepages`: each number with
//! the name out of its brackets and its longest character), FAR's real ANSI and OEM code pages (the
//! legacy ones while the system's are UTF-8), and bytes of any of them to text and back, a character cut
//! by the end of the bytes left for the bytes after them.

use std::cell::RefCell;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{ERROR_INVALID_FLAGS, GetLastError};
use windows_sys::Win32::Globalization::{
    CP_INSTALLED, CP_UTF8, CPINFO, CPINFOEXW, EnumSystemCodePagesW, GetACP, GetCPInfo,
    GetCPInfoExW, GetLocaleInfoEx, GetOEMCP, LOCALE_IUSEUTF8LEGACYACP, LOCALE_IUSEUTF8LEGACYOEMCP,
    LOCALE_RETURN_NUMBER, MB_ERR_INVALID_CHARS, MultiByteToWideChar, WideCharToMultiByte,
};

use super::locale::decode;
use crate::platform::locale::CodePageInfo;

thread_local! {
    /// What `EnumSystemCodePagesW` gives, on the thread that asks (its callback has no context).
    static ENUMERATED: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

/// The installed code pages by their numbers, read once (FAR's `InstalledCodepages`); UTF-8 and
/// UTF-16 are among them or not as the system says.
pub fn installed_code_pages() -> &'static [CodePageInfo] {
    static INSTALLED: OnceLock<Vec<CodePageInfo>> = OnceLock::new();
    INSTALLED.get_or_init(read_installed)
}

fn read_installed() -> Vec<CodePageInfo> {
    ENUMERATED.with_borrow_mut(Vec::clear);
    // SAFETY: the callback is a plain function that only reads the string it is given, on this thread.
    unsafe { EnumSystemCodePagesW(Some(enumerated), CP_INSTALLED) };
    let numbers = ENUMERATED.with_borrow_mut(std::mem::take);
    let mut pages: Vec<CodePageInfo> = numbers.into_iter().filter_map(code_page_info).collect();
    pages.sort_by_key(|page| page.id);
    pages.dedup_by_key(|page| page.id);
    pages
}

/// `CODEPAGE_ENUMPROCW`: one installed code page's number as text.
unsafe extern "system" fn enumerated(number: *const u16) -> windows_sys::core::BOOL {
    if number.is_null() {
        return 1;
    }
    // SAFETY: the system passes a NUL-terminated string that lives for this call.
    let length = (0..)
        .take_while(|&at| unsafe { *number.add(at) } != 0)
        .count();
    // SAFETY: `length` units before the NUL are readable.
    let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(number, length) });
    if let Ok(id) = text.trim().parse() {
        ENUMERATED.with_borrow_mut(|numbers| numbers.push(id));
    }
    1
}

/// FAR's `get_codepage_info`: the name from `GetCPInfoEx` out of its brackets ("1251  (ANSI -
/// Cyrillic)" is "ANSI - Cyrillic"), else the number; none for a code page with no characters.
fn code_page_info(id: u32) -> Option<CodePageInfo> {
    let mut info = CPINFOEXW::default();
    // SAFETY: `info` is a live, writable CPINFOEXW.
    if unsafe { GetCPInfoExW(id, 0, &raw mut info) } != 0 {
        let length = (info.CodePageName.iter()).position(|&unit| unit == 0);
        let name = String::from_utf16_lossy(
            &info.CodePageName[..length.unwrap_or(info.CodePageName.len())],
        );
        return (info.MaxCharSize > 0).then(|| CodePageInfo {
            id,
            name: name_in_brackets(&name).to_string(),
            max_char_size: info.MaxCharSize,
        });
    }
    let short = cp_info(id)?;
    (short.MaxCharSize > 0).then(|| CodePageInfo {
        id,
        name: id.to_string(),
        max_char_size: short.MaxCharSize,
    })
}

/// FAR's `extract_codepage_name`: "XXXX (Name)" is "Name", a name without brackets stays.
pub(super) fn name_in_brackets(text: &str) -> &str {
    let Some((_, name)) = text.split_once('(') else {
        return text;
    };
    name.rfind(')').map_or(text, |close| &name[..close])
}

fn cp_info(id: u32) -> Option<CPINFO> {
    let mut info = CPINFO::default();
    // SAFETY: `info` is a live, writable CPINFO.
    (unsafe { GetCPInfo(id, &raw mut info) } != 0).then_some(info)
}

/// FAR's `real_ansi`: the system's ANSI code page, the legacy one when the system's are UTF-8.
pub fn ansi_code_page() -> u32 {
    // SAFETY: neither takes an argument.
    let (ansi, oem) = unsafe { (GetACP(), GetOEMCP()) };
    if ansi == CP_UTF8 && oem == CP_UTF8 {
        return legacy(LOCALE_IUSEUTF8LEGACYACP).unwrap_or(ansi);
    }
    ansi
}

/// FAR's `real_oem`.
pub fn oem_code_page() -> u32 {
    // SAFETY: neither takes an argument.
    let (ansi, oem) = unsafe { (GetACP(), GetOEMCP()) };
    if ansi == CP_UTF8 && oem == CP_UTF8 {
        return legacy(LOCALE_IUSEUTF8LEGACYOEMCP).unwrap_or(oem);
    }
    oem
}

fn legacy(kind: u32) -> Option<u32> {
    let mut value = 0u32;
    // SAFETY: with `LOCALE_RETURN_NUMBER` the buffer is a u32, two UTF-16 units long; a null locale
    // name is the user's default.
    let written = unsafe {
        GetLocaleInfoEx(
            std::ptr::null(),
            kind | LOCALE_RETURN_NUMBER,
            (&raw mut value).cast::<u16>(),
            2,
        )
    };
    (written > 0 && value > 0).then_some(value)
}

/// `bytes` in `code_page` as text and how many of them it took: all when `last`, else up to the end of
/// the last whole character (FAR's `get_chars` with its `PartialInput`). UTF-16 is not decoded here.
pub fn decode_page(code_page: u32, bytes: &[u8], last: bool) -> (String, usize) {
    let whole = if last {
        bytes.len()
    } else {
        whole_characters(code_page, bytes)
    };
    (decode(code_page, &bytes[..whole]), whole)
}

/// How many of `bytes` are whole characters in `code_page`.
fn whole_characters(code_page: u32, bytes: &[u8]) -> usize {
    let Some(info) = cp_info(code_page) else {
        return bytes.len();
    };
    if info.MaxCharSize <= 1 {
        return bytes.len();
    }
    if info.LeadByte[0] != 0 {
        return whole_double_bytes(&info.LeadByte, bytes);
    }
    // Longer characters (UTF-8, GB18030): the longest start of the bytes that decodes without a fault.
    let longest = usize::try_from(info.MaxCharSize).unwrap_or(1);
    for cut in 0..longest.min(bytes.len() + 1) {
        let start = &bytes[..bytes.len() - cut];
        match decodes_whole(code_page, start) {
            Some(true) => return start.len(),
            Some(false) => {}
            None => break,
        }
    }
    bytes.len()
}

/// A double-byte code page: each lead byte takes the byte after it.
fn whole_double_bytes(lead_bytes: &[u8; 12], bytes: &[u8]) -> usize {
    let is_lead = |byte: u8| {
        (lead_bytes.as_chunks::<2>().0.iter())
            .take_while(|range| range[0] != 0)
            .any(|range| (range[0]..=range[1]).contains(&byte))
    };
    let mut at = 0;
    while at < bytes.len() {
        let step = if is_lead(bytes[at]) { 2 } else { 1 };
        if at + step > bytes.len() {
            break;
        }
        at += step;
    }
    at
}

/// Whether `bytes` decode with no fault; none when the code page cannot tell (`ERROR_INVALID_FLAGS`).
fn decodes_whole(code_page: u32, bytes: &[u8]) -> Option<bool> {
    let length = i32::try_from(bytes.len()).ok()?;
    if length == 0 {
        return Some(true);
    }
    let null = std::ptr::null_mut();
    // SAFETY: `bytes` holds `length` bytes; no buffer is passed, only the size is asked.
    let units = unsafe {
        MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            length,
            null,
            0,
        )
    };
    if units > 0 {
        return Some(true);
    }
    // SAFETY: no argument; read at once after the call that failed.
    (unsafe { GetLastError() } != ERROR_INVALID_FLAGS).then_some(false)
}

/// `text` in `code_page` (not UTF-16); what it cannot hold becomes its default character.
pub fn encode_page(code_page: u32, text: &str) -> Vec<u8> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let Ok(length) = i32::try_from(wide.len()) else {
        return text.as_bytes().to_vec();
    };
    if length == 0 {
        return Vec::new();
    }
    let null = std::ptr::null_mut();
    let no_flag = std::ptr::null_mut();
    // SAFETY: `wide` holds `length` units; no buffer is passed, only the size is asked.
    let size = unsafe {
        WideCharToMultiByte(
            code_page,
            0,
            wide.as_ptr(),
            length,
            null,
            0,
            std::ptr::null(),
            no_flag,
        )
    };
    let Some(room) = usize::try_from(size).ok().filter(|&room| room > 0) else {
        return text.as_bytes().to_vec();
    };
    let mut bytes = vec![0u8; room];
    // SAFETY: `wide` holds `length` units, `bytes` room for `size` bytes.
    let written = unsafe {
        WideCharToMultiByte(
            code_page,
            0,
            wide.as_ptr(),
            length,
            bytes.as_mut_ptr(),
            size,
            std::ptr::null(),
            no_flag,
        )
    };
    bytes.truncate(usize::try_from(written).unwrap_or(0));
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_is_taken_out_of_its_brackets() {
        assert_eq!(
            name_in_brackets("1251  (ANSI - Cyrillic)"),
            "ANSI - Cyrillic"
        );
        assert_eq!(name_in_brackets("Plain"), "Plain");
        assert_eq!(
            name_in_brackets("20127 (US-ASCII (7-bit))"),
            "US-ASCII (7-bit)"
        );
    }

    #[test]
    fn the_system_has_its_code_pages_by_number() {
        let pages = installed_code_pages();
        assert!(
            pages.windows(2).all(|pair| pair[0].id < pair[1].id),
            "sorted"
        );
        let ansi = pages
            .iter()
            .find(|page| page.id == 1252)
            .expect("1252 is always installed");
        assert_eq!(ansi.max_char_size, 1);
        // In the language of the system ("ANSI - Latin I", "ANSI - латиница 1").
        assert!(ansi.name.starts_with("ANSI - "), "{}", ansi.name);
        assert!(pages.iter().any(|page| page.id == ansi_code_page()));
    }

    #[test]
    fn a_double_byte_character_cut_waits_for_its_second_byte() {
        // Shift-JIS "あ" is 82 A0.
        let (text, used) = decode_page(932, b"a\x82\xA0b\x82", false);
        assert_eq!((text.as_str(), used), ("aあb", 4));
        let (text, used) = decode_page(932, b"\xB1c", false);
        assert_eq!(
            (text.as_str(), used),
            ("\u{FF71}c", 2),
            "a lone byte is a character of its own"
        );
    }

    #[test]
    fn a_long_character_cut_waits_for_the_rest() {
        let bytes = "яя".as_bytes();
        let (text, used) = decode_page(CP_UTF8, &bytes[..3], false);
        assert_eq!((text.as_str(), used), ("я", 2));
        let (text, used) = decode_page(CP_UTF8, &bytes[..3], true);
        assert_eq!(used, 3, "the last bytes are all taken");
        assert!(text.starts_with('я'));
    }

    #[test]
    fn a_four_byte_gb18030_character_cut_waits_for_the_rest() {
        // GB18030 "€" outside its two-byte part: U+0080 is 81 30 81 30.
        let bytes = b"a\x81\x30\x81\x30";
        for cut in 2..5 {
            let (text, used) = decode_page(54936, &bytes[..cut], false);
            assert_eq!(
                (text.as_str(), used),
                ("a", 1),
                "cut after {} bytes",
                cut - 1
            );
        }
        let (text, used) = decode_page(54936, bytes, false);
        assert_eq!((text.as_str(), used), ("a\u{80}", 5));
    }

    #[test]
    fn a_single_byte_code_page_takes_every_byte() {
        let (text, used) = decode_page(1251, b"\xEF\xF0\xE8", false);
        assert_eq!((text.as_str(), used), ("при", 3));
        assert_eq!(encode_page(1251, "при"), b"\xEF\xF0\xE8");
        assert_eq!(encode_page(866, "при"), b"\xAF\xE0\xA8");
        assert_eq!(encode_page(1252, ""), b"");
    }
}
