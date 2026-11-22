//! A plugin's panel calls (ADR-0016 §13): its folders listed, its title and modes, its end; the entries
//! copied into the program's before the plugin's next call.

use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nc_plugin_api::{
    ENTRY_DIR, ENTRY_HIDDEN, ENTRY_READ_ONLY, ENTRY_SYSTEM, ERROR_DENIED, ERROR_EXISTS,
    ERROR_NOT_FOUND, ERROR_UNSUPPORTED, MODE_ALIGN_EXTENSIONS, MODE_CASE_CONVERSION,
    MODE_DETAILED_STATUS, MODE_FULL_SCREEN, NcEntry, NcPanelInfo, NcPanelMode, NcStr, NcTimes,
    PANEL_NAMES_ONLY, PANEL_PRESERVE_CASE, PANEL_RIGHT_ALIGN_NAMES, PANEL_TAKES_PATHS, REMOVE_FILE,
    REMOVE_FOLDER, SIZE_NONE, TIME_NONE, WRITE_ABORT, WRITE_APPEND, WRITE_END, WRITE_NEW,
};

use super::{Loaded, text};
use crate::platform::files::{Attributes, FileTimes};
use crate::vfs::{Entry, EntryTexts, InfoLine, PanelKey, PanelLook, PanelMode};

/// The most entries a folder of a plugin's panel gives; more are not read.
const MOST_ENTRIES: usize = 10_000_000;

/// The most modes read: Ctrl+0..9.
const MOST_MODES: usize = 10;

/// The most texts of an entry's or a mode's columns read: `C0`..`C99`.
const MOST_COLUMNS: usize = 100;

/// The most keys of a panel's own read.
const MOST_PANEL_KEYS: usize = 100;
/// The most info panel lines read (FAR reads all, the panel shows what fits).
const MOST_INFO_LINES: usize = 1000;

/// The most bytes one read asks for: a transfer's chunk.
pub const MOST_READ: usize = 1 << 20;

/// A call on one file of a panel (5c-2).
#[derive(Debug)]
pub enum FileCall {
    Stat,
    Read {
        offset: u64,
        size: usize,
    },
    /// `append` false: a new file of `data`.
    Write {
        append: bool,
        data: Vec<u8>,
    },
    /// No more is written to it: whole (API 9's `WRITE_END`).
    End,
    /// No more is written to it: its copy failed or stopped (API 9's `WRITE_ABORT`).
    Abort,
    MakeDir,
    /// To another path of the same panel.
    Rename {
        to: String,
        replace: bool,
    },
    /// `folder`: an empty folder.
    Remove {
        folder: bool,
    },
    /// What it keeps of its source: the times given, the attributes when given.
    SetTimes {
        times: FileTimes,
        attributes: Option<Attributes>,
    },
}

/// What a file's call gives back.
#[derive(Debug)]
pub enum FileAnswer {
    Entry(Entry),
    Data(Vec<u8>),
    Done,
}

/// Windows' attribute bits the entries' flags stand for.
const READ_ONLY: u32 = 0x1;
const HIDDEN: u32 = 0x2;
const SYSTEM: u32 = 0x4;
const DIRECTORY: u32 = 0x10;
const REPARSE_POINT: u32 = 0x400;

impl Loaded {
    /// The folder `path` of the panel `instance`.
    pub fn list(&self, instance: &str, path: &str) -> io::Result<Vec<Entry>> {
        self.quietly(|| self.list_now(instance, path))
    }

    fn list_now(&self, instance: &str, path: &str) -> io::Result<Vec<Entry>> {
        let table = self.table();
        let mut entries: *const NcEntry = std::ptr::null();
        let mut count = 0;
        // SAFETY: as in `info`; the texts and the answer's places live through the call.
        let code = unsafe {
            (table.list)(
                table.state,
                NcStr::new(instance),
                NcStr::new(path),
                &raw mut entries,
                &raw mut count,
            )
        };
        if code != 0 {
            return Err(error(code, self.last_error()));
        }
        if entries.is_null() || count == 0 {
            return Ok(Vec::new());
        }
        // SAFETY: the plugin hands `count` entries at `entries`, valid until its next call; each is
        // copied now.
        let raw = unsafe { std::slice::from_raw_parts(entries, count.min(MOST_ENTRIES)) };
        Ok(raw
            .iter()
            .enumerate()
            .map(|(at, raw)| entry(raw, at))
            .collect())
    }

