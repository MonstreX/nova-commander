//! What the library calls back while it extracts or tests an archive's items (arclite's
//! `ArchiveExtractor`, `ArchiveTester`): the progress (`IProgress`), each item as it comes and where its
//! data goes (`GetStream` with an `ISequentialOutStream` of the program's), how it came out
//! (`SetOperationResult`), the password (`ICryptoGetTextPassword`). The plugin's [`Extracting`] is reached
//! only during the extract call and on the caller's thread only, whichever thread the library calls
//! from ([`super::marshal`]): a call after it is refused.

use std::ffi::c_void;
use std::mem::offset_of;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::core::{BSTR, GUID, HRESULT};

use super::com::{
    Com, E_ABORT, E_FAIL, E_NOINTERFACE, IID_CRYPTO_GET_TEXT_PASSWORD, IID_EXTRACT_CALLBACK,
    IID_PROGRESS, IID_SEQUENTIAL_OUT_STREAM, IID_UNKNOWN, Raw, S_OK, Unknown, new_bstr, same,
};
use super::marshal::Marshal;
use super::{AskMode, Extracting, Outcome};

/// `IArchiveExtractCallback`'s table (`IProgress`'s first).
#[repr(C)]
struct CallbackTable {
    unknown: Unknown,
    set_total: unsafe extern "system" fn(Raw, u64) -> HRESULT,
    set_completed: unsafe extern "system" fn(Raw, *const u64) -> HRESULT,
    stream: unsafe extern "system" fn(Raw, u32, *mut Raw, i32) -> HRESULT,
    prepare: unsafe extern "system" fn(Raw, i32) -> HRESULT,
    result: unsafe extern "system" fn(Raw, i32) -> HRESULT,
}

/// `ICryptoGetTextPassword`'s table.
#[repr(C)]
struct PasswordTable {
    unknown: Unknown,
    password: unsafe extern "system" fn(Raw, *mut BSTR) -> HRESULT,
}

/// `ISequentialOutStream`'s table.
#[repr(C)]
struct OutTable {
    unknown: Unknown,
    write: unsafe extern "system" fn(Raw, *const c_void, u32, *mut u32) -> HRESULT,
}

/// The plugin's sink as the callbacks reach it: on the caller's thread, during the extract call only
/// (shared with the item streams).
type Reach = Arc<Marshal<dyn Extracting>>;

#[repr(C)]
struct CallbackObject {
    callback: *const CallbackTable,
    password: *const PasswordTable,
    refs: AtomicU32,
    reach: Reach,
}

#[repr(C)]
struct OutObject {
    table: *const OutTable,
    refs: AtomicU32,
    reach: Reach,
}

/// The callback of one extract call.
pub(super) struct Extractor {
    com: Com,
    reach: Reach,
}

impl Extractor {
    pub fn new() -> Self {
        let reach: Reach = Arc::new(Marshal::new());
        let object = Box::new(CallbackObject {
            callback: &CALLBACK_TABLE,
            password: &PASSWORD_TABLE,
            refs: AtomicU32::new(1),
            reach: Arc::clone(&reach),
        });
        Self {
            com: Com(Box::into_raw(object).cast()),
            reach,
        }
    }

    /// The pointer the library is given (its `IArchiveExtractCallback`).
    pub fn raw(&self) -> Raw {
        self.com.raw()
    }

