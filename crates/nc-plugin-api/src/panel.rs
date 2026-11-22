//! A plugin's panels in the ABI (ADR-0016 §13-15): its entries, what the panel shows besides them (its
//! title, modes, keys, flags), its own keys' events and answers.

use crate::NcStr;

/// [`NcEntry::flags`]: a folder.
pub const ENTRY_DIR: u32 = 1;
pub const ENTRY_HIDDEN: u32 = 2;
pub const ENTRY_SYSTEM: u32 = 4;
pub const ENTRY_READ_ONLY: u32 = 8;

/// A time not known ([`NcEntry::modified`] and the others).
pub const TIME_NONE: i64 = i64::MIN;

/// A size not known ([`NcEntry::allocated`]).
pub const SIZE_NONE: u64 = u64::MAX;

/// One entry of a plugin's panel; times in milliseconds since 1970-01-01 UTC, or [`TIME_NONE`].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcEntry {
    pub name: NcStr,
    /// `ENTRY_*`.
    pub flags: u32,
    pub size: u64,
    pub modified: i64,
    pub created: i64,
    pub accessed: i64,
    /// When its data or attributes last changed (FAR's `ChangeTime`).
    pub changed: i64,
    /// The OS's own attribute bits (Windows' `FILE_ATTRIBUTE_*`, FAR's `FileAttributes`); 0: as the
    /// `ENTRY_*` flags say.
    pub attributes: u32,
    /// The file on a disk the entry is (FAR's `PFLAGS_REALNAMES`): an absolute path, or empty for none.
    /// Viewing, editing, running, copying and moving it act on that file, never through the plugin;
    /// deleting it asks the plugin.
    pub real_path: NcStr,
    /// `column_count` texts for the panel modes' columns `C0`, `C1`... (FAR's `CustomColumnData`).
    pub columns: *const NcStr,
    pub column_count: usize,
    /// The text of the column `Z` (FAR's `Description`).
    pub description: NcStr,
    /// The space it takes where it is (FAR's `AllocationSize`: an archive's packed size), shown and
    /// sorted as a disk's allocated size; [`SIZE_NONE`] when not told (API 5).
    pub allocated: u64,
}

impl NcEntry {
    /// An entry of nothing: no name, no flags, no times, no texts.
    pub const EMPTY: Self = Self {
        name: NcStr::EMPTY,
        flags: 0,
        size: 0,
        modified: TIME_NONE,
        created: TIME_NONE,
        accessed: TIME_NONE,
        changed: TIME_NONE,
        attributes: 0,
        real_path: NcStr::EMPTY,
        columns: core::ptr::null(),
        column_count: 0,
        description: NcStr::EMPTY,
        allocated: SIZE_NONE,
    };
}

/// What a file or folder put into a plugin's panel keeps of its source ([`NcPlugin::set_times`](crate::NcPlugin::set_times),
/// FAR's `PutFiles` items' times and `FileAttributes`): its times in milliseconds since 1970-01-01 UTC,
/// [`TIME_NONE`] for a time not given; with a non-zero `flags_given` its `ENTRY_HIDDEN`, `ENTRY_SYSTEM`
/// and `ENTRY_READ_ONLY` as `flags` say (each one not there cleared).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcTimes {
    pub modified: i64,
    pub created: i64,
    pub accessed: i64,
    /// The time its data or attributes last changed (FAR's `ChangeTime`).
    pub changed: i64,
    pub flags: u32,
    pub flags_given: u32,
    /// The source's own attribute bits (Windows' `FILE_ATTRIBUTE_*`), given with the flags; 0: none told.
    pub attributes: u32,
}

impl NcTimes {
    /// Nothing given: no times, no flags.
    pub const NONE: Self = Self {
        modified: TIME_NONE,
        created: TIME_NONE,
        accessed: TIME_NONE,
        changed: TIME_NONE,
        flags: 0,
        flags_given: 0,
        attributes: 0,
    };
}

