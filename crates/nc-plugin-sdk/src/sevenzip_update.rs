//! Writing an archive through the library (arclite's `Archive::update`, `Archive::create`,
//! `delete_files`, `create_dir`): `IOutArchive::UpdateItems` with the program's update callback
//! (`IArchiveUpdateCallback` with `ICryptoGetTextPassword2`) and output stream (`IOutStream`), one
//! object answering the three; the format's parameters through `ISetProperties` first. The plugin's
//! [`Updating`] is reached on the caller's thread only, during the call ([`super::marshal`]).

use std::ffi::c_void;
use std::io::{self, SeekFrom};
use std::mem::offset_of;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::core::{BSTR, GUID, HRESULT};

use super::archive::{Archive, Library, Sent};
use super::com::{
    Com, E_ABORT, E_FAIL, E_NOINTERFACE, IID_CRYPTO_GET_TEXT_PASSWORD2, IID_OUT_ARCHIVE,
    IID_OUT_STREAM, IID_PROGRESS, IID_SEQUENTIAL_OUT_STREAM, IID_SET_PROPERTIES, IID_UNKNOWN,
    IID_UPDATE_CALLBACK, PropVariant, Raw, S_FALSE, S_OK, Unknown, check, new_bstr, same,
};
use super::marshal::Marshal;
use super::stream::InStream;
use super::{Feed, Prop, Updating};

/// `IOutArchive`'s table, up to the function used (`GetFileTimeType` after it).
#[repr(C)]
struct OutArchiveTable {
    unknown: Unknown,
    update_items: unsafe extern "system" fn(Raw, Raw, u32, Raw) -> HRESULT,
}

/// `ISetProperties`' table.
#[repr(C)]
struct SetPropertiesTable {
    unknown: Unknown,
    set_properties:
        unsafe extern "system" fn(Raw, *const *const u16, *const PropVariant, u32) -> HRESULT,
}

/// `IArchiveUpdateCallback`'s table (`IProgress`'s first).
#[repr(C)]
struct CallbackTable {
    unknown: Unknown,
    set_total: unsafe extern "system" fn(Raw, u64) -> HRESULT,
    set_completed: unsafe extern "system" fn(Raw, *const u64) -> HRESULT,
    item_info: unsafe extern "system" fn(Raw, u32, *mut i32, *mut i32, *mut u32) -> HRESULT,
    property: unsafe extern "system" fn(Raw, u32, u32, *mut PropVariant) -> HRESULT,
    stream: unsafe extern "system" fn(Raw, u32, *mut Raw) -> HRESULT,
    result: unsafe extern "system" fn(Raw, i32) -> HRESULT,
}

/// `ICryptoGetTextPassword2`'s table.
#[repr(C)]
struct PasswordTable {
    unknown: Unknown,
    password: unsafe extern "system" fn(Raw, *mut i32, *mut BSTR) -> HRESULT,
}

/// `IOutStream`'s table (`ISequentialOutStream`'s first).
#[repr(C)]
struct OutStreamTable {
    unknown: Unknown,
    write: unsafe extern "system" fn(Raw, *const c_void, u32, *mut u32) -> HRESULT,
    seek: unsafe extern "system" fn(Raw, i64, u32, *mut u64) -> HRESULT,
    set_size: unsafe extern "system" fn(Raw, u64) -> HRESULT,
}

/// The plugin's sink as the callbacks reach it: on the caller's thread, during the call only.
type Reach = Arc<Marshal<dyn Updating>>;

#[repr(C)]
struct UpdaterObject {
    callback: *const CallbackTable,
    password: *const PasswordTable,
    out: *const OutStreamTable,
    refs: AtomicU32,
    reach: Reach,
}

/// The callback and output stream of one update call.
struct Updater {
    com: Com,
    reach: Reach,
}

impl Updater {
    fn new() -> Self {
        let reach: Reach = Arc::new(Marshal::new());
        let object = Box::new(UpdaterObject {
            callback: &CALLBACK_TABLE,
            password: &PASSWORD_TABLE,
            out: &OUT_TABLE,
            refs: AtomicU32::new(1),
            reach: Arc::clone(&reach),
        });
        Self {
            com: Com(Box::into_raw(object).cast()),
            reach,
        }
    }

    /// The callback's pointer and the output stream's (the same object's).
    fn pointers(&self) -> (Raw, Raw) {
        let base = self.com.raw();
        // SAFETY: the field's address inside the live object.
        let out = unsafe { base.cast::<u8>().add(offset_of!(UpdaterObject, out)) };
        (base, out.cast())
    }

