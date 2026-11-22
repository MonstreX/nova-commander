//! Through 7-Zip's installed library: a zip a DOS tool wrote, its name's byte 0x80 in the OEM code page
//! (CP866: `А`, CP437: `Ç`), read with the code pages set.

use super::super::{InStream, Library, Opening, prop};
use super::*;

/// A zip of one stored file named by `name`'s bytes, its host FAT, no UTF-8 flag.
fn zip_named(name: &[u8]) -> Vec<u8> {
    let data = b"x";
    let crc: u32 = 0x8CDC_1683; // CRC-32 of "x".
    let name_len = u16::try_from(name.len()).unwrap();
    let common = |out: &mut Vec<u8>| {
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
    };
    let mut out = 0x0403_4b50_u32.to_le_bytes().to_vec();
    common(&mut out);
    out.extend_from_slice(name);
    out.extend_from_slice(data);
    let start = u32::try_from(out.len()).unwrap();
    let mut central = 0x0201_4b50_u32.to_le_bytes().to_vec();
    central.extend_from_slice(&20u16.to_le_bytes());
    common(&mut central);
    central.extend_from_slice(&[0; 6]);
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(name);
    let size = u32::try_from(central.len()).unwrap();
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0]);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

struct Quiet;
impl Opening for Quiet {}

/// The module's code pages held by one test at a time (they are the process's).
fn pages_held() -> std::sync::MutexGuard<'static, ()> {
    static PAGES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    PAGES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The system's pages again however a test ends.
struct SystemAgain;
impl Drop for SystemAgain {
    fn drop(&mut self) {
        set(0, 0);
    }
}

/// The zip's one name as the library reads it.
fn name_read(library: &Library, path: &std::path::Path) -> String {
    let zip = (library.formats().iter())
        .position(|format| format.name == "zip")
        .unwrap();
    let stream = InStream::file(path).unwrap();
    let archive = (library.open(zip, &stream, Some(path), false, &mut Quiet))
        .unwrap()
        .unwrap();
    archive.property(0, prop::PATH).text().unwrap().to_string()
}

/// The OEM code page set: the library reads the name in it (its imports answered by this module's);
/// the code pages back to the system's when the library goes.
#[test]
fn the_library_reads_names_in_the_code_pages_set() {
    /// The system's pages again however the test ends (the other tests share the module's).
    struct SystemPages;
    impl Drop for SystemPages {
        fn drop(&mut self) {
            set(0, 0);
        }
    }
    let _pages = pages_held();
    let _system = SystemPages;
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("dos.zip");
    std::fs::write(&path, zip_named(b"\x80.txt")).unwrap();
    let library = Library::find(folder.path());
    library.set_code_pages(437, 1252);
    assert_eq!(name_read(&library, &path), "Ç.txt");
    assert!(
        !library.0.patched.borrow().is_empty(),
        "7z.dll's imports changed"
    );
    library.set_code_pages(866, 1252);
    assert_eq!(name_read(&library, &path), "А.txt");
    let patched = library.0.patched.borrow().len();
    assert!(patched >= 2, "{patched}");
}

/// The imports arclite patches that 7-Zip's library takes (`GetOEMCP` and both conversions; `GetACP`
/// it does not import) point at this module's, each in place of kernel32's own; all put back as the
/// library goes (its module held loaded here to look).
#[test]
fn the_imports_are_patched_and_put_back() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::FreeLibrary;
    use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;
    let _pages = pages_held();
    let _system = SystemAgain;
    let folder = tempfile::tempdir().unwrap();
    let library = Library::find(folder.path());
    let seven = (library.0.loaded.iter())
        .find(|lib| (lib.path.file_name()).is_some_and(|name| name.eq_ignore_ascii_case("7z.dll")))
        .unwrap();
    let wide: Vec<u16> = (seven.path.as_os_str().encode_wide()).chain([0]).collect();
    // SAFETY: a NUL-terminated path; the reference is given back below.
    let held = unsafe { LoadLibraryW(wide.as_ptr()) };
    assert!(!held.is_null());
    library.set_code_pages(other_than(oem_cp(), 866, 437), 0);
    let entries: Vec<(usize, usize, usize)> = (library.0.patched.borrow().iter())
        .map(|entry| (entry.slot, entry.original, entry.ours))
        .collect();
    // SAFETY: the slots of the loaded module's import table.
    let now: Vec<usize> = (entries.iter())
        .map(|(slot, _, _)| unsafe { *(*slot as *const usize) })
        .collect();
    let ours: Vec<usize> = entries.iter().map(|(_, _, ours)| *ours).collect();
    assert_eq!(now, ours);
    for (function, name) in [
        (patched_oem_cp as usize, &b"GetOEMCP\0"[..]),
        (patched_to_wide as usize, b"MultiByteToWideChar\0"),
        (patched_to_bytes as usize, b"WideCharToMultiByte\0"),
    ] {
        let entry = entries.iter().find(|(_, _, ours)| *ours == function);
        let (_, original, _) = entry.unwrap_or_else(|| panic!("{name:?} not patched"));
        assert_eq!(Some(*original), kernel32(name));
    }
    drop(library);
    // SAFETY: as above, the module still held loaded.
    let after: Vec<usize> = (entries.iter())
        .map(|(slot, _, _)| unsafe { *(*slot as *const usize) })
        .collect();
    let originals: Vec<usize> = entries.iter().map(|(_, original, _)| *original).collect();
    assert_eq!(after, originals, "put back");
    // SAFETY: the reference taken above, given back once.
    unsafe { FreeLibrary(held) };
}