/// What a plugin's panel shows besides its entries.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcPanelInfo {
    /// The panel's title (FAR's `PanelTitle`); empty: the panel's path.
    pub title: NcStr,
    /// The panel's own view modes (FAR's `PanelModesArray`): `mode_count` of them at `modes`, the one at
    /// `n` for Ctrl+`n` (0..9).
    pub modes: *const NcPanelMode,
    pub mode_count: usize,
    /// `PANEL_*`.
    pub flags: u32,
    /// The mode the panel opens in (FAR's `StartPanelMode`): Ctrl+`n`'s `n`, or [`START_MODE_NONE`].
    pub start_mode: i32,
    /// The sort the panel opens with (FAR's `StartSortMode`, API 2): `SORT_*`; [`SORT_KEEP`]: the
    /// panel's own.
    pub start_sort: u32,
    /// Its order (FAR's `StartSortOrder`): `ORDER_*`, read with a sort only.
    pub start_order: u32,
    /// The panel's kind as the copy dialog's target names it: `<format>:` (FAR's `Format`); empty: `::`.
    pub format: NcStr,
    /// `key_count` keys of the panel's own (FAR's `ProcessPanelInput` keys and `KeyBar`), sent to
    /// [`NcPlugin::panel_key`] instead of the program's.
    pub keys: *const NcPanelKey,
    pub key_count: usize,
    /// `info_line_count` lines the info panel beside it shows in its "Plugin" section (FAR's
    /// `InfoLines`, API 3).
    pub info_lines: *const NcInfoLine,
    pub info_line_count: usize,
}

/// A line of the info panel's "Plugin" section (FAR's `InfoPanelLine`): `text` at the left, `data` at
/// the right in the info colour; with [`INFO_SEPARATOR`] a separator with `text` in its middle.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcInfoLine {
    pub text: NcStr,
    pub data: NcStr,
    pub flags: u32,
}

/// [`NcInfoLine::flags`]: a separator (FAR's `IPLFLAGS_SEPARATOR`).
pub const INFO_SEPARATOR: u32 = 1;

/// [`NcPanelInfo::start_sort`]: the panel keeps its sort.
pub const SORT_KEEP: u32 = 0;
pub const SORT_NAME: u32 = 1;
pub const SORT_EXTENSION: u32 = 2;
pub const SORT_MODIFIED: u32 = 3;
pub const SORT_CREATED: u32 = 4;
pub const SORT_ACCESSED: u32 = 5;
pub const SORT_SIZE: u32 = 6;
pub const SORT_UNSORTED: u32 = 7;
/// By the change time of data or attributes (FAR's `SM_CHTIME`).
pub const SORT_CHANGED: u32 = 8;
/// By the space on the disk (FAR's `SM_COMPRESSEDSIZE`).
pub const SORT_ALLOCATED: u32 = 9;
/// By the entries' descriptions (FAR's `SM_DESCR`, Ctrl+F10), names breaking ties.
pub const SORT_DESCRIPTION: u32 = 10;

/// [`NcPanelInfo::start_order`].
pub const ORDER_ASCENDING: u32 = 0;
pub const ORDER_DESCENDING: u32 = 1;

/// A key of a plugin's panel: its name as the program writes keys ("F7", "Alt+Shift+F2",
/// "Alt+Shift+0") and its key bar label (empty: the program's stays).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcPanelKey {
    pub key: NcStr,
    pub label: NcStr,
}

/// A key pressed on a plugin's panel ([`NcPlugin::panel_key`]).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcKeyEvent {
    /// The key's name as the panel named it.
    pub key: NcStr,
    /// The entry under the cursor (its name; empty on "..").
    pub current: NcStr,
    /// `selected_count` names selected; none: the current one is meant.
    pub selected: *const NcStr,
    pub selected_count: usize,
    /// The other panel's folder on a disk; empty: none.
    pub passive: NcStr,
    /// The program's folder for names given without one: the disk's folder the panel's plugin was
    /// opened from (FAR's current folder, which a plugin's panel never changes); empty: none.
    pub folder: NcStr,
}

/// What the program does after [`NcPlugin::panel_key`], [`NcPlugin::open_command`] or
/// [`NcPlugin::open_file`]: `KEY_*` flags with their `path`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcKeyAnswer {
    pub flags: u32,
    pub path: NcStr,
}

impl NcKeyAnswer {
    pub const NONE: Self = Self {
        flags: 0,
        path: NcStr::EMPTY,
    };
}

