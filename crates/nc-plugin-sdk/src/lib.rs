//! Safe Rust for nova-commander plugins (ADR-0004 §7, ADR-0016): implement [`Plugin`] and name the type in
//! [`export_plugin!`]; the macro exports the library's entry, and this crate turns the C ABI of
//! [`nc_plugin_api`] into the trait's calls. A panic in a call is caught at the boundary and reported as
//! that call's failure; the host keeps working. A plugin that starts threads ends them in its `Drop`: the
//! library is unloaded after it. A plugin never prints (`println!`): its stdout is the program's screen;
//! [`Host::log`] writes its log.

#[cfg(panic = "abort")]
compile_error!(
    "nova-commander plugins are built with panic = \"unwind\": a panic is the call's failure, never the program's end"
);

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub use nc_plugin_api as api;

mod dialog;
mod editor;
mod files;
mod get;
mod host;
mod items;
mod kept;
mod keys;
mod open;
mod own_ops;
mod panel;
mod panels;
mod panics;
mod program;
mod put;
mod settings;
pub mod sevenzip;
pub mod system;

pub use dialog::{Closed, Controls, Dialog, DialogEvent, DialogItem, ItemKind, Value};
pub use editor::{Block, EditorInfo, EditorLine, Position, Selection};
pub use get::{GetMode, GetRequest};
pub use host::{ButtonSet, Chosen, Host, InputOptions, Menu, MenuItem, Message};
pub use keys::{KeyAnswer, KeyGo, KeyPress, PanelKey};
pub use open::{CodePage, OpenHow};
pub use own_ops::HostFileDone;
pub use panel::{
    Attributes, Error, ErrorKind, FileEntry, InfoLine, PanelFlags, PanelMode, SortBy, Times,
    WriteMode,
};
pub use panels::{PanelItem, PanelKind, PanelState, Which};
pub use program::{Screen, Synchro, Transfer};
pub use put::PutRequest;

pub(crate) use kept::Kept;
pub use panics::keep_panics;
pub(crate) use panics::panic_text;

use api::{API_VERSION, NcHost, NcPlugin, NcPluginInfo, NcStr};

/// What a plugin tells about itself (FAR's "Plugin information").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    pub title: String,
    pub description: String,
    pub author: String,
    pub version: String,
}

