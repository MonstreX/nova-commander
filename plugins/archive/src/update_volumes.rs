//! Where a new archive written by 7-Zip's library goes (arclite's update streams): one file
//! (`SimpleUpdateStream`), or volumes of one size each, `name.001`, `name.002`, ... (its
//! `MultiVolumeUpdateStream`: the place moved and the size set across them, the volumes made as they are
//! reached, all of them deleted when the writing fails).

use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// arclite's `c_volume_ext`.
pub const VOLUME_EXT: &str = ".001";

/// arclite's `get_volume_path`: `index`'s volume of `path` (`.001` on from the first; a path ending in
/// `.001` has it taken off first).
pub fn volume_path(path: &Path, index: u64) -> PathBuf {
    let text = path.to_string_lossy();
    let base = match text.rfind('.') {
        Some(dot) if dot > 0 && text[dot..].eq_ignore_ascii_case(VOLUME_EXT) => &text[..dot],
        _ => &text[..],
    };
    PathBuf::from(format!("{base}.{:03}", index + 1))
}

/// The new archive's bytes as the library writes them: one file, volumes, or a file after the SFX
/// module its first bytes hold (arclite's `SfxUpdateStream`: its places counted from the module's end).
pub enum Output {
    File(File),
    Volumes(Volumes),
    Shifted(File, u64),
}

impl Output {
    pub fn write(&mut self, data: &[u8]) -> io::Result<()> {
        match self {
            Self::File(file) | Self::Shifted(file, _) => file.write_all(data),
            Self::Volumes(volumes) => volumes.write(data),
        }
    }

    pub fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        match self {
            Self::File(file) => file.seek(to),
            Self::Volumes(volumes) => volumes.seek(to),
            Self::Shifted(file, start) => {
                let to = match to {
                    SeekFrom::Start(at) => SeekFrom::Start(at.saturating_add(*start)),
                    other => other,
                };
                let at = file.seek(to)?;
                // arclite's `E_INVALIDARG` for a place inside the module.
                (at.checked_sub(*start)).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))
            }
        }
    }

    pub fn set_size(&mut self, size: u64) -> io::Result<()> {
        match self {
            Self::File(file) => file.set_len(size),
            Self::Volumes(volumes) => volumes.set_size(size),
            Self::Shifted(file, start) => file.set_len(size.saturating_add(*start)),
        }
    }
}

/// A file made anew (arclite's `CREATE_ALWAYS`).
fn created(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}

/// A volume that is there opened to write into.
fn reopened(path: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).open(path)
}

/// arclite's `MultiVolumeUpdateStream`.
pub struct Volumes {
    path: PathBuf,
    size: u64,
    /// Where the writing is, where it was asked to go, how long the whole is.
    position: u64,
    asked: u64,
    length: u64,
    /// The volume open and its index.
    volume: File,
    index: u64,
}

impl Volumes {
    /// The first volume of `path` made; `size` each.
    pub fn new(path: &Path, size: u64) -> io::Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            size: size.max(1),
            position: 0,
            asked: 0,
            length: 0,
            volume: created(&volume_path(path, 0))?,
            index: 0,
        })
    }

    /// The last volume's index (arclite's `get_last_volume_idx`).
    fn last(&self) -> u64 {
        self.length.saturating_sub(1) / self.size
    }

    /// Volume `index` open: made when past the last (those before it made full), else opened.
    fn open(&mut self, index: u64) -> io::Result<()> {
        if index == self.index {
            return Ok(());
        }
        let last = self.last();
        if index > last {
            for filler in last + 1..index {
                let made = created(&volume_path(&self.path, filler))?;
                made.set_len(self.size)?;
            }
            self.volume = created(&volume_path(&self.path, index))?;
        } else {
            self.volume = reopened(&volume_path(&self.path, index))?;
        }
        self.index = index;
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        if self.asked != self.position {
            self.position = self.asked;
        }
        let mut rest = data;
        while !rest.is_empty() {
            let index = self.position / self.size;
            self.open(index)?;
            let at = self.position - index * self.size;
            self.volume.seek(SeekFrom::Start(at))?;
            let room = usize::try_from(self.size - at).unwrap_or(usize::MAX);
            let (piece, after) = rest.split_at(rest.len().min(room));
            self.volume.write_all(piece)?;
            self.position += piece.len() as u64;
            self.length = self.length.max(self.position);
            rest = after;
        }
        self.asked = self.position;
        Ok(())
    }

    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let invalid = || io::Error::from(io::ErrorKind::InvalidInput);
        self.asked = match to {
            SeekFrom::Start(at) => at,
            SeekFrom::Current(by) => self.asked.checked_add_signed(by).ok_or_else(invalid)?,
            SeekFrom::End(by) => self.length.checked_add_signed(by).ok_or_else(invalid)?,
        };
        Ok(self.asked)
    }

    fn set_size(&mut self, size: u64) -> io::Result<()> {
        if size == self.length {
            return Ok(());
        }
        let last = self.last();
        let index = size / self.size;
        self.open(index)?;
        self.volume.set_len(size - index * self.size)?;
        for extra in index + 1..=last {
            std::fs::remove_file(volume_path(&self.path, extra))?;
        }
        self.length = size;
        Ok(())
    }

    /// Every volume written deleted (arclite's `clean_files`), the one open too (made, its writing
    /// failed before the length grew).
    pub fn clean(self) {
        let last = self.last().max(self.index);
        drop(self.volume);
        for index in 0..=last {
            let _ = std::fs::remove_file(volume_path(&self.path, index));
        }
    }
}

#[cfg(test)]
#[path = "update_volumes_tests.rs"]
mod tests;
