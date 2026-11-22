//! What a file's text is told by, shared by the view (`editor::view`), Alt+F7's search (`jobs::find`)
//! and the line editors (`app`) (ADR-0028, R7 review E4): FAR's default word dividers and its
//! `GetFileCodepage` without the system's tables (those are `platform::locale`'s).

/// FAR's default `WordDiv` (`WordDiv0` in `far/config.cpp`).
pub const WORD_DIVIDERS: &str = "~!%^&*()+|{}:\"<>?`-=\\[];',./";

/// What a file's first block tells of its code page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    Utf8,
    Utf16Le,
    Utf16Be,
    /// Nothing tells: the system's default (FAR: ANSI).
    System,
}

/// The code page of a file starting with `first` (its first block, or all of a shorter file) and the
/// length of its byte order mark (FAR's `GetFileCodepage`: the mark, UTF-16 by its zero bytes as
/// `IsTextUnicode` tells it, UTF-8 that decodes past ASCII). `whole`: `first` is all the file, so a
/// character cut by its end is no UTF-8 (FAR's `is_valid_utf8` without `Partial`).
pub fn detect(first: &[u8], whole: bool) -> (Detected, u64) {
    if first.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (Detected::Utf8, 3);
    }
    if first.starts_with(&[0xFF, 0xFE]) {
        return (Detected::Utf16Le, 2);
    }
    if first.starts_with(&[0xFE, 0xFF]) {
        return (Detected::Utf16Be, 2);
    }
    if let Some(utf16) = utf16_by_zeros(first) {
        return (utf16, 0);
    }
    let page = if is_utf8(first, whole) {
        Detected::Utf8
    } else {
        Detected::System
    };
    (page, 0)
}

/// UTF-8 with something past ASCII; a character cut by the block's end still counts unless the file
/// ends there.
fn is_utf8(bytes: &[u8], whole: bool) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(text) => !text.is_ascii(),
        Err(error) => {
            !whole && error.error_len().is_none() && error.valid_up_to() > 0 && !bytes.is_ascii()
        }
    }
}

/// Text in UTF-16 has a zero in most of its odd (LE) or even (BE) bytes and few on the other side.
fn utf16_by_zeros(bytes: &[u8]) -> Option<Detected> {
    let pairs = bytes.len() / 2;
    if pairs < 2 {
        return None;
    }
    let zeros = |side: usize| {
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .filter(|pair| pair[side] == 0)
            .count()
    };
    let (even, odd) = (zeros(0), zeros(1));
    let most = pairs * 2 / 3;
    let few = pairs / 10;
    if odd >= most && even <= few {
        Some(Detected::Utf16Le)
    } else if even >= most && odd <= few {
        Some(Detected::Utf16Be)
    } else {
        None
    }
}