/// A plugin. Its calls come one at a time on one thread of the host's.
pub trait Plugin: 'static {
    /// The plugin starts; `host` is what it may ask of the program later (its messages are not shown
    /// while it starts, in `new` and the first `info`, `commands` and `disks`: they answer as Esc at once).
    fn new(host: Host) -> Self
    where
        Self: Sized;

    fn info(&self) -> Info;

    /// Its items in the plugin commands menu (F11).
    fn commands(&self) -> Vec<String> {
        Vec::new()
    }

    /// Runs the menu item `index`; an error is shown by the host. `Some(instance)`: the program opens
    /// the plugin's panel `instance` at its root in the active panel (FAR's `OpenW` returning a panel).
    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(None)
    }

    /// Its items in the drive menu (Alt+F1 / Alt+F2, under the drives; FAR's `DiskMenuStrings`).
    fn disks(&self) -> Vec<String> {
        Vec::new()
    }

    /// The drive menu's item `index` chosen; an error is shown by the host. `Some(instance)`: the
    /// program opens that panel of the plugin in the panel the menu belongs to.
    fn open_disk(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(None)
    }

    /// Its items in Options > Plugins configuration (FAR's `PluginConfig`; F11's Shift+F9 runs the first).
    fn config_items(&self) -> Vec<String> {
        Vec::new()
    }

    /// The configuration item `index` chosen (FAR's `ConfigureW`): true when the settings changed, so the
    /// program asks for its items again and reads its panels again.
    fn configure(&mut self, _index: usize) -> bool {
        false
    }

    /// Its command line prefixes, without the `:` (FAR's `CommandPrefix`): `prefix:text` typed comes to
    /// [`Plugin::open_command`]. Asked as it starts and after a configuration that changed the settings.
    fn prefixes(&self) -> Vec<String> {
        Vec::new()
    }

    /// The masks of the files it opens, each in the program's masks (`*.temp,*.lst`): Enter or Ctrl+PgDn
    /// on such a file comes to [`Plugin::open_file`]. Asked as [`Plugin::prefixes`] is.
    fn file_masks(&self) -> Vec<String> {
        Vec::new()
    }

    /// `prefix:command` run from the command line (FAR's `OpenW` with `OPEN_COMMANDLINE`), `folder` the
    /// program's folder for names given without one (FAR's current folder). Not a panel's quiet call: it
    /// may show windows. The answer: [`KeyGo::Open`] its panel in the active panel, a folder for it
    /// ([`KeyGo::ActiveFolder`]), the command line, a file opened by the system; none: nothing.
    fn open_command(
        &mut self,
        _prefix: &str,
        _command: &str,
        _folder: Option<&std::path::Path>,
    ) -> Result<KeyAnswer, Error> {
        Ok(KeyAnswer::default())
    }

    /// The file `path` of its masks offered as `how` says: Enter, Ctrl+PgDn, Shift+F2 or Shift+F3 on it
    /// (FAR's `AnalyseW` and `OpenW` with `OPEN_ANALYSE`). [`KeyAnswer::pass`]: not its file (the next
    /// plugin's, then the program's own); [`KeyGo::Open`]: its panel over the file's, ".." at its root
    /// back to the file (for Shift+F2 / Shift+F3 a panel the program reads, uses and closes); the other
    /// places as for a command; none: done by the plugin (FAR's `PANEL_STOP`).
    fn open_file(
        &mut self,
        _path: &std::path::Path,
        _how: OpenHow,
        _folder: Option<&std::path::Path>,
    ) -> Result<KeyAnswer, Error> {
        Ok(KeyAnswer::pass())
    }

    /// Its items in the editor's plugin commands menu (F11 in the editor; FAR's `PF_EDITOR`, API 10).
    fn editor_items(&self) -> Vec<String> {
        Vec::new()
    }

    /// The editor's menu item `index` chosen, the editor up (FAR's `OpenW` with `OPEN_EDITOR`): the
    /// plugin reads and changes it through [`Host`]'s `editor_*` calls; an error is shown by the host.
    fn run_editor_item(&mut self, _index: usize) -> Result<(), String> {
        Ok(())
    }

    /// Whether it takes the editor's keys (FAR's `ProcessEditorInputW`, API 10): each key the editor
    /// has done comes to [`Plugin::editor_key`], the keys after it waiting for its end.
    fn takes_editor_keys(&self) -> bool {
        false
    }

    /// The key `key` (its name: "a", "Enter", "Ctrl+Z") the editor has just done; the plugin may change
    /// the editor through [`Host`]'s `editor_*` calls.
    fn editor_key(&mut self, _key: &str) {}

    /// Whether it takes Alt+F7's found files (FAR's plugin with `SetFindListW`): the results' "Panel"
    /// button is on only while some plugin does.
    fn takes_find_list(&self) -> bool {
        false
    }

    /// Alt+F7's found files and folders, each named by its whole path with what the search saw of it
    /// (FAR's `OPEN_FINDLIST` and `SetFindList`): the
    /// answer [`KeyGo::Open`] shows its panel in the active panel; [`KeyAnswer::pass`] leaves them to
    /// the next plugin.
    fn find_list(&mut self, _found: &[FileEntry]) -> Result<KeyAnswer, Error> {
        Ok(KeyAnswer::pass())
    }

    /// The folder `path` (`/`-separated, absolute; `/` its root) of its panel `instance` (ADR-0016 §13).
    /// Asked on a thread of the host's while the user works on: a panel's calls never show windows. The
    /// instance may be one it never opened or closed already (a folder from the history, a path typed):
    /// it answers as it sees fit (`NotFound`, or the panel opened anew).
    fn list(&mut self, _instance: &str, _path: &str) -> Result<Vec<FileEntry>, Error> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "The plugin has no panels",
        ))
    }

    /// The panel's title at `path` (FAR's `PanelTitle`); empty: its path.
    fn panel_title(&self, _instance: &str, _path: &str) -> String {
        String::new()
    }

    /// The panel's own view modes at `path` (FAR's `PanelModesArray`): the one at `n` for Ctrl+`n` (0..9,
    /// more are not read); `None` at a key, or no modes at all: the program's mode there. Asked with the
    /// title each time a folder of the panel is read.
    fn panel_modes(&self, _instance: &str, _path: &str) -> Vec<Option<PanelMode>> {
        Vec::new()
    }

    /// The mode its panel opens in (FAR's `StartPanelMode`): the one at Ctrl+`n`; `None`: the mode the
    /// program's panel has. Asked with the title.
    fn panel_start_mode(&self, _instance: &str, _path: &str) -> Option<usize> {
        None
    }

    /// The sort its panel opens with (FAR's `StartSortMode` and `StartSortOrder`): the mode, and true for
    /// the descending order; `None`: the sort the program's panel has. Asked with the title.
    fn panel_start_sort(&self, _instance: &str, _path: &str) -> Option<(SortBy, bool)> {
        None
    }

    /// The lines the info panel beside its panel shows in its "Plugin" section (FAR's `InfoLines`).
    /// Asked with the title.
    fn panel_info_lines(&self, _instance: &str, _path: &str) -> Vec<InfoLine> {
        Vec::new()
    }

    /// How its panel `instance` is shown and filled ([`PanelFlags`], FAR's `OPIF_*`).
    fn panel_flags(&self, _instance: &str) -> PanelFlags {
        PanelFlags::default()
    }

    /// Its panel's own keys with their key bar labels (FAR's `ProcessPanelInput` keys and `KeyBar`):
    /// pressed on the panel they come to [`Plugin::panel_key`] instead of the program's.
    fn panel_keys(&self, _instance: &str) -> Vec<PanelKey> {
        Vec::new()
    }

    /// A key of [`Plugin::panel_keys`] pressed on its panel `instance` at `path`. Not a panel's quiet
    /// call: it may show windows; the answer says what the program does next.
    fn panel_key(
        &mut self,
        _instance: &str,
        _path: &str,
        _press: &KeyPress,
    ) -> Result<KeyAnswer, Error> {
        keys::unhandled()
    }

    /// Its panel's kind as the copy dialog's target names it, `<format>:` (FAR's `Format`: TmpPanel's
    /// "Temporary panel"); empty: `::`. That target left as it is copies into the panel.
    fn panel_format(&self, _instance: &str, _path: &str) -> String {
        String::new()
    }

    /// The files and folders on a disk `request` names copied or moved into the folder `path` of its
    /// panel `instance` when it takes paths (FAR's `PutFiles`; [`GetMode::edit`] the editor's copy put
    /// back after F4); the program removes nothing after it. Not a panel's quiet call: it may show
    /// windows (its dialog, a question, a busy message); [`ErrorKind::Cancelled`] when the user stopped
    /// it.
    fn put(&mut self, _instance: &str, _path: &str, _request: &PutRequest) -> Result<(), Error> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "The plugin takes no paths",
        ))
    }

    /// The entries `request` names in the folder `path` of its panel `instance` written out to the disk's
    /// folder it gives, as the plugin does it (FAR's `GetFilesW`; its panel sets
    /// [`PanelFlags::gives_files`]): F5 / F6 with its own dialog, F3 / F4 one file for the viewer or
    /// the editor without one. Not a panel's quiet call: it may show windows (a question, a busy message
    /// with its progress); [`ErrorKind::Cancelled`] when the user stopped it.
    fn get(&mut self, _instance: &str, path: &str, _request: &GetRequest) -> Result<(), Error> {
        Err(unsupported(format!("{path} gives no files")))
    }

    /// The entries `names` of the folder `path` of its panel `instance` deleted as the plugin does it
    /// (FAR's `DeleteFilesW`; its panel sets [`PanelFlags::deletes`]): F8 with its own question (none
    /// with [`GetMode::silent`]). Not a panel's quiet call; [`ErrorKind::Cancelled`] when the user
    /// stopped it.
    fn delete_files(
        &mut self,
        _instance: &str,
        path: &str,
        _names: &[String],
        _mode: GetMode,
    ) -> Result<(), Error> {
        Err(unsupported(format!("{path} deletes nothing itself")))
    }

    /// A folder made in the folder `path` of its panel `instance` (FAR's `MakeDirectoryW`; its panel
    /// sets [`PanelFlags::makes_folders`]): `name` empty for F7, the plugin asking it itself; the name
    /// made, which the cursor goes to. Not a panel's quiet call; [`ErrorKind::Cancelled`] when the user
    /// stopped it.
    fn make_folder(
        &mut self,
        _instance: &str,
        path: &str,
        _name: &str,
        _mode: GetMode,
    ) -> Result<String, Error> {
        Err(unsupported(format!("{path} makes no folders itself")))
    }

    /// A panel of its own with nothing in it yet, for Shift+F1 (FAR's `OpenFilePlugin` with no file,
    /// `OFP_CREATE`): its instance, which the program puts the active panel's files into
    /// ([`Plugin::put`] to its folder `/`) and then closes; none: it makes none (the next plugin's).
    /// Not a panel's quiet call.
    fn open_new(&mut self) -> Result<Option<String>, Error> {
        Ok(None)
    }

    /// The entries `names` of the folder `path` of its panel `instance`, opened from a file, handed to
    /// its commands on that file (FAR's `ProcessHostFileW`: Shift+F3; [`GetMode::top_level`] for a
    /// disk's files, S6.2e): it asks what to do itself; what it did. Not a panel's quiet call;
    /// [`ErrorKind::Cancelled`] when the user stopped it. By default it does nothing.
    fn process_host_file(
        &mut self,
        _instance: &str,
        _path: &str,
        _names: &[String],
        _mode: GetMode,
    ) -> Result<HostFileDone, Error> {
        Ok(HostFileDone::default())
    }

    /// The panel `instance` is no longer shown (FAR's `ClosePanel`); also for an instance reached by a path
    /// typed or from the history.
    fn close_panel(&mut self, _instance: &str) {}

    /// One entry of its panel (`/`: the root, a folder with no name); by default found in its folder's
    /// listing - the whole folder listed for each entry asked (the quick view asks every second, a copy
    /// for each file): a plugin with large folders tells it itself.
    fn stat(&mut self, instance: &str, path: &str) -> Result<FileEntry, Error> {
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        if name.is_empty() {
            return Ok(FileEntry::folder(""));
        }
        let parent = if parent.is_empty() { "/" } else { parent };
        (self.list(instance, parent)?.into_iter())
            .find(|entry| entry.name == name)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("{path} is not there")))
    }

    /// Up to `buffer.len()` bytes of the file `path` from `offset`: how many (fewer only at its end).
    fn read(
        &mut self,
        _instance: &str,
        path: &str,
        _offset: u64,
        _buffer: &mut [u8],
    ) -> Result<usize, Error> {
        Err(unsupported(format!("{path} cannot be read")))
    }

    /// `data` written to the file `path`: `New` makes it (never over an existing entry: `Exists`),
    /// `Append` adds to its end. A file copied in comes as one `New` and `Append`s.
    fn write(
        &mut self,
        _instance: &str,
        path: &str,
        _mode: WriteMode,
        _data: &[u8],
    ) -> Result<(), Error> {
        Err(unsupported(format!("{path} cannot be written")))
    }

    /// No more is written to the file `path` (API 9): it is whole - a plugin that streams a file to a
    /// server finishes it here (a failure fails the copy). By default nothing is left to do.
    fn write_end(&mut self, _instance: &str, _path: &str) -> Result<(), Error> {
        Ok(())
    }

    /// No more is written to the file `path` (API 9): its copy failed or stopped - what was kept for
    /// it let go, never sent as the file (the program removes what was made). By default nothing.
    fn write_abort(&mut self, _instance: &str, _path: &str) -> Result<(), Error> {
        Ok(())
    }

    /// Makes the folder `path` (FAR's `MakeDirectory`).
    fn make_dir(&mut self, _instance: &str, path: &str) -> Result<(), Error> {
        Err(unsupported(format!("{path} cannot be created")))
    }

    /// Renames `from` to `to`; an existing `to` replaced only with `replace` (else `Exists`).
    fn rename(
        &mut self,
        _instance: &str,
        from: &str,
        _to: &str,
        _replace: bool,
    ) -> Result<(), Error> {
        Err(unsupported(format!("{from} cannot be renamed")))
    }

    /// Removes one entry: a file, or (`folder`) an empty folder (FAR's `DeleteFiles`).
    fn remove(&mut self, _instance: &str, path: &str, _folder: bool) -> Result<(), Error> {
        Err(unsupported(format!("{path} cannot be deleted")))
    }

    /// The file or folder `path` just put into its panel by a copy or a move (a file once written, a
    /// folder once what went into it is there), given what it keeps of its source (FAR's `PutFiles`
    /// items' times and attributes). By default it keeps none: the copy says so when the user asked
    /// for the times.
    fn set_times(&mut self, _instance: &str, path: &str, _times: &Times) -> Result<(), Error> {
        Err(unsupported(format!("{path} keeps no times")))
    }

    /// What it asked with [`Synchro::ask`] from a thread of its own, now on the plugin's thread (FAR's
    /// `ProcessSynchroEventW`): it may do anything a command may (its panel read again).
    fn synchro(&mut self, _param: u64) {}
}

