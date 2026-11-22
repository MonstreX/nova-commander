//! The shell's objects as [`super::Shell`]'s thread holds them (FAR's emenu `Plugin.cpp`): the interfaces
//! by hand (windows-sys has no COM interfaces), each owned and released once; an item list (ITEMIDLIST)
//! freed by its owner. Only the thread that made them uses them.

use std::ffi::c_void;
use std::io;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::CMINVOKECOMMANDINFO;
use windows_sys::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
use windows_sys::Win32::UI::WindowsAndMessaging::HMENU;
use windows_sys::core::{GUID, HRESULT, PWSTR};

/// A COM object's pointer.
pub(super) type Raw = *mut c_void;

/// `IUnknown`'s functions, first in every interface's table.
#[repr(C)]
struct Unknown {
    query: unsafe extern "system" fn(Raw, *const GUID, *mut Raw) -> HRESULT,
    add_ref: unsafe extern "system" fn(Raw) -> u32,
    release: unsafe extern "system" fn(Raw) -> u32,
}

/// `IShellFolder`'s table (the functions not called: their places only).
#[repr(C)]
pub(super) struct FolderTable {
    unknown: Unknown,
    parse_display_name: usize,
    pub enum_objects: unsafe extern "system" fn(Raw, HWND, u32, *mut Raw) -> HRESULT,
    pub bind_to_object:
        unsafe extern "system" fn(Raw, *const ITEMIDLIST, Raw, *const GUID, *mut Raw) -> HRESULT,
    bind_to_storage: usize,
    compare_ids: usize,
    create_view_object: usize,
    pub get_attributes_of:
        unsafe extern "system" fn(Raw, u32, *const *const ITEMIDLIST, *mut u32) -> HRESULT,
    pub get_ui_object_of: unsafe extern "system" fn(
        Raw,
        HWND,
        u32,
        *const *const ITEMIDLIST,
        *const GUID,
        *mut u32,
        *mut Raw,
    ) -> HRESULT,
    pub get_display_name_of:
        unsafe extern "system" fn(Raw, *const ITEMIDLIST, u32, *mut STRRET) -> HRESULT,
}

/// `IEnumIDList`'s table.
#[repr(C)]
pub(super) struct EnumTable {
    unknown: Unknown,
    pub next: unsafe extern "system" fn(Raw, u32, *mut *mut ITEMIDLIST, *mut u32) -> HRESULT,
}

/// `IContextMenu`'s table.
#[repr(C)]
pub(super) struct MenuTable {
    unknown: Unknown,
    pub query_context_menu: unsafe extern "system" fn(Raw, HMENU, u32, u32, u32, u32) -> HRESULT,
    pub invoke_command: unsafe extern "system" fn(Raw, *const CMINVOKECOMMANDINFO) -> HRESULT,
    pub get_command_string:
        unsafe extern "system" fn(Raw, usize, u32, *mut u32, *mut u8, u32) -> HRESULT,
}

/// `IContextMenu2`'s table.
#[repr(C)]
struct Menu2Table {
    menu: MenuTable,
    handle_menu_msg: unsafe extern "system" fn(Raw, u32, WPARAM, LPARAM) -> HRESULT,
}

/// `IContextMenu3`'s table.
#[repr(C)]
struct Menu3Table {
    menu2: Menu2Table,
    handle_menu_msg2: unsafe extern "system" fn(Raw, u32, WPARAM, LPARAM, *mut LRESULT) -> HRESULT,
}

pub(super) const IID_SHELL_FOLDER: GUID = GUID::from_u128(0x000214e6_0000_0000_c000_000000000046);
pub(super) const IID_CONTEXT_MENU: GUID = GUID::from_u128(0x000214e4_0000_0000_c000_000000000046);
const IID_CONTEXT_MENU2: GUID = GUID::from_u128(0x000214f4_0000_0000_c000_000000000046);
const IID_CONTEXT_MENU3: GUID = GUID::from_u128(0xbcfce0a0_ec17_11d0_8d10_00a0c90f2719);

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
pub(super) struct Com(Raw);

impl Com {
    /// `raw` taken over (one reference of its own); none for null (made only then: a null one would be
    /// released when dropped).
    pub fn take(raw: Raw) -> Option<Self> {
        (!raw.is_null()).then(|| Self(raw))
    }

