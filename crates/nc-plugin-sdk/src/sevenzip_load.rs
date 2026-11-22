//! 7-Zip's library found and loaded as arclite does (`ArcAPI::load`, `load_libs`): every `*.dll` of the
//! plugin's folder that exports `CreateObject` and either `GetNumberOfFormats` with `GetHandlerProperty2`
//! or `GetHandlerProperty`; when none, `7z.dll` of the folder 7-Zip's registry key names (the user's, then
//! the machine's); when none, `7z.dll` on the search path; then `Formats\*.format` beside it. Its formats
//! read from it (`GetHandlerProperty2`), one per class id (the newer library's), in the class ids' order.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::core::{GUID, HRESULT};

use super::com::{PropVariant, Raw, S_OK};
use super::{Format, Module, Prop, handler};

pub(super) type CreateObject =
    unsafe extern "system" fn(*const GUID, *const GUID, *mut Raw) -> HRESULT;
/// An export as the system gives it (`FARPROC`).
type Proc = Option<unsafe extern "system" fn() -> isize>;
type NumberOfFormats = unsafe extern "system" fn(*mut u32) -> HRESULT;
type HandlerProperty = unsafe extern "system" fn(u32, *mut PropVariant) -> HRESULT;
type HandlerProperty2 = unsafe extern "system" fn(u32, u32, *mut PropVariant) -> HRESULT;
/// 7-Zip's `Func_IsArc`: a quick look at the bytes a signature starts.
pub(super) type IsArc = unsafe extern "system" fn(*const u8, usize) -> u32;
type GetIsArc = unsafe extern "system" fn(u32, *mut Option<IsArc>) -> HRESULT;

/// One library loaded, freed when dropped.
pub(super) struct Loaded {
    handle: HMODULE,
    pub path: PathBuf,
    pub version: u64,
    pub create_object: CreateObject,
    number_of_formats: Option<NumberOfFormats>,
    handler_property: Option<HandlerProperty>,
    handler_property2: Option<HandlerProperty2>,
    get_is_arc: Option<GetIsArc>,
}

impl Drop for Loaded {
    fn drop(&mut self) {
        // SAFETY: the handle `LoadLibraryW` gave, freed once; nothing of it is used after.
        unsafe { FreeLibrary(self.handle) };
    }
}

/// A format with what is needed to use it.
pub(super) struct Found {
    pub format: Format,
    pub is_arc: Option<IsArc>,
}

/// Every library found and the formats they give.
#[derive(Default)]
pub(super) struct Libraries {
    pub loaded: Vec<Loaded>,
    pub folder: Option<PathBuf>,
    pub formats: Vec<Found>,
    /// The SFX modules found with the library (arclite's `sfx_modules`).
    pub sfx: Vec<PathBuf>,
    /// The imports changed for the configuration's code pages (put back before the libraries go).
    pub patched: std::cell::RefCell<Vec<super::codepage::Patched>>,
}

impl Libraries {
    /// arclite's `Patch7zCP::SetCP`: the code pages answered from now on; `7z.dll`'s imports changed
    /// once, the first time they differ from the system's.
    pub fn set_code_pages(&self, oem: u32, ansi: u32) {
        let differ = super::codepage::set(oem, ansi);
        let mut patched = self.patched.borrow_mut();
        if !differ || !patched.is_empty() {
            return;
        }
        for lib in &self.loaded {
            let named =
                (lib.path.file_name()).is_some_and(|name| name.eq_ignore_ascii_case("7z.dll"));
            if named {
                // SAFETY: the library is loaded (its `Loaded` holds the handle).
                patched.extend(unsafe { super::codepage::patch(lib.handle) });
            }
        }
    }
}

impl Drop for Libraries {
    fn drop(&mut self) {
        // SAFETY: the libraries are still loaded: their handles are freed after this, with the fields.
        unsafe { super::codepage::restore(&self.patched.borrow()) };
    }
}

/// The libraries found as arclite finds them, `plugin_folder` first; the SFX modules with them (its
/// `find_sfx_modules`: the plugin's folder's, else those beside the library found after it).
pub(super) fn find(plugin_folder: &Path) -> Libraries {
    let mut loaded = load_all(&files_in(plugin_folder, "dll"));
    let mut sfx = sfx_in(plugin_folder);
    let mut folder = Some(plugin_folder.to_path_buf());
    if (loaded.is_empty() || sfx.is_empty())
        && let Some(path) = seven_zip_path()
    {
        let at = PathBuf::from(path);
        if loaded.is_empty() {
            loaded = load_all(&[at.join("7z.dll")]);
            folder = Some(at.clone());
        }
        sfx_beside(&loaded, &mut sfx, &at);
    }
    if loaded.is_empty()
        && let Some(path) = search_path("7z.dll")
    {
        loaded = load_all(std::slice::from_ref(&path));
        folder = path.parent().map(Path::to_path_buf);
        sfx_beside(
            &loaded,
            &mut sfx,
            folder.as_deref().unwrap_or(plugin_folder),
        );
    }
    if loaded.is_empty() {
        let mut none = Libraries::default();
        none.sfx = sfx;
        return none;
    }
    if let Some(folder) = &folder {
        loaded.extend(load_all(&files_in(&folder.join("Formats"), "format")));
    }
    let formats = formats(&loaded);
    Libraries {
        loaded,
        folder,
        formats,
        sfx,
        patched: std::cell::RefCell::default(),
    }
}

