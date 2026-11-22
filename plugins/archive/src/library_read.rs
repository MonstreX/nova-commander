//! A file of an archive 7-Zip's library reads, read from an offset (the quick view's and Find file's
//! `read`; FAR extracts the file once and reads that). The library reads an item from its start only,
//! so what it gave is kept (in memory up to 16 MB, then in a temporary file that goes with it) and a
//! read past it extracts again to at least twice as far, the call stopped there: reading a file through
//! decodes it a few times its size at most, whatever its size and whether the archive tells it (bzip2
//! tells none, gzip's is modulo 4 GB). The password is the archive's as it was given or asked (none
//! asked here).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

use nc_plugin_sdk::sevenzip::{Archive, AskMode, Extracting, Outcome};

/// The most kept in memory; more goes into a temporary file.
pub const KEPT: u64 = 16 << 20;

/// The least a first extraction goes to.
const FIRST: u64 = 1 << 20;

/// Where what the library gave is kept.
enum Store {
    Memory(Vec<u8>),
    File(File),
}

impl Store {
    /// What is kept cut back to `length` bytes (what an extraction that failed added goes).
    fn truncate(&mut self, length: u64) -> io::Result<()> {
        match self {
            Self::Memory(kept) => {
                kept.truncate(usize::try_from(length).unwrap_or(usize::MAX));
                Ok(())
            }
            Self::File(file) => {
                file.set_len(length)?;
                file.seek(SeekFrom::End(0)).map(drop)
            }
        }
    }

    fn append(&mut self, data: &[u8]) -> io::Result<()> {
        if let Self::Memory(kept) = self
            && (kept.len() + data.len()) as u64 > KEPT
        {
            let mut file = temp_file()?;
            file.write_all(kept)?;
            *self = Self::File(file);
        }
        match self {
            Self::Memory(kept) => {
                kept.extend_from_slice(data);
                Ok(())
            }
            Self::File(file) => file.write_all(data),
        }
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Memory(kept) => {
                let start = usize::try_from(offset)
                    .unwrap_or(usize::MAX)
                    .min(kept.len());
                let piece = &kept[start..(start + buffer.len()).min(kept.len())];
                buffer[..piece.len()].copy_from_slice(piece);
                Ok(piece.len())
            }
            Self::File(file) => {
                let end = file.seek(SeekFrom::End(0))?;
                file.seek(SeekFrom::Start(offset.min(end)))?;
                let mut read = 0;
                while read < buffer.len() {
                    match file.read(&mut buffer[read..])? {
                        0 => break,
                        count => read += count,
                    }
                }
                file.seek(SeekFrom::End(0))?;
                Ok(read)
            }
        }
    }
}

/// A new temporary file, gone when it is closed (the system's temporary folder).
fn temp_file() -> io::Result<File> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!(
        "vc-archive-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let path = std::env::temp_dir().join(name);
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // `FILE_FLAG_DELETE_ON_CLOSE`: the system deletes it however the program ends.
        options.custom_flags(0x0400_0000);
    }
    let file = options.open(&path)?;
    #[cfg(not(windows))]
    std::fs::remove_file(&path)?;
    Ok(file)
}

/// One item's data as the library reads it, kept from where the store ends until `target`.
struct Window<'a> {
    wanted: u32,
    at: u64,
    from: u64,
    target: u64,
    store: &'a mut Store,
    outcome: Option<Outcome>,
    password: Option<String>,
    failed: Option<io::Error>,
}

impl Extracting for Window<'_> {
    fn item(&mut self, index: u32, mode: AskMode) -> Option<bool> {
        Some(index == self.wanted && mode == AskMode::Extract)
    }

    fn write(&mut self, data: &[u8]) -> bool {
        let start = self.at;
        self.at += data.len() as u64;
        if self.at > self.from {
            let skip = usize::try_from(self.from.saturating_sub(start)).unwrap_or(usize::MAX);
            let piece = data.get(skip..).unwrap_or_default();
            if let Err(error) = self.store.append(piece) {
                self.failed = Some(error);
                return false;
            }
        }
        self.at < self.target
    }

    fn done(&mut self, outcome: Outcome) -> bool {
        self.outcome = Some(outcome);
        true
    }

    fn password(&mut self) -> Option<String> {
        self.password
            .clone()
            .filter(|password| !password.is_empty())
    }
}

/// What of one item the library gave so far.
pub struct Spool {
    item: u32,
    /// The bytes kept, from the item's start.
    have: u64,
    /// Whether the library gave all of it.
    whole: bool,
    store: Store,
}

impl Spool {
    pub fn new(item: u32) -> Self {
        Self {
            item,
            have: 0,
            whole: false,
            store: Store::Memory(Vec::new()),
        }
    }

    /// The archive's item it keeps.
    pub fn item(&self) -> u32 {
        self.item
    }

    /// Up to `buffer.len()` bytes from `offset` (`size`: what the archive tells of the item's size, a
    /// hint only), extracted further first when they are not kept yet.
    pub fn read(
        &mut self,
        archive: &Archive,
        (offset, buffer): (u64, &mut [u8]),
        (size, password): (u64, Option<String>),
    ) -> io::Result<usize> {
        let end = offset.saturating_add(buffer.len() as u64);
        if !self.whole && self.have < end {
            let target = (end.max(self.have.saturating_mul(2)).max(FIRST)).max(size.min(KEPT));
            self.extend(archive, target, password)?;
        }
        if offset >= self.have {
            return Ok(0);
        }
        self.store.read_at(offset, buffer)
    }

    /// The item extracted again from its start, kept from where it was kept to until `target`; a
    /// failure keeps what was kept before.
    fn extend(
        &mut self,
        archive: &Archive,
        target: u64,
        password: Option<String>,
    ) -> io::Result<()> {
        let extended = self.extend_from(archive, target, password);
        if extended.is_err() {
            self.store.truncate(self.have)?;
        }
        extended
    }

    fn extend_from(
        &mut self,
        archive: &Archive,
        target: u64,
        password: Option<String>,
    ) -> io::Result<()> {
        let mut window = Window {
            wanted: self.item,
            at: 0,
            from: self.have,
            target,
            store: &mut self.store,
            outcome: None,
            password,
            failed: None,
        };
        let result = archive.extract(&[self.item], false, &mut window);
        let reached = window.at >= target;
        let given = window.at.max(self.have);
        if let Some(error) = window.failed {
            return Err(error);
        }
        let outcome = window.outcome;
        match result {
            Err(error) if error.kind() == io::ErrorKind::Interrupted && reached => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "The file is encrypted: a password is needed",
                ));
            }
            Err(error) => return Err(error),
            Ok(()) => {}
        }
        let words =
            outcome.and_then(|outcome| crate::extract::library::outcome_words(outcome, false));
        match words {
            Some(words) if !reached => Err(io::Error::new(io::ErrorKind::InvalidData, words)),
            _ => {
                self.have = given;
                self.whole = !reached;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "library_read_tests.rs"]
mod tests;