    pub fn raw(&self) -> Raw {
        self.0
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

/// An item list the shell allocated, freed when dropped.
#[derive(Debug)]
pub(super) struct Pidl(pub *mut ITEMIDLIST);

impl Drop for Pidl {
    fn drop(&mut self) {
        // SAFETY: the shell allocated it with the task allocator; freed once (null does nothing).
        unsafe { CoTaskMemFree(self.0.cast_const().cast()) };
    }
}

/// The text of a `STRRET` the shell gave for `pidl`; none when it cannot be read.
pub(super) fn strret_text(name: &mut STRRET, pidl: *const ITEMIDLIST) -> Option<String> {
    use windows_sys::Win32::UI::Shell::StrRetToStrW;
    let mut text: PWSTR = null_mut();
    // SAFETY: the `STRRET` as the shell filled it, for its own item; the text it allocates is freed below.
    let result = unsafe { StrRetToStrW(name, pidl, &raw mut text) };
    if result < 0 || text.is_null() {
        return None;
    }
    // SAFETY: a NUL-terminated wide string from the task allocator, read then freed once.
    unsafe {
        let length = (0..).take_while(|&at| *text.add(at) != 0).count();
        let read = String::from_utf16_lossy(std::slice::from_raw_parts(text, length));
        CoTaskMemFree(text.cast_const().cast());
        Some(read)
    }
}

/// An item's context menu: the object and the later interfaces it has (FAR's `pCMenu1/2/3`).
#[derive(Debug)]
pub(super) struct ContextMenu {
    one: Com,
    two: Option<Com>,
    three: Option<Com>,
}

impl ContextMenu {
    pub fn new(one: Com) -> Self {
        let two = one.query(&IID_CONTEXT_MENU2);
        let three = one.query(&IID_CONTEXT_MENU3);
        Self { one, two, three }
    }

    /// The interface FAR prefers for the menu's calls: the latest it has.
    fn preferred(&self) -> &Com {
        self.three
            .as_ref()
            .or(self.two.as_ref())
            .unwrap_or(&self.one)
    }

    /// Its items put in `menu` with ids `first..=last`, `flags` FAR's (`CMF_*`).
    pub fn fill(&self, menu: HMENU, first: u32, last: u32, flags: u32) -> io::Result<()> {
        let menu_object = self.preferred();
        // SAFETY: an `IContextMenu` (or later) object; the menu is live.
        check(unsafe {
            (menu_object.table::<MenuTable>().query_context_menu)(
                menu_object.raw(),
                menu,
                0,
                first,
                last,
                flags,
            )
        })
    }

    /// The string `kind` (`GCS_VERBW`, `GCS_HELPTEXTW`) of the command at `offset`; none when it has none.
    pub fn command_string(&self, offset: u32, kind: u32) -> Option<String> {
        let menu_object = self.preferred();
        let mut buffer = [0u16; 200];
        let length = u32::try_from(buffer.len()).unwrap_or(0);
        // SAFETY: a wide buffer of the length given (the `W` kinds write wide text there).
        let result = unsafe {
            (menu_object.table::<MenuTable>().get_command_string)(
                menu_object.raw(),
                usize::try_from(offset).unwrap_or(0),
                kind,
                null_mut(),
                buffer.as_mut_ptr().cast(),
                length,
            )
        };
        let end = buffer
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(buffer.len());
        let text = String::from_utf16_lossy(&buffer[..end]);
        (result >= 0 && !text.is_empty()).then_some(text)
    }

    /// The command at `offset` run (FAR's `InvokeCommand` with `MAKEINTRESOURCE`).
    pub fn invoke(&self, offset: u32) -> io::Result<()> {
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let menu_object = self.preferred();
        let info = CMINVOKECOMMANDINFO {
            cbSize: u32::try_from(size_of::<CMINVOKECOMMANDINFO>()).unwrap_or(0),
            // The command's number in place of a verb's text (`MAKEINTRESOURCEA`).
            lpVerb: usize::try_from(offset).unwrap_or(0) as *const u8,
            nShow: SW_SHOWNORMAL,
            ..CMINVOKECOMMANDINFO::default()
        };
        // SAFETY: an `IContextMenu` (or later) object; the information lives through the call.
        check(unsafe {
            (menu_object.table::<MenuTable>().invoke_command)(menu_object.raw(), &raw const info)
        })
    }

    /// A menu message handed to the object (owner-drawn items, a submenu filled as it opens); its answer
    /// when it has `IContextMenu3`'s, none when it has no way to take it.
    pub fn handle(&self, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        if let Some(three) = &self.three {
            let mut answer: LRESULT = 0;
            // SAFETY: an `IContextMenu3` object (queried for it); the answer's place lives through it.
            unsafe {
                (three.table::<Menu3Table>().handle_menu_msg2)(
                    three.raw(),
                    message,
                    wparam,
                    lparam,
                    &raw mut answer,
                )
            };
            return Some(answer);
        }
        let two = self.two.as_ref()?;
        // SAFETY: an `IContextMenu2` object (queried for it).
        unsafe { (two.table::<Menu2Table>().handle_menu_msg)(two.raw(), message, wparam, lparam) };
        Some(0)
    }
}