    /// How the panel looks at `path`: its title (`None` when the plugin gives none) and its modes.
    pub fn panel_look(&self, instance: &str, path: &str) -> PanelLook {
        self.quietly(|| self.panel_look_now(instance, path))
    }

    fn panel_look_now(&self, instance: &str, path: &str) -> PanelLook {
        let table = self.table();
        // SAFETY: as in `info`; the texts live through the call.
        let info =
            unsafe { (table.panel_info)(table.state, NcStr::new(instance), NcStr::new(path)) };
        look(&info)
    }

    /// `call` on the file `path` of the panel `instance`.
    pub fn file(&self, instance: &str, path: &str, call: FileCall) -> io::Result<FileAnswer> {
        self.quietly(|| self.file_now(instance, path, call))
    }

    fn file_now(&self, instance: &str, path: &str, call: FileCall) -> io::Result<FileAnswer> {
        let (instance, path) = (NcStr::new(instance), NcStr::new(path));
        match call {
            FileCall::Stat => self.stat(instance, path),
            FileCall::Read { offset, size } => self.read(instance, path, offset, size),
            other => {
                let code = self.change(instance, path, other);
                if code == 0 {
                    Ok(FileAnswer::Done)
                } else {
                    Err(error(code, self.last_error()))
                }
            }
        }
    }

    fn stat(&self, instance: NcStr, path: NcStr) -> io::Result<FileAnswer> {
        let table = self.table();
        let mut raw = NcEntry::EMPTY;
        // SAFETY: as in `info`; the texts and the answer's place live through the call; the name is
        // copied before the next call.
        let code = unsafe { (table.stat)(table.state, instance, path, &raw mut raw) };
        if code != 0 {
            return Err(error(code, self.last_error()));
        }
        Ok(FileAnswer::Entry(entry(&raw, 0)))
    }

    fn read(
        &self,
        instance: NcStr,
        path: NcStr,
        offset: u64,
        size: usize,
    ) -> io::Result<FileAnswer> {
        let table = self.table();
        let mut buffer = vec![0; size.min(MOST_READ)];
        let mut done = 0;
        // SAFETY: as in `info`; the buffer and the answer's place are the host's, alive through the
        // call; the plugin writes at most their length.
        let code = unsafe {
            (table.read)(
                table.state,
                instance,
                path,
                offset,
                buffer.as_mut_ptr(),
                buffer.len(),
                &raw mut done,
            )
        };
        if code != 0 {
            return Err(error(code, self.last_error()));
        }
        buffer.truncate(done);
        Ok(FileAnswer::Data(buffer))
    }

    /// A call that changes the panel: its code.
    fn change(&self, instance: NcStr, path: NcStr, call: FileCall) -> i32 {
        let table = self.table();
        let state = table.state;
        // SAFETY (each call): as in `info`; the texts and the data live through the call.
        match call {
            FileCall::Write { append, data } => {
                let mode = if append { WRITE_APPEND } else { WRITE_NEW };
                // SAFETY: as above.
                unsafe { (table.write)(state, instance, path, mode, data.as_ptr(), data.len()) }
            }
            // SAFETY: as above; no data.
            FileCall::End => unsafe {
                (table.write)(state, instance, path, WRITE_END, std::ptr::null(), 0)
            },
            // SAFETY: as above; no data.
            FileCall::Abort => unsafe {
                (table.write)(state, instance, path, WRITE_ABORT, std::ptr::null(), 0)
            },
            // SAFETY: as above.
            FileCall::MakeDir => unsafe { (table.make_dir)(state, instance, path) },
            FileCall::Rename { to, replace } => {
                let to = NcStr::new(&to);
                // SAFETY: as above.
                unsafe { (table.rename)(state, instance, path, to, u32::from(replace)) }
            }
            FileCall::Remove { folder } => {
                let how = if folder { REMOVE_FOLDER } else { REMOVE_FILE };
                // SAFETY: as above.
                unsafe { (table.remove)(state, instance, path, how) }
            }
            FileCall::SetTimes { times, attributes } => {
                let raw = raw_times(&times, attributes);
                // SAFETY: as above; the times live through the call.
                unsafe { (table.set_times)(state, instance, path, &raw const raw) }
            }
            FileCall::Stat | FileCall::Read { .. } => nc_plugin_api::ERROR_UNSUPPORTED,
        }
    }

