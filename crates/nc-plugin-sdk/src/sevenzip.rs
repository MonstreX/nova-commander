//! 7-Zip's library (`7z.dll`) as FAR's arclite uses it (ADR-0022): found as arclite finds it, its formats,
//! an archive opened by one of them and read through its interfaces (`IInArchive`). The plugin sees safe
//! types; the library's interfaces, the streams it reads and the callbacks it calls are this module's.

use std::path::PathBuf;

/// A value 7-Zip gives or is given (`PROPVARIANT`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prop {
    Empty,
    Bool(bool),
    U32(u32),
    U64(u64),
    I64(i64),
    /// A `FILETIME`: 100 ns units from 1601.
    Time(u64),
    Text(String),
    /// Binary data in a `BSTR` (a class id, a signature).
    Bytes(Vec<u8>),
    /// A type this module does not read.
    Other(u16),
}

impl Prop {
    /// A number of any width (arclite's `is_uint` / `get_uint`).
    pub fn number(&self) -> Option<u64> {
        match self {
            Self::U32(value) => Some(u64::from(*value)),
            Self::U64(value) => Some(*value),
            _ => None,
        }
    }

    /// A text (arclite's `is_str` / `get_str`).
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// 7-Zip's item and archive property ids (`PropID.h`).
pub mod prop {
    pub const MAIN_SUBFILE: u32 = 1;
    pub const PATH: u32 = 3;
    pub const NAME: u32 = 4;
    pub const EXTENSION: u32 = 5;
    pub const IS_DIR: u32 = 6;
    pub const SIZE: u32 = 7;
    pub const PACKED_SIZE: u32 = 8;
    pub const ATTRIBUTES: u32 = 9;
    pub const CREATED: u32 = 10;
    pub const ACCESSED: u32 = 11;
    pub const MODIFIED: u32 = 12;
    pub const SOLID: u32 = 13;
    pub const COMMENTED: u32 = 14;
    pub const ENCRYPTED: u32 = 15;
    pub const CRC: u32 = 19;
    pub const IS_ANTI: u32 = 21;
    pub const METHOD: u32 = 22;
    pub const COMMENT: u32 = 28;
    pub const OFFSET: u32 = 36;
    pub const PHYSICAL_SIZE: u32 = 44;
    pub const POSIX_ATTRIBUTES: u32 = 53;
    pub const ERROR: u32 = 55;
    pub const IS_ALT_STREAM: u32 = 63;
    pub const ERROR_FLAGS: u32 = 71;
    pub const WARNING_FLAGS: u32 = 72;
    pub const WARNING: u32 = 73;
    pub const CHANGED: u32 = 98;
}

/// The format properties (`NArchive::NHandlerPropID`), read where the library is loaded (Windows).
#[cfg(windows)]
pub(crate) mod handler {
    pub const NAME: u32 = 0;
    pub const CLASS_ID: u32 = 1;
    pub const EXTENSION: u32 = 2;
    pub const ADD_EXTENSION: u32 = 3;
    pub const UPDATE: u32 = 4;
    pub const KEEP_NAME: u32 = 5;
    pub const SIGNATURE: u32 = 6;
    pub const MULTI_SIGNATURE: u32 = 7;
    pub const SIGNATURE_OFFSET: u32 = 8;
    pub const ALT_STREAMS: u32 = 9;
    pub const NT_SECURE: u32 = 10;
    pub const FLAGS: u32 = 11;
}

/// A format's flags (`NArcInfoFlags`).
pub mod flags {
    pub const KEEP_NAME: u32 = 1 << 0;
    pub const ALT_STREAMS: u32 = 1 << 1;
    pub const NT_SECURE: u32 = 1 << 2;
    pub const FIND_SIGNATURE: u32 = 1 << 3;
    pub const PRE_ARC: u32 = 1 << 9;
    pub const BY_EXT_ONLY_OPEN: u32 = 1 << 12;
}

/// A format a library gives (arclite's `ArcFormat`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Format {
    pub class_id: Vec<u8>,
    pub name: String,
    pub updatable: bool,
    /// Its extensions without the dot, as the library lists them.
    pub extensions: Vec<String>,
    /// What each extension stands for inside (`kAddExtension`: `.tar` for `tgz`; `*` for none).
    pub add_extensions: Vec<String>,
    pub flags: u32,
    pub signatures: Vec<Vec<u8>>,
    pub signature_offset: u32,
    /// The library that gives it (its place in [`Library::modules`]).
    pub library: usize,
}

/// A library loaded, as "7z library info" tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub path: PathBuf,
    /// The file version, its high word first.
    pub version: u64,
}

/// A format's answer to a quick look at a signature's bytes (7-Zip's `k_IsArc_Res_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsArc {
    No,
    Yes,
    NeedMore,
}

/// An item property an archive has (`GetPropertyInfo`): its name for a property of no id of 7-Zip's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropInfo {
    pub name: Option<String>,
    pub id: u32,
    pub var_type: u16,
}

/// What the plugin answers while an archive opens (arclite's `ArchiveOpener`).
pub trait Opening {
    /// The totals the library expects; false stops the opening.
    fn set_total(&mut self, _files: Option<u64>, _bytes: Option<u64>) -> bool {
        true
    }