/// [`NcKeyAnswer::flags`]: the panel read again (FAR's `FCTL_UPDATEPANEL`).
pub const KEY_REREAD: u32 = 1;
/// The other panel goes to the folder `path` (FAR's `FCTL_SETPANELDIRECTORY` on `PANEL_PASSIVE`).
pub const KEY_PASSIVE_FOLDER: u32 = 2;
/// The other panel goes to the folder of the file `path`, its cursor on it (TmpPanel's `GoToFile`).
pub const KEY_PASSIVE_FILE: u32 = 4;
/// The panel shows the plugin's panel instance `path` instead (TmpPanel's switch of panels).
pub const KEY_OPEN: u32 = 8;
/// The other panel read again.
pub const KEY_REREAD_PASSIVE: u32 = 16;
/// The panel the key was pressed on goes to the folder `path` on a disk, leaving the plugin's panels (FAR's
/// `FCTL_SETPANELDIRECTORY` on `PANEL_ACTIVE`).
pub const KEY_ACTIVE_FOLDER: u32 = 32;
/// The panel the key was pressed on goes to the folder of the file `path`, its cursor on it (TmpPanel's
/// `GoToFile` on the active panel).
pub const KEY_ACTIVE_FILE: u32 = 64;
/// The command line's text becomes `path`, an empty one too (FAR's `FCTL_SETCMDLINE`).
pub const KEY_COMMAND_LINE: u32 = 128;
/// The plugin leaves the key to the program: it does what the key does on any panel, never sending it to
/// the plugin again (FAR's `ProcessPanelInput` returning FALSE). The other flags are not read then.
pub const KEY_PASS: u32 = 256;
/// The other panel, when it is a quick view, shows its file again (TmpPanel's F7, `PTYPE_QVIEWPANEL`).
pub const KEY_REREAD_QUICK_VIEW: u32 = 512;
/// The program opens the file or folder `path` with the system (FAR's `ShellExecute` "open", TmpPanel's
/// menu from a list).
pub const KEY_SHELL_OPEN: u32 = 1024;

/// [`NcPanelInfo::flags`]: copying or moving into the panel hands the plugin the sources' paths
/// ([`NcPlugin::put`]), not their data.
pub const PANEL_TAKES_PATHS: u32 = 1;
/// The program's modes show the entries' names without their folders (FAR's `OPIF_SHOWNAMESONLY`).
pub const PANEL_NAMES_ONLY: u32 = 2;
/// The program's modes keep long names' ends (FAR's `OPIF_SHOWRIGHTALIGNNAMES`).
pub const PANEL_RIGHT_ALIGN_NAMES: u32 = 4;
/// The program's modes show names as given, their cases unchanged (FAR's `OPIF_SHOWPRESERVECASE`).
pub const PANEL_PRESERVE_CASE: u32 = 8;

/// [`NcPanelInfo::start_mode`]: the panel keeps the mode it has.
pub const START_MODE_NONE: i32 = -1;

/// [`NcPanelMode::flags`]: the panel takes the whole screen (FAR's `PMFLAGS_FULLSCREEN`).
pub const MODE_FULL_SCREEN: u32 = 1;
/// The status line shows the name, size, date and time when the mode gives no status columns.
pub const MODE_DETAILED_STATUS: u32 = 2;
/// Files' extensions in a column of their own.
pub const MODE_ALIGN_EXTENSIONS: u32 = 4;
/// Names' cases changed as the program's mode at that key says; without it names are as given.
pub const MODE_CASE_CONVERSION: u32 = 8;

/// One view mode of a plugin's panel (FAR's `PanelMode`), in the program's panel-mode strings.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcPanelMode {
    /// The column types (`N,S,D,T`; `C0`..`C99` the entries' `columns`, `Z` their description); empty: no
    /// mode of the plugin's at this key, the program's stands.
    pub columns: NcStr,
    /// The columns' widths (`0,10,15%`).
    pub widths: NcStr,
    /// `title_count` titles of the columns in their order (FAR's `ColumnTitles`): a null one the
    /// program's, any other (an empty one too) shown as it is.
    pub titles: *const NcStr,
    pub title_count: usize,
    /// The status line's column types and widths; empty: by [`MODE_DETAILED_STATUS`].
    pub status_columns: NcStr,
    pub status_widths: NcStr,
    /// `MODE_*`.
    pub flags: u32,
}

impl NcPanelMode {
    /// No mode: the program's.
    pub const EMPTY: Self = Self {
        columns: NcStr::EMPTY,
        widths: NcStr::EMPTY,
        titles: core::ptr::null(),
        title_count: 0,
        status_columns: NcStr::EMPTY,
        status_widths: NcStr::EMPTY,
        flags: 0,
    };
}

