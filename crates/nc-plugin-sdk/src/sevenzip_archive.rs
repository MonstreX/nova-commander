//! The library as the plugin holds it, and an archive one of its formats opened (arclite's `ArcAPI` and
//! `Archive`: `create_in_archive`, `Archive::open` with its opener, `get_skip_header`, the item and
//! archive properties, `get_stream`, `get_main_file`).

use std::io;
use std::path::Path;
use std::rc::Rc;

use windows_sys::core::{BSTR, GUID};

use super::com::{
    AllowTailTable, Com, GetStreamTable, IID_ALLOW_TAIL, IID_GET_STREAM, IID_IN_ARCHIVE,
    IID_IN_STREAM, INVALID_DATA, InArchiveTable, PropVariant, Raw, S_FALSE, S_OK, check, take_bstr,
};
use super::load::{self, Libraries};
use super::open::Opener;
use super::stream::InStream;
use super::{Format, IsArc, Module, Opening, Prop, PropInfo, flags, prop};

/// 7-Zip's library, found once and shared by every archive opened through it (kept loaded while one is).
#[derive(Clone)]
pub struct Library(pub(super) Rc<Libraries>);

impl Library {
    /// The library found as arclite finds it, `plugin_folder` first; no formats when none is found.
    pub fn find(plugin_folder: &Path) -> Self {
        Self(Rc::new(load::find(plugin_folder)))
    }

    /// The folder the library was found in (its `Formats`, `Codecs` and SFX modules beside it).
    pub fn folder(&self) -> Option<std::path::PathBuf> {
        self.0.folder.clone()
    }

    /// The SFX modules found with the library (arclite's `ArcAPI::sfx`), in the order found.
    pub fn sfx_modules(&self) -> Vec<std::path::PathBuf> {
        self.0.sfx.clone()
    }

    /// The configuration's code pages for the names the library reads and writes (0: the system's),
    /// arclite's `Patch7zCP::SetCP`.
    pub fn set_code_pages(&self, oem: u32, ansi: u32) {
        self.0.set_code_pages(oem, ansi);
    }

    /// The libraries loaded ("7z library info"); none when 7-Zip was not found.
    pub fn modules(&self) -> Vec<Module> {
        load::modules(&self.0)
    }

    /// The formats, in their class ids' order (arclite's `ArcFormats`).
    pub fn formats(&self) -> Vec<Format> {
        (self.0.formats.iter())
            .map(|found| found.format.clone())
            .collect()
    }

    /// The format `format`'s quick look at bytes a signature of it starts; yes when it has none.
    pub fn is_arc(&self, format: usize, data: &[u8]) -> IsArc {
        let Some(is_arc) = self.0.formats.get(format).and_then(|found| found.is_arc) else {
            return IsArc::Yes;
        };
        // SAFETY: the library's function, given `data` and its length.
        match unsafe { is_arc(data.as_ptr(), data.len()) } {
            0 => IsArc::No,
            2 => IsArc::NeedMore,
            _ => IsArc::Yes,
        }
    }

    /// A new `IInArchive` of the format `format`.
    fn create(&self, format: usize) -> io::Result<Com> {
        self.create_object(format, &IID_IN_ARCHIVE)
    }

    /// A new object of the format `format`, its interface `iid` (arclite's `create_in_archive`,
    /// `create_out_archive`).
    pub(super) fn create_object(&self, format: usize, iid: &GUID) -> io::Result<Com> {
        let found =
            (self.0.formats.get(format)).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        let class_id = guid(&found.format.class_id)?;
        let create_object = self.0.loaded[found.format.library].create_object;
        let mut raw: Raw = std::ptr::null_mut();
        // SAFETY: the library's `CreateObject`, given a class id and an interface id it knows.
        check(unsafe { create_object(&raw const class_id, iid, &raw mut raw) })?;
        Com::take(raw).ok_or_else(|| io::Error::from(io::ErrorKind::Unsupported))
    }