    /// The panel `instance` is no longer shown.
    pub fn close_panel(&self, instance: &str) {
        let table = self.table();
        // SAFETY: as in `info`; the text lives through the call.
        self.quietly(|| unsafe { (table.close_panel)(table.state, NcStr::new(instance)) });
    }

    /// What the plugin asked for with `NcHost::synchro`: not a quiet call (FAR's `ProcessSynchroEventW`).
    pub fn synchro(&self, param: u64) {
        let table = self.table();
        // SAFETY: as in `info`.
        unsafe { (table.synchro)(table.state, param) };
    }
}

/// A failure's code as the program's errors tell it apart.
fn error(code: i32, message: String) -> io::Error {
    let kind = match code {
        ERROR_NOT_FOUND => io::ErrorKind::NotFound,
        ERROR_DENIED => io::ErrorKind::PermissionDenied,
        ERROR_EXISTS => io::ErrorKind::AlreadyExists,
        ERROR_UNSUPPORTED => io::ErrorKind::Unsupported,
        nc_plugin_api::ERROR_CANCELLED => io::ErrorKind::Interrupted,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, message)
}

/// The plugin's entry as the program's, at its place in the listing: its own attribute bits when it
/// gives them, its flags' otherwise.
pub(super) fn entry(raw: &NcEntry, position: usize) -> Entry {
    let flagged = [
        (ENTRY_DIR, DIRECTORY),
        (ENTRY_HIDDEN, HIDDEN),
        (ENTRY_SYSTEM, SYSTEM),
        (ENTRY_READ_ONLY, READ_ONLY),
    ]
    .iter()
    .filter(|(flag, _)| raw.flags & flag != 0)
    .fold(0, |bits, (_, bit)| bits | bit);
    let folder = flagged & DIRECTORY;
    let bits = if raw.attributes == 0 {
        flagged
    } else {
        (raw.attributes & !DIRECTORY) | folder
    };
    Entry {
        name: text(raw.name),
        is_dir: raw.flags & ENTRY_DIR != 0,
        size: raw.size,
        modified: time(raw.modified),
        created: time(raw.created),
        accessed: time(raw.accessed),
        changed: time(raw.changed),
        attributes: Attributes::from_windows_bits(bits),
        position,
        real_path: real_path(raw.real_path),
        texts: EntryTexts::of(texts(raw.columns, raw.column_count), text(raw.description)),
        allocated: (raw.allocated != SIZE_NONE).then_some(raw.allocated),
        // A plugin's link (its reparse bit) is a link: a move never walks into it and removes what it
        // leads to (R7 review C1).
        link: (bits & REPARSE_POINT != 0).then_some(crate::platform::files::LinkKind::Symlink),
        ..Entry::default()
    }
}

/// As [`texts`], a null text `None` (FAR's null `ColumnTitles[i]`: the program's), an empty one kept.
fn optional_texts(list: *const NcStr, count: usize) -> Vec<Option<String>> {
    if list.is_null() || count == 0 {
        return Vec::new();
    }
    // SAFETY: the plugin hands `count` texts at `list`, valid until its next call; each is copied now.
    let raw = unsafe { std::slice::from_raw_parts(list, count.min(MOST_COLUMNS)) };
    (raw.iter())
        .map(|raw| (!raw.ptr.is_null()).then(|| text(*raw)))
        .collect()
}

/// `count` texts at `list` as the plugin hands them (null: none), at most [`MOST_COLUMNS`].
fn texts(list: *const NcStr, count: usize) -> Vec<String> {
    if list.is_null() || count == 0 {
        return Vec::new();
    }
    // SAFETY: the plugin hands `count` texts at `list`, valid until its next call; each is copied now.
    let raw = unsafe { std::slice::from_raw_parts(list, count.min(MOST_COLUMNS)) };
    raw.iter().map(|raw| text(*raw)).collect()
}

/// The plugin's `SORT_*` of the program's sort (FAR's `SM_*` of a panel's `SortMode`).
pub fn sort_code(mode: crate::vfs::SortMode) -> u32 {
    use crate::vfs::SortMode;
    match mode {
        SortMode::Name | SortMode::NameOnly => nc_plugin_api::SORT_NAME,
        SortMode::Extension => nc_plugin_api::SORT_EXTENSION,
        SortMode::Modified => nc_plugin_api::SORT_MODIFIED,
        SortMode::Created => nc_plugin_api::SORT_CREATED,
        SortMode::Accessed => nc_plugin_api::SORT_ACCESSED,
        SortMode::Size => nc_plugin_api::SORT_SIZE,
        SortMode::Unsorted => nc_plugin_api::SORT_UNSORTED,
        SortMode::Changed => nc_plugin_api::SORT_CHANGED,
        SortMode::Allocated => nc_plugin_api::SORT_ALLOCATED,
        SortMode::Description => nc_plugin_api::SORT_DESCRIPTION,
    }
}