/// The SFX modules of `folder` when a library was loaded from there and none were found before it.
fn sfx_beside(loaded: &[Loaded], sfx: &mut Vec<PathBuf>, folder: &Path) {
    if !loaded.is_empty() && sfx.is_empty() {
        *sfx = sfx_in(folder);
    }
}

/// The `*.sfx` files of `folder` that are programs (their first two bytes `MZ`), in the system's order
/// (arclite's `find_sfx_modules`).
fn sfx_in(folder: &Path) -> Vec<PathBuf> {
    (files_in(folder, "sfx").into_iter())
        .filter(|path| {
            let mut start = [0u8; 2];
            std::fs::File::open(path)
                .and_then(|mut file| io::Read::read_exact(&mut file, &mut start))
                .is_ok_and(|()| &start == b"MZ")
        })
        .collect()
}

/// The folder 7-Zip's setup wrote (`Software\7-Zip`'s `Path`): the user's, else the machine's; an empty
/// value is none (arclite's `query_str_nt` then the machine's key).
fn seven_zip_path() -> Option<String> {
    crate::system::registry_texts(r"Software\7-Zip", Some("Path"))
        .into_iter()
        .find(|path| !path.is_empty())
}

/// The files of `folder` with the extension `extension` (whatever its case), in the system's order.
fn files_in(folder: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            (path.extension()).is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
                && path.is_file()
        })
        .collect()
}

/// Those of `paths` that are 7-Zip's libraries, loaded; the others let go.
fn load_all(paths: &[PathBuf]) -> Vec<Loaded> {
    paths.iter().filter_map(|path| load(path)).collect()
}

fn load(path: &Path) -> Option<Loaded> {
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain([0])
        .collect();
    // SAFETY: a NUL-terminated path; the handle is freed by `Loaded` or below.
    let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: the export cast to the type 7-Zip's `IArchive.h` gives it.
    let create_object = unsafe {
        std::mem::transmute::<Proc, Option<CreateObject>>(export(handle, c"CreateObject"))
    };
    let Some(create_object) = create_object else {
        // SAFETY: the handle just loaded, freed once.
        unsafe { FreeLibrary(handle) };
        return None;
    };
    // SAFETY: each export cast to the type 7-Zip's `IArchive.h` gives it; `Loaded` frees the handle.
    let loaded = unsafe {
        Loaded {
            handle,
            path: path.to_path_buf(),
            version: crate::system::fixed_version(path).unwrap_or(0),
            create_object,
            number_of_formats: std::mem::transmute::<Proc, Option<NumberOfFormats>>(export(
                handle,
                c"GetNumberOfFormats",
            )),
            handler_property: std::mem::transmute::<Proc, Option<HandlerProperty>>(export(
                handle,
                c"GetHandlerProperty",
            )),
            handler_property2: std::mem::transmute::<Proc, Option<HandlerProperty2>>(export(
                handle,
                c"GetHandlerProperty2",
            )),
            get_is_arc: std::mem::transmute::<Proc, Option<GetIsArc>>(export(handle, c"GetIsArc")),
        }
    };
    let formats_known = (loaded.number_of_formats.is_some() && loaded.handler_property2.is_some())
        || loaded.handler_property.is_some();
    formats_known.then_some(loaded)
}

/// An exported function's address, none when the library has none of that name.
///
/// # Safety
/// `handle` must be a loaded library's.
unsafe fn export(handle: HMODULE, name: &std::ffi::CStr) -> Proc {
    // SAFETY: the caller's handle; a NUL-terminated name.
    unsafe { GetProcAddress(handle, name.as_ptr().cast()) }
}

/// `name` on the search path (arclite's `search_path`: `SearchPathW` with no path of its own).
fn search_path(name: &str) -> Option<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::SearchPathW;
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let mut buffer = vec![0u16; 260];
    for _ in 0..2 {
        let capacity = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        // SAFETY: the name is NUL-terminated; the buffer holds `capacity` units.
        let size = unsafe {
            SearchPathW(
                std::ptr::null(),
                wide.as_ptr(),
                std::ptr::null(),
                capacity,
                buffer.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        } as usize;
        if size == 0 {
            return None;
        }
        if size < buffer.len() {
            return Some(PathBuf::from(String::from_utf16_lossy(&buffer[..size])));
        }
        buffer = vec![0u16; size];
    }
    None
}