    /// `stream` opened as the format `format` from where it stands (arclite's `Archive::open`): `None` when
    /// it is not one; `path` the archive's file when it is on the disk (its next volumes beside it);
    /// `allow_tail` lets an archive with data after it open (`IArchiveAllowTail`, for a `kPreArc` format).
    /// A stop or a refused password is `ErrorKind::Interrupted`.
    pub fn open(
        &self,
        format: usize,
        stream: &InStream,
        path: Option<&Path>,
        allow_tail: bool,
        asker: &mut dyn Opening,
    ) -> io::Result<Option<Archive>> {
        let archive = self.create(format)?;
        let pre_arc = (self.0.formats.get(format))
            .is_some_and(|found| found.format.flags & flags::PRE_ARC != 0);
        if allow_tail && pre_arc {
            allow_tail_on(&archive);
        }
        let opener = Opener::new(path);
        let start = 0u64;
        // SAFETY: an `IInArchive`; the stream and the callback are live objects through the call.
        let result = opener.with(asker, || unsafe {
            (archive.table::<InArchiveTable>().open)(
                archive.raw(),
                stream.0.raw(),
                &raw const start,
                opener.raw(),
            )
        });
        match result {
            S_OK => Ok(Some(Archive {
                archive,
                volumes: opener.volumes(),
                _library: self.clone(),
            })),
            S_FALSE | INVALID_DATA => Ok(None),
            super::com::E_ABORT => Err(io::Error::from(io::ErrorKind::Interrupted)),
            other => Err(io::Error::from_raw_os_error(other)),
        }
    }

    /// arclite's `get_skip_header`: `stream` opened as `format` searching `max_start` bytes for its start,
    /// no callback; the size of what it found, when it opens.
    pub fn physical_size(&self, format: usize, stream: &InStream, max_start: u64) -> Option<u64> {
        let archive = self.create(format).ok()?;
        if (self.0.formats.get(format))
            .is_some_and(|found| found.format.flags & flags::PRE_ARC != 0)
        {
            allow_tail_on(&archive);
        }
        stream.seek(io::SeekFrom::Start(0)).ok()?;
        // SAFETY: an `IInArchive`; the stream lives through the call; no callback.
        let result = unsafe {
            (archive.table::<InArchiveTable>().open)(
                archive.raw(),
                stream.0.raw(),
                &raw const max_start,
                std::ptr::null_mut(),
            )
        };
        let opened = Archive {
            archive,
            volumes: Vec::new(),
            _library: self.clone(),
        };
        (result == S_OK).then(|| {
            opened
                .archive_property(prop::PHYSICAL_SIZE)
                .number()
                .unwrap_or(0)
        })
    }
}

/// A class id's bytes as a `GUID`.
fn guid(bytes: &[u8]) -> io::Result<GUID> {
    let bytes: [u8; 16] = (bytes.get(..16))
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
    Ok(GUID {
        data1: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        data2: u16::from_le_bytes([bytes[4], bytes[5]]),
        data3: u16::from_le_bytes([bytes[6], bytes[7]]),
        data4: [
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ],
    })
}

/// `IArchiveAllowTail::AllowTail(TRUE)` when the archive has it.
fn allow_tail_on(archive: &Com) {
    if let Some(tail) = archive.query(&IID_ALLOW_TAIL) {
        // SAFETY: an `IArchiveAllowTail` the archive gave.
        unsafe { (tail.table::<AllowTailTable>().allow_tail)(tail.raw(), 1) };
    }
}

/// An archive open: its items and properties read through the library.
pub struct Archive {
    pub(super) archive: Com,
    volumes: Vec<String>,
    /// The library stays loaded while the archive is open.
    _library: Library,
}

impl std::fmt::Debug for Archive {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Archive")
            .field("volumes", &self.volumes)
            .finish_non_exhaustive()
    }
}

impl Archive {
    fn table(&self) -> &InArchiveTable {
        // SAFETY: the object is an `IInArchive` (made by `Library::create`).
        unsafe { self.archive.table::<InArchiveTable>() }
    }

    /// Its number of items (0 when the library fails to tell).
    pub fn count(&self) -> u32 {
        let mut count = 0u32;
        // SAFETY: an `IInArchive`; the count's place lives through the call.
        let result = unsafe { (self.table().number_of_items)(self.archive.raw(), &raw mut count) };
        if result == S_OK { count } else { 0 }
    }

    /// The property `id` of item `index` (empty when it has none).
    pub fn property(&self, index: u32, id: u32) -> Prop {
        let mut value = PropVariant::empty();
        // SAFETY: an `IInArchive`; the variant lives through the call and is freed after.
        let result =
            unsafe { (self.table().property)(self.archive.raw(), index, id, &raw mut value) };
        if result == S_OK {
            value.read(false)
        } else {
            Prop::Empty
        }
    }

    /// The archive's property `id` (empty when it has none).
    pub fn archive_property(&self, id: u32) -> Prop {
        let mut value = PropVariant::empty();
        // SAFETY: as above.
        let result =
            unsafe { (self.table().archive_property)(self.archive.raw(), id, &raw mut value) };
        if result == S_OK {
            value.read(false)
        } else {
            Prop::Empty
        }
    }

