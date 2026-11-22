//! Whether the shell would put one object into the Recycle Bin: FAR's `os::com::can_recycle`
//! (`platform.com.cpp:268-375`). There is no question for it, so FAR deletes the object with
//! `IFileOperation` (undo asked for, the top level only, no UI), reads the shell's flag
//! `TSF_DELETE_RECYCLE_IF_POSSIBLE` in the progress sink's `PreDeleteItem` and aborts there: nothing is
//! deleted. `windows-sys` has no COM interfaces, so their tables are laid out here in the order of
//! `shobjidl_core.h` (Wine's `shobjidl.idl` gives the same).

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{E_ABORT, E_NOINTERFACE, S_OK};
use windows_sys::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows_sys::Win32::UI::Shell::{
    FOF_ALLOWUNDO, FOF_NO_UI, FOF_NORECURSION, FileOperation, SHCreateItemFromParsingName,
    TSF_DELETE_RECYCLE_IF_POSSIBLE,
};
use windows_sys::core::{GUID, HRESULT};

const IID_IUNKNOWN: GUID = GUID::from_u128(0x0000_0000_0000_0000_c000_0000_0000_0046);
const IID_IFILE_OPERATION: GUID = GUID::from_u128(0x947a_ab5f_0a5c_4c13_b4d6_4bf7_836f_c9f8);
const IID_ISHELL_ITEM: GUID = GUID::from_u128(0x4382_6d1e_e718_42ee_bc55_a1e2_61c3_7bfe);
const IID_IPROGRESS_SINK: GUID = GUID::from_u128(0x04b0_f1a7_9490_44bc_96e1_4296_a312_52e2);

/// Whether the shell would recycle `object` (an absolute path); `None` when it could not be asked (FAR then
/// falls back to the drive's kind, here to `has_recycle_bin`).
pub fn can_recycle(object: &Path) -> Option<bool> {
    let name: Vec<u16> = object.as_os_str().encode_wide().chain([0]).collect();
    let _com = super::com::Apartment::enter();
    let mut sink = Sink {
        vtable: &SINK_VTABLE,
        can_recycle: None,
    };
    // SAFETY: the operation and the item are created, used and released here, each through its own
    // table; `name` is NUL-terminated and outlives the calls; `sink` lives on this stack through
    // `PerformOperations`, which is the only call that calls it back, on this thread.
    unsafe {
        let operation = Com::create(&FileOperation, &IID_IFILE_OPERATION)?;
        let item = Com::item(&name)?;
        let table = &**operation.0.cast::<*const FileOperationVtbl>();
        let flags = FOF_ALLOWUNDO | FOF_NORECURSION | FOF_NO_UI;
        if (table.set_operation_flags)(operation.0, flags) < 0 {
            return None;
        }
        let sink_pointer = (&raw mut sink).cast::<c_void>();
        if (table.delete_item)(operation.0, item.0, sink_pointer) < 0 {
            return None;
        }
        // The sink aborts the deletion: any other result means it was not asked, or something else broke.
        if (table.perform_operations)(operation.0) != E_ABORT {
            return None;
        }
    }
    sink.can_recycle
}

/// A COM object owned here: released on drop through its table's `Release` (the third entry of any).
struct Com(*mut c_void);

impl Com {
    /// SAFETY (caller): COM is set up on this thread.
    unsafe fn create(class: &GUID, interface: &GUID) -> Option<Self> {
        let mut object = std::ptr::null_mut();
        // SAFETY: the GUIDs live through the call; `object` receives an owned reference or stays null.
        let result = unsafe {
            CoCreateInstance(
                class,
                std::ptr::null_mut(),
                CLSCTX_INPROC_SERVER,
                interface,
                &raw mut object,
            )
        };
        (result >= 0 && !object.is_null()).then(|| Self(object))
    }