/// What a configuration that changed the settings answers.
const CHANGED: &str = "changed";

fn unsupported(message: String) -> Error {
    Error::new(ErrorKind::Unsupported, message)
}

/// The plugin and what it hands out: its strings live until the next call. The host's table is in its
/// own allocation, so a call's `&mut State` never covers what the host reads.
struct State<P> {
    table: *mut NcPlugin,
    /// The generation it was loaded as (its [`Synchro`]s ask nothing once it is released).
    generation: u64,
    plugin: P,
    kept: Kept,
    /// The items of each place as the last `item_count` of that place told them.
    labels: [Vec<String>; 8],
    /// The entries the last `list` handed out (their texts in `kept`).
    entries: Vec<api::NcEntry>,
    /// The modes the last `panel_info` handed out (their texts in `kept`).
    modes: Vec<api::NcPanelMode>,
    /// The keys the last `panel_info` handed out (their texts in `kept`).
    keys: Vec<api::NcPanelKey>,
    /// The info lines the last `panel_info` handed out (their texts in `kept`).
    info_lines: Vec<api::NcInfoLine>,
    error: String,
}

impl<P> State<P> {
    /// `text` kept until the next call, as the host reads it.
    fn keep(&mut self, text: String) -> NcStr {
        self.kept.text(text)
    }
}

