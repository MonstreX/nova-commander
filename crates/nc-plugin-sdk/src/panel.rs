//! A plugin's panels (ADR-0016 §13): the folders of a panel instance by path, as the program's providers
//! answer them, its title, its end. The calls come on the host's thread for the plugin while the user
//! works on; a panel's call never shows windows.

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::api::{
    ENTRY_DIR, ENTRY_HIDDEN, ENTRY_READ_ONLY, ENTRY_SYSTEM, ERROR_CANCELLED, ERROR_DENIED,
    ERROR_EXISTS, ERROR_NOT_FOUND, ERROR_OTHER, ERROR_UNSUPPORTED, MODE_ALIGN_EXTENSIONS,
    MODE_CASE_CONVERSION, MODE_DETAILED_STATUS, MODE_FULL_SCREEN, NcEntry, NcPanelInfo,
    NcPanelMode, NcStr, NcTimes, PANEL_NAMES_ONLY, PANEL_PRESERVE_CASE, PANEL_RIGHT_ALIGN_NAMES,
    PANEL_TAKES_PATHS, START_MODE_NONE, TIME_NONE,
};
use crate::panels::time;
use crate::{Kept, Plugin, panic_text, state};

/// The most modes read: Ctrl+0..9.
const MOST_MODES: usize = 10;

/// What kind of failure a panel's call reports: the program tells these apart as it does a disk's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Other,
    NotFound,
    Denied,
    Exists,
    Unsupported,
    /// The user stopped it (Esc at a question): nothing done.
    Cancelled,
}

/// A panel's call failed: its kind and the text the user reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn code(&self) -> i32 {
        match self.kind {
            ErrorKind::Other => ERROR_OTHER,
            ErrorKind::NotFound => ERROR_NOT_FOUND,
            ErrorKind::Denied => ERROR_DENIED,
            ErrorKind::Exists => ERROR_EXISTS,
            ErrorKind::Unsupported => ERROR_UNSUPPORTED,
            ErrorKind::Cancelled => ERROR_CANCELLED,
        }
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::new(ErrorKind::Other, message)
    }
}

/// How [`Plugin::write`](crate::Plugin::write) writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    New,
    Append,
}

/// One entry of a plugin's panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    /// When its data or attributes last changed (FAR's `ChangeTime`).
    pub changed: Option<SystemTime>,
    pub hidden: bool,
    pub system: bool,
    pub read_only: bool,
    /// The OS's own attribute bits (Windows' `FILE_ATTRIBUTE_*`, FAR's `FileAttributes`; with `std`'s
    /// `MetadataExt::file_attributes`); 0: as the flags above say.
    pub attributes: u32,
    /// The file on a disk the entry is (FAR's real names): viewed, edited, run, copied and moved as that
    /// file by the program; deleting it still asks the plugin.
    pub real_path: Option<std::path::PathBuf>,
    /// The texts of the panel modes' columns `C0`, `C1`... (FAR's `CustomColumnData`).
    pub columns: Vec<String>,
    /// The text of the column `Z` (FAR's `Description`).
    pub description: String,
    /// The space it takes where it is (FAR's `AllocationSize`: an archive's packed size), shown and
    /// sorted as a disk's allocated size; `None`: not told.
    pub allocated: Option<u64>,
}

/// The marks a file or folder put into a panel keeps ([`Times::attributes`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Attributes {
    pub hidden: bool,
    pub system: bool,
    pub read_only: bool,
    /// The source's own attribute bits (Windows' `FILE_ATTRIBUTE_*`); 0: not told.
    pub bits: u32,
}

/// What a file or folder put into its panel keeps of its source ([`Plugin::set_times`](crate::Plugin::set_times)):
/// each time `None` when not given; its marks when given (each one not set cleared).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Times {
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    /// When its data or attributes last changed (FAR's `ChangeTime`).
    pub changed: Option<SystemTime>,
    pub attributes: Option<Attributes>,
}

impl Times {
    /// The host's times as the plugin reads them.
    pub(crate) fn of(raw: &NcTimes) -> Self {
        let flag = |flag: u32| raw.flags & flag != 0;
        Self {
            modified: time(raw.modified),
            created: time(raw.created),
            accessed: time(raw.accessed),
            changed: time(raw.changed),
            attributes: (raw.flags_given != 0).then(|| Attributes {
                hidden: flag(ENTRY_HIDDEN),
                system: flag(ENTRY_SYSTEM),
                read_only: flag(ENTRY_READ_ONLY),
                bits: raw.attributes,
            }),
        }
    }
}

