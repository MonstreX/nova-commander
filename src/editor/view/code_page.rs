//! A file's code page as the viewer reads it (ADR-0015 §3): told from its first block, decoded one
//! character at a time where the screen needs it.

use std::fmt;
use std::sync::Arc;

/// A code page of the system (ANSI, OEM: single or double bytes), decoded by the caller's means.
pub trait ByteDecoder: Send + Sync + fmt::Debug {
    /// The character at the start of `bytes` (not empty) and how many bytes it takes (at least 1).
    fn decode(&self, bytes: &[u8]) -> (char, usize);
    /// `text` in the code page (the editor's typing; a character it has not: its `?`).
    fn encode(&self, text: &str) -> Vec<u8>;
    /// Whether `byte` begins a character of two bytes (a double-byte page's lead byte; its trail
    /// bytes may be such values too).
    fn is_lead(&self, _byte: u8) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    /// A system code page; its number names it (FAR's F8 label).
    Page(u32, Arc<dyn ByteDecoder>),
}

pub use crate::vfs::text::Detected;

/// The code page of a file starting with `first` (its first block, or all of a shorter file) and the
/// length of its byte order mark: `vfs::text::detect` (ADR-0028), a character cut by the block's end
/// taken as UTF-8's.
pub fn detect(first: &[u8]) -> (Detected, u64) {
    crate::vfs::text::detect(first, false)
}

impl Encoding {
    /// `text` in this code page (what the editor puts in).
    pub fn encode(&self, text: &str) -> Vec<u8> {
        match self {
            Self::Utf8 => text.as_bytes().to_vec(),
            Self::Utf16Le => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            Self::Utf16Be => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            Self::Page(_, page) => page.encode(text),
        }
    }

    /// The bytes of its smallest unit: a line break is one unit.
    pub fn unit(&self) -> u64 {
        match self {
            Self::Utf16Le | Self::Utf16Be => 2,
            _ => 1,
        }
    }

    /// The unit that `bytes` start with (`bytes` holds at least one), as a number.
    pub fn unit_value(&self, bytes: &[u8]) -> u32 {
        match self {
            Self::Utf16Le => u32::from(u16::from_le_bytes([bytes[0], *bytes.get(1).unwrap_or(&0)])),
            Self::Utf16Be => u32::from(u16::from_be_bytes([bytes[0], *bytes.get(1).unwrap_or(&0)])),
            _ => u32::from(bytes[0]),
        }
    }

    /// The character at the start of `bytes` (not empty) and its length; `None` when `bytes` end in
    /// its middle and more of the file follows (`more`).
    pub fn decode(&self, bytes: &[u8], more: bool) -> Option<(char, usize)> {
        match self {
            Self::Utf8 => decode_utf8(bytes, more),
            Self::Utf16Le => decode_utf16(bytes, more, u16::from_le_bytes),
            Self::Utf16Be => decode_utf16(bytes, more, u16::from_be_bytes),
            Self::Page(_, decoder) if more && bytes.len() < 2 => {
                // A lead byte needs the byte after it: ask again with it.
                let (ch, len) = decoder.decode(bytes);
                (len <= bytes.len()).then_some((ch, len))
            }
            Self::Page(_, decoder) => {
                let (ch, len) = decoder.decode(bytes);
                Some((ch, len.clamp(1, bytes.len())))
            }
        }
    }
}

fn decode_utf8(bytes: &[u8], more: bool) -> Option<(char, usize)> {
    let head = &bytes[..bytes.len().min(4)];
    let valid = match std::str::from_utf8(head) {
        Ok(text) => text,
        Err(error) if error.valid_up_to() > 0 => {
            std::str::from_utf8(&head[..error.valid_up_to()]).unwrap_or_default()
        }
        Err(error) if error.error_len().is_none() && more => return None,
        Err(error) => return Some((char::REPLACEMENT_CHARACTER, error.error_len().unwrap_or(1))),
    };
    valid.chars().next().map(|ch| (ch, ch.len_utf8()))
}

fn decode_utf16(bytes: &[u8], more: bool, unit: fn([u8; 2]) -> u16) -> Option<(char, usize)> {
    if bytes.len() < 2 {
        return (!more).then_some((char::REPLACEMENT_CHARACTER, bytes.len()));
    }
    let first = unit([bytes[0], bytes[1]]);
    if !(0xD800..0xDC00).contains(&first) {
        let ch = char::from_u32(u32::from(first)).unwrap_or(char::REPLACEMENT_CHARACTER);
        return Some((ch, 2));
    }
    if bytes.len() < 4 {
        return (!more).then_some((char::REPLACEMENT_CHARACTER, 2));
    }
    let second = unit([bytes[2], bytes[3]]);
    match char::decode_utf16([first, second]).next() {
        Some(Ok(ch)) => Some((ch, 4)),
        _ => Some((char::REPLACEMENT_CHARACTER, 2)),
    }
}

/// How FAR's OEM font shows the control characters 1..31 (code page 437's glyphs); 0 is a space.
const GLYPHS: [char; 32] = [
    ' ', '☺', '☻', '♥', '♦', '♣', '♠', '•', '◘', '○', '◙', '♂', '♀', '♪', '♫', '☼', '►', '◄', '↕',
    '‼', '¶', '§', '▬', '↨', '↑', '↓', '→', '←', '∟', '↔', '▲', '▼',
];

/// A character as the screen shows it: control characters as FAR's glyphs (the line's breaks and tabs
/// are taken before this).
pub fn shown(ch: char) -> char {
    match ch {
        '\0'..='\x1f' => GLYPHS[ch as usize],
        '\x7f' => '⌂',
        '\u{80}'..='\u{9f}' => '?',
        _ => ch,
    }
}

#[cfg(test)]
#[path = "code_page_tests.rs"]
mod tests;
