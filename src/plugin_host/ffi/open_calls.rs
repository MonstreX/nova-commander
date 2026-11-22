//! A plugin opened by the command line or by a file (ADR-0016 §14, FAR's `OpenW` with `OPEN_COMMANDLINE`
//! and `OPEN_ANALYSE`), its answer read as a key's; and the program's code pages for a plugin's texts
//! (`NcHost::decode`, FAR's `MultiByteToWideChar`).

use std::ffi::c_void;
use std::path::Path;

use nc_plugin_api::{CODE_PAGE_ANSI, CODE_PAGE_OEM, NcKeyAnswer, NcStr};

use super::calls::context;
use super::panel_keys::answer_of;
use super::{KeyAnswer, Loaded};
use crate::platform::locale::{ansi_code_page, decode_page, installed_code_pages, oem_code_page};

/// UTF-8's page: decoded whether the system lists it or not.
const UTF8: u32 = 65001;

impl Loaded {
    /// `prefix:command` from the command line, `folder` the program's folder (none: empty).
    pub fn open_command(
        &self,
        prefix: &str,
        command: &str,
        folder: Option<&Path>,
    ) -> Result<KeyAnswer, String> {
        let table = self.table();
        let folder = folder_text(folder);
        let mut answer = NcKeyAnswer::NONE;
        // SAFETY: as in `info`; the texts and the answer's place live through the call.
        let code = unsafe {
            (table.open_command)(
                table.state,
                NcStr::new(prefix),
                NcStr::new(command),
                NcStr::new(&folder),
                &raw mut answer,
            )
        };
        if code != 0 {
            return Err(self.last_error());
        }
        Ok(answer_of(&answer))
    }

    /// The file `path` of the plugin's masks offered as `how` says (`OPEN_*`: Enter, Ctrl+PgDn, Shift+F2,
    /// Shift+F3).
    pub fn open_file(
        &self,
        path: &Path,
        how: u32,
        folder: Option<&Path>,
    ) -> Result<KeyAnswer, String> {
        let table = self.table();
        let (path, folder) = (exact(path)?, folder_text(folder));
        let mut answer = NcKeyAnswer::NONE;
        // SAFETY: as in `open_command`.
        let code = unsafe {
            (table.open_file)(
                table.state,
                NcStr::new(&path),
                how,
                NcStr::new(&folder),
                &raw mut answer,
            )
        };
        if code != 0 {
            return Err(self.last_error());
        }
        Ok(answer_of(&answer))
    }
}

impl Loaded {
    /// Alt+F7's `found` objects (each named by its whole path, with what the search saw of it) for a
    /// panel of the plugin's.
    pub fn find_list(&self, found: &[crate::vfs::Entry]) -> Result<KeyAnswer, String> {
        let table = self.table();
        // An object whose path UTF-8 cannot hold is left out (review R49: never another name).
        let found: Vec<&crate::vfs::Entry> = (found.iter())
            .filter(|entry| (entry.real_path.as_deref()).is_none_or(|path| path.to_str().is_some()))
            .collect();
        let texts: Vec<(String, String)> = (found.iter())
            .map(|entry| {
                let real = (entry.real_path.as_deref()).and_then(Path::to_str);
                (entry.name.clone(), real.unwrap_or_default().to_string())
            })
            .collect();
        let raw: Vec<nc_plugin_api::NcEntry> = (found.iter().zip(&texts))
            .map(|(entry, (name, real))| super::panel_calls::raw_entry(entry, name, real))
            .collect();
        let mut answer = NcKeyAnswer::NONE;
        // SAFETY: as in `open_command`; the entries and their texts live through the call.
        let code =
            unsafe { (table.find_list)(table.state, raw.as_ptr(), raw.len(), &raw mut answer) };
        if code != 0 {
            return Err(self.last_error());
        }
        Ok(answer_of(&answer))
    }
}

/// A path as the plugin's UTF-8 text, exactly (review R49): one the system names with what UTF-8 cannot
/// hold (an unpaired surrogate) is not handed over, as the plugin would act on another name.
pub(in crate::plugin_host) fn exact(path: &Path) -> Result<String, String> {
    path.to_str().map(str::to_string).ok_or_else(|| {
        format!(
            "{} cannot be handed to a plugin: its name is not Unicode",
            path.display()
        )
    })
}

/// The program's folder for names given without one; none when it cannot be handed over exactly.
fn folder_text(folder: Option<&Path>) -> String {
    folder
        .and_then(|folder| exact(folder).ok())
        .unwrap_or_default()
}

/// The system's number for the plugin's page; none for one the system has not.
fn page_number(code_page: u32) -> Option<u32> {
    match code_page {
        CODE_PAGE_ANSI => Some(ansi_code_page()),
        CODE_PAGE_OEM => Some(oem_code_page()),
        UTF8 => Some(UTF8),
        number => (installed_code_pages().iter())
            .any(|page| page.id == number)
            .then_some(number),
    }
}

/// `NcHost::decode`.
pub unsafe extern "C" fn decode(
    context: *mut c_void,
    code_page: u32,
    bytes: *const u8,
    len: usize,
    text: *mut NcStr,
) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        // Another thread of the plugin's (review R45).
        return -1;
    };
    let Some(page) = page_number(code_page) else {
        return -1;
    };
    // Review R46: a length past the most text taken is not read.
    if len > super::MOST_TEXT {
        return -1;
    }
    let bytes = if len == 0 || bytes.is_null() {
        &[][..]
    } else {
        // SAFETY: the plugin hands `len` readable bytes at `bytes`, alive through the call.
        unsafe { std::slice::from_raw_parts(bytes, len) }
    };
    let mut answer = context.answer.borrow_mut();
    *answer = decode_page(page, bytes, true).0;
    if !text.is_null() {
        // SAFETY: the plugin hands a writable `NcStr` or null; the text stays in the context until the
        // next answer replaces it.
        unsafe { text.write(NcStr::new(&answer)) };
    }
    0
}

#[cfg(test)]
#[path = "open_calls_tests.rs"]
mod tests;