/// How a panel is shown and filled (FAR's `OPIF_*`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PanelFlags {
    /// Copying or moving into it hands the plugin the sources' paths ([`Plugin::put`](crate::Plugin::put)),
    /// not their data: a panel of references, as FAR's TmpPanel.
    pub takes_paths: bool,
    /// The program's modes show the entries' names without their folders (`OPIF_SHOWNAMESONLY`).
    pub names_only: bool,
    /// The program's modes keep long names' ends (`OPIF_SHOWRIGHTALIGNNAMES`).
    pub right_align_names: bool,
    /// The program's modes show names with their cases as given (`OPIF_SHOWPRESERVECASE`).
    pub preserve_case: bool,
    /// With real names, copying out and the quick view still go through the plugin ([`Plugin::read`](crate::Plugin::read);
    /// FAR's plugin with its own `GetFiles`); off: the program takes the real files.
    pub own_get: bool,
    /// With real names, deleting still goes through the plugin ([`Plugin::remove`](crate::Plugin::remove);
    /// FAR's plugin with its own `DeleteFiles`); off: the program deletes the real files.
    pub own_delete: bool,
    /// The plugin writes its files out itself ([`Plugin::get`](crate::Plugin::get), FAR's `GetFilesW`): F5 /
    /// F6 to a disk come to it, F3 / F4 have it write the file first.
    pub gives_files: bool,
    /// F8 hands the entries to [`Plugin::delete_files`](crate::Plugin::delete_files) (FAR's
    /// `DeleteFilesW`): the plugin asks itself, the program shows no question.
    pub deletes: bool,
    /// F7 calls [`Plugin::make_folder`](crate::Plugin::make_folder) with no name (FAR's
    /// `MakeDirectoryW`): the plugin asks the name itself.
    pub makes_folders: bool,
    /// A file edited after F4 goes back ([`Plugin::put`](crate::Plugin::put) with
    /// [`GetMode::edit`](crate::GetMode)) at each save in the editor, not only once the editor is left
    /// (FAR's NetBox's upload at `EE_SAVE`); with [`PanelFlags::takes_paths`].
    pub puts_on_save: bool,
}

impl PanelFlags {
    fn bits(self) -> u32 {
        [
            (self.takes_paths, PANEL_TAKES_PATHS),
            (self.names_only, PANEL_NAMES_ONLY),
            (self.right_align_names, PANEL_RIGHT_ALIGN_NAMES),
            (self.preserve_case, PANEL_PRESERVE_CASE),
            (self.own_get, crate::api::PANEL_OWN_GET),
            (self.own_delete, crate::api::PANEL_OWN_DELETE),
            (self.gives_files, crate::api::PANEL_GIVES_FILES),
            (self.deletes, crate::api::PANEL_DELETES),
            (self.makes_folders, crate::api::PANEL_MAKES_FOLDERS),
            (self.puts_on_save, crate::api::PANEL_PUTS_ON_SAVE),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .fold(0, |bits, (_, bit)| bits | bit)
    }
}

/// One of a panel's own view modes (FAR's `PanelMode`), in the program's panel-mode strings (Options >
/// File panel modes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelMode {
    /// The column types: `N,S,D,T` and the like; `C0`..`C99` the entries' `columns`, `Z` their
    /// description. Empty: the program's mode.
    pub columns: String,
    /// The columns' widths: `0,10,15%`.
    pub widths: String,
    /// The columns' titles in their order (FAR's `ColumnTitles`): `None` (or none at all) the program's,
    /// `Some` shown as it is, an empty one too.
    pub titles: Vec<Option<String>>,
    /// The status line's column types and widths; empty: the name, or with `detailed_status` the name,
    /// size, date and time.
    pub status_columns: String,
    pub status_widths: String,
    /// The panel takes the whole screen.
    pub full_screen: bool,
    pub detailed_status: bool,
    /// Files' extensions in a column of their own.
    pub align_extensions: bool,
    /// Names' cases changed as the program's mode at that key says; off: names as given.
    pub case_conversion: bool,
}

impl PanelMode {
    fn flags(&self) -> u32 {
        [
            (self.full_screen, MODE_FULL_SCREEN),
            (self.detailed_status, MODE_DETAILED_STATUS),
            (self.align_extensions, MODE_ALIGN_EXTENSIONS),
            (self.case_conversion, MODE_CASE_CONVERSION),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .fold(0, |flags, (_, flag)| flags | flag)
    }
}

/// `mode` as the ABI carries it (none: an empty one), its texts kept until the plugin's next call.
fn raw_mode(mode: Option<PanelMode>, kept: &mut Kept) -> NcPanelMode {
    let Some(mode) = mode.filter(|mode| !mode.columns.is_empty()) else {
        return NcPanelMode::EMPTY;
    };
    let flags = mode.flags();
    let (titles, title_count) = kept.optional_list(mode.titles);
    NcPanelMode {
        columns: kept.text(mode.columns),
        widths: kept.text(mode.widths),
        titles,
        title_count,
        status_columns: kept.text(mode.status_columns),
        status_widths: kept.text(mode.status_widths),
        flags,
    }
}

impl FileEntry {
    pub fn file(name: impl Into<String>, size: u64) -> Self {
        Self {
            name: name.into(),
            size,
            ..Self::default()
        }
    }