/// The state behind the host's pointer.
///
/// # Safety
/// `state` is the pointer [`entry`] put in the table, not yet released; the host makes one call at a time.
pub(crate) unsafe fn state<'a, P>(state: *mut c_void) -> &'a mut State<P> {
    // SAFETY: the caller promises the pointer is this plugin's live state, used by one call at a time.
    unsafe { &mut *state.cast::<State<P>>() }
}

unsafe extern "C" fn info<P: Plugin>(raw: *mut c_void) -> NcPluginInfo {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    let info = catch_unwind(AssertUnwindSafe(|| state.plugin.info())).unwrap_or_default();
    NcPluginInfo {
        title: state.keep(info.title),
        description: state.keep(info.description),
        author: state.keep(info.author),
        version: state.keep(info.version),
    }
}

unsafe extern "C" fn last_error<P: Plugin>(raw: *mut c_void) -> NcStr {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    NcStr::new(&state.error)
}

unsafe extern "C" fn release<P: Plugin>(raw: *mut c_void) {
    // SAFETY: the host's last call: the state `entry` boxed goes back to its box once.
    let state = unsafe { Box::from_raw(raw.cast::<State<P>>()) };
    let (table, generation) = (state.table, state.generation);
    // A panic in the plugin's drop stays here.
    let _ = catch_unwind(AssertUnwindSafe(move || drop(state)));
    program::released(generation);
    // SAFETY: the table `entry` boxed, not read by the host after this call.
    drop(unsafe { Box::from_raw(table) });
}