/// The program's sort for the plugin's `SORT_*` (FAR's `StartSortMode`); none: the panel keeps its own.
pub fn start_sort(code: u32) -> Option<crate::vfs::SortMode> {
    use crate::vfs::SortMode;
    Some(match code {
        nc_plugin_api::SORT_NAME => SortMode::Name,
        nc_plugin_api::SORT_EXTENSION => SortMode::Extension,
        nc_plugin_api::SORT_MODIFIED => SortMode::Modified,
        nc_plugin_api::SORT_CREATED => SortMode::Created,
        nc_plugin_api::SORT_ACCESSED => SortMode::Accessed,
        nc_plugin_api::SORT_SIZE => SortMode::Size,
        nc_plugin_api::SORT_UNSORTED => SortMode::Unsorted,
        nc_plugin_api::SORT_CHANGED => SortMode::Changed,
        nc_plugin_api::SORT_ALLOCATED => SortMode::Allocated,
        nc_plugin_api::SORT_DESCRIPTION => SortMode::Description,
        _ => return None,
    })
}

/// The panel's look as the plugin hands it: its title (none when empty), modes, start mode (a key or
/// none) and whether it takes paths.
fn look(info: &NcPanelInfo) -> PanelLook {
    PanelLook {
        title: Some(text(info.title)).filter(|title| !title.is_empty()),
        modes: modes(info),
        start_mode: usize::try_from(info.start_mode)
            .ok()
            .filter(|mode| *mode < MOST_MODES),
        start_sort: start_sort(info.start_sort)
            .map(|mode| (mode, info.start_order == nc_plugin_api::ORDER_DESCENDING)),
        takes_paths: info.flags & PANEL_TAKES_PATHS != 0,
        format: Some(text(info.format)).filter(|format| !format.is_empty()),
        names_only: info.flags & PANEL_NAMES_ONLY != 0,
        right_align_names: info.flags & PANEL_RIGHT_ALIGN_NAMES != 0,
        preserve_case: info.flags & PANEL_PRESERVE_CASE != 0,
        own_get: info.flags & nc_plugin_api::PANEL_OWN_GET != 0,
        own_delete: info.flags & nc_plugin_api::PANEL_OWN_DELETE != 0,
        gives_files: info.flags & nc_plugin_api::PANEL_GIVES_FILES != 0,
        deletes: info.flags & nc_plugin_api::PANEL_DELETES != 0,
        makes_folders: info.flags & nc_plugin_api::PANEL_MAKES_FOLDERS != 0,
        puts_on_save: info.flags & nc_plugin_api::PANEL_PUTS_ON_SAVE != 0,
        keys: keys(info),
        info_lines: info_lines(info),
    }
}

/// The info panel's lines as the plugin hands them, at most [`MOST_INFO_LINES`].
fn info_lines(info: &NcPanelInfo) -> std::sync::Arc<[InfoLine]> {
    if info.info_lines.is_null() || info.info_line_count == 0 {
        return std::sync::Arc::default();
    }
    let count = info.info_line_count.min(MOST_INFO_LINES);
    // SAFETY: the plugin hands `info_line_count` lines at `info_lines`, valid until its next call; each
    // is copied now.
    let raw = unsafe { std::slice::from_raw_parts(info.info_lines, count) };
    (raw.iter())
        .map(|line| InfoLine {
            text: text(line.text),
            data: text(line.data),
            separator: line.flags & nc_plugin_api::INFO_SEPARATOR != 0,
        })
        .collect()
}

/// The panel's own keys as the plugin hands them, at most [`MOST_COLUMNS`].
fn keys(info: &NcPanelInfo) -> Vec<PanelKey> {
    if info.keys.is_null() || info.key_count == 0 {
        return Vec::new();
    }
    // SAFETY: the plugin hands `key_count` keys at `keys`, valid until its next call; each is copied now.
    let raw = unsafe { std::slice::from_raw_parts(info.keys, info.key_count.min(MOST_PANEL_KEYS)) };
    (raw.iter())
        .map(|key| PanelKey {
            key: text(key.key),
            label: text(key.label),
        })
        .filter(|key| !key.key.is_empty())
        .collect()
}