    pub fn folder(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            is_dir: true,
            ..Self::default()
        }
    }

    fn flags(&self) -> u32 {
        [
            (self.is_dir, ENTRY_DIR),
            (self.hidden, ENTRY_HIDDEN),
            (self.system, ENTRY_SYSTEM),
            (self.read_only, ENTRY_READ_ONLY),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .fold(0, |flags, (_, flag)| flags | flag)
    }
}

/// `entry` as the ABI carries it, its texts kept until the plugin's next call.
pub(crate) fn raw_entry(entry: FileEntry, kept: &mut Kept) -> NcEntry {
    let flags = entry.flags();
    // A path that is no exact text is none (R7 review A8: its file acted on as a sibling with U+FFFD).
    let real_path = (entry.real_path.as_ref())
        .and_then(|path| path.to_str())
        .map_or(NcStr::EMPTY, |path| kept.text(path.to_string()));
    let (columns, column_count) = kept.list(entry.columns);
    let description = if entry.description.is_empty() {
        NcStr::EMPTY
    } else {
        kept.text(entry.description)
    };
    NcEntry {
        real_path,
        flags,
        size: entry.size,
        modified: millis(entry.modified),
        created: millis(entry.created),
        accessed: millis(entry.accessed),
        changed: millis(entry.changed),
        attributes: entry.attributes,
        name: kept.text(entry.name),
        columns,
        column_count,
        description,
        allocated: entry.allocated.unwrap_or(crate::api::SIZE_NONE),
    }
}

/// A line of the info panel's "Plugin" section ([`crate::Plugin::panel_info_lines`], FAR's
/// `InfoPanelLine`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InfoLine {
    /// At the left; a separator's title.
    pub text: String,
    /// At the right in the info colour.
    pub data: String,
    /// A separator with `text` in its middle (FAR's `IPLFLAGS_SEPARATOR`).
    pub separator: bool,
}

impl InfoLine {
    /// `text` at the left, `data` at the right.
    pub fn new(text: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            data: data.into(),
            separator: false,
        }
    }

    /// A separator with `title` in its middle (empty: none).
    pub fn separator(title: impl Into<String>) -> Self {
        Self {
            text: title.into(),
            data: String::new(),
            separator: true,
        }
    }
}

/// A time as the ABI carries it: milliseconds from 1970, before it negative.
pub(crate) fn millis(time: Option<SystemTime>) -> i64 {
    let Some(time) = time else {
        return TIME_NONE;
    };
    let whole = |duration: Duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX);
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => whole(after),
        Err(before) => -whole(before.duration()),
    }
}

/// The host's text for the call (it sends UTF-8 and keeps it alive through the call).
///
/// # Safety
/// `text` is the host's, valid through the call.
pub(crate) unsafe fn host_text<'a>(text: NcStr) -> &'a str {
    // SAFETY: the caller promises the host's live UTF-8 text.
    unsafe { text.as_str() }
}

pub(crate) unsafe extern "C" fn list<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
    entries: *mut *const NcEntry,
    count: *mut usize,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    state.entries.clear();
    // SAFETY: the host's texts live through the call.
    let (instance, path) = unsafe { (host_text(instance), host_text(path)) };
    let listed = match catch_unwind(AssertUnwindSafe(|| state.plugin.list(instance, path))) {
        Ok(Ok(listed)) => listed,
        Ok(Err(error)) => {
            state.error = error.message.clone();
            return error.code();
        }
        Err(_) => {
            state.error = panic_text();
            return ERROR_OTHER;
        }
    };
    let mut made = Vec::with_capacity(listed.len());
    for entry in listed {
        made.push(raw_entry(entry, &mut state.kept));
    }
    state.entries = made;
    if !entries.is_null() && !count.is_null() {
        // SAFETY: the host's places for the answer, alive through the call; the entries and their names
        // are kept until the plugin's next call.
        unsafe {
            *entries = state.entries.as_ptr();
            *count = state.entries.len();
        }
    }
    0
}