/// The library's entry for plugin `P` ([`export_plugin!`] calls it): its table, null for a host of another
/// API version or another build of it (a table of another size) or a plugin that panicked starting.
///
/// # Safety
/// `host` is null or the host's table, alive until the plugin is released.
pub unsafe fn entry<P: Plugin>(host: *const NcHost) -> *const NcPlugin {
    if host.is_null() {
        return std::ptr::null();
    }
    // SAFETY: the caller promises a live host table; its version and size come first in any build's.
    let (version, size) = unsafe { ((*host).api_version, (*host).size) };
    // The size exactly (review R3): a smaller table lacks functions the plugin calls, and the ABI's other
    // structs carry no size of their own, so another build of them would be written and read wrongly.
    // SAFETY: as above; read once the size says the field is there (R7 review A3: the other structs).
    if version != API_VERSION || size != size_of::<NcHost>() || unsafe { (*host).abi } != api::ABI {
        return std::ptr::null();
    }
    let generation = program::loaded(host);
    let Ok(plugin) = catch_unwind(|| P::new(Host { table: host })) else {
        program::released(generation);
        return std::ptr::null();
    };
    let table = Box::into_raw(Box::new(NcPlugin {
        api_version: API_VERSION,
        size: size_of::<NcPlugin>(),
        abi: api::ABI,
        state: std::ptr::null_mut(),
        info: info::<P>,
        item_count: items::item_count::<P>,
        item: items::item::<P>,
        run_item: items::run_item::<P>,
        last_error: last_error::<P>,
        release: release::<P>,
        list: panel::list::<P>,
        panel_info: panel::panel_info::<P>,
        close_panel: panel::close_panel::<P>,
        stat: files::stat::<P>,
        read: files::read::<P>,
        write: files::write::<P>,
        make_dir: files::make_dir::<P>,
        rename: files::rename::<P>,
        remove: files::remove::<P>,
        panel_key: keys::panel_key::<P>,
        put: put::put::<P>,
        delete_files: own_ops::delete_files::<P>,
        make_folder: own_ops::make_folder::<P>,
        open_new: own_ops::open_new::<P>,
        process_host_file: own_ops::process_host_file::<P>,
        open_command: open::open_command::<P>,
        open_file: open::open_file::<P>,
        find_list: open::find_list::<P>,
        set_times: files::set_times::<P>,
        synchro: program::synchro::<P>,
        get: get::get::<P>,
        editor_key: editor::editor_key::<P>,
    }));
    let state = Box::into_raw(Box::new(State {
        table,
        generation,
        plugin,
        kept: Kept::default(),
        labels: Default::default(),
        entries: Vec::new(),
        modes: Vec::new(),
        keys: Vec::new(),
        info_lines: Vec::new(),
        error: String::new(),
    }));
    // SAFETY: the table was just boxed; it is the host's until `release` frees it with the state.
    unsafe {
        (*table).state = state.cast();
    }
    table
}

/// Exports the library's entry for the plugin type: `export_plugin!(MyPlugin);`.
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        /// The plugin library's entry (generated by `export_plugin!`).
        ///
        /// # Safety
        /// Called by the host with its table.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn nc_plugin_entry(
            host: *const $crate::api::NcHost,
        ) -> *const $crate::api::NcPlugin {
            $crate::keep_panics();
            // SAFETY: the host calls the entry with its live table.
            unsafe { $crate::entry::<$plugin>(host) }
        }
    };
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