/// The libraries' formats, one per class id (the newer library's when two give it), in their ids' order.
fn formats(loaded: &[Loaded]) -> Vec<Found> {
    let mut by_id: BTreeMap<Vec<u8>, (u64, Found)> = BTreeMap::new();
    for (library, lib) in loaded.iter().enumerate() {
        for index in 0..number_of_formats(lib) {
            let Some(found) = format(lib, library, index) else {
                continue;
            };
            let newer = by_id
                .get(&found.format.class_id)
                .is_none_or(|(version, _)| *version < lib.version);
            if newer {
                by_id.insert(found.format.class_id.clone(), (lib.version, found));
            }
        }
    }
    by_id.into_values().map(|(_, found)| found).collect()
}

fn number_of_formats(lib: &Loaded) -> u32 {
    let Some(number_of_formats) = lib.number_of_formats else {
        return 1;
    };
    let mut count = 0u32;
    // SAFETY: the library's export; the count's place lives through the call.
    let result = unsafe { number_of_formats(&raw mut count) };
    if result == S_OK { count } else { 0 }
}

/// The property `id` of the format `index`; none when the library fails it.
fn handler_prop(lib: &Loaded, index: u32, id: u32, binary: bool) -> Option<Prop> {
    let mut value = PropVariant::empty();
    // SAFETY: the library's exports; the variant lives through the call and is freed after.
    let result = unsafe {
        match (lib.handler_property2, lib.handler_property) {
            (Some(property2), _) => property2(index, id, &raw mut value),
            (None, Some(property)) => property(id, &raw mut value),
            (None, None) => return None,
        }
    };
    (result == S_OK).then(|| value.read(binary))
}

/// arclite's reading of a format (`ArcAPI::load`): none without a class id.
fn format(lib: &Loaded, library: usize, index: u32) -> Option<Found> {
    let class_id = match handler_prop(lib, index, handler::CLASS_ID, true)? {
        Prop::Bytes(bytes) => bytes,
        _ => return None,
    };
    let text = |id| match handler_prop(lib, index, id, false) {
        Some(Prop::Text(text)) => text,
        _ => String::new(),
    };
    let flag = |id| matches!(handler_prop(lib, index, id, false), Some(Prop::Bool(true)));
    let number = |id| match handler_prop(lib, index, id, false) {
        Some(Prop::U32(value)) => Some(value),
        _ => None,
    };
    let bytes = |id| match handler_prop(lib, index, id, true) {
        Some(Prop::Bytes(bytes)) => bytes,
        _ => Vec::new(),
    };
    let flags = number(handler::FLAGS).unwrap_or_else(|| old_flags(&flag));
    let signature = bytes(handler::SIGNATURE);
    let signatures = if signature.is_empty() {
        split_signatures(&bytes(handler::MULTI_SIGNATURE))
    } else {
        vec![signature]
    };
    let format = Format {
        class_id,
        name: text(handler::NAME),
        updatable: flag(handler::UPDATE),
        extensions: words(&text(handler::EXTENSION)),
        add_extensions: words(&text(handler::ADD_EXTENSION)),
        flags,
        signatures,
        signature_offset: number(handler::SIGNATURE_OFFSET).unwrap_or(0),
        library,
    };
    Some(Found {
        format,
        is_arc: is_arc(lib, index),
    })
}

/// The flags of a library before 9.31 (no `kFlags`): from its single answers.
fn old_flags(flag: &dyn Fn(u32) -> bool) -> u32 {
    let mut flags = 0;
    for (id, bit) in [
        (handler::KEEP_NAME, super::flags::KEEP_NAME),
        (handler::ALT_STREAMS, super::flags::ALT_STREAMS),
        (handler::NT_SECURE, super::flags::NT_SECURE),
    ] {
        if flag(id) {
            flags |= bit;
        }
    }
    flags
}

/// The format's quick check of its signature (`GetIsArc`), when the library has one.
fn is_arc(lib: &Loaded, index: u32) -> Option<IsArc> {
    let get_is_arc = lib.get_is_arc?;
    let mut is_arc: Option<IsArc> = None;
    // SAFETY: the library's export; the function's place lives through the call.
    let result = unsafe { get_is_arc(index, &raw mut is_arc) };
    if result == S_OK { is_arc } else { None }
}

/// A list split at spaces, no empty words.
fn words(text: &str) -> Vec<String> {
    text.split(' ')
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// 7-Zip's `kMultiSignature`: each signature after its length byte (arclite's `ParseSignatures`); a
/// length past the end ends the list empty.
pub(super) fn split_signatures(mut data: &[u8]) -> Vec<Vec<u8>> {
    let mut signatures = Vec::new();
    while let Some((&length, rest)) = data.split_first() {
        let length = usize::from(length);
        if length > rest.len() {
            return Vec::new();
        }
        signatures.push(rest[..length].to_vec());
        data = &rest[length..];
    }
    signatures
}

/// The modules loaded, as "7z library info" lists them.
pub(super) fn modules(libraries: &Libraries) -> Vec<Module> {
    (libraries.loaded.iter())
        .map(|lib| Module {
            path: lib.path.clone(),
            version: lib.version,
        })
        .collect()
}

#[cfg(test)]
#[path = "sevenzip_load_tests.rs"]
mod tests;