    /// What it has read; false stops the opening.
    fn set_completed(&mut self, _files: Option<u64>, _bytes: Option<u64>) -> bool {
        true
    }

    /// The password of encrypted headers; none stops the opening.
    fn password(&mut self) -> Option<String> {
        None
    }
}

/// Why an item comes to the extract call (7-Zip's `NAskMode`): to be written, to be tested, passed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskMode {
    Extract,
    Test,
    Skip,
}

/// How an item came out (7-Zip's `NExtract::NOperationResult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    UnsupportedMethod,
    DataError,
    CrcError,
    Unavailable,
    UnexpectedEnd,
    DataAfterEnd,
    IsNotArc,
    HeadersError,
    WrongPassword,
    Other(i32),
}

/// What the plugin answers while the library extracts or tests items (arclite's `ArchiveExtractor`,
/// `ArchiveTester`). Each item comes by [`item`](Self::item), its data (when wanted) by
/// [`write`](Self::write), its end by [`done`](Self::done). Every call comes on the thread that called
/// [`Archive::extract`], one at a time, whichever thread the library calls back from.
pub trait Extracting {
    /// The total the library will report its progress of; false stops.
    fn set_total(&mut self, _total: u64) -> bool {
        true
    }

    /// How far it has come; false stops.
    fn set_completed(&mut self, _done: u64) -> bool {
        true
    }

    /// Item `index` comes as `mode` says: true when its data is wanted (given to `write`), false when
    /// not; none stops.
    fn item(&mut self, index: u32, mode: AskMode) -> Option<bool>;

    /// A piece of the wanted item's data; false stops.
    fn write(&mut self, data: &[u8]) -> bool;

    /// How the item came out; false stops.
    fn done(&mut self, outcome: Outcome) -> bool;

    /// The password of encrypted data; none stops.
    fn password(&mut self) -> Option<String> {
        None
    }
}

/// One item of the archive being written (7-Zip's `GetUpdateItemInfo`): whether its data and its
/// properties are new (the plugin's) and which item of the archive it is (none for a new one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateItem {
    pub new_data: bool,
    pub new_properties: bool,
    pub in_archive: Option<u32>,
}

/// A new item's data as the plugin gives it (7-Zip's `GetStream`).
#[derive(Debug)]
pub enum Feed {
    /// Read from this file (on the library's threads).
    File(std::fs::File),
    /// No data: a folder.
    Nothing,
    /// The file could not be read and that was let be: the library leaves the item out.
    Skipped,
    /// The update stops.
    Stop,
}

/// What the plugin answers while the library writes an archive (arclite's `ArchiveUpdater` with its
/// update stream): the progress, each item of the new archive, a new item's properties and data, how it
/// went, the password, and the new archive's bytes (written, the place moved, the size set) wherever
/// the plugin puts them. Every call comes on the thread that started the update, one at a time.
pub trait Updating {
    /// The total the library will report its progress of; false stops.
    fn set_total(&mut self, _total: u64) -> bool {
        true
    }

    /// How far it has come (the bytes of the new data read); false stops.
    fn set_completed(&mut self, _done: u64) -> bool {
        true
    }

    /// Item `index` of the new archive.
    fn item(&mut self, index: u32) -> UpdateItem;

    /// A property ([`prop`]) of new item `index`; [`Prop::Empty`] for one it has none of.
    fn property(&mut self, index: u32, id: u32) -> Prop;

    /// New item `index`'s data.
    fn stream(&mut self, index: u32) -> Feed;

    /// How the last item given went (false: the library failed to write it); false stops.
    fn done(&mut self, ok: bool) -> bool;

    /// The password new data is encrypted with; none: not encrypted.
    fn password(&mut self) -> Option<String> {
        None
    }

    /// Bytes of the new archive, at its place.
    fn write(&mut self, data: &[u8]) -> std::io::Result<()>;

    /// The new archive's place moved; where it is.
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64>;

    /// The new archive's size set.
    fn set_size(&mut self, size: u64) -> std::io::Result<()>;
}

#[cfg(windows)]
#[path = "sevenzip_com.rs"]
mod com;

#[cfg(windows)]
#[path = "sevenzip_update.rs"]
mod update;

#[cfg(windows)]
#[path = "sevenzip_stream.rs"]
mod stream;

#[cfg(windows)]
#[path = "sevenzip_marshal.rs"]
mod marshal;

#[cfg(windows)]
#[path = "sevenzip_open.rs"]
mod open;

#[cfg(windows)]
#[path = "sevenzip_load.rs"]
mod load;

#[cfg(windows)]
#[path = "sevenzip_extract.rs"]
mod extract;

#[cfg(windows)]
#[path = "sevenzip_codepage.rs"]
mod codepage;

#[cfg(windows)]
#[path = "sevenzip_archive.rs"]
mod archive;

#[cfg(windows)]
pub use archive::{Archive, Library};
#[cfg(windows)]
pub use stream::InStream;

#[cfg(not(windows))]
#[path = "sevenzip_other.rs"]
mod other;

#[cfg(not(windows))]
pub use other::{Archive, InStream, Library};
