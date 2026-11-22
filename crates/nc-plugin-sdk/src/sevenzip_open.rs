//! What the library calls back while it opens an archive (arclite's `ArchiveOpener`): the progress
//! (`IArchiveOpenCallback`), the password of encrypted headers (`ICryptoGetTextPassword`) and the next
//! volumes beside the first (`IArchiveOpenVolumeCallback`: the volume's name, size, attributes and times,
//! a sibling file by its name). The plugin's [`Opening`] is reached only during the open call: a call after
//! it is refused.

use std::mem::offset_of;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};

use windows_sys::core::{BSTR, GUID, HRESULT};

use super::com::{
    Com, E_ABORT, E_FAIL, E_NOINTERFACE, IID_CRYPTO_GET_TEXT_PASSWORD, IID_OPEN_CALLBACK,
    IID_OPEN_VOLUME_CALLBACK, IID_UNKNOWN, PropVariant, Raw, S_FALSE, S_OK, Unknown, new_bstr,
    same,
};
use super::stream::InStream;
use super::{Opening, Prop, prop};

/// `IArchiveOpenCallback`'s table.
#[repr(C)]
struct OpenTable {
    unknown: Unknown,
    set_total: unsafe extern "system" fn(Raw, *const u64, *const u64) -> HRESULT,
    set_completed: unsafe extern "system" fn(Raw, *const u64, *const u64) -> HRESULT,
}

/// `IArchiveOpenVolumeCallback`'s table.
#[repr(C)]
struct VolumeTable {
    unknown: Unknown,
    property: unsafe extern "system" fn(Raw, u32, *mut PropVariant) -> HRESULT,
    stream: unsafe extern "system" fn(Raw, *const u16, *mut Raw) -> HRESULT,
}

/// `ICryptoGetTextPassword`'s table.
#[repr(C)]
struct PasswordTable {
    unknown: Unknown,
    password: unsafe extern "system" fn(Raw, *mut BSTR) -> HRESULT,
}

/// A file of the disk as the volume callback tells it (arclite's `volume_file_info`).
#[derive(Debug, Clone, Default)]
struct VolumeFile {
    name: String,
    is_dir: bool,
    size: u64,
    attributes: u32,
    created: u64,
    accessed: u64,
    modified: u64,
}

impl VolumeFile {
    fn of(path: &Path) -> Option<Self> {
        use std::os::windows::fs::MetadataExt;
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            name: path.file_name()?.to_string_lossy().into_owned(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            attributes: metadata.file_attributes(),
            created: metadata.creation_time(),
            accessed: metadata.last_access_time(),
            modified: metadata.last_write_time(),
        })
    }

    fn property(&self, id: u32) -> Prop {
        match id {
            prop::NAME => Prop::Text(self.name.clone()),
            prop::IS_DIR => Prop::Bool(self.is_dir),
            prop::SIZE => Prop::U64(self.size),
            prop::ATTRIBUTES => Prop::U32(self.attributes),
            prop::CREATED => Prop::Time(self.created),
            prop::ACCESSED => Prop::Time(self.accessed),
            prop::MODIFIED => Prop::Time(self.modified),
            _ => Prop::Empty,
        }
    }
}

/// What the callback knows and finds.
struct State {
    /// The plugin's, during the open call only.
    asker: Option<NonNull<dyn Opening + 'static>>,
    /// The folder of the archive's file (none: no volumes, the archive is inside another).
    folder: Option<PathBuf>,
    volume: VolumeFile,
    /// The names of the volumes opened after the first (arclite's `volume_names`).
    volumes: Vec<String>,
}

#[repr(C)]
struct OpenerObject {
    open: *const OpenTable,
    volume: *const VolumeTable,
    password: *const PasswordTable,
    refs: AtomicU32,
    state: Mutex<State>,
}

/// The callback of one open call.
pub(super) struct Opener(Com);

impl Opener {
    /// For the archive at `path` (its folder's volumes reachable), or for one inside another (`None`).
    pub fn new(path: Option<&Path>) -> Self {
        let volume = path.and_then(VolumeFile::of).unwrap_or_default();
        let object = Box::new(OpenerObject {
            open: &OPEN_TABLE,
            volume: &VOLUME_TABLE,
            password: &PASSWORD_TABLE,
            refs: AtomicU32::new(1),
            state: Mutex::new(State {
                asker: None,
                folder: path.and_then(Path::parent).map(Path::to_path_buf),
                volume,
                volumes: Vec::new(),
            }),
        });
        Self(Com(Box::into_raw(object).cast()))
    }

    /// The pointer the library is given (its `IArchiveOpenCallback`).
    pub fn raw(&self) -> Raw {
        self.0.raw()
    }