    /// `call` run on a thread of its own with `sink` reached from the library's calls on this thread.
    fn with<T: Send>(&self, sink: &mut dyn Updating, call: impl FnOnce() -> T + Send) -> T {
        let mut sink = NonNull::from(sink);
        // SAFETY: only the lifetime is erased: the marshal serves the sink until `serve` returns, inside
        // the plugin's borrow of it, and refuses every job after.
        let sink: &mut (dyn Updating + 'static) = unsafe { std::mem::transmute(sink.as_mut()) };
        self.reach.serve(sink, call)
    }
}

/// `out`'s `UpdateItems` of `count` items with `properties` set first (arclite's `set_properties`;
/// a format with no parameters takes none), `sink` answering; a stop is `ErrorKind::Interrupted`.
fn update_items(
    out: &Com,
    count: u32,
    properties: &[(String, Prop)],
    sink: &mut dyn Updating,
) -> io::Result<()> {
    if !properties.is_empty()
        && let Some(set) = out.query(&IID_SET_PROPERTIES)
    {
        set_properties(&set, properties)?;
    }
    let updater = Updater::new();
    let (callback, stream) = updater.pointers();
    // SAFETY: an `IOutArchive`'s table.
    let update = unsafe { out.table::<OutArchiveTable>() }.update_items;
    let pointers = Sent((out.raw(), stream, callback));
    let result = updater.with(sink, move || {
        let (Sent((archive, stream, callback)),) = (pointers,);
        // SAFETY: an `IOutArchive`, its output stream and callback, living through `with`, which waits
        // for this call; the caller's thread meanwhile only serves the callbacks.
        unsafe { update(archive, stream, count, callback) }
    });
    match result {
        E_ABORT => Err(io::Error::from(io::ErrorKind::Interrupted)),
        other => check(other),
    }
}

/// `ISetProperties::SetProperties` with the names and values.
fn set_properties(set: &Com, properties: &[(String, Prop)]) -> io::Result<()> {
    let names: Vec<Vec<u16>> = (properties.iter())
        .map(|(name, _)| name.encode_utf16().chain([0]).collect())
        .collect();
    let pointers: Vec<*const u16> = names.iter().map(|name| name.as_ptr()).collect();
    let values: Vec<PropVariant> = (properties.iter())
        .map(|(_, value)| PropVariant::of(value))
        .collect();
    let count = u32::try_from(values.len()).unwrap_or(u32::MAX);
    // SAFETY: an `ISetProperties`; the names (NUL-ended) and values live through the call.
    let result = unsafe {
        (set.table::<SetPropertiesTable>().set_properties)(
            set.raw(),
            pointers.as_ptr(),
            values.as_ptr(),
            count,
        )
    };
    check(result)
}

impl Archive {
    /// The archive written anew into the plugin's output (arclite's `Archive::update`: its
    /// `IOutArchive`, the items kept copied by the library): `count` items, each answered by `sink`;
    /// `Unsupported` when its format writes none.
    pub fn update(
        &self,
        count: u32,
        properties: &[(String, Prop)],
        sink: &mut dyn Updating,
    ) -> io::Result<()> {
        let out = (self.archive.query(&IID_OUT_ARCHIVE))
            .ok_or_else(|| io::Error::from(io::ErrorKind::Unsupported))?;
        update_items(&out, count, properties, sink)
    }
}

impl Library {
    /// A new archive of the format `format` written into the plugin's output (arclite's
    /// `Archive::create`): `count` new items, each answered by `sink`.
    pub fn create_archive(
        &self,
        format: usize,
        count: u32,
        properties: &[(String, Prop)],
        sink: &mut dyn Updating,
    ) -> io::Result<()> {
        let out = self.create_object(format, &IID_OUT_ARCHIVE)?;
        update_items(&out, count, properties, sink)
    }
}

static CALLBACK_TABLE: CallbackTable = CallbackTable {
    unknown: Unknown {
        query: callback_query,
        add_ref: callback_add_ref,
        release: callback_release,
    },
    set_total,
    set_completed,
    item_info,
    property,
    stream,
    result,
};

static PASSWORD_TABLE: PasswordTable = PasswordTable {
    unknown: Unknown {
        query: password_query,
        add_ref: password_add_ref,
        release: password_release,
    },
    password,
};

static OUT_TABLE: OutStreamTable = OutStreamTable {
    unknown: Unknown {
        query: out_query,
        add_ref: out_add_ref,
        release: out_release,
    },
    write,
    seek,
    set_size,
};

/// The object from an interface pointer of it, by the place of that interface's table.
///
/// # Safety
/// `this` must point at the field `at` of an `UpdaterObject`.
unsafe fn base(this: Raw, at: usize) -> *mut UpdaterObject {
    // SAFETY: the caller's: the object starts `at` bytes before `this`, inside one allocation.
    unsafe { this.cast::<u8>().sub(at).cast::<UpdaterObject>() }
}

/// # Safety
/// `object` must be a live `UpdaterObject`.
unsafe fn query(object: *mut UpdaterObject, iid: *const GUID, out: *mut Raw) -> HRESULT {
    if out.is_null() || iid.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library's id, read.
    let iid = unsafe { &*iid };
    let callback = [IID_UNKNOWN, IID_PROGRESS, IID_UPDATE_CALLBACK];
    let at = if callback.iter().any(|id| same(id, iid)) {
        Some(offset_of!(UpdaterObject, callback))
    } else if same(iid, &IID_CRYPTO_GET_TEXT_PASSWORD2) {
        Some(offset_of!(UpdaterObject, password))
    } else if same(iid, &IID_SEQUENTIAL_OUT_STREAM) || same(iid, &IID_OUT_STREAM) {
        Some(offset_of!(UpdaterObject, out))
    } else {
        None
    };
    // SAFETY: the library's place for the answer; the field's address is inside the object.
    unsafe {
        match at {
            Some(at) => {
                add_ref(object);
                *out = object.cast::<u8>().add(at).cast();
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
/// `object` must be a live `UpdaterObject`.
unsafe fn add_ref(object: *mut UpdaterObject) -> u32 {
    // SAFETY: the caller's: live; a reference only for the count.
    unsafe { &(*object).refs }.fetch_add(1, Ordering::Relaxed) + 1
}

/// # Safety
/// `object` must be a live `UpdaterObject` the caller holds a reference of.
unsafe fn release(object: *mut UpdaterObject) -> u32 {
    // SAFETY: the caller's: live; the reference to the count ends before the object may be freed.
    let left = unsafe { &(*object).refs }.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        // SAFETY: the last reference: the box made in `Updater::new` is freed once.
        drop(unsafe { Box::from_raw(object) });
    }
    left
}

/// Each interface's `IUnknown` functions, finding the object from its own table's place.
macro_rules! unknown_of {
    ($field:ident, $query:ident, $add_ref:ident, $release:ident) => {
        unsafe extern "system" fn $query(this: Raw, iid: *const GUID, out: *mut Raw) -> HRESULT {
            // SAFETY: the library calls through this interface's pointer into a live object.
            unsafe { query(base(this, offset_of!(UpdaterObject, $field)), iid, out) }
        }
        unsafe extern "system" fn $add_ref(this: Raw) -> u32 {
            // SAFETY: as above.
            unsafe { add_ref(base(this, offset_of!(UpdaterObject, $field))) }
        }
        unsafe extern "system" fn $release(this: Raw) -> u32 {
            // SAFETY: as above; the library gives up one reference it holds.
            unsafe { release(base(this, offset_of!(UpdaterObject, $field))) }
        }
    };
}

unknown_of!(callback, callback_query, callback_add_ref, callback_release);
unknown_of!(password, password_query, password_add_ref, password_release);
unknown_of!(out, out_query, out_add_ref, out_release);

/// The sink of the object whose interface at field `at` `this` is.
///
/// # Safety
/// `this` must be that interface of a live `UpdaterObject`.
unsafe fn reach_of<'a>(this: Raw, at: usize) -> &'a Reach {
    // SAFETY: the caller's; a reference to a field that lives while the library holds the object.
    unsafe { &(*base(this, at)).reach }
}

/// The callback's sink.
///
/// # Safety
/// `this` must be the callback interface of a live `UpdaterObject`.
unsafe fn callback_reach<'a>(this: Raw) -> &'a Reach {
    // SAFETY: the caller's.
    unsafe { reach_of(this, offset_of!(UpdaterObject, callback)) }
}

/// The output stream's sink.
///
/// # Safety
/// `this` must be the output stream interface of a live `UpdaterObject`.
unsafe fn out_reach<'a>(this: Raw) -> &'a Reach {
    // SAFETY: the caller's.
    unsafe { reach_of(this, offset_of!(UpdaterObject, out)) }
}

fn answer(go_on: Option<bool>) -> HRESULT {
    if go_on == Some(true) { S_OK } else { E_ABORT }
}

/// An I/O failure as the library is told it (`HRESULT_FROM_WIN32`).
fn failure(error: &io::Error) -> HRESULT {
    match error.raw_os_error() {
        Some(code) if code > 0 => ((code.cast_unsigned() & 0xFFFF) | 0x8007_0000).cast_signed(),
        _ => E_FAIL,
    }
}

unsafe extern "system" fn set_total(this: Raw, total: u64) -> HRESULT {
    // SAFETY: the library calls through the callback's pointer into a live object.
    answer(unsafe { callback_reach(this) }.ask(|sink| sink.set_total(total)))
}

unsafe extern "system" fn set_completed(this: Raw, done: *const u64) -> HRESULT {
    if done.is_null() {
        return S_OK;
    }
    // SAFETY: the library's value.
    let done = unsafe { *done };
    // SAFETY: as above.
    answer(unsafe { callback_reach(this) }.ask(|sink| sink.set_completed(done)))
}

unsafe extern "system" fn item_info(
    this: Raw,
    index: u32,
    new_data: *mut i32,
    new_properties: *mut i32,
    in_archive: *mut u32,
) -> HRESULT {
    // SAFETY: the library calls through the callback's pointer into a live object.
    let Some(item) = unsafe { callback_reach(this) }.ask(|sink| sink.item(index)) else {
        return E_ABORT;
    };
    // SAFETY: the library's places for the answers (each may be null).
    unsafe {
        if !new_data.is_null() {
            *new_data = i32::from(item.new_data);
        }
        if !new_properties.is_null() {
            *new_properties = i32::from(item.new_properties);
        }
        if !in_archive.is_null() {
            *in_archive = item.in_archive.unwrap_or(u32::MAX);
        }
    }
    S_OK
}

unsafe extern "system" fn property(
    this: Raw,
    index: u32,
    id: u32,
    value: *mut PropVariant,
) -> HRESULT {
    // SAFETY: the library calls through the callback's pointer into a live object.
    let Some(prop) = unsafe { callback_reach(this) }.ask(|sink| sink.property(index, id)) else {
        return E_ABORT;
    };
    // SAFETY: the library's `PROPVARIANT` (or null), empty until written.
    unsafe { PropVariant::write(value, &prop) };
    S_OK
}

unsafe extern "system" fn stream(this: Raw, index: u32, out: *mut Raw) -> HRESULT {
    if out.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library's place for the stream, empty until one is given.
    unsafe { *out = std::ptr::null_mut() };
    // SAFETY: the library calls through the callback's pointer into a live object.
    match unsafe { callback_reach(this) }.ask(|sink| sink.stream(index)) {
        Some(Feed::File(file)) => match InStream::of_file(file) {
            Ok(stream) => {
                // SAFETY: as above; the stream's one reference is the library's.
                unsafe { *out = stream.0.into_raw() };
                S_OK
            }
            Err(error) => failure(&error),
        },
        Some(Feed::Nothing) => S_OK,
        Some(Feed::Skipped) => S_FALSE,
        Some(Feed::Stop) | None => E_ABORT,
    }
}

unsafe extern "system" fn result(this: Raw, result: i32) -> HRESULT {
    // SAFETY: the library calls through the callback's pointer into a live object.
    answer(unsafe { callback_reach(this) }.ask(|sink| sink.done(result == 0)))
}

unsafe extern "system" fn password(this: Raw, defined: *mut i32, out: *mut BSTR) -> HRESULT {
    if defined.is_null() || out.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library calls through the password's pointer into a live object.
    let reach = unsafe { reach_of(this, offset_of!(UpdaterObject, password)) };
    let Some(password) = reach.ask(|sink| sink.password()) else {
        return E_ABORT;
    };
    // SAFETY: the library's places for the answer and the `BSTR` it frees (arclite gives an empty one
    // with no password).
    unsafe {
        *defined = i32::from(password.is_some());
        *out = new_bstr(password.as_deref().unwrap_or_default());
    }
    S_OK
}

unsafe extern "system" fn write(
    this: Raw,
    data: *const c_void,
    size: u32,
    done: *mut u32,
) -> HRESULT {
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
    // SAFETY: the library's `size` bytes.
    let data = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) };
    // SAFETY: the library calls through the stream's pointer into a live object.
    match unsafe { out_reach(this) }.ask(|sink| sink.write(data)) {
        Some(Ok(())) => {
            if !done.is_null() {
                // SAFETY: as above.
                unsafe { *done = size };
            }
            S_OK
        }
        Some(Err(error)) => failure(&error),
        None => E_ABORT,
    }
}

unsafe extern "system" fn seek(this: Raw, offset: i64, origin: u32, at: *mut u64) -> HRESULT {
    let to = match origin {
        0 => match u64::try_from(offset) {
            Ok(start) => SeekFrom::Start(start),
            Err(_) => return E_FAIL,
        },
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return E_FAIL,
    };
    // SAFETY: the library calls through the stream's pointer into a live object.
    match unsafe { out_reach(this) }.ask(|sink| sink.seek(to)) {
        Some(Ok(position)) => {
            if !at.is_null() {
                // SAFETY: the library's place for the answer.
                unsafe { *at = position };
            }
            S_OK
        }
        Some(Err(error)) => failure(&error),
        None => E_ABORT,
    }
}

unsafe extern "system" fn set_size(this: Raw, size: u64) -> HRESULT {
    // SAFETY: the library calls through the stream's pointer into a live object.
    match unsafe { out_reach(this) }.ask(|sink| sink.set_size(size)) {
        Some(Ok(())) => S_OK,
        Some(Err(error)) => failure(&error),
        None => E_ABORT,
    }
}

#[cfg(test)]
#[path = "sevenzip_update_tests.rs"]
mod tests;
