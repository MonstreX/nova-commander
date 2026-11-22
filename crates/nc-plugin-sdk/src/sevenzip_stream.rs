//! The streams 7-Zip's library reads an archive through (`IInStream`): a file of the disk (arclite's
//! `ArchiveOpenStream`), a stream seen from an offset on (its `ArchiveSubStream`: an archive after an SFX
//! stub or another signature's place), or one the library gives (an item's: `IInArchiveGetStream`).

use std::ffi::c_void;
use std::fs::File;
use std::io::{self, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};

use windows_sys::core::{GUID, HRESULT};

use super::com::{
    Com, E_FAIL, E_NOINTERFACE, IID_IN_STREAM, IID_SEQUENTIAL_IN_STREAM, IID_UNKNOWN,
    InStreamTable, Raw, S_OK, Unknown,
};

/// `STREAM_SEEK_SET`, `_CUR`, `_END`.
const SEEK_SET: u32 = 0;
const SEEK_CUR: u32 = 1;
const SEEK_END: u32 = 2;
/// `HRESULT_FROM_WIN32(ERROR_NEGATIVE_SEEK)`, 7-Zip's answer to a place before the start.
const NEGATIVE_SEEK: HRESULT = 0x8007_0083_u32.cast_signed();

/// A stream the library reads: one of the program's or one the library gave.
#[derive(Debug, Clone)]
pub struct InStream(pub(super) Com);

impl InStream {
    /// The file at `path`, opened for reading (others may read and write it meanwhile, as arclite's).
    pub fn file(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        Ok(Self::of(Backing::File {
            file,
            size,
            position: 0,
        }))
    }

    /// `file` read from its start (arclite's `FileReadStream` of a file going into an archive).
    pub fn of_file(file: File) -> io::Result<Self> {
        let size = file.metadata()?.len();
        Ok(Self::of(Backing::File {
            file,
            size,
            position: 0,
        }))
    }

    /// This stream from `offset` on: its start is there (arclite's `ArchiveSubStream`).
    pub fn from_offset(&self, offset: u64) -> io::Result<Self> {
        self.seek(SeekFrom::Start(offset))?;
        Ok(Self::of(Backing::Sub {
            base: self.clone(),
            offset,
        }))
    }

    fn of(backing: Backing) -> Self {
        let object = Box::new(StreamObject {
            table: &TABLE,
            refs: AtomicU32::new(1),
            backing: Mutex::new(backing),
        });
        Self(Com(Box::into_raw(object).cast()))
    }

    /// Up to `buffer.len()` bytes from where it stands (fewer only at its end).
    pub fn read(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut done = 0;
        while done < buffer.len() {
            let rest = &mut buffer[done..];
            let ask = u32::try_from(rest.len()).unwrap_or(u32::MAX);
            let mut read = 0u32;
            // SAFETY: an `IInStream` (or `ISequentialInStream`): `ask` bytes of `rest` may be written.
            let result = unsafe {
                (self.0.table::<InStreamTable>().read)(
                    self.0.raw(),
                    rest.as_mut_ptr().cast(),
                    ask,
                    &raw mut read,
                )
            };
            super::com::check(result)?;
            if read == 0 {
                break;
            }
            done += read as usize;
        }
        Ok(done)
    }

    /// The stream's place moved; where it stands now.
    pub fn seek(&self, to: SeekFrom) -> io::Result<u64> {
        let (offset, origin) = match to {
            SeekFrom::Start(at) => (at.cast_signed(), SEEK_SET),
            SeekFrom::Current(by) => (by, SEEK_CUR),
            SeekFrom::End(by) => (by, SEEK_END),
        };
        let mut position = 0u64;
        // SAFETY: an `IInStream`; the place's answer lives through the call.
        let result = unsafe {
            (self.0.table::<InStreamTable>().seek)(self.0.raw(), offset, origin, &raw mut position)
        };
        super::com::check(result)?;
        Ok(position)
    }
}

/// What a stream of the program's reads.
#[derive(Debug)]
enum Backing {
    File {
        file: File,
        size: u64,
        position: u64,
    },
    Sub {
        base: InStream,
        offset: u64,
    },
}

