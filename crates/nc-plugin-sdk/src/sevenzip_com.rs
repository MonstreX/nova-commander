//! 7-Zip's interfaces as its library lays them out (`IArchive.h`, `IStream.h`, `IPassword.h`,
//! `IProgress.h`, `IDecl.h`; arclite's `7z\h`): the ids, the tables of the functions called, an owned
//! interface pointer, and `PROPVARIANT` as 7-Zip's Windows build fills it (`VT_BSTR` from `SysAllocString`,
//! binary data in a `BSTR` by its byte length).

use std::ffi::c_void;
use std::io;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{SysAllocStringLen, SysFreeString, SysStringByteLen};
use windows_sys::core::{BSTR, GUID, HRESULT};

use super::Prop;

/// A COM object's pointer.
pub(super) type Raw = *mut c_void;

pub(super) const S_OK: HRESULT = 0;
pub(super) const S_FALSE: HRESULT = 1;
/// `E_ABORT`.
pub(super) const E_ABORT: HRESULT = 0x8000_4004_u32.cast_signed();
/// `E_NOINTERFACE`.
pub(super) const E_NOINTERFACE: HRESULT = 0x8000_4002_u32.cast_signed();
/// `E_FAIL`.
pub(super) const E_FAIL: HRESULT = 0x8000_4005_u32.cast_signed();
/// `HRESULT_FROM_WIN32(ERROR_INVALID_DATA)`: arclite reads it from `Open` as "not this format".
pub(super) const INVALID_DATA: HRESULT = 0x8007_000D_u32.cast_signed();

/// 7-Zip's interface `group`, `sub`: `{23170F69-40C1-278A-0000-00gg00ss0000}`.
const fn seven(group: u8, sub: u8) -> GUID {
    GUID {
        data1: 0x2317_0F69,
        data2: 0x40C1,
        data3: 0x278A,
        data4: [0, 0, 0, group, 0, sub, 0, 0],
    }
}

pub(super) const IID_UNKNOWN: GUID = GUID::from_u128(0x0000_0000_0000_0000_c000_0000_0000_0046);
pub(super) const IID_PROGRESS: GUID = seven(0, 0x05);
pub(super) const IID_SEQUENTIAL_IN_STREAM: GUID = seven(3, 0x01);
pub(super) const IID_SEQUENTIAL_OUT_STREAM: GUID = seven(3, 0x02);
pub(super) const IID_IN_STREAM: GUID = seven(3, 0x03);
pub(super) const IID_CRYPTO_GET_TEXT_PASSWORD: GUID = seven(5, 0x10);
pub(super) const IID_ALLOW_TAIL: GUID = seven(6, 0x05);
pub(super) const IID_OPEN_CALLBACK: GUID = seven(6, 0x10);
pub(super) const IID_EXTRACT_CALLBACK: GUID = seven(6, 0x20);
pub(super) const IID_OPEN_VOLUME_CALLBACK: GUID = seven(6, 0x30);
pub(super) const IID_GET_STREAM: GUID = seven(6, 0x40);
pub(super) const IID_IN_ARCHIVE: GUID = seven(6, 0x60);
pub(super) const IID_OUT_STREAM: GUID = seven(3, 0x04);
pub(super) const IID_CRYPTO_GET_TEXT_PASSWORD2: GUID = seven(5, 0x11);
pub(super) const IID_SET_PROPERTIES: GUID = seven(6, 0x03);
pub(super) const IID_UPDATE_CALLBACK: GUID = seven(6, 0x80);
pub(super) const IID_OUT_ARCHIVE: GUID = seven(6, 0xA0);

/// An id as one number, its parts in the order it is written.
pub(super) fn id_number(id: &GUID) -> u128 {
    (u128::from(id.data1) << 96)
        | (u128::from(id.data2) << 80)
        | (u128::from(id.data3) << 64)
        | u128::from(u64::from_be_bytes(id.data4))
}

/// Whether two ids are one.
pub(super) fn same(one: &GUID, other: &GUID) -> bool {
    id_number(one) == id_number(other)
}

/// `IUnknown`'s functions, first in every interface's table (Windows' `IUnknown`: no destructor).
#[repr(C)]
pub(super) struct Unknown {
    pub query: unsafe extern "system" fn(Raw, *const GUID, *mut Raw) -> HRESULT,
    pub add_ref: unsafe extern "system" fn(Raw) -> u32,
    pub release: unsafe extern "system" fn(Raw) -> u32,
}

/// `ISequentialInStream` and `IInStream`'s table (`IInStream` adds `Seek`).
#[repr(C)]
pub(super) struct InStreamTable {
    pub unknown: Unknown,
    pub read: unsafe extern "system" fn(Raw, *mut c_void, u32, *mut u32) -> HRESULT,
    pub seek: unsafe extern "system" fn(Raw, i64, u32, *mut u64) -> HRESULT,
}

