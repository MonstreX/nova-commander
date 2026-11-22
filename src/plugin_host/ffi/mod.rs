//! The raw calls across the C ABI (ADR-0004 §7: the only `unsafe` of the plugin host): a plugin library
//! loaded, its entry called with the host's table, its table checked and called, its state released before
//! the library is unloaded. Everything here runs on the plugin's own thread (`Loaded` holds raw pointers:
//! it is neither `Send` nor `Sync`).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use nc_plugin_api::{API_VERSION, ENTRY, EntryFn, NcHost, NcPlugin, NcStr};

pub use nc_plugin_api::{
    GET_EDIT, GET_SILENT, GET_VIEW, HOST_FILE_CHANGED, HOST_FILE_UNSELECT, PLACE_COMMANDS,
    PLACE_CONFIG, PLACE_DISKS, PLACE_EDITOR, PLACE_EDITOR_KEYS, PLACE_FILE_MASKS, PLACE_FIND_LIST,
    PLACE_PREFIXES,
};

use super::log::PluginLog;
use super::{Event, PluginInfo};
use crate::platform::libraries::Library;

mod calls;
mod dialog_calls;
mod editor_calls;
mod get_calls;
mod open_calls;
mod panel_calls;
mod panel_control_calls;
mod panel_keys;
mod program_calls;
mod settings_calls;

pub use get_calls::{GetFiles, PutFiles};
pub(super) use open_calls::exact;
pub use panel_calls::{FileAnswer, FileCall, MOST_READ, sort_code, start_sort as sort_mode};
pub use panel_keys::{KeyAnswer, KeyGo, KeyPress};

/// The most items a plugin puts in one of the program's menus; more are not asked for.
const MOST_ITEMS: usize = 1000;

/// The longest text taken from a plugin (review R46: a length past it is a plugin's error, never read).
const MOST_TEXT: usize = 64 << 20;

/// What the host's functions need: whose they are and where the program hears them. Only the plugin's
/// thread uses it (the `Cell`s), after it is made.
pub struct HostContext {
    pub plugin: String,
    pub events: Sender<Event>,
    /// The plugin's folder, as its table hands it out.
    pub folder: String,
    /// The plugin's data folder (`settings\Plugins\<name>`, API 7), as its table hands it out.
    pub data_folder: String,
    pub log: PluginLog,
    /// Set once the plugin has started: before that the program does not listen yet.
    started: AtomicBool,
    /// A window without buttons is up for the call running (gone when it ends).
    busy: Cell<bool>,
    /// A panel's call runs (a folder, a file, a title): the program may be waiting for it with the
    /// screen held, so its windows are answered Esc at once (ADR-0016 §13).
    quiet: Cell<bool>,
    /// The text the host last answered with (an input line's), valid until the plugin's next call to the
    /// host.
    answer: RefCell<String>,
    /// The texts of the last dialog's items, handed back as [`answer`](Self::answer) is.
    dialog_texts: RefCell<Vec<String>>,
    /// The panels' last answers, whose texts the plugin reads (API 3).
    panel_answers: RefCell<panel_control_calls::PanelAnswers>,
    /// The editor's last answer's texts (API 10).
    editor_texts: RefCell<editor_calls::EditorTexts>,
    /// The panels as the program last saw them, read in a panel's call.
    panels_seen: super::panel_requests::PanelsSeen,
    /// The dialogs whose handler runs now (nested when a handler opens another), by the address of their
    /// call: the handles `dialog_change` takes.
    handlers: RefCell<Vec<usize>>,
    /// The plugin's own values (FAR's `PluginSettings`), as the program's settings had them at the start
    /// and the plugin set them since.
    settings: RefCell<BTreeMap<String, String>>,
    /// The call running: whether it was asked to stop, what it told of its progress (ADR-0019 §5).
    calls: Arc<super::calls::CallState>,
    /// The plugin's own thread, once it runs there: the host's functions that use the cells above answer
    /// only it (review R45: a C plugin's other thread gets the failure answer).
    thread: std::sync::OnceLock<std::thread::ThreadId>,
}

