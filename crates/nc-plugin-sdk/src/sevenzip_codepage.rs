//! The configuration's code pages given to 7-Zip's library as arclite gives them (`Patch7zCP`): the
//! library's imports of `GetOEMCP`, `GetACP`, `MultiByteToWideChar` and `WideCharToMultiByte` from
//! kernel32 pointed at this module's, which answer the code pages set (the system's for one not valid)
//! and put them in place of `CP_OEMCP` / `CP_ACP` / `CP_THREAD_ACP`. The imports are put back before the
//! library is let go (another user of the same `7z.dll` must never call into a module unloaded). A slot
//! is changed only from kernel32's own function and put back only from this module's: one another
//! patcher holds is let be, so no chain of patchers can end in a module unloaded.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::Globalization::{
    CPINFO, GetACP, GetCPInfo, GetOEMCP, MultiByteToWideChar, WideCharToMultiByte,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Memory::{
    MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READWRITE, PAGE_READONLY, PAGE_READWRITE,
    VirtualProtect, VirtualQuery,
};
use windows_sys::core::{BOOL, PCSTR, PCWSTR, PSTR, PWSTR};

/// The code pages the library is answered (0 until set).
static OEM: AtomicU32 = AtomicU32::new(0);
static ANSI: AtomicU32 = AtomicU32::new(0);

/// `CP_ACP`, `CP_OEMCP`, `CP_THREAD_ACP`.
const CP_ACP: u32 = 0;
const CP_OEMCP: u32 = 1;
const CP_THREAD_ACP: u32 = 3;

/// The page `cp` stands for, as the library is answered.
fn mapped(cp: u32) -> u32 {
    let over = |page: &AtomicU32, system: fn() -> u32| match page.load(Ordering::Relaxed) {
        0 => system(),
        page => page,
    };
    match cp {
        CP_OEMCP => over(&OEM, oem_cp),
        CP_ACP | CP_THREAD_ACP => over(&ANSI, ansi_cp),
        other => other,
    }
}

fn oem_cp() -> u32 {
    // SAFETY: no arguments.
    unsafe { GetOEMCP() }
}

fn ansi_cp() -> u32 {
    // SAFETY: no arguments.
    unsafe { GetACP() }
}

unsafe extern "system" fn patched_oem_cp() -> u32 {
    mapped(CP_OEMCP)
}

unsafe extern "system" fn patched_acp() -> u32 {
    mapped(CP_ACP)
}

unsafe extern "system" fn patched_to_wide(
    cp: u32,
    flags: u32,
    bytes: PCSTR,
    length: i32,
    wide: PWSTR,
    room: i32,
) -> i32 {
    // SAFETY: the library's own arguments passed on, the code page mapped.
    unsafe { MultiByteToWideChar(mapped(cp), flags, bytes, length, wide, room) }
}

unsafe extern "system" fn patched_to_bytes(
    cp: u32,
    flags: u32,
    wide: PCWSTR,
    length: i32,
    bytes: PSTR,
    room: i32,
    default: PCSTR,
    used: *mut BOOL,
) -> i32 {
    // SAFETY: as above.
    unsafe { WideCharToMultiByte(mapped(cp), flags, wide, length, bytes, room, default, used) }
}

/// arclite's `is_valid_cp`: a code page of its own the system knows.
fn valid(cp: u32) -> bool {
    let mut info: CPINFO = CPINFO {
        MaxCharSize: 0,
        DefaultChar: [0; 2],
        LeadByte: [0; 12],
    };
    // SAFETY: the information's place lives through the call.
    cp > CP_THREAD_ACP && unsafe { GetCPInfo(cp, &raw mut info) } != 0
}

/// The code pages to answer (0 or one not valid: the system's); whether they differ from the system's
/// (arclite patches the library only then).
pub(super) fn set(oem: u32, ansi: u32) -> bool {
    let oem = if valid(oem) { oem } else { oem_cp() };
    let ansi = if valid(ansi) { ansi } else { ansi_cp() };
    OEM.store(oem, Ordering::Relaxed);
    ANSI.store(ansi, Ordering::Relaxed);
    oem != oem_cp() || ansi != ansi_cp()
}

/// An import changed: its slot, what was there, what was put.
pub(super) struct Patched {
    slot: usize,
    original: usize,
    ours: usize,
}