/// `IInArchive`'s table.
#[repr(C)]
pub(super) struct InArchiveTable {
    pub unknown: Unknown,
    pub open: unsafe extern "system" fn(Raw, Raw, *const u64, Raw) -> HRESULT,
    pub close: unsafe extern "system" fn(Raw) -> HRESULT,
    pub number_of_items: unsafe extern "system" fn(Raw, *mut u32) -> HRESULT,
    pub property: unsafe extern "system" fn(Raw, u32, u32, *mut PropVariant) -> HRESULT,
    pub extract: unsafe extern "system" fn(Raw, *const u32, u32, i32, Raw) -> HRESULT,
    pub archive_property: unsafe extern "system" fn(Raw, u32, *mut PropVariant) -> HRESULT,
    pub number_of_properties: unsafe extern "system" fn(Raw, *mut u32) -> HRESULT,
    pub property_info:
        unsafe extern "system" fn(Raw, u32, *mut BSTR, *mut u32, *mut u16) -> HRESULT,
    pub number_of_archive_properties: unsafe extern "system" fn(Raw, *mut u32) -> HRESULT,
    pub archive_property_info:
        unsafe extern "system" fn(Raw, u32, *mut BSTR, *mut u32, *mut u16) -> HRESULT,
}

/// `IInArchiveGetStream`'s table.
#[repr(C)]
pub(super) struct GetStreamTable {
    pub unknown: Unknown,
    pub stream: unsafe extern "system" fn(Raw, u32, *mut Raw) -> HRESULT,
}

/// `IArchiveAllowTail`'s table.
#[repr(C)]
pub(super) struct AllowTailTable {
    pub unknown: Unknown,
    pub allow_tail: unsafe extern "system" fn(Raw, i32) -> HRESULT,
}

/// A failed `HRESULT` as an error.
pub(super) fn check(result: HRESULT) -> io::Result<()> {
    if result < 0 {
        Err(io::Error::from_raw_os_error(result))
    } else {
        Ok(())
    }
}

/// An interface pointer owned: released once when dropped.
#[derive(Debug)]
pub(super) struct Com(pub(super) Raw);

impl Com {
    /// `raw` taken over (one reference of its own); none for null (made only then: a null one would be
    /// released when dropped).
    pub fn take(raw: Raw) -> Option<Self> {
        (!raw.is_null()).then(|| Self(raw))
    }

    pub fn raw(&self) -> Raw {
        self.0
    }

    /// The pointer with its reference handed over (to the library, which releases it).
    pub fn into_raw(self) -> Raw {
        let raw = self.0;
        std::mem::forget(self);
        raw
    }

    /// Its table as `T`.
    ///
    /// # Safety
    /// `T` must be the table of the interface the pointer is of (or of one it derives from).
    pub unsafe fn table<T>(&self) -> &T {
        // SAFETY: a COM object starts with its table's pointer; the caller names the table's type.
        unsafe { &**self.0.cast::<*const T>() }
    }

    /// The object's interface `iid`; none when it has not.
    pub fn query(&self, iid: &GUID) -> Option<Self> {
        let mut out: Raw = null_mut();
        // SAFETY: every interface starts with `IUnknown`'s table; the answer's place lives through it.
        let result = unsafe { (self.table::<Unknown>().query)(self.0, iid, &raw mut out) };
        if result < 0 { None } else { Self::take(out) }
    }
}

impl Clone for Com {
    fn clone(&self) -> Self {
        // SAFETY: `IUnknown`'s `AddRef` on a live object; the copy releases it.
        unsafe { (self.table::<Unknown>().add_ref)(self.0) };
        Self(self.0)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: the one reference this owns, released once.
        unsafe { (self.table::<Unknown>().release)(self.0) };
    }
}

const VT_EMPTY: u16 = 0;
const VT_I2: u16 = 2;
const VT_I4: u16 = 3;
const VT_BSTR: u16 = 8;
const VT_BOOL: u16 = 11;
const VT_I1: u16 = 16;
const VT_UI1: u16 = 17;
const VT_UI2: u16 = 18;
const VT_UI4: u16 = 19;
const VT_I8: u16 = 20;
const VT_UI8: u16 = 21;
const VT_INT: u16 = 22;
const VT_UINT: u16 = 23;
const VT_FILETIME: u16 = 64;

/// `PROPVARIANT`: its type, three reserved words, its value (at most 16 bytes): the system's 24 bytes on
/// 64-bit Windows, so an array of them (`SetProperties`' values) has the system's stride. A 32-bit build
/// (the system's 16 bytes) would need its own layout: the program is built for 64 bits only.
#[repr(C)]
#[derive(Debug)]
pub(super) struct PropVariant {
    vt: u16,
    reserved: [u16; 3],
    value: [u64; 2],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<PropVariant>() == 24);