    /// SAFETY (caller): COM is set up on this thread; `name` is NUL-terminated.
    unsafe fn item(name: &[u16]) -> Option<Self> {
        let mut object = std::ptr::null_mut();
        // SAFETY: `name` is NUL-terminated and lives through the call; no bind context.
        let result = unsafe {
            SHCreateItemFromParsingName(
                name.as_ptr(),
                std::ptr::null_mut(),
                &IID_ISHELL_ITEM,
                &raw mut object,
            )
        };
        (result >= 0 && !object.is_null()).then(|| Self(object))
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: `self.0` is an owned COM reference; every COM table starts with IUnknown's three entries.
        unsafe {
            let table = &**self.0.cast::<*const UnknownVtbl>();
            (table.release)(self.0);
        }
    }
}

#[repr(C)]
struct UnknownVtbl {
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

/// `IFileOperation`'s table: IUnknown, Advise, Unadvise, SetOperationFlags, SetProgressMessage ...
/// CopyItems (12), DeleteItem, DeleteItems, NewItem, PerformOperations, GetAnyOperationsAborted.
#[repr(C)]
struct FileOperationVtbl {
    unknown: [usize; 3],
    advise_unadvise: [usize; 2],
    set_operation_flags: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
    message_to_copy_items: [usize; 12],
    delete_item: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> HRESULT,
    delete_items_new_item: [usize; 2],
    perform_operations: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    get_any_operations_aborted: usize,
}

/// The progress sink: one object on the probe's stack, never counted (FAR's `AddRef`/`Release` return 1).
#[repr(C)]
struct Sink {
    vtable: &'static SinkVtbl,
    can_recycle: Option<bool>,
}

type Pointer = *mut c_void;

/// `IFileOperationProgressSink`'s table, in its order.
#[repr(C)]
struct SinkVtbl {
    query_interface: unsafe extern "system" fn(Pointer, *const GUID, *mut Pointer) -> HRESULT,
    add_ref: unsafe extern "system" fn(Pointer) -> u32,
    release: unsafe extern "system" fn(Pointer) -> u32,
    start_operations: unsafe extern "system" fn(Pointer) -> HRESULT,
    finish_operations: unsafe extern "system" fn(Pointer, HRESULT) -> HRESULT,
    pre_rename_item: unsafe extern "system" fn(Pointer, u32, Pointer, *const u16) -> HRESULT,
    post_rename_item:
        unsafe extern "system" fn(Pointer, u32, Pointer, *const u16, HRESULT, Pointer) -> HRESULT,
    pre_move_item: unsafe extern "system" fn(Pointer, u32, Pointer, Pointer, *const u16) -> HRESULT,
    post_move_item: unsafe extern "system" fn(
        Pointer,
        u32,
        Pointer,
        Pointer,
        *const u16,
        HRESULT,
        Pointer,
    ) -> HRESULT,
    pre_copy_item: unsafe extern "system" fn(Pointer, u32, Pointer, Pointer, *const u16) -> HRESULT,
    post_copy_item: unsafe extern "system" fn(
        Pointer,
        u32,
        Pointer,
        Pointer,
        *const u16,
        HRESULT,
        Pointer,
    ) -> HRESULT,
    pre_delete_item: unsafe extern "system" fn(Pointer, u32, Pointer) -> HRESULT,
    post_delete_item: unsafe extern "system" fn(Pointer, u32, Pointer, HRESULT, Pointer) -> HRESULT,
    pre_new_item: unsafe extern "system" fn(Pointer, u32, Pointer, *const u16) -> HRESULT,
    post_new_item: unsafe extern "system" fn(
        Pointer,
        u32,
        Pointer,
        *const u16,
        *const u16,
        u32,
        HRESULT,
        Pointer,
    ) -> HRESULT,
    update_progress: unsafe extern "system" fn(Pointer, u32, u32) -> HRESULT,
    reset_timer: unsafe extern "system" fn(Pointer) -> HRESULT,
    pause_timer: unsafe extern "system" fn(Pointer) -> HRESULT,
    resume_timer: unsafe extern "system" fn(Pointer) -> HRESULT,
}

static SINK_VTABLE: SinkVtbl = SinkVtbl {
    query_interface,
    add_ref,
    release: add_ref,
    start_operations: ok,
    finish_operations: finished,
    pre_rename_item: named,
    post_rename_item: renamed,
    pre_move_item: placed,
    post_move_item: moved,
    pre_copy_item: placed,
    post_copy_item: moved,
    pre_delete_item,
    post_delete_item: deleted,
    pre_new_item: named,
    post_new_item: made,
    update_progress: progress,
    reset_timer: ok,
    pause_timer: ok,
    resume_timer: ok,
};

// The sink's other calls: nothing to do (FAR's return `S_OK`), each with its own arguments.
unsafe extern "system" fn ok(_: Pointer) -> HRESULT {
    S_OK
}

unsafe extern "system" fn finished(_: Pointer, _: HRESULT) -> HRESULT {
    S_OK
}

unsafe extern "system" fn named(_: Pointer, _: u32, _: Pointer, _: *const u16) -> HRESULT {
    S_OK
}

unsafe extern "system" fn renamed(
    _: Pointer,
    _: u32,
    _: Pointer,
    _: *const u16,
    _: HRESULT,
    _: Pointer,
) -> HRESULT {
    S_OK
}

unsafe extern "system" fn placed(
    _: Pointer,
    _: u32,
    _: Pointer,
    _: Pointer,
    _: *const u16,
) -> HRESULT {
    S_OK
}

unsafe extern "system" fn moved(
    _: Pointer,
    _: u32,
    _: Pointer,
    _: Pointer,
    _: *const u16,
    _: HRESULT,
    _: Pointer,
) -> HRESULT {
    S_OK
}

unsafe extern "system" fn deleted(
    _: Pointer,
    _: u32,
    _: Pointer,
    _: HRESULT,
    _: Pointer,
) -> HRESULT {
    S_OK
}

unsafe extern "system" fn made(
    _: Pointer,
    _: u32,
    _: Pointer,
    _: *const u16,
    _: *const u16,
    _: u32,
    _: HRESULT,
    _: Pointer,
) -> HRESULT {
    S_OK
}

unsafe extern "system" fn progress(_: Pointer, _: u32, _: u32) -> HRESULT {
    S_OK
}

unsafe extern "system" fn add_ref(_: Pointer) -> u32 {
    1
}

/// IUnknown and the sink's own interface are this object; nothing else.
unsafe extern "system" fn query_interface(
    this: Pointer,
    interface: *const GUID,
    object: *mut Pointer,
) -> HRESULT {
    if object.is_null() {
        return E_NOINTERFACE;
    }
    // SAFETY: COM hands a valid GUID and a writable out pointer.
    unsafe {
        let asked = &*interface;
        let known = |guid: &GUID| {
            guid.data1 == asked.data1
                && guid.data2 == asked.data2
                && guid.data3 == asked.data3
                && guid.data4 == asked.data4
        };
        if known(&IID_IUNKNOWN) || known(&IID_IPROGRESS_SINK) {
            *object = this;
            S_OK
        } else {
            *object = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
}

/// The shell's verdict on the object, then the deletion aborted: nothing is deleted.
unsafe extern "system" fn pre_delete_item(this: Pointer, flags: u32, _item: Pointer) -> HRESULT {
    // SAFETY: `this` is the `Sink` the probe handed to `DeleteItem`, alive on its stack during the call.
    unsafe {
        (*this.cast::<Sink>()).can_recycle =
            Some(flags & TSF_DELETE_RECYCLE_IF_POSSIBLE as u32 != 0);
    }
    E_ABORT
}

#[cfg(test)]
#[path = "recycle_probe_tests.rs"]
mod tests;