/// kernel32's function `name` (NUL-terminated), as the loader resolves its imports.
fn kernel32(name: &[u8]) -> Option<usize> {
    let module: Vec<u16> = "kernel32.dll\0".encode_utf16().collect();
    // SAFETY: a NUL-terminated name; kernel32 is loaded in every process.
    let handle = unsafe { GetModuleHandleW(module.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: a loaded module and a NUL-terminated name.
    unsafe { GetProcAddress(handle, name.as_ptr()) }.map(|function| function as usize)
}

/// The value of `bytes` at `at` of a module (a `u16` / `u32` / pointer-sized).
///
/// # Safety
/// `base + at` must be readable for `T`'s size inside the module's image.
unsafe fn read<T: Copy>(base: usize, at: usize) -> T {
    // SAFETY: the caller's.
    unsafe { std::ptr::read_unaligned((base + at) as *const T) }
}

/// The kernel32 imports of the module at `module` that arclite patches pointed at this module's
/// (`patch_7z_dll`); what was changed, to put back.
///
/// # Safety
/// `module` must be a loaded module's handle (its image mapped as the loader mapped it).
pub(super) unsafe fn patch(module: HMODULE) -> Vec<Patched> {
    let base = module as usize;
    let mut done = Vec::new();
    // SAFETY: the caller's: a mapped image; its headers are read where the PE format puts them.
    unsafe {
        let headers = base + read::<u32>(base, 0x3C) as usize;
        let optional = headers + 0x18;
        let directories = match read::<u16>(optional, 0) {
            0x20B => optional + 112,
            _ => optional + 96,
        };
        let imports = read::<u32>(directories, 8) as usize;
        if imports == 0 {
            return done;
        }
        let mut descriptor = base + imports;
        while read::<u32>(descriptor, 0) != 0 {
            let name = std::ffi::CStr::from_ptr(
                (base + read::<u32>(descriptor, 12) as usize) as *const i8,
            );
            if name.to_bytes().eq_ignore_ascii_case(b"kernel32.dll") {
                let names = base + read::<u32>(descriptor, 0) as usize;
                let slots = base + read::<u32>(descriptor, 16) as usize;
                patch_slots(base, names, slots, &mut done);
            }
            descriptor += 20;
        }
    }
    done
}

/// One kernel32 import table's entries by name (not by ordinal) pointed at this module's.
///
/// # Safety
/// `names` and `slots` must be the module's import name and address tables.
unsafe fn patch_slots(base: usize, names: usize, slots: usize, done: &mut Vec<Patched>) {
    let size = size_of::<usize>();
    let by_ordinal = 1usize << (usize::BITS - 1);
    let mut at = 0;
    loop {
        // SAFETY: the caller's tables, read up to their closing zero.
        let entry = unsafe { read::<usize>(names, at * size) };
        if entry == 0 {
            break;
        }
        let slot = slots + at * size;
        at += 1;
        if entry & by_ordinal != 0 {
            continue;
        }
        // SAFETY: a name import's `IMAGE_IMPORT_BY_NAME`: its hint, then its NUL-terminated name.
        let name =
            unsafe { std::ffi::CStr::from_ptr((base + (entry & 0x7FFF_FFFF) + 2) as *const i8) };
        let ours: usize = match name.to_bytes() {
            b"GetOEMCP" => patched_oem_cp as *const () as usize,
            b"GetACP" => patched_acp as *const () as usize,
            b"MultiByteToWideChar" => patched_to_wide as *const () as usize,
            b"WideCharToMultiByte" => patched_to_bytes as *const () as usize,
            _ => continue,
        };
        let Some(original) = kernel32(name.to_bytes_with_nul()) else {
            continue;
        };
        // SAFETY: the slot is the module's import address table entry, pointer-sized.
        if unsafe { write_slot(slot, original, ours) } {
            done.push(Patched {
                slot,
                original,
                ours,
            });
        }
    }
}

/// `value` written into the import slot at `slot` when it holds `expected` (arclite's `patch_IAT`: a
/// page not writable made so for it, executable kept so, its protection put back); whether it was.
///
/// # Safety
/// `slot` must be a pointer-sized, aligned import address table entry of a loaded module.
unsafe fn write_slot(slot: usize, expected: usize, value: usize) -> bool {
    // SAFETY: the slot is an aligned pointer-sized word of the module's image.
    let word = || unsafe { AtomicUsize::from_ptr(slot as *mut usize) };
    // SAFETY: the information's place lives through the call.
    let mut page: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let size = size_of::<MEMORY_BASIC_INFORMATION>();
    // SAFETY: an address of the module's image and the place for its region's information.
    if unsafe { VirtualQuery(slot as *const _, &raw mut page, size) } == 0 {
        return false;
    }
    let swap = || {
        word()
            .compare_exchange(expected, value, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    };
    let writable = match page.Protect {
        PAGE_READWRITE | PAGE_EXECUTE_READWRITE => return swap(),
        PAGE_READONLY => PAGE_READWRITE,
        _ => PAGE_EXECUTE_READWRITE,
    };
    let mut old = 0u32;
    // SAFETY: the slot's region, its protection changed and put back.
    unsafe {
        if VirtualProtect(page.BaseAddress, page.RegionSize, writable, &raw mut old) == 0 {
            return false;
        }
        let swapped = swap();
        VirtualProtect(
            page.BaseAddress,
            page.RegionSize,
            page.Protect,
            &raw mut old,
        );
        swapped
    }
}

/// The imports changed put back, each only while it still holds this module's function.
///
/// # Safety
/// The module the slots are of must still be loaded.
pub(super) unsafe fn restore(patched: &[Patched]) {
    for entry in patched {
        // SAFETY: the caller's: the slot written by `patch`, its module still loaded.
        unsafe { write_slot(entry.slot, entry.ours, entry.original) };
    }
}

#[cfg(test)]
#[path = "sevenzip_codepage_tests.rs"]
mod tests;