    /// `call` run with `asker` reachable from the library's calls; unreachable again after it, also
    /// when `call` unwinds.
    pub fn with<T>(&self, asker: &mut dyn Opening, call: impl FnOnce() -> T) -> T {
        /// Takes the pointer away as it goes, whichever way the call ends.
        struct Reachable<'a>(&'a Opener);
        impl Drop for Reachable<'_> {
            fn drop(&mut self) {
                self.0.state().asker = None;
            }
        }
        // SAFETY: the pointer is taken away by `Reachable` before `asker`'s borrow ends; calls after it
        // find none.
        let erased: NonNull<dyn Opening + 'static> =
            unsafe { std::mem::transmute(NonNull::from(asker)) };
        self.state().asker = Some(erased);
        let _reachable = Reachable(self);
        call()
    }

    /// The volumes opened after the first, by name.
    pub fn volumes(&self) -> Vec<String> {
        self.state().volumes.clone()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        // SAFETY: the object made in `new`, alive while this holds its reference.
        let object = unsafe { &*self.0.raw().cast::<OpenerObject>() };
        object.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

static OPEN_TABLE: OpenTable = OpenTable {
    unknown: Unknown {
        query: open_query,
        add_ref: open_add_ref,
        release: open_release,
    },
    set_total,
    set_completed,
};

static VOLUME_TABLE: VolumeTable = VolumeTable {
    unknown: Unknown {
        query: volume_query,
        add_ref: volume_add_ref,
        release: volume_release,
    },
    property: volume_property,
    stream: volume_stream,
};

static PASSWORD_TABLE: PasswordTable = PasswordTable {
    unknown: Unknown {
        query: password_query,
        add_ref: password_add_ref,
        release: password_release,
    },
    password,
};

/// The object of an interface pointer of it, by the place of that interface's table in it.
///
/// # Safety
/// `this` must point at the field `at` of a live `OpenerObject`.
unsafe fn object<'a>(this: Raw, at: usize) -> &'a OpenerObject {
    // SAFETY: the caller's: the object starts `at` bytes before `this`.
    unsafe { &*base(this, at) }
}

/// The object's own pointer from an interface pointer of it (no reference made: `release` may free it).
///
/// # Safety
/// `this` must point at the field `at` of an `OpenerObject`.
unsafe fn base(this: Raw, at: usize) -> *mut OpenerObject {
    // SAFETY: the caller's: the object starts `at` bytes before `this`, inside one allocation.
    unsafe { this.cast::<u8>().sub(at).cast::<OpenerObject>() }
}

/// `QueryInterface` on the object at `object`: a pointer to the table asked for, derived from the
/// object's own pointer.
///
/// # Safety
/// `object` must be a live `OpenerObject`.
unsafe fn query(object: *mut OpenerObject, iid: *const GUID, out: *mut Raw) -> HRESULT {
    if out.is_null() || iid.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library's id, read.
    let iid = unsafe { &*iid };
    let base = object.cast::<u8>();
    let at = if same(iid, &IID_UNKNOWN) || same(iid, &IID_OPEN_CALLBACK) {
        Some(offset_of!(OpenerObject, open))
    } else if same(iid, &IID_OPEN_VOLUME_CALLBACK) {
        Some(offset_of!(OpenerObject, volume))
    } else if same(iid, &IID_CRYPTO_GET_TEXT_PASSWORD) {
        Some(offset_of!(OpenerObject, password))
    } else {
        None
    };
    // SAFETY: the library's place for the answer; the field's address is inside the object.
    unsafe {
        match at {
            Some(at) => {
                add_ref(object);
                *out = base.add(at).cast();
                S_OK
            }
            None => {
                *out = std::ptr::null_mut();
                E_NOINTERFACE
            }
        }
    }
}

/// # Safety
/// `object` must be a live `OpenerObject`.
unsafe fn add_ref(object: *mut OpenerObject) -> u32 {
    // SAFETY: the caller's: live; a reference only for the count.
    unsafe { &(*object).refs }.fetch_add(1, Ordering::Relaxed) + 1
}

/// # Safety
/// `object` must be a live `OpenerObject` the caller holds a reference of.
unsafe fn release(object: *mut OpenerObject) -> u32 {
    // SAFETY: the caller's: live; the reference to the count ends before the object may be freed.
    let left = unsafe { &(*object).refs }.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        // SAFETY: the last reference: the box made in `Opener::new` is freed once, no reference to it
        // left.
        drop(unsafe { Box::from_raw(object) });
    }
    left
}