    /// `call` run on a thread of its own with `sink` reached from the library's calls on this thread
    /// ([`Marshal::serve`]); unreachable again after it.
    pub fn with<T: Send>(&self, sink: &mut dyn Extracting, call: impl FnOnce() -> T + Send) -> T {
        let mut sink = NonNull::from(sink);
        // SAFETY: only the lifetime is erased: the marshal serves the sink until `serve` returns, inside
        // the plugin's borrow of it, and refuses every job after.
        let sink: &mut (dyn Extracting + 'static) = unsafe { std::mem::transmute(sink.as_mut()) };
        self.reach.serve(sink, call)
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
    stream,
    prepare,
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

static OUT_TABLE: OutTable = OutTable {
    unknown: Unknown {
        query: out_query,
        add_ref: out_add_ref,
        release: out_release,
    },
    write,
};

/// The callback object from an interface pointer of it, by the place of that interface's table.
///
/// # Safety
/// `this` must point at the field `at` of a `CallbackObject`.
unsafe fn base(this: Raw, at: usize) -> *mut CallbackObject {
    // SAFETY: the caller's: the object starts `at` bytes before `this`, inside one allocation.
    unsafe { this.cast::<u8>().sub(at).cast::<CallbackObject>() }
}

/// # Safety
/// `object` must be a live `CallbackObject`.
unsafe fn query(object: *mut CallbackObject, iid: *const GUID, out: *mut Raw) -> HRESULT {
    if out.is_null() || iid.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library's id, read.
    let iid = unsafe { &*iid };
    let callback = [IID_UNKNOWN, IID_PROGRESS, IID_EXTRACT_CALLBACK];
    let at = if callback.iter().any(|id| same(id, iid)) {
        Some(offset_of!(CallbackObject, callback))
    } else if same(iid, &IID_CRYPTO_GET_TEXT_PASSWORD) {
        Some(offset_of!(CallbackObject, password))
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
/// `object` must be a live `CallbackObject`.
unsafe fn add_ref(object: *mut CallbackObject) -> u32 {
    // SAFETY: the caller's: live; a reference only for the count.
    unsafe { &(*object).refs }.fetch_add(1, Ordering::Relaxed) + 1
}

/// # Safety
/// `object` must be a live `CallbackObject` the caller holds a reference of.
unsafe fn release(object: *mut CallbackObject) -> u32 {
    // SAFETY: the caller's: live; the reference to the count ends before the object may be freed.
    let left = unsafe { &(*object).refs }.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        // SAFETY: the last reference: the box made in `Extractor::new` is freed once.
        drop(unsafe { Box::from_raw(object) });
    }
    left
}

/// The two interfaces' `IUnknown` functions, each finding the object from its own table's place.
macro_rules! unknown_of {
    ($field:ident, $query:ident, $add_ref:ident, $release:ident) => {
        unsafe extern "system" fn $query(this: Raw, iid: *const GUID, out: *mut Raw) -> HRESULT {
            // SAFETY: the library calls through this interface's pointer into a live object.
            unsafe { query(base(this, offset_of!(CallbackObject, $field)), iid, out) }
        }
        unsafe extern "system" fn $add_ref(this: Raw) -> u32 {
            // SAFETY: as above.
            unsafe { add_ref(base(this, offset_of!(CallbackObject, $field))) }
        }
        unsafe extern "system" fn $release(this: Raw) -> u32 {
            // SAFETY: as above; the library gives up one reference it holds.
            unsafe { release(base(this, offset_of!(CallbackObject, $field))) }
        }
    };
}

unknown_of!(callback, callback_query, callback_add_ref, callback_release);
unknown_of!(password, password_query, password_add_ref, password_release);

/// `call` with the plugin's sink on the caller's thread (one at a time); `E_ABORT` when the extract call
/// is over or the plugin panics.
fn ask(
    reach: &Reach,
    call: impl FnOnce(&mut (dyn Extracting + 'static)) -> HRESULT + Send,
) -> HRESULT {
    reach.ask(call).unwrap_or(E_ABORT)
}

fn answer(go_on: bool) -> HRESULT {
    if go_on { S_OK } else { E_ABORT }
}

/// The callback's sink, from its interface pointer.
///
/// # Safety
/// `this` must be the callback interface of a live `CallbackObject`.
unsafe fn reach_of<'a>(this: Raw) -> &'a Reach {
    // SAFETY: the caller's; a reference to a field that lives while the library holds the object.
    unsafe { &(*base(this, offset_of!(CallbackObject, callback))).reach }
}

unsafe extern "system" fn set_total(this: Raw, total: u64) -> HRESULT {
    // SAFETY: the library calls through the callback's pointer into a live object.
    ask(unsafe { reach_of(this) }, |sink| {
        answer(sink.set_total(total))
    })
}

unsafe extern "system" fn set_completed(this: Raw, done: *const u64) -> HRESULT {
    if done.is_null() {
        return S_OK;
    }
    // SAFETY: the library's value.
    let done = unsafe { *done };
    // SAFETY: as above.
    ask(unsafe { reach_of(this) }, |sink| {
        answer(sink.set_completed(done))
    })
}

unsafe extern "system" fn stream(this: Raw, index: u32, out: *mut Raw, mode: i32) -> HRESULT {
    if out.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library's place for the stream, empty until a stream is given.
    unsafe { *out = std::ptr::null_mut() };
    let mode = match mode {
        0 => AskMode::Extract,
        1 => AskMode::Test,
        _ => AskMode::Skip,
    };
    // SAFETY: the library calls through the callback's pointer into a live object.
    let reach = unsafe { reach_of(this) };
    match reach.ask(|sink| sink.item(index, mode)) {
        None | Some(None) => E_ABORT,
        Some(Some(false)) => S_OK,
        Some(Some(true)) => {
            let object = Box::new(OutObject {
                table: &OUT_TABLE,
                refs: AtomicU32::new(1),
                reach: Arc::clone(reach),
            });
            // SAFETY: the library's place; the stream's one reference is the library's.
            unsafe { *out = Box::into_raw(object).cast() };
            S_OK
        }
    }
}

unsafe extern "system" fn prepare(_this: Raw, _mode: i32) -> HRESULT {
    S_OK
}

unsafe extern "system" fn result(this: Raw, result: i32) -> HRESULT {
    let outcome = match result {
        0 => Outcome::Ok,
        1 => Outcome::UnsupportedMethod,
        2 => Outcome::DataError,
        3 => Outcome::CrcError,
        4 => Outcome::Unavailable,
        5 => Outcome::UnexpectedEnd,
        6 => Outcome::DataAfterEnd,
        7 => Outcome::IsNotArc,
        8 => Outcome::HeadersError,
        9 => Outcome::WrongPassword,
        other => Outcome::Other(other),
    };
    // SAFETY: the library calls through the callback's pointer into a live object.
    ask(unsafe { reach_of(this) }, |sink| answer(sink.done(outcome)))
}

unsafe extern "system" fn password(this: Raw, out: *mut BSTR) -> HRESULT {
    if out.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library calls through the password's pointer into a live object.
    let object = unsafe { base(this, offset_of!(CallbackObject, password)) };
    // SAFETY: as above.
    let reach = unsafe { &(*object).reach };
    match reach.ask(|sink| sink.password()) {
        Some(Some(text)) => {
            // SAFETY: the library's place for the `BSTR` it frees.
            unsafe { *out = new_bstr(&text) };
            S_OK
        }
        _ => E_ABORT,
    }
}

/// # Safety
/// `this` must be a live `OutObject` of this module's.
unsafe fn out_object<'a>(this: Raw) -> &'a OutObject {
    // SAFETY: the caller's.
    unsafe { &*this.cast::<OutObject>() }
}

unsafe extern "system" fn out_query(this: Raw, iid: *const GUID, out: *mut Raw) -> HRESULT {
    if out.is_null() || iid.is_null() {
        return E_FAIL;
    }
    // SAFETY: the library's id, read.
    let iid = unsafe { &*iid };
    if same(iid, &IID_UNKNOWN) || same(iid, &IID_SEQUENTIAL_OUT_STREAM) {
        // SAFETY: a live object of this module's; the answer is its own pointer with one more reference.
        unsafe {
            out_add_ref(this);
            *out = this;
        }
        S_OK
    } else {
        // SAFETY: the library's place for the answer.
        unsafe { *out = std::ptr::null_mut() };
        E_NOINTERFACE
    }
}

unsafe extern "system" fn out_add_ref(this: Raw) -> u32 {
    // SAFETY: the library holds a reference, so the object lives.
    unsafe { out_object(this) }
        .refs
        .fetch_add(1, Ordering::Relaxed)
        + 1
}

unsafe extern "system" fn out_release(this: Raw) -> u32 {
    // SAFETY: the library holds a reference; the count's reference ends before the object may go.
    let left = unsafe { out_object(this) }
        .refs
        .fetch_sub(1, Ordering::AcqRel)
        - 1;
    if left == 0 {
        // SAFETY: the last reference: the box made in `stream` is freed once.
        drop(unsafe { Box::from_raw(this.cast::<OutObject>()) });
    }
    left
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
    let reach = unsafe { &out_object(this).reach };
    let result = ask(reach, |sink| answer(sink.write(data)));
    if result == S_OK && !done.is_null() {
        // SAFETY: as above.
        unsafe { *done = size };
    }
    result
}

#[cfg(test)]
#[path = "sevenzip_extract_tests.rs"]
mod tests;