impl PropVariant {
    /// `VT_EMPTY`, as 7-Zip's callers set it before a call fills it.
    pub fn empty() -> Self {
        Self {
            vt: VT_EMPTY,
            reserved: [0; 3],
            value: [0; 2],
        }
    }

    /// `prop` as a value of the program's own (a text's `BSTR` freed with it when dropped).
    pub fn of(prop: &Prop) -> Self {
        let mut value = Self::empty();
        // SAFETY: a `PROPVARIANT` of the caller's, `VT_EMPTY`, written whole.
        unsafe { Self::write(&raw mut value, prop) };
        value
    }

    /// The value read out (the `BSTR` freed with it when dropped).
    pub fn read(&self, binary: bool) -> Prop {
        let low = self.value[0];
        match self.vt {
            VT_BOOL => Prop::Bool(low as u16 != 0),
            VT_UI1 => Prop::U32(u32::from(low as u8)),
            VT_UI2 => Prop::U32(u32::from(low as u16)),
            VT_UI4 | VT_UINT => Prop::U32(low as u32),
            VT_UI8 => Prop::U64(low),
            VT_I1 => Prop::I64(i64::from(low as u8 as i8)),
            VT_I2 => Prop::I64(i64::from(low as u16 as i16)),
            VT_I4 | VT_INT => Prop::I64(i64::from(low as u32 as i32)),
            VT_I8 => Prop::I64(low.cast_signed()),
            VT_FILETIME => Prop::Time(low),
            VT_BSTR if binary => Prop::Bytes(bstr_bytes(low as usize as BSTR)),
            VT_BSTR => Prop::Text(bstr_text(low as usize as BSTR)),
            VT_EMPTY => Prop::Empty,
            other => Prop::Other(other),
        }
    }

    /// `prop` written into `out`, a `PROPVARIANT` the library gave (`VT_EMPTY`); a text as a new `BSTR`
    /// the library frees.
    ///
    /// # Safety
    /// `out` must be null or point to a `PROPVARIANT` that may be written.
    pub unsafe fn write(out: *mut Self, prop: &Prop) {
        if out.is_null() {
            return;
        }
        let (vt, value) = match prop {
            Prop::Bool(on) => (VT_BOOL, if *on { 0xFFFF } else { 0 }),
            Prop::U32(value) => (VT_UI4, u64::from(*value)),
            Prop::U64(value) => (VT_UI8, *value),
            Prop::I64(value) => (VT_I8, value.cast_unsigned()),
            Prop::Time(value) => (VT_FILETIME, *value),
            Prop::Text(text) => (VT_BSTR, new_bstr(text) as usize as u64),
            Prop::Empty | Prop::Bytes(_) | Prop::Other(_) => (VT_EMPTY, 0),
        };
        // SAFETY: the caller's `PROPVARIANT`, written whole (its type and its value's first word).
        unsafe {
            (*out).vt = vt;
            (*out).value = [value, 0];
        }
    }
}

impl Drop for PropVariant {
    fn drop(&mut self) {
        if self.vt == VT_BSTR {
            // SAFETY: the library's `BSTR`, from `SysAllocString*`, freed once (null does nothing).
            unsafe { SysFreeString(self.value[0] as usize as BSTR) };
        }
    }
}

/// A new `BSTR` of `text` (null when the system has no memory).
pub(super) fn new_bstr(text: &str) -> BSTR {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let length = u32::try_from(wide.len()).unwrap_or(u32::MAX);
    // SAFETY: `length` units of `wide` are read; the system adds the end.
    unsafe { SysAllocStringLen(wide.as_ptr(), length) }
}

/// The bytes a `BSTR` holds (7-Zip's binary values: a class id, a signature).
fn bstr_bytes(text: BSTR) -> Vec<u8> {
    if text.is_null() {
        return Vec::new();
    }
    // SAFETY: a live `BSTR`: its byte length is stored before it.
    let length = unsafe { SysStringByteLen(text) } as usize;
    // SAFETY: `length` bytes from its start belong to it.
    unsafe { std::slice::from_raw_parts(text.cast::<u8>(), length) }.to_vec()
}

/// The text a `BSTR` holds.
pub(super) fn bstr_text(text: BSTR) -> String {
    let bytes = bstr_bytes(text);
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// A `BSTR` the library gave, read and freed.
pub(super) fn take_bstr(text: BSTR) -> Option<String> {
    if text.is_null() {
        return None;
    }
    let read = bstr_text(text);
    // SAFETY: the library's `BSTR`, now the caller's; freed once.
    unsafe { SysFreeString(text) };
    Some(read)
}

#[cfg(test)]
#[path = "sevenzip_com_tests.rs"]
mod tests;