/// The three interfaces' `IUnknown` functions, each finding the object from its own table's place.
macro_rules! unknown_of {
    ($field:ident, $query:ident, $add_ref:ident, $release:ident) => {
        unsafe extern "system" fn $query(this: Raw, iid: *const GUID, out: *mut Raw) -> HRESULT {
            // SAFETY: the library calls through this interface's pointer into a live object.
            unsafe { query(base(this, offset_of!(OpenerObject, $field)), iid, out) }
        }
        unsafe extern "system" fn $add_ref(this: Raw) -> u32 {
            // SAFETY: as above.
            unsafe { add_ref(base(this, offset_of!(OpenerObject, $field))) }
        }
        unsafe extern "system" fn $release(this: Raw) -> u32 {
            // SAFETY: as above; the library gives up one reference it holds.
            unsafe { release(base(this, offset_of!(OpenerObject, $field))) }
        }
    };
}

unknown_of!(open, open_query, open_add_ref, open_release);
unknown_of!(volume, volume_query, volume_add_ref, volume_release);
unknown_of!(password, password_query, password_add_ref, password_release);

/// `call` with the plugin's asker; `E_ABORT` when the open call is over, `E_FAIL` when it panics.
fn ask(object: &OpenerObject, call: impl FnOnce(&mut dyn Opening) -> HRESULT) -> HRESULT {
    let asker = object
        .state
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .asker;
    let Some(mut asker) = asker else {
        return E_ABORT;
    };
    // SAFETY: set by `Opener::with` for its call, which this runs inside; the lock is not held, so the
    // plugin's window may wait as long as it likes.
    let asker = unsafe { asker.as_mut() };
    catch_unwind(AssertUnwindSafe(|| call(asker))).unwrap_or(E_FAIL)
}

/// A count the library may leave out.
fn count(value: *const u64) -> Option<u64> {
    // SAFETY: the library's value or null.
    (!value.is_null()).then(|| unsafe { *value })
}

unsafe extern "system" fn set_total(this: Raw, files: *const u64, bytes: *const u64) -> HRESULT {
    // SAFETY: the library calls through the open callback's pointer into a live object.
    let object = unsafe { object(this, offset_of!(OpenerObject, open)) };
    let (files, bytes) = (count(files), count(bytes));
    ask(object, |asker| answer(asker.set_total(files, bytes)))
}

unsafe extern "system" fn set_completed(
    this: Raw,
    files: *const u64,
    bytes: *const u64,
) -> HRESULT {
    // SAFETY: as above.
    let object = unsafe { object(this, offset_of!(OpenerObject, open)) };
    let (files, bytes) = (count(files), count(bytes));
    ask(object, |asker| answer(asker.set_completed(files, bytes)))
}

fn answer(go_on: bool) -> HRESULT {
    if go_on { S_OK } else { E_ABORT }
}

unsafe extern "system" fn password(this: Raw, out: *mut BSTR) -> HRESULT {
    if out.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library calls through the password's pointer into a live object.
    let object = unsafe { object(this, offset_of!(OpenerObject, password)) };
    ask(object, |asker| match asker.password() {
        Some(text) => {
            // SAFETY: the library's place for the `BSTR` it frees.
            unsafe { *out = new_bstr(&text) };
            S_OK
        }
        None => E_ABORT,
    })
}

unsafe extern "system" fn volume_property(this: Raw, id: u32, out: *mut PropVariant) -> HRESULT {
    // SAFETY: the library calls through the volume callback's pointer into a live object.
    let object = unsafe { object(this, offset_of!(OpenerObject, volume)) };
    let value = (object.state.lock().unwrap_or_else(PoisonError::into_inner))
        .volume
        .property(id);
    // SAFETY: the library's `PROPVARIANT`, empty, or null.
    unsafe { PropVariant::write(out, &value) };
    S_OK
}

unsafe extern "system" fn volume_stream(this: Raw, name: *const u16, out: *mut Raw) -> HRESULT {
    if out.is_null() || name.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library calls through the volume callback's pointer into a live object.
    let object = unsafe { object(this, offset_of!(OpenerObject, volume)) };
    // SAFETY: the library's NUL-terminated name.
    let name = unsafe { wide_text(name) };
    let mut state = object.state.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(path) = state.folder.as_ref().map(|folder| folder.join(&name)) else {
        return S_FALSE;
    };
    let Some(file) = VolumeFile::of(&path).filter(|file| !file.is_dir) else {
        return S_FALSE;
    };
    let Ok(stream) = InStream::file(&path) else {
        return S_FALSE;
    };
    state.volumes.push(name);
    state.volume = file;
    let raw = stream.0.raw();
    std::mem::forget(stream);
    // SAFETY: the library's place for the stream; its reference is the one `stream` held.
    unsafe { *out = raw };
    S_OK
}

/// A NUL-terminated wide text.
///
/// # Safety
/// `text` must point at a NUL-terminated wide string.
unsafe fn wide_text(text: *const u16) -> String {
    // SAFETY: the caller's: the units up to the NUL are readable.
    unsafe {
        let length = (0..).take_while(|&at| *text.add(at) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
    }
}

#[cfg(test)]
#[path = "sevenzip_open_tests.rs"]
mod tests;
