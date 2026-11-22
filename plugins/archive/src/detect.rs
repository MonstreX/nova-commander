//! Which formats a file is tried as and where (arclite's `Archive::detect`, `msearch`,
//! `accepted_signature`, `prioritize`): the formats whose signatures its first bytes hold, where they
//! are (a zip's local header after the start passed over when the file's end record names another volume);
//! before them the formats of its extension; after them every other one not opened by extension only;
//! UDF before ISO, RAR before Split, DMG before HFS.

use nc_plugin_sdk::sevenzip::{Format, IsArc, flags};

/// A format to try and where its archive starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub format: usize,
    pub offset: u64,
}

/// arclite's `max_check_size`: how much of a file's start is searched for signatures.
pub const MAX_CHECK: usize = 1 << 20;

/// A zip's local header, end record, and how much of a file's end is searched for the record.
const ZIP_LOCAL: &[u8] = b"PK\x03\x04";
const ZIP_END: &[u8] = b"PK\x05\x06";
const TAIL_CHECK: usize = 16 * 1024;
const MIN_LOCAL: usize = 30;
const MIN_END: usize = 22;

/// What detection is given: the formats, those enabled (in the formats' order), each one's quick look.
pub struct Formats<'a> {
    pub all: &'a [Format],
    pub enabled: &'a [usize],
    pub is_arc: &'a dyn Fn(usize, &[u8]) -> IsArc,
}

/// The file's start (`buffer`, all of it when `eof`), its extension with the dot; `tail` reads the last
/// 16 KB of the file when the start is not all of it.
pub fn detect(
    formats: &Formats,
    buffer: &[u8],
    eof: bool,
    extension: &str,
    tail: &mut dyn FnMut() -> Option<Vec<u8>>,
) -> Vec<Entry> {
    let mut entries = by_signature(formats, buffer, eof, tail);
    let mut found: Vec<usize> = entries.iter().map(|entry| entry.format).collect();
    for format in by_extension(formats, extension) {
        if !found.contains(&format) {
            found.push(format);
            entries.insert(0, Entry { format, offset: 0 });
        }
    }
    for &format in formats.enabled {
        let by_ext_only =
            (formats.all.get(format)).is_some_and(|it| it.flags & flags::BY_EXT_ONLY_OPEN != 0);
        if !found.contains(&format) && !by_ext_only {
            entries.push(Entry { format, offset: 0 });
        }
    }
    let named = |name: &str| formats.all.iter().position(|it| it.name == name);
    for (first, second) in [("Udf", "Iso"), ("Rar", "Split"), ("Dmg", "HFS")] {
        if let (Some(first), Some(second)) = (named(first), named(second)) {
            prioritize(&mut entries, first, second);
        }
    }
    entries
}

/// The enabled formats with the extension `extension` (arclite's `find_by_ext`, whatever its case),
/// in the formats' order (each put first in turn: the last ends first).
fn by_extension(formats: &Formats, extension: &str) -> Vec<usize> {
    if extension.is_empty() {
        return Vec::new();
    }
    let wanted = extension.trim_start_matches('.');
    (formats.all.iter().enumerate())
        .filter(|(index, it)| {
            formats.enabled.contains(index)
                && (it.extensions.iter()).any(|ext| ext.eq_ignore_ascii_case(wanted))
        })
        .map(|(index, _)| index)
        .collect()
}

/// arclite's `prioritize`: `first` moved before the last `second` ahead of it.
fn prioritize(entries: &mut Vec<Entry>, first: usize, second: usize) {
    let mut before = None;
    for at in 0..entries.len() {
        if entries[at].format == second {
            before = Some(at);
        } else if entries[at].format == first {
            if let Some(before) = before {
                let entry = entries.remove(at);
                entries.insert(before, entry);
            }
            return;
        }
    }
}

/// One signature to look for: its bytes, its format.
struct Signature<'a> {
    bytes: &'a [u8],
    format: usize,
}

