//! FAR's `get_shell_filetype_description` (`platform.com.cpp`): what the system calls a file's type
//! ("Text Document"), the default value of its program identifier's key, the identifier found as FAR
//! finds it: the effective default of the extension, else the user's choice under `FileExts`, the
//! application chosen there, the extension's own key. Nothing for a type the system does not know.
//! Ours: an identifier without that value (a Store application's, Notepad's for `.txt`) gives its
//! `FriendlyTypeName`, read as the shell reads it (FAR shows nothing then).

use windows_sys::Win32::System::Registry::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER};
use windows_sys::Win32::UI::Shell::{
    ASSOCF_NONE, ASSOCSTR_PROGID, AssocQueryStringW, SHLoadIndirectString,
};

use super::registry::string;
use super::wide;

const FILE_EXTS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\";

/// The description of the type of the file named `name` (`.` for a name without an extension).
pub fn file_type_name(name: &str) -> Option<String> {
    let program = program_id(extension_of(name))?;
    let named = string(HKEY_CLASSES_ROOT, &program, "").filter(|text| !text.is_empty());
    named.or_else(|| {
        let friendly = string(HKEY_CLASSES_ROOT, &program, "FriendlyTypeName")?;
        indirect(&friendly).filter(|text| !text.is_empty())
    })
}

/// FAR's `name_ext`: from the last dot on; `.` without one.
fn extension_of(name: &str) -> &str {
    name.rfind('.').map_or(".", |dot| &name[dot..])
}

/// FAR's `get_shell_type`.
fn program_id(extension: &str) -> Option<String> {
    if let Some(program) = effective_default(extension) {
        return Some(program);
    }
    let user = format!("{FILE_EXTS}{extension}");
    if let Some(program) = string(HKEY_CURRENT_USER, &user, "ProgId").filter(|id| is_program(id)) {
        return Some(program);
    }
    if let Some(application) = string(HKEY_CURRENT_USER, &user, "Application") {
        let program = format!(r"Applications\{application}");
        if is_program(&program) {
            return Some(program);
        }
    }
    string(HKEY_CLASSES_ROOT, extension, "").filter(|id| is_program(id))
}

/// FAR's `IApplicationAssociationRegistration::QueryCurrentDefault` (`AL_EFFECTIVE`): the identifier
/// the shell opens the extension with.
fn effective_default(extension: &str) -> Option<String> {
    let extension = wide(extension);
    let mut buffer = vec![0u16; 512];
    let mut length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: `extension` is NUL-terminated; `buffer` holds `length` characters; no extra string.
    let result = unsafe {
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_PROGID,
            extension.as_ptr(),
            std::ptr::null(),
            buffer.as_mut_ptr(),
            &raw mut length,
        )
    };
    if result < 0 {
        return None;
    }
    let end = buffer.iter().position(|&unit| unit == 0).unwrap_or(0);
    let program = String::from_utf16(&buffer[..end]).ok()?;
    (!program.is_empty()).then_some(program)
}

/// A string as the shell resolves it: `@file,-id` or `@{package?ms-resource://...}` read from its
/// resources, any other text as it is.
fn indirect(source: &str) -> Option<String> {
    let source = wide(source);
    let mut buffer = vec![0u16; 512];
    let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: `source` is NUL-terminated; `buffer` holds `length` characters; the reserved pointer is
    // null as the call asks.
    let result = unsafe {
        SHLoadIndirectString(
            source.as_ptr(),
            buffer.as_mut_ptr(),
            length,
            std::ptr::null(),
        )
    };
    if result < 0 {
        return None;
    }
    let end = buffer.iter().position(|&unit| unit == 0).unwrap_or(0);
    String::from_utf16(&buffer[..end]).ok()
}

/// FAR's `is_proper_progid`: a key of that name under `HKEY_CLASSES_ROOT`.
fn is_program(id: &str) -> bool {
    !id.is_empty() && super::registry::key_exists(HKEY_CLASSES_ROOT, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_file_has_a_description_and_an_unknown_type_none() {
        // Every Windows knows .bat; the words depend on its language.
        assert!(file_type_name("run.bat").is_some_and(|text| !text.is_empty()));
        assert_eq!(file_type_name("x.no-such-type-of-vc"), None);
    }

    #[test]
    fn a_name_without_an_extension_asks_for_the_dot() {
        assert_eq!(extension_of("README"), ".");
        assert_eq!(extension_of(".gitignore"), ".gitignore");
        assert_eq!(extension_of("a.b.c"), ".c");
    }
}