/// [`NcPanelInfo::flags`]: with real names, copying out of the panel and the quick view go through the plugin
/// (FAR's plugin with `GetFiles` and no `OPIF_EXTERNALGET`); without it the program takes the real files.
pub const PANEL_OWN_GET: u32 = 16;
/// With real names, deleting goes through the plugin (FAR's plugin with `DeleteFiles` and no
/// `OPIF_EXTERNALDELETE`); without it the program deletes the real files.
pub const PANEL_OWN_DELETE: u32 = 32;
/// The plugin writes its files out itself (FAR's `GetFilesW`, API 5): copying or moving out of the panel
/// to a disk (F5, F6) hands it the items and the folder ([`NcPlugin::get`](crate::NcPlugin::get)), which
/// shows its own dialog; viewing or editing a file (F3, F4) has it write the file into a folder of the
/// program's first. Without it the program copies out through `read`.
pub const PANEL_GIVES_FILES: u32 = 64;
/// F8 (Del, Shift+Del, Shift+F8) on the panel hands the entries to the plugin's
/// [`NcPlugin::delete_files`](crate::NcPlugin::delete_files) (FAR's `DeleteFilesW`, API 5): the plugin
/// asks itself, the program shows no question. Without it the program asks, then removes each entry.
pub const PANEL_DELETES: u32 = 128;
/// F7 on the panel calls the plugin's [`NcPlugin::make_folder`](crate::NcPlugin::make_folder) with no
/// name (FAR's `MakeDirectoryW`, API 5): the plugin asks the name itself. Without it the program asks,
/// then makes the folder through `make_dir`.
pub const PANEL_MAKES_FOLDERS: u32 = 256;
/// A file of the panel edited after F4 goes back at each save in the editor, not only once the editor
/// is left (API 8; FAR's NetBox's upload at `EE_SAVE`): with [`PANEL_TAKES_PATHS`] the program hands
/// the saved copy to [`NcPlugin::put`](crate::NcPlugin::put) with `GET_EDIT` while the editor stays.
pub const PANEL_PUTS_ON_SAVE: u32 = 512;

/// [`NcPlugin::get`]'s `mode` (FAR's `OPM_*` of `GetFilesW`): no windows but the errors' (`OPM_SILENT`).
///
/// [`NcPlugin::get`]: crate::NcPlugin::get
pub const GET_SILENT: u32 = 0x1;
/// For Find file's search in the files (`OPM_FIND`): no windows at all.
pub const GET_FIND: u32 = 0x2;
/// For the viewer (`OPM_VIEW`): one file into a folder of the program's, no dialog.
pub const GET_VIEW: u32 = 0x4;
/// For the editor (`OPM_EDIT`), as for the viewer.
pub const GET_EDIT: u32 = 0x8;
/// Archives of a disk's panel at once (`OPM_TOPLEVEL`: Shift+F2).
pub const GET_TOP_LEVEL: u32 = 0x10;
/// For the quick view (`OPM_QUICKVIEW`): no windows at all.
pub const GET_QUICK_VIEW: u32 = 0x40;

/// [`NcPlugin::open_file`]'s `alternative`: why the file is offered (FAR's `OPENFILEPLUGINTYPE`). Enter
/// (`OFP_NORMAL`).
///
/// [`NcPlugin::open_file`]: crate::NcPlugin::open_file
pub const OPEN_ENTER: u32 = 0;
/// Ctrl+PgDn (`OFP_ALTERNATIVE`, FAR's `OPM_PGDN`).
pub const OPEN_PGDN: u32 = 1;
/// Shift+F2 on a disk's files (`OFP_EXTRACT`, API 5): a panel opened is read at its root and its entries
/// got with [`GET_TOP_LEVEL`], then closed.
pub const OPEN_EXTRACT: u32 = 2;
/// Shift+F3 on a disk's files (`OFP_COMMANDS`, FAR's `OPM_COMMANDS`, API 5): a panel opened is read at
/// its root and its entries handed to its `process_host_file` with [`GET_TOP_LEVEL`], then closed.
pub const OPEN_COMMANDS: u32 = 3;
/// Alt+F7's "Search in archives" (FAR's `OFP_SEARCH` with `OPM_FIND`, API 6): the file opened on a panel
/// without any window (no progress, no password, no menu, no error), from the search's thread; a panel
/// opened is walked by the search, then closed. A file it cannot open so: [`KEY_PASS`](crate::KEY_PASS).
pub const OPEN_SEARCH: u32 = 4;

/// [`NcPlugin::process_host_file`]'s `*done`: the plugin changed something (FAR's `ProcessHostFileW`
/// returning TRUE): the selection goes, both panels are read again.
///
/// [`NcPlugin::process_host_file`]: crate::NcPlugin::process_host_file
pub const HOST_FILE_CHANGED: u32 = 1;
/// The entries handed are no longer selected (FAR's plugin clearing their `PPIF_SELECTED`), nothing
/// changed: arclite after testing them.
pub const HOST_FILE_UNSELECT: u32 = 2;
