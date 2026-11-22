//! A plugin opened by the command line and by a file (ADR-0016 §14, FAR's `OpenW` with `OPEN_COMMANDLINE`
//! and `OPEN_ANALYSE`): `prefix:text` typed comes to [`Plugin::open_command`], Enter or Ctrl+PgDn on a file
//! of its masks to [`Plugin::open_file`]; the answer is a [`KeyAnswer`] as a key's (its panel opened, the
//! active panel to a folder, the command line, a file opened by the system; a file passed on). The
//! program's code pages for texts a plugin cannot read in Rust ([`Host::decode`]).

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use crate::api::{self as api, NcEntry, NcKeyAnswer, NcStr};
use crate::panel::host_text;
use crate::{FileEntry, Host, KeyAnswer, Plugin, files, state};

/// Why a file is offered to [`Plugin::open_file`] (FAR's `OPENFILEPLUGINTYPE`, the ABI's `OPEN_*`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OpenHow {
    /// Enter (`OFP_NORMAL`).
    #[default]
    Enter,
    /// Ctrl+PgDn (`OFP_ALTERNATIVE`).
    PgDn,
    /// Shift+F2 on a disk's files (`OFP_EXTRACT`): a panel opened has its root got with
    /// [`GetMode::top_level`](crate::GetMode), then is closed.
    Extract,
    /// Shift+F3 on a disk's files (`OFP_COMMANDS`): a panel opened has its root handed to
    /// [`Plugin::process_host_file`] with [`GetMode::top_level`](crate::GetMode), then is closed.
    Commands,
    /// Alt+F7's "Search in archives" (`OFP_SEARCH` with `OPM_FIND`, API 6): opened without any window
    /// (no progress, password, menu or error) or passed; a panel opened is walked by the search, then
    /// closed.
    Search,
}

impl OpenHow {
    /// The ABI's `OPEN_*` as the reason (another number: Enter).
    pub fn of(code: u32) -> Self {
        match code {
            api::OPEN_PGDN => Self::PgDn,
            api::OPEN_EXTRACT => Self::Extract,
            api::OPEN_COMMANDS => Self::Commands,
            api::OPEN_SEARCH => Self::Search,
            _ => Self::Enter,
        }
    }
}

/// A code page for [`Host::decode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodePage {
    /// The system's ANSI one (Windows' `CP_ACP`).
    Ansi,
    /// The system's OEM one, the console's (`CP_OEMCP`).
    Oem,
    /// A page by its number (1251, 866...).
    Number(u32),
}

impl Host {
    /// `bytes` in the code page `page` as text (FAR's `MultiByteToWideChar`); `None` for a page the system
    /// has not.
    pub fn decode(&self, page: CodePage, bytes: &[u8]) -> Option<String> {
        let page = match page {
            CodePage::Ansi => crate::api::CODE_PAGE_ANSI,
            CodePage::Oem => crate::api::CODE_PAGE_OEM,
            CodePage::Number(number) => number,
        };
        let mut text = NcStr::EMPTY;
        let table = self.table();
        // SAFETY: the host's own function with its own context, on the plugin's thread; the bytes live
        // through the call and the host's answer is copied before the next call to the host.
        unsafe {
            let done = (table.decode)(
                table.context,
                page,
                bytes.as_ptr(),
                bytes.len(),
                &raw mut text,
            );
            (done == 0).then(|| text.as_str().to_string())
        }
    }
}

/// A folder the host gives, none when empty.
///
/// # Safety
/// `raw` is the host's live text.
unsafe fn folder<'a>(raw: NcStr) -> Option<&'a Path> {
    // SAFETY: the caller promises the host's live text.
    Some(unsafe { host_text(raw) })
        .filter(|folder| !folder.is_empty())
        .map(Path::new)
}

/// The plugin's answer written to the host's place, its text kept until the plugin's next call.
fn answered<P>(state: &mut crate::State<P>, answer: *mut NcKeyAnswer, told: KeyAnswer) {
    let made = told.raw(&mut state.kept);
    if !answer.is_null() {
        // SAFETY: the host's place for the answer, alive through the call; its text is kept until the
        // plugin's next call.
        unsafe { *answer = made };
    }
}

pub(crate) unsafe extern "C" fn open_command<P: Plugin>(
    raw: *mut c_void,
    prefix: NcStr,
    command: NcStr,
    folder_text: NcStr,
    answer: *mut NcKeyAnswer,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    // SAFETY: the host's texts live through the call.
    let (prefix, command, folder) =
        unsafe { (host_text(prefix), host_text(command), folder(folder_text)) };
    files::code(
        state,
        |plugin| plugin.open_command(prefix, command, folder),
        |state, told| answered(state, answer, told),
    )
}

pub(crate) unsafe extern "C" fn open_file<P: Plugin>(
    raw: *mut c_void,
    path: NcStr,
    alternative: u32,
    folder_text: NcStr,
    answer: *mut NcKeyAnswer,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    // SAFETY: the host's texts live through the call.
    let (path, folder) = unsafe { (PathBuf::from(host_text(path)), folder(folder_text)) };
    files::code(
        state,
        |plugin| plugin.open_file(&path, OpenHow::of(alternative), folder),
        |state, told| answered(state, answer, told),
    )
}

pub(crate) unsafe extern "C" fn find_list<P: Plugin>(
    raw: *mut c_void,
    found: *const NcEntry,
    count: usize,
    answer: *mut NcKeyAnswer,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    let found: Vec<FileEntry> = if found.is_null() || count == 0 {
        Vec::new()
    } else {
        // SAFETY: the host's `count` entries at `found`, alive through the call.
        unsafe { std::slice::from_raw_parts(found, count) }
            .iter()
            // SAFETY: each the host's live entry, its texts alive through the call.
            .map(|raw| unsafe { found_entry(raw) })
            .collect()
    };
    files::code(
        state,
        |plugin| plugin.find_list(&found),
        |state, told| answered(state, answer, told),
    )
}

/// What the search saw of a found object, as the plugin's entry.
///
/// # Safety
/// `raw`'s texts are the host's, alive through the call.
unsafe fn found_entry(raw: &NcEntry) -> FileEntry {
    // SAFETY: the caller's promise.
    let (name, real) = unsafe { (host_text(raw.name), host_text(raw.real_path)) };
    FileEntry {
        is_dir: raw.flags & api::ENTRY_DIR != 0,
        size: raw.size,
        modified: crate::panels::time(raw.modified),
        created: crate::panels::time(raw.created),
        accessed: crate::panels::time(raw.accessed),
        changed: crate::panels::time(raw.changed),
        attributes: raw.attributes,
        hidden: raw.flags & api::ENTRY_HIDDEN != 0,
        system: raw.flags & api::ENTRY_SYSTEM != 0,
        read_only: raw.flags & api::ENTRY_READ_ONLY != 0,
        real_path: (!real.is_empty()).then(|| PathBuf::from(real)),
        allocated: (raw.allocated != api::SIZE_NONE).then_some(raw.allocated),
        ..FileEntry::file(name, 0)
    }
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