/// arclite's signature search (`msearch`) and its check of each place found, in the places' order.
fn by_signature(
    formats: &Formats,
    buffer: &[u8],
    eof: bool,
    tail: &mut dyn FnMut() -> Option<Vec<u8>>,
) -> Vec<Entry> {
    let signatures = signatures(formats);
    let mut places = search(formats, &signatures, buffer, eof);
    places.sort_by_key(|(_, at)| *at);
    let mut whole = eof;
    let mut refused = false;
    let mut entries = Vec::new();
    for (index, at) in places {
        let signature = &signatures[index];
        if accepted(at, signature.bytes, buffer, (whole, refused), tail) {
            let offset = formats.all[signature.format].signature_offset;
            entries.push(Entry {
                format: signature.format,
                offset: (at as u64).saturating_sub(u64::from(offset)),
            });
        } else {
            refused = true;
            whole = false;
        }
    }
    entries
}

/// The enabled formats' signatures (DMG's not: arclite leaves it out), one place for each distinct
/// signature (the last format's, as arclite's state matrix keeps it).
fn signatures<'a>(formats: &Formats<'a>) -> Vec<Signature<'a>> {
    let mut signatures: Vec<Signature> = Vec::new();
    for &format in formats.enabled {
        let Some(it) = formats.all.get(format) else {
            continue;
        };
        if it.name == "Dmg" {
            continue;
        }
        for bytes in &it.signatures {
            if bytes.is_empty() {
                continue;
            }
            match signatures
                .iter_mut()
                .find(|known| known.bytes == bytes.as_slice())
            {
                Some(known) => known.format = format,
                None => signatures.push(Signature { bytes, format }),
            }
        }
    }
    signatures
}

/// Every place of `buffer` a signature starts at whose format's quick look takes it (or asks for more
/// when the buffer is not the whole file).
fn search(
    formats: &Formats,
    signatures: &[Signature],
    buffer: &[u8],
    eof: bool,
) -> Vec<(usize, usize)> {
    // The signatures by their first byte: a place is checked against those only.
    let mut by_first: Vec<Vec<usize>> = vec![Vec::new(); 256];
    for (index, signature) in signatures.iter().enumerate() {
        by_first[usize::from(signature.bytes[0])].push(index);
    }
    let mut places = Vec::new();
    for (at, byte) in buffer.iter().enumerate() {
        for &index in &by_first[usize::from(*byte)] {
            let signature = &signatures[index];
            if !buffer[at..].starts_with(signature.bytes) {
                continue;
            }
            let offset = (formats.all[signature.format].signature_offset) as usize;
            let Some(start) = at.checked_sub(offset) else {
                continue;
            };
            let look = (formats.is_arc)(signature.format, &buffer[start..]);
            if look == IsArc::Yes || (look == IsArc::NeedMore && !eof) {
                places.push((index, at));
            }
        }
    }
    places
}

/// arclite's `accepted_signature`: a zip local header after the start is taken unless the file's end
/// record says it is not the first volume; after one refused, none is (`eof_i = -1`).
fn accepted(
    at: usize,
    signature: &[u8],
    buffer: &[u8],
    (whole, refused): (bool, bool),
    tail: &mut dyn FnMut() -> Option<Vec<u8>>,
) -> bool {
    if at == 0 || signature != ZIP_LOCAL {
        return true;
    }
    if refused {
        return false;
    }
    let read;
    let (bytes, base) = if whole {
        let end = buffer.len().saturating_sub(MIN_END - ZIP_END.len());
        let mut start = at + MIN_LOCAL;
        if start >= end {
            return true;
        }
        if start + TAIL_CHECK < end {
            start = end - TAIL_CHECK;
        }
        (&buffer[start..end], buffer)
    } else {
        let Some(last) = tail().filter(|last| last.len() == TAIL_CHECK) else {
            return true;
        };
        read = last;
        (
            &read[..TAIL_CHECK - MIN_END + ZIP_END.len()],
            read.as_slice(),
        )
    };
    let Some(record) = rfind(bytes, ZIP_END) else {
        return true;
    };
    // The disk number after the record's signature: 0 for the first volume.
    let offset = bytes.as_ptr() as usize - base.as_ptr() as usize;
    let disk = offset + record + ZIP_END.len();
    base.get(disk..disk + 2)
        .is_none_or(|number| number == [0, 0])
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    (0..=haystack.len().checked_sub(needle.len())?)
        .rev()
        .find(|&at| haystack[at..].starts_with(needle))
}

#[cfg(test)]
#[path = "detect_tests.rs"]
mod tests;
