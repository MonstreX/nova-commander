//! "Verify" (owner's, from Total Commander and TeraCopy): each copy is read back and compared with its
//! source. On a disk the copy is read past the file cache, so what is compared is what reached the disk.
//! A copy that differs is an error of the copy: it goes away and the user is asked Retry / Skip / Cancel.

use std::io::{self, Read};

use super::engine::Run;
use super::sides;
use super::{Progress, VfsPath};
use crate::platform;
use crate::vfs::provider_for;

const CHUNK: usize = 1024 * 1024;
/// A read past the cache needs a buffer aligned to the sector, and 4096 is a multiple of the sizes in use.
const ALIGNMENT: usize = 4096;
const DIFFERS: &str = "the copy differs from the source";

/// What is read back: a read past the cache that came up short was the last one (reading on from an
/// offset that is not a whole number of sectors is an error there).
struct Copy {
    reader: Box<dyn Read + Send>,
    short_read_ends: bool,
}

/// An aligned part of `storage` for a read past the cache.
fn aligned(storage: &mut [u8]) -> &mut [u8] {
    let start = storage.as_ptr().align_offset(ALIGNMENT);
    &mut storage[start..start + CHUNK]
}

fn read_retrying(reader: &mut dyn Read, buffer: &mut [u8]) -> io::Result<usize> {
    loop {
        match reader.read(buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}

/// Fills `buffer` unless the reader ends first; the bytes read.
fn fill(reader: &mut dyn Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match read_retrying(reader, &mut buffer[filled..])? {
            0 => break,
            read => filled += read,
        }
    }
    Ok(filled)
}

/// Whether the two hold the same bytes; `report(done)` false stops it.
fn same(
    source: &mut dyn Read,
    copy: &mut Copy,
    report: &mut dyn FnMut(u64) -> bool,
) -> io::Result<bool> {
    let mut storage = vec![0u8; CHUNK + ALIGNMENT];
    let written = aligned(&mut storage);
    let mut original = vec![0u8; CHUNK];
    let mut done = 0u64;
    loop {
        let read = read_retrying(copy.reader.as_mut(), written)?;
        if fill(source, &mut original[..read])? != read || written[..read] != original[..read] {
            return Ok(false);
        }
        if read == 0 || (copy.short_read_ends && read < CHUNK) {
            // The copy has ended: so must the source.
            let mut one = [0u8; 1];
            return Ok(fill(source, &mut one)? == 0);
        }
        done += read as u64;
        if !report(done) {
            return Err(io::Error::other("copy cancelled"));
        }
    }
}

fn open_copy(copy: &VfsPath, past_the_cache: bool) -> io::Result<Copy> {
    if past_the_cache
        && let Some(on_disk) = sides::disk(copy)
        && let Ok(file) = platform::files::open_past_cache(&on_disk)
    {
        return Ok(Copy {
            reader: Box::new(file),
            short_read_ends: true,
        });
    }
    Ok(Copy {
        reader: provider_for(copy)?.open_read(copy)?,
        short_read_ends: false,
    })
}

impl Run<'_> {
    fn set_verified(&mut self, verified: Option<u64>) {
        (self.host).progress(&mut |progress: &mut Progress| progress.verified = verified);
    }

    /// Compares and says what came of it; `skip` bytes of the copy were there before it.
    fn compare(
        &mut self,
        source: &VfsPath,
        copy: &VfsPath,
        skip: u64,
        past_the_cache: bool,
    ) -> io::Result<bool> {
        let mut original = provider_for(source)?.open_read(source)?;
        let mut written = open_copy(copy, past_the_cache)?;
        io::copy(&mut written.reader.by_ref().take(skip), &mut io::sink())?;
        let Run { host, stopped, .. } = self;
        same(original.as_mut(), &mut written, &mut |done| {
            host.progress(&mut |progress| progress.verified = Some(done));
            *stopped = host.cancelled();
            !*stopped
        })
    }

    /// The copy reads as its source does, from `skip` on; an error otherwise.
    pub(super) fn verify(&mut self, source: &VfsPath, copy: &VfsPath, skip: u64) -> io::Result<()> {
        self.set_verified(Some(0));
        // Skipping cannot start at an offset that is not a whole number of sectors.
        let mut compared = self.compare(source, copy, skip, skip == 0);
        if skip == 0
            && compared
                .as_ref()
                .is_err_and(|error| error.kind() == io::ErrorKind::InvalidInput)
        {
            // This file system takes no read past its cache.
            compared = self.compare(source, copy, skip, false);
        }
        self.set_verified(None);
        match compared {
            Ok(true) => Ok(()),
            Ok(false) => Err(io::Error::other(DIFFERS)),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A reader that hands out one byte at a time.
    struct Trickle(Cursor<Vec<u8>>);

    impl Read for Trickle {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let end = buffer.len().min(1);
            self.0.read(&mut buffer[..end])
        }
    }

    fn compare(source: Vec<u8>, copy: Vec<u8>) -> bool {
        let mut copy = Copy {
            reader: Box::new(Trickle(Cursor::new(copy))),
            short_read_ends: false,
        };
        same(&mut Trickle(Cursor::new(source)), &mut copy, &mut |_| true).unwrap()
    }

    #[test]
    fn readers_that_hand_out_little_compare_all_the_same() {
        assert!(compare(vec![1, 2, 3], vec![1, 2, 3]));
        assert!(compare(Vec::new(), Vec::new()));
    }

    #[test]
    fn a_different_byte_is_found_wherever_it_is() {
        assert!(!compare(vec![1, 2, 3], vec![1, 2, 4]));
        assert!(!compare(vec![9, 2, 3], vec![1, 2, 3]));
    }

    #[test]
    fn a_copy_shorter_or_longer_differs() {
        assert!(!compare(vec![1, 2, 3], vec![1, 2]));
        assert!(!compare(vec![1, 2], vec![1, 2, 3]));
        assert!(!compare(vec![1], Vec::new()));
        assert!(!compare(Vec::new(), vec![1]));
    }

    #[test]
    fn the_buffer_for_a_read_past_the_cache_is_aligned() {
        let mut storage = vec![0u8; CHUNK + ALIGNMENT];
        let buffer = aligned(&mut storage);
        assert_eq!(buffer.len(), CHUNK);
        assert_eq!(buffer.as_ptr().align_offset(ALIGNMENT), 0);
    }

    #[test]
    fn a_stop_is_an_error_and_not_a_verdict() {
        let bytes = vec![7u8; CHUNK * 2];
        let mut copy = Copy {
            reader: Box::new(Cursor::new(bytes.clone())),
            short_read_ends: false,
        };
        let stopped = same(&mut Cursor::new(bytes), &mut copy, &mut |_| false);
        assert!(stopped.is_err());
    }
}