impl HostContext {
    pub fn new(plugin: &str, events: Sender<Event>, folder: &Path, log: PluginLog) -> Self {
        Self {
            plugin: plugin.to_string(),
            events,
            folder: folder.display().to_string(),
            data_folder: String::new(),
            log,
            started: AtomicBool::new(false),
            busy: Cell::new(false),
            quiet: Cell::new(false),
            answer: RefCell::new(String::new()),
            dialog_texts: RefCell::new(Vec::new()),
            panel_answers: RefCell::default(),
            editor_texts: RefCell::default(),
            panels_seen: super::panel_requests::PanelsSeen::default(),
            handlers: RefCell::new(Vec::new()),
            settings: RefCell::new(BTreeMap::new()),
            calls: Arc::default(),
            thread: std::sync::OnceLock::new(),
        }
    }

    /// Whether the plugin calls on its own thread (or the context is not bound to one yet) and is still
    /// the program's (not let go, P6.0e).
    fn on_its_thread(&self) -> bool {
        let own = (self.thread.get()).is_none_or(|own| *own == std::thread::current().id());
        own && !self.calls.is_let_go()
    }

    /// With the state of its calls shared with the program.
    #[must_use]
    pub fn with_calls(mut self, calls: Arc<super::calls::CallState>) -> Self {
        self.calls = calls;
        self
    }

    /// With the panels as the program publishes them.
    #[must_use]
    pub fn with_panels_seen(mut self, seen: super::panel_requests::PanelsSeen) -> Self {
        self.panels_seen = seen;
        self
    }

    /// With the plugin's data folder (API 7).
    #[must_use]
    pub fn with_data_folder(mut self, folder: &Path) -> Self {
        self.data_folder = folder.display().to_string();
        self
    }

    /// With the plugin's values as the program's settings keep them.
    #[must_use]
    pub fn with_settings(self, settings: BTreeMap<String, String>) -> Self {
        *self.settings.borrow_mut() = settings;
        self
    }
}

impl HostContext {
    /// Whether the plugin has started: before, the program does not listen.
    fn started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }

    /// Whether a panel's call runs (a folder, a file, a title).
    fn in_quiet_call(&self) -> bool {
        self.quiet.get()
    }

    /// Whether a window the plugin asks for can be shown: not while it starts (nobody listens yet) nor
    /// in a panel's call (the program may wait for it; the plugin's log says why).
    fn listens(&self) -> bool {
        if self.quiet.get() {
            self.log
                .write("A window asked for in a panel's call is not shown: answered Esc");
            return false;
        }
        self.started.load(Ordering::Acquire)
    }
}

/// A plugin started. Dropping it releases the plugin, frees the host's table, then unloads the library.
pub struct Loaded {
    table: *const NcPlugin,
    host: *mut NcHost,
    context: *mut HostContext,
    // Last: the code of the plugin's functions stays loaded until they are done with.
    _library: Library,
}