    /// The item properties the format has, in its order.
    pub fn properties(&self) -> Vec<PropInfo> {
        let table = self.table();
        self.infos(table.number_of_properties, table.property_info)
    }

    /// The archive properties the format has, in its order.
    pub fn archive_properties(&self) -> Vec<PropInfo> {
        let table = self.table();
        self.infos(
            table.number_of_archive_properties,
            table.archive_property_info,
        )
    }

    fn infos(
        &self,
        number: unsafe extern "system" fn(Raw, *mut u32) -> windows_sys::core::HRESULT,
        info: unsafe extern "system" fn(
            Raw,
            u32,
            *mut BSTR,
            *mut u32,
            *mut u16,
        ) -> windows_sys::core::HRESULT,
    ) -> Vec<PropInfo> {
        let mut count = 0u32;
        // SAFETY: the archive's own functions; the places live through each call; a name is freed.
        unsafe {
            if number(self.archive.raw(), &raw mut count) != S_OK {
                return Vec::new();
            }
            (0..count)
                .filter_map(|index| {
                    let (mut name, mut id, mut var_type): (BSTR, u32, u16) =
                        (std::ptr::null_mut(), 0, 0);
                    let result = info(
                        self.archive.raw(),
                        index,
                        &raw mut name,
                        &raw mut id,
                        &raw mut var_type,
                    );
                    let name = take_bstr(name);
                    (result == S_OK).then_some(PropInfo { name, id, var_type })
                })
                .collect()
        }
    }

    /// The item `index`'s data as a stream an archive inside it is opened from (arclite's
    /// `get_stream`); none when the format cannot give one that seeks.
    pub fn stream(&self, index: u32) -> Option<InStream> {
        if index >= self.count() {
            return None;
        }
        let getter = self.archive.query(&IID_GET_STREAM)?;
        let mut raw: Raw = std::ptr::null_mut();
        // SAFETY: an `IInArchiveGetStream` the archive gave; the stream's place lives through the call.
        let result =
            unsafe { (getter.table::<GetStreamTable>().stream)(getter.raw(), index, &raw mut raw) };
        let sequential = Com::take(raw)?;
        (result == S_OK)
            .then(|| sequential.query(&IID_IN_STREAM))
            .flatten()
            .map(InStream)
    }

    /// The item an archive of one file holds (arclite's `get_main_file`: `kpidMainSubfile`).
    pub fn main_file(&self) -> Option<u32> {
        match self.archive_property(prop::MAIN_SUBFILE) {
            Prop::U32(index) => Some(index),
            _ => None,
        }
    }

    /// The items `indices` extracted (`test`: tested) through the library in one pass (arclite's
    /// `Archive::extract`, `Archive::test`), `sink` answering for each; a stop is `ErrorKind::Interrupted`.
    pub fn extract(
        &self,
        indices: &[u32],
        test: bool,
        sink: &mut dyn super::Extracting,
    ) -> io::Result<()> {
        let extractor = super::extract::Extractor::new();
        let count = u32::try_from(indices.len()).unwrap_or(u32::MAX);
        let extract = self.table().extract;
        let pointers = Sent((self.archive.raw(), extractor.raw()));
        let indices = Sent(indices.as_ptr());
        let result = extractor.with(sink, move || {
            let (Sent((archive, callback)), Sent(indices)) = (pointers, indices);
            // SAFETY: an `IInArchive` (7-Zip's objects have no thread of their own: arclite calls them
            // from FAR's threads), its indices and the callback, all living through `with`, which waits
            // for this call; the caller's thread meanwhile only serves the callbacks.
            unsafe { extract(archive, indices, count, i32::from(test), callback) }
        });
        match result {
            super::com::E_ABORT => Err(io::Error::from(io::ErrorKind::Interrupted)),
            other => check(other),
        }
    }

    /// The names of the volumes opened after the first.
    pub fn volumes(&self) -> &[String] {
        &self.volumes
    }
}

/// Pointers taken to the thread the library's call runs on ([`super::marshal`]).
pub(super) struct Sent<T>(pub(super) T);

// SAFETY: what is sent is the library's objects and the caller's data, used by the one call the
// caller's thread waits for (`Extractor::with`).
unsafe impl<T> Send for Sent<T> {}

impl Drop for Archive {
    fn drop(&mut self) {
        // SAFETY: an `IInArchive`; closed once before its reference goes.
        unsafe { (self.table().close)(self.archive.raw()) };
    }
}

#[cfg(test)]
#[path = "sevenzip_archive_tests.rs"]
mod tests;