pub(crate) unsafe extern "C" fn panel_info<P: Plugin>(
    raw: *mut c_void,
    instance: NcStr,
    path: NcStr,
) -> NcPanelInfo {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    state.modes.clear();
    // SAFETY: the host's texts live through the call.
    let (instance, path) = unsafe { (host_text(instance), host_text(path)) };
    let plugin = &state.plugin;
    let title =
        catch_unwind(AssertUnwindSafe(|| plugin.panel_title(instance, path))).unwrap_or_default();
    let mut modes =
        catch_unwind(AssertUnwindSafe(|| plugin.panel_modes(instance, path))).unwrap_or_default();
    let start_mode = catch_unwind(AssertUnwindSafe(|| plugin.panel_start_mode(instance, path)))
        .ok()
        .flatten()
        .filter(|mode| *mode < MOST_MODES)
        .and_then(|mode| i32::try_from(mode).ok())
        .unwrap_or(START_MODE_NONE);
    let start_sort = catch_unwind(AssertUnwindSafe(|| plugin.panel_start_sort(instance, path)))
        .ok()
        .flatten();
    let flags = catch_unwind(AssertUnwindSafe(|| plugin.panel_flags(instance))).unwrap_or_default();
    let format =
        catch_unwind(AssertUnwindSafe(|| plugin.panel_format(instance, path))).unwrap_or_default();
    let keys = catch_unwind(AssertUnwindSafe(|| plugin.panel_keys(instance))).unwrap_or_default();
    let lines = catch_unwind(AssertUnwindSafe(|| plugin.panel_info_lines(instance, path)))
        .unwrap_or_default();
    state.info_lines = (lines.into_iter())
        .map(|line| crate::api::NcInfoLine {
            text: state.kept.text(line.text),
            data: state.kept.text(line.data),
            flags: if line.separator {
                crate::api::INFO_SEPARATOR
            } else {
                0
            },
        })
        .collect();
    state.keys = (keys.into_iter())
        .map(|key| crate::api::NcPanelKey {
            key: state.kept.text(key.key),
            label: state.kept.text(key.label),
        })
        .collect();
    modes.truncate(MOST_MODES);
    let made: Vec<NcPanelMode> = (modes.into_iter())
        .map(|mode| raw_mode(mode, &mut state.kept))
        .collect();
    state.modes = made;
    NcPanelInfo {
        title: state.keep(title),
        modes: if state.modes.is_empty() {
            std::ptr::null()
        } else {
            state.modes.as_ptr()
        },
        mode_count: state.modes.len(),
        flags: flags.bits(),
        start_mode,
        start_sort: start_sort.map_or(crate::api::SORT_KEEP, |(by, _)| by.code()),
        start_order: if start_sort.is_some_and(|(_, descending)| descending) {
            crate::api::ORDER_DESCENDING
        } else {
            crate::api::ORDER_ASCENDING
        },
        format: state.kept.text(format),
        keys: if state.keys.is_empty() {
            std::ptr::null()
        } else {
            state.keys.as_ptr()
        },
        key_count: state.keys.len(),
        info_lines: if state.info_lines.is_empty() {
            std::ptr::null()
        } else {
            state.info_lines.as_ptr()
        },
        info_line_count: state.info_lines.len(),
    }
}

pub(crate) unsafe extern "C" fn close_panel<P: Plugin>(raw: *mut c_void, instance: NcStr) {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    // SAFETY: the host's text lives through the call.
    let instance = unsafe { host_text(instance) };
    // A panic closing is the plugin's own: the panel is gone either way.
    let _ = catch_unwind(AssertUnwindSafe(|| state.plugin.close_panel(instance)));
}

/// A sort a plugin's panel opens with ([`crate::Plugin::panel_start_sort`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Extension,
    Modified,
    Created,
    Accessed,
    Size,
    /// As the plugin lists them.
    Unsorted,
    /// By the entries' descriptions (FAR's `SM_DESCR`).
    Description,
}

impl SortBy {
    /// The ABI's `SORT_*`.
    pub fn code(self) -> u32 {
        match self {
            Self::Name => crate::api::SORT_NAME,
            Self::Extension => crate::api::SORT_EXTENSION,
            Self::Modified => crate::api::SORT_MODIFIED,
            Self::Created => crate::api::SORT_CREATED,
            Self::Accessed => crate::api::SORT_ACCESSED,
            Self::Size => crate::api::SORT_SIZE,
            Self::Unsorted => crate::api::SORT_UNSORTED,
            Self::Description => crate::api::SORT_DESCRIPTION,
        }
    }
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "panel_refs_tests.rs"]
mod refs_tests;