/// No page set yet (0): the system's are answered.
#[test]
fn no_page_set_is_the_systems() {
    let _pages = pages_held();
    let _system = SystemAgain;
    OEM.store(0, Ordering::Relaxed);
    ANSI.store(0, Ordering::Relaxed);
    assert_eq!(
        (mapped(1), mapped(0), mapped(3)),
        (oem_cp(), ansi_cp(), ansi_cp())
    );
}

/// A slot is written only while it holds what is expected (another patcher's value let be), whatever
/// its page's protection.
#[test]
fn a_slot_is_written_only_from_what_is_expected() {
    let mut word = Box::new(5usize);
    let slot = (&raw mut *word) as usize;
    // SAFETY: an aligned pointer-sized word of the test's own.
    unsafe {
        assert!(!write_slot(slot, 4, 9));
        assert_eq!(*word, 5);
        assert!(write_slot(slot, 5, 9));
    }
    assert_eq!(*word, 9);
}

/// kernel32's functions are found as the loader resolves them.
#[test]
fn kernel32_functions_are_found() {
    assert!(kernel32(b"GetOEMCP\0").is_some());
    assert!(kernel32(b"NoSuchFunction\0").is_none());
}

/// Pages not valid are the system's; the special ones are mapped to the pages set.
#[test]
fn pages_are_mapped() {
    assert!(!valid(0) && !valid(1) && !valid(3) && !valid(65_000_123));
    assert!(valid(866) && valid(1252));
    assert_eq!(mapped(1251), 1251);
    assert!(!valid(2), "CP_MACCP is a special one too");
}

/// A page other than the system's.
fn other_than(system: u32, one: u32, another: u32) -> u32 {
    if system == one { another } else { one }
}

/// The pages set answer `CP_OEMCP` and `CP_ACP` / `CP_THREAD_ACP` (others as they are), the system's
/// before any is set; whether they differ from the system's is told; one not valid is the system's.
#[test]
fn the_pages_set_are_answered() {
    let _pages = pages_held();
    let _system = SystemAgain;
    assert!(!set(0, 0));
    assert_eq!(
        (mapped(1), mapped(0), mapped(3)),
        (oem_cp(), ansi_cp(), ansi_cp())
    );
    assert!(!set(oem_cp(), ansi_cp()), "the system's own");
    let oem = other_than(oem_cp(), 866, 437);
    let ansi = other_than(ansi_cp(), 1251, 1252);
    assert!(set(oem, 0));
    assert_eq!((mapped(1), mapped(0)), (oem, ansi_cp()));
    assert!(set(0, ansi));
    assert_eq!((mapped(1), mapped(0), mapped(3)), (oem_cp(), ansi, ansi));
    assert_eq!((mapped(2), mapped(1250)), (2, 1250));
    assert!(!set(2, 65_000_123), "neither valid");
    assert_eq!((mapped(1), mapped(0)), (oem_cp(), ansi_cp()));
    assert!(set(oem, ansi));
    // SAFETY: no arguments.
    let (got_oem, got_ansi) = unsafe { (patched_oem_cp(), patched_acp()) };
    assert_eq!((got_oem, got_ansi), (oem, ansi));
}

