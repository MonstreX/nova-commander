//! The fields of the helper's `Set` request (Ctrl+A, ADR-0007 §1's attribute, owner and encryption rows):
//! which change, then what it takes. A text is UTF-8 with its length; a time is whether it is given and
//! on which side of 1970, the seconds and the nanoseconds.

use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{Fields, is_plain_absolute, malformed};
use crate::platform::files::{Attributes, FileTimes, Setting};

const LINK_TARGET: u8 = 0;
const OWNER: u8 = 1;
const COMPRESSED: u8 = 2;
const ENCRYPTED: u8 = 3;
const SPARSE: u8 = 4;
const REMOVE_REPARSE_POINT: u8 = 5;
const ATTRIBUTES: u8 = 6;
const TIMES: u8 = 7;

/// A link's new target the helper may write: a relative one, or a full path of a file or folder as
/// every path it takes (`is_plain_absolute`); never a device or the kernel's namespace in any spelling.
fn plain_link_target(target: &str) -> bool {
    let path = Path::new(target);
    if path.is_absolute() {
        return is_plain_absolute(path);
    }
    let kernel = [r"\??\", "/??/"]
        .iter()
        .any(|prefix| target.starts_with(prefix));
    let two_separators = target.chars().take(2).all(std::path::is_separator) && target.len() >= 2;
    !kernel && !two_separators
}

fn put_text(fields: &mut Vec<u8>, text: &str) -> io::Result<()> {
    let length = u32::try_from(text.len()).map_err(|_| malformed())?;
    fields.extend_from_slice(&length.to_le_bytes());
    fields.extend_from_slice(text.as_bytes());
    Ok(())
}

fn put_time(fields: &mut Vec<u8>, time: Option<SystemTime>) {
    let (side, span) = match time.map(|time| time.duration_since(UNIX_EPOCH)) {
        None => (0, Duration::ZERO),
        Some(Ok(after)) => (1, after),
        Some(Err(before)) => (2, before.duration()),
    };
    fields.push(side);
    fields.extend_from_slice(&span.as_secs().to_le_bytes());
    fields.extend_from_slice(&span.subsec_nanos().to_le_bytes());
}

pub(super) fn put_setting(fields: &mut Vec<u8>, setting: &Setting) -> io::Result<()> {
    match setting {
        Setting::LinkTarget(target) => {
            // Refused here, before it costs the helper (as `put_path`).
            if !plain_link_target(target) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the helper takes only full paths of files and folders",
                ));
            }
            fields.push(LINK_TARGET);
            put_text(fields, target)?;
        }
        Setting::Owner(name) => {
            fields.push(OWNER);
            put_text(fields, name)?;
        }
        Setting::Compressed(on) => fields.extend([COMPRESSED, u8::from(*on)]),
        Setting::Encrypted(on) => fields.extend([ENCRYPTED, u8::from(*on)]),
        Setting::Sparse(on) => fields.extend([SPARSE, u8::from(*on)]),
        Setting::RemoveReparsePoint => fields.push(REMOVE_REPARSE_POINT),
        Setting::Attributes(attributes) => {
            fields.push(ATTRIBUTES);
            fields.extend_from_slice(&attributes.windows_flags().to_le_bytes());
        }
        Setting::Times(times) => {
            fields.push(TIMES);
            for time in [times.created, times.accessed, times.modified, times.changed] {
                put_time(fields, time);
            }
        }
    }
    Ok(())
}

impl Fields<'_> {
    fn text(&mut self) -> io::Result<String> {
        let length: [u8; 4] = self.bytes(4)?.try_into().map_err(|_| malformed())?;
        let bytes = self.bytes(u32::from_le_bytes(length) as usize)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| malformed())
    }

    fn time(&mut self) -> io::Result<Option<SystemTime>> {
        let side = self.byte()?;
        let seconds = self.number()?;
        let nanos: [u8; 4] = self.bytes(4)?.try_into().map_err(|_| malformed())?;
        let nanos = u32::from_le_bytes(nanos);
        if nanos >= 1_000_000_000 {
            return Err(malformed());
        }
        let span = Duration::new(seconds, nanos);
        match side {
            0 if span.is_zero() => Ok(None),
            1 => UNIX_EPOCH.checked_add(span).map(Some).ok_or_else(malformed),
            2 => UNIX_EPOCH.checked_sub(span).map(Some).ok_or_else(malformed),
            _ => Err(malformed()),
        }
    }

    pub(super) fn setting(&mut self) -> io::Result<Setting> {
        Ok(match self.byte()? {
            LINK_TARGET => {
                let target = self.text()?;
                if !plain_link_target(&target) {
                    return Err(malformed());
                }
                Setting::LinkTarget(target)
            }
            OWNER => Setting::Owner(self.text()?),
            COMPRESSED => Setting::Compressed(self.flag()?),
            ENCRYPTED => Setting::Encrypted(self.flag()?),
            SPARSE => Setting::Sparse(self.flag()?),
            REMOVE_REPARSE_POINT => Setting::RemoveReparsePoint,
            ATTRIBUTES => {
                let bits: [u8; 4] = self.bytes(4)?.try_into().map_err(|_| malformed())?;
                Setting::Attributes(Attributes::from_windows_bits(u32::from_le_bytes(bits)))
            }
            TIMES => Setting::Times(FileTimes {
                created: self.time()?,
                accessed: self.time()?,
                modified: self.time()?,
                changed: self.time()?,
            }),
            _ => return Err(malformed()),
        })
    }
}