impl Loaded {
    /// Loads the library at `path` and starts the plugin in it.
    pub fn start(path: &Path, context: HostContext) -> Result<Self, String> {
        let library = Library::open(path).map_err(|error| error.to_string())?;
        let entry = library
            .symbol(ENTRY)
            .ok_or_else(|| format!("The library exports no {ENTRY}"))?;
        // Its functions answer this thread only (review R45).
        let _ = context.thread.set(std::thread::current().id());
        let context = Box::into_raw(Box::new(context));
        // SAFETY: `context` was just boxed and is not freed before the host's table.
        let folder = unsafe { NcStr::new(&(*context).folder) };
        // SAFETY: as `folder`.
        let data_folder = unsafe { NcStr::new(&(*context).data_folder) };
        let host = Box::into_raw(Box::new(NcHost {
            api_version: API_VERSION,
            size: size_of::<NcHost>(),
            abi: nc_plugin_api::ABI,
            context: context.cast(),
            folder,
            message: calls::message,
            menu: calls::menu,
            input: calls::input,
            log: calls::log,
            dialog: dialog_calls::dialog,
            dialog_change: dialog_calls::dialog_change,
            setting: settings_calls::setting,
            set_setting: settings_calls::set_setting,
            decode: open_calls::decode,
            progress: calls::progress,
            cancelled: calls::cancelled,
            panel_state: panel_control_calls::panel_state,
            panel_items: panel_control_calls::panel_items,
            panel_do: panel_control_calls::panel_do,
            synchro: program_calls::synchro,
            post_keys: program_calls::post_keys,
            screen: program_calls::screen,
            filter: program_calls::filter,
            clipboard: program_calls::clipboard,
            show_help: program_calls::show_help,
            plugins_control: program_calls::plugins_control,
            confirmation: program_calls::confirmation,
            match_masks: program_calls::match_masks,
            resume: calls::resume,
            data_folder,
            transfer: program_calls::transfer,
            editor: editor_calls::editor,
        }));
        // SAFETY: the exported entry has the ABI's signature (the plugin was built against it); a function
        // pointer and a data pointer have one size on the systems the program runs on.
        let entry: EntryFn = unsafe { std::mem::transmute::<*const c_void, EntryFn>(entry) };
        // SAFETY: the host's table lives until the plugin is released (or, not started, below).
        let table = unsafe { entry(host) };
        let checked = check(table);
        if let Err(reason) = checked {
            if table.is_null() {
                // SAFETY: both were boxed above; a plugin that did not start keeps no pointer to them.
                unsafe {
                    drop(Box::from_raw(host));
                    drop(Box::from_raw(context));
                }
            } else {
                // A table rejected is not released: any thread the plugin started may still run in its
                // library and read the host's table, so all three stay.
                std::mem::forget(library);
            }
            return Err(reason);
        }
        Ok(Self {
            table,
            host,
            context,
            _library: library,
        })
    }

    /// The program listens now (the plugin's start told): its messages are shown from here on.
    pub fn listen(&self) {
        // SAFETY: the context lives as long as the plugin.
        unsafe { (*self.context).started.store(true, Ordering::Release) };
    }

    /// The call `id` begins on the plugin's thread: not asked to stop, nothing told of it yet.
    pub fn begin_call(&self, id: u64) {
        // SAFETY: the context lives as long as the plugin.
        unsafe { (*self.context).calls.begin(id) };
    }

    /// A job of the program's begins on the plugin's thread: the items read in an earlier one are not
    /// read again (the panels may have changed since).
    pub fn new_job(&self) {
        // SAFETY: the context lives as long as the plugin; only this thread uses its cells.
        unsafe { (*self.context).panel_answers.borrow_mut().forget_items() };
    }

    /// The call running has ended.
    pub fn end_call(&self) {
        // SAFETY: the context lives as long as the plugin.
        unsafe { (*self.context).calls.end() };
    }

    /// Whether the program let the plugin go (P6.0e).
    pub fn let_go(&self) -> bool {
        // SAFETY: the context lives as long as the plugin.
        unsafe { (*self.context).calls.is_let_go() }
    }

    /// True once after a call that left a window without buttons up: the program takes it away.
    pub fn take_busy(&self) -> bool {
        // SAFETY: the context lives as long as the plugin; only this thread uses it.
        unsafe { (*self.context).busy.replace(false) }
    }

    /// A panel's call: the plugin's windows answered Esc meanwhile.
    fn quietly<T>(&self, call: impl FnOnce() -> T) -> T {
        // SAFETY: the context lives as long as the plugin; only this thread uses it.
        let context = unsafe { &*self.context };
        context.quiet.set(true);
        let result = call();
        context.quiet.set(false);
        result
    }

    fn table(&self) -> &NcPlugin {
        // SAFETY: the table lives until `release`, called only by `drop`.
        unsafe { &*self.table }
    }