/// The library's conversions go through the pages set: OEM's byte 0x80 is CP866's `А`, CP437's `Ç`;
/// ANSI's (both `CP_ACP` and `CP_THREAD_ACP`) 0xC0 CP1251's `А`; back the same way.
#[test]
fn the_conversions_use_the_pages_set() {
    let _pages = pages_held();
    let _system = SystemAgain;
    let to_wide = |cp: u32, byte: u8| {
        let mut wide = [0u16; 4];
        // SAFETY: one byte of the test's own, the room of four units.
        let length = unsafe { patched_to_wide(cp, 0, &raw const byte, 1, wide.as_mut_ptr(), 4) };
        String::from_utf16_lossy(&wide[..usize::try_from(length).unwrap()])
    };
    let to_byte = |cp: u32, text: &str| {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut bytes = [0u8; 4];
        let length = i32::try_from(wide.len()).unwrap();
        // SAFETY: the test's own units and room, no default character.
        let written = unsafe {
            patched_to_bytes(
                cp,
                0,
                wide.as_ptr(),
                length,
                bytes.as_mut_ptr(),
                4,
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        };
        bytes[..usize::try_from(written).unwrap()].to_vec()
    };
    set(866, 1251);
    assert_eq!(to_wide(1, 0x80), "А");
    assert_eq!(to_wide(0, 0xC0), "А");
    assert_eq!(to_wide(3, 0xC0), "А");
    assert_eq!(to_byte(1, "А"), [0x80]);
    assert_eq!(to_byte(0, "А"), [0xC0]);
    assert_eq!(to_byte(3, "А"), [0xC0]);
    set(437, 1252);
    assert_eq!(to_wide(1, 0x80), "Ç");
    assert_eq!(to_wide(0, 0xC0), "À");
    assert_eq!(to_byte(1, "Ç"), [0x80]);
    assert_eq!(to_wide(866, 0x80), "А", "a page named is itself");
}

/// Pages the same as the system's: the library is not changed.
#[test]
fn the_systems_pages_change_nothing() {
    let _pages = pages_held();
    let _system = SystemAgain;
    let folder = tempfile::tempdir().unwrap();
    let library = Library::find(folder.path());
    library.set_code_pages(0, 0);
    assert!(library.0.patched.borrow().is_empty());
}

/// A slot is put back to kernel32's function only while it holds this module's.
#[test]
fn a_slot_is_put_back_only_from_ours() {
    let mut word = Box::new(7usize);
    let slot = (&raw mut *word) as usize;
    let patched = [Patched {
        slot,
        original: 3,
        ours: 7,
    }];
    // SAFETY: an aligned pointer-sized word of the test's own.
    unsafe { restore(&patched) };
    assert_eq!(*word, 3);
    *word = 8;
    // SAFETY: as above.
    unsafe { restore(&patched) };
    assert_eq!(*word, 8, "another patcher's let be");
}

/// A slot on a page not writable is written all the same, its page's protection put back; one that
/// holds another value is not.
#[test]
fn a_slot_on_a_read_only_page_is_written() {
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READ, VirtualAlloc, VirtualFree,
    };
    for protection in [PAGE_READONLY, PAGE_EXECUTE_READ] {
        // SAFETY: a page of the test's own, freed below.
        let page = unsafe {
            VirtualAlloc(
                std::ptr::null(),
                4096,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        assert!(!page.is_null());
        let slot = page as usize + 64;
        let mut old = 0u32;
        // SAFETY: the page's own word, then its protection changed.
        unsafe {
            *(slot as *mut usize) = 5;
            assert_ne!(VirtualProtect(page, 4096, protection, &raw mut old), 0);
        }
        let protect = || {
            // SAFETY: an address of the test's page and the place for its information.
            let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
            let size = size_of::<MEMORY_BASIC_INFORMATION>();
            // SAFETY: as above.
            assert_ne!(unsafe { VirtualQuery(page, &raw mut info, size) }, 0);
            info.Protect
        };
        // SAFETY: an aligned pointer-sized word of the test's page.
        unsafe {
            assert!(!write_slot(slot, 4, 9));
            assert_eq!(protect(), protection);
            assert!(write_slot(slot, 5, 9), "{protection:#x}");
            assert_eq!(*(slot as *const usize), 9);
        }
        assert_eq!(protect(), protection, "put back");
        // SAFETY: the page allocated above, freed once.
        unsafe { VirtualFree(page, 0, MEM_RELEASE) };
    }
}