/// The panel's modes as the plugin hands them, at most [`MOST_MODES`].
fn modes(info: &NcPanelInfo) -> std::sync::Arc<[Option<PanelMode>]> {
    if info.modes.is_null() || info.mode_count == 0 {
        return std::sync::Arc::default();
    }
    // SAFETY: the plugin hands `mode_count` modes at `modes`, valid until its next call; each is copied
    // now.
    let raw = unsafe { std::slice::from_raw_parts(info.modes, info.mode_count.min(MOST_MODES)) };
    raw.iter().map(mode).collect()
}

/// One mode; none where its columns are empty (the program's mode there).
fn mode(raw: &NcPanelMode) -> Option<PanelMode> {
    let columns = text(raw.columns);
    if columns.is_empty() {
        return None;
    }
    let flag = |flag: u32| raw.flags & flag != 0;
    Some(PanelMode {
        columns,
        widths: text(raw.widths),
        titles: optional_texts(raw.titles, raw.title_count),
        status_columns: text(raw.status_columns),
        status_widths: text(raw.status_widths),
        full_screen: flag(MODE_FULL_SCREEN),
        detailed_status: flag(MODE_DETAILED_STATUS),
        align_extensions: flag(MODE_ALIGN_EXTENSIONS),
        case_conversion: flag(MODE_CASE_CONVERSION),
    })
}

/// The entry's file on a disk: an absolute path only (another would be taken from the program's folder),
/// without a NUL (R7 review A2: the system's calls stop at it, F8 sent the folder before it to the bin).
fn real_path(raw: NcStr) -> Option<std::path::PathBuf> {
    let text = text(raw);
    let path = std::path::PathBuf::from(&text);
    (path.is_absolute() && !text.contains('\0')).then_some(path)
}

/// The marks of `attributes` as the ABI's entries carry them (not the folder's).
fn marks(attributes: Attributes) -> u32 {
    [
        (attributes.is_hidden(), ENTRY_HIDDEN),
        (attributes.is_system(), ENTRY_SYSTEM),
        (attributes.is_read_only(), ENTRY_READ_ONLY),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .fold(0, |flags, (_, flag)| flags | flag)
}

/// What an object put in keeps, as the ABI hands it to a plugin.
pub(super) fn raw_times(times: &FileTimes, attributes: Option<Attributes>) -> NcTimes {
    NcTimes {
        modified: millis(times.modified),
        created: millis(times.created),
        accessed: millis(times.accessed),
        changed: millis(times.changed),
        flags: attributes.map_or(0, marks),
        flags_given: u32::from(attributes.is_some()),
        attributes: attributes.map_or(0, Attributes::windows_flags),
    }
}

/// A found object as the ABI hands it to a plugin, its texts `name` and `real` (kept by the caller).
pub(super) fn raw_entry(entry: &Entry, name: &str, real: &str) -> NcEntry {
    let folder = if entry.is_dir { ENTRY_DIR } else { 0 };
    NcEntry {
        name: NcStr::new(name),
        flags: folder | marks(entry.attributes),
        size: entry.size,
        modified: millis(entry.modified),
        created: millis(entry.created),
        accessed: millis(entry.accessed),
        changed: millis(entry.changed),
        attributes: entry.attributes.windows_flags(),
        real_path: NcStr::new(real),
        allocated: entry.allocated.unwrap_or(SIZE_NONE),
        ..NcEntry::EMPTY
    }
}

/// A time as the ABI carries it: milliseconds from 1970, before it negative.
pub(super) fn millis(time: Option<SystemTime>) -> i64 {
    let Some(time) = time else {
        return TIME_NONE;
    };
    let whole = |distance: Duration| i64::try_from(distance.as_millis()).unwrap_or(i64::MAX);
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => whole(after),
        Err(before) => -whole(before.duration()),
    }
}

/// A time from the ABI's milliseconds from 1970.
fn time(millis: i64) -> Option<SystemTime> {
    if millis == TIME_NONE {
        return None;
    }
    let distance = Duration::from_millis(millis.unsigned_abs());
    if millis < 0 {
        UNIX_EPOCH.checked_sub(distance)
    } else {
        UNIX_EPOCH.checked_add(distance)
    }
}

#[cfg(test)]
#[path = "panel_calls_tests.rs"]
mod tests;