    pub fn info(&self) -> PluginInfo {
        let table = self.table();
        // SAFETY: the table's own state, one call at a time on the plugin's thread, before `release`.
        let info = unsafe { (table.info)(table.state) };
        PluginInfo {
            title: text(info.title),
            description: text(info.description),
            author: text(info.author),
            version: text(info.version),
        }
    }

    /// Its items in the program's menu `place`, asked in a quiet call: no windows.
    pub fn quiet_items(&self, place: u32) -> Vec<String> {
        self.quietly(|| self.items(place))
    }

    /// Its items in the program's menu `place` (`PLACE_*`).
    pub fn items(&self, place: u32) -> Vec<String> {
        let table = self.table();
        // SAFETY: as in `info`.
        let count = unsafe { (table.item_count)(table.state, place) }.min(MOST_ITEMS);
        (0..count)
            // SAFETY: as in `info`; each label is copied before the next call.
            .map(|index| text(unsafe { (table.item)(table.state, place, index) }))
            .collect()
    }

    /// Its item `index` of `place` chosen: the instance of a panel it opens, if any.
    pub fn run_item(&self, place: u32, index: usize) -> Result<Option<String>, String> {
        let table = self.table();
        let mut panel = NcStr::EMPTY;
        // SAFETY: as in `info`; `panel` lives through the call and is copied before the next one.
        if unsafe { (table.run_item)(table.state, place, index, &raw mut panel) } == 0 {
            return Ok(Some(text(panel)).filter(|instance| !instance.is_empty()));
        }
        Err(self.last_error())
    }

    pub(super) fn last_error(&self) -> String {
        let table = self.table();
        // SAFETY: as in `info`.
        text(unsafe { (table.last_error)(table.state) })
    }
}

impl Drop for Loaded {
    fn drop(&mut self) {
        let table = self.table();
        // SAFETY: the last call on the table; nothing calls it after.
        unsafe { (table.release)(table.state) };
        // SAFETY: boxed by `start`, freed once, after the plugin let go of them.
        unsafe {
            drop(Box::from_raw(self.host));
            drop(Box::from_raw(self.context));
        }
    }
}

/// A table the host can use: not null, of this API version and exactly this build's size.
fn check(table: *const NcPlugin) -> Result<(), String> {
    if table.is_null() {
        return Err("The plugin did not start".to_string());
    }
    // SAFETY: a table the entry returned lives until its `release`.
    let (version, size) = unsafe { ((*table).api_version, (*table).size) };
    if version != API_VERSION {
        return Err(format!(
            "The plugin speaks API version {version}; this program speaks {API_VERSION}"
        ));
    }
    // The size exactly (review R3): the ABI's other structs (`NcEntry`, `NcKeyAnswer`, ...) carry no size
    // of their own, so a plugin built against other ones would write past the host's or be read wrongly.
    let expected = size_of::<NcPlugin>();
    if size != expected {
        return Err(format!(
            "The plugin was built for another build of API version {version}: its table has {size} \
             bytes, this program's {expected}"
        ));
    }
    // SAFETY: as the version; a table of this size has the field (R7 review A3: the other structs).
    let abi = unsafe { (*table).abi };
    if abi != nc_plugin_api::ABI {
        return Err(format!(
            "The plugin was built for another build of API version {version}: its structs differ \
             (fingerprint {abi:x}, this program's {:x})",
            nc_plugin_api::ABI
        ));
    }
    Ok(())
}

/// A plugin's string, copied while it is valid (until the plugin's next call); not UTF-8: replaced; one
/// longer than [`MOST_TEXT`] is not read (review R46): none.
fn text(raw: NcStr) -> String {
    if raw.len == 0 || raw.ptr.is_null() || raw.len > MOST_TEXT {
        return String::new();
    }
    // SAFETY: the plugin hands `len` readable bytes at `ptr`, valid until its next call.
    let bytes = unsafe { std::slice::from_raw_parts(raw.ptr, raw.len) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// A message's lines as the plugin gave them, joined by LF (or CR LF): an empty line at the end stays;
/// no text is no lines.
fn message_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect()
}

#[cfg(test)]
mod tests;