impl Backing {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::File {
                file,
                size,
                position,
            } => {
                if *position >= *size {
                    return Ok(0);
                }
                let read = crate::system::read_at(file, *position, buffer)?;
                *position += read as u64;
                Ok(read)
            }
            Self::Sub { base, .. } => base.read(buffer),
        }
    }

    /// arclite's `Seek`: a sub-stream's place from its start is its base's less its offset.
    fn seek(&mut self, offset: i64, origin: u32) -> Result<u64, HRESULT> {
        match self {
            Self::File { size, position, .. } => {
                let from = match origin {
                    SEEK_SET => 0,
                    SEEK_CUR => *position,
                    SEEK_END => *size,
                    _ => return Err(E_FAIL),
                };
                let target = from.checked_add_signed(offset).ok_or(NEGATIVE_SEEK)?;
                *position = target;
                Ok(target)
            }
            Self::Sub {
                base,
                offset: start,
            } => {
                let to = match origin {
                    SEEK_SET => {
                        SeekFrom::Start(start.checked_add_signed(offset).ok_or(NEGATIVE_SEEK)?)
                    }
                    SEEK_CUR => SeekFrom::Current(offset),
                    SEEK_END => SeekFrom::End(offset),
                    _ => return Err(E_FAIL),
                };
                let at = base
                    .seek(to)
                    .map_err(|error| error.raw_os_error().unwrap_or(E_FAIL))?;
                Ok(at.wrapping_sub(*start))
            }
        }
    }
}

/// A stream of the program's as the library sees it.
#[repr(C)]
struct StreamObject {
    table: *const InStreamTable,
    refs: AtomicU32,
    backing: Mutex<Backing>,
}

static TABLE: InStreamTable = InStreamTable {
    unknown: Unknown {
        query,
        add_ref,
        release,
    },
    read,
    seek,
};

/// The object behind `this`.
///
/// # Safety
/// `this` must be a live `StreamObject` of this module's.
unsafe fn object<'a>(this: Raw) -> &'a StreamObject {
    // SAFETY: the caller's: `this` is a `StreamObject` while it has references.
    unsafe { &*this.cast::<StreamObject>() }
}

unsafe extern "system" fn query(this: Raw, iid: *const GUID, out: *mut Raw) -> HRESULT {
    if out.is_null() || iid.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library's id, read.
    let iid = unsafe { &*iid };
    let known = [IID_UNKNOWN, IID_SEQUENTIAL_IN_STREAM, IID_IN_STREAM];
    if known.iter().any(|id| super::com::same(id, iid)) {
        // SAFETY: a live object of this module's; the answer is its own pointer with one more reference.
        unsafe {
            add_ref(this);
            *out = this;
        }
        S_OK
    } else {
        // SAFETY: the library's place for the answer.
        unsafe { *out = std::ptr::null_mut() };
        E_NOINTERFACE
    }
}

unsafe extern "system" fn add_ref(this: Raw) -> u32 {
    // SAFETY: the library holds a reference, so the object lives.
    unsafe { object(this) }.refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn release(this: Raw) -> u32 {
    // SAFETY: the library holds a reference, so the object lives until it is let go here.
    let left = unsafe { object(this) }.refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        // SAFETY: the last reference: the box made in `InStream::of` is freed once.
        drop(unsafe { Box::from_raw(this.cast::<StreamObject>()) });
    }
    left
}

unsafe extern "system" fn read(this: Raw, data: *mut c_void, size: u32, done: *mut u32) -> HRESULT {
    if !done.is_null() {
        // SAFETY: the library's place for the count.
        unsafe { *done = 0 };
    }
    if size == 0 {
        return S_OK;
    }
    if data.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library's buffer of `size` bytes.
    let buffer = unsafe { std::slice::from_raw_parts_mut(data.cast::<u8>(), size as usize) };
    // SAFETY: a live object of this module's.
    let object = unsafe { object(this) };
    let mut backing = object
        .backing
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    match backing.read(buffer) {
        Ok(read) => {
            if !done.is_null() {
                // SAFETY: as above.
                unsafe { *done = u32::try_from(read).unwrap_or(size) };
            }
            S_OK
        }
        Err(error) => error.raw_os_error().map_or(E_FAIL, hresult_of),
    }
}

unsafe extern "system" fn seek(this: Raw, offset: i64, origin: u32, at: *mut u64) -> HRESULT {
    // SAFETY: a live object of this module's.
    let object = unsafe { object(this) };
    let mut backing = object
        .backing
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    match backing.seek(offset, origin) {
        Ok(position) => {
            if !at.is_null() {
                // SAFETY: the library's place for the answer.
                unsafe { *at = position };
            }
            S_OK
        }
        Err(result) => result,
    }
}

/// A system error as an `HRESULT` (`HRESULT_FROM_WIN32`); one that is already an `HRESULT` as it is.
fn hresult_of(code: i32) -> HRESULT {
    if code <= 0 {
        code
    } else {
        ((code.cast_unsigned() & 0xFFFF) | 0x8007_0000).cast_signed()
    }
}

#[cfg(test)]
#[path = "sevenzip_stream_tests.rs"]
mod tests;
