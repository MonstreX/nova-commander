//! What a plugin asks of the program beyond its panels and windows (API 3, FAR's `AdvControl`,
//! `FileFilterControl`, `CopyToClipboard`, `ShowHelp`): a call back on its own thread for a thread of its
//! own ([`Synchro`]), keys done as typed, the screen, a panel's filter, the clipboard, its help.

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::api::{NcHost, NcScreen, NcStr};
use crate::panel::raw_entry;
use crate::{FileEntry, Host, Kept, Plugin, Which, state};

/// A copy handed to the program's background tasks ([`Host::background_transfer`], API 9).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transfer {
    /// The plugin's panel.
    pub instance: String,
    /// Its folder.
    pub folder: String,
    /// The entries copied: of `folder`, or with `upload` of `disk`.
    pub names: Vec<String>,
    /// The disk's folder.
    pub disk: String,
    /// From the disk into the panel.
    pub upload: bool,
    /// The sources removed once copied.
    pub moving: bool,
}

/// The program's screen (FAR's `ACTL_GETFARRECT`, `ACTL_GETFARHWND`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Screen {
    pub columns: u32,
    pub rows: u32,
    /// The console's window (Windows' `HWND`, for a menu of the system's to belong to); 0: none.
    pub window: usize,
}

/// The plugins loaded in this library: the host's table of each and the generation it was loaded as.
static LOADED: Mutex<Vec<(usize, u64)>> = Mutex::new(Vec::new());
static GENERATION: AtomicU64 = AtomicU64::new(1);

fn loaded_list() -> MutexGuard<'static, Vec<(usize, u64)>> {
    LOADED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A plugin starts with the host's table `host`: its generation.
pub(crate) fn loaded(host: *const NcHost) -> u64 {
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    loaded_list().push((host as usize, generation));
    generation
}

/// The plugin of `generation` released: its [`Synchro`]s ask nothing after it.
pub(crate) fn released(generation: u64) {
    loaded_list().retain(|&(_, loaded)| loaded != generation);
}

/// A way for any thread of the plugin's to have [`Plugin::synchro`] called on the plugin's thread (FAR's
/// `ACTL_SYNCHRO`): a worker that saw a change asks, the plugin then reads its panel again. A plugin
/// ends its threads in its `Drop`; one kept past it asks nothing (R7 review A7: it called the host's
/// table freed with the plugin).
#[derive(Debug, Clone, Copy)]
pub struct Synchro {
    table: *const NcHost,
    generation: u64,
}

// SAFETY: the host's `synchro` is one of the functions of its table any thread may call (the ABI says so), and
// the table stays where it is while the plugin is loaded; the plugin's threads end before it is released.
unsafe impl Send for Synchro {}
// SAFETY: as for `Send`: the only call made through it is any thread's.
unsafe impl Sync for Synchro {}

impl Synchro {
    /// [`Plugin::synchro`] called with `param` once the plugin's thread is free; never blocks.
    pub fn ask(&self, param: u64) {
        // Held through the call: the plugin is not released meanwhile.
        let loaded = loaded_list();
        if !loaded
            .iter()
            .any(|&(_, generation)| generation == self.generation)
        {
            return;
        }
        // SAFETY: as `Send` says; the host's table is alive while the plugin is (checked above).
        unsafe {
            let table = &*self.table;
            (table.synchro)(table.context, param);
        }
    }
}

impl Host {
    /// A [`Synchro`] for the plugin's own threads.
    pub fn synchro(&self) -> Synchro {
        let loaded = loaded_list();
        let generation = (loaded.iter().rev())
            .find(|&&(table, _)| table == self.table as usize)
            .map_or(0, |&(_, generation)| generation);
        Synchro {
            table: self.table,
            generation,
        }
    }

    /// The keys named in `keys` ("Shift+F6", separated by spaces) done by the program as if typed, once
    /// the plugin's call has returned (FAR's `ACTL_POSTKEYSEQUENCE`): false when a name is not a key
    /// (nothing done).
    pub fn post_keys(&self, keys: &str) -> bool {
        let table = self.table();
        // SAFETY: as every call; the text lives through it.
        unsafe { (table.post_keys)(table.context, NcStr::new(keys)) == 0 }
    }

    /// The program's screen; none when the program does not answer.
    pub fn screen(&self) -> Option<Screen> {
        let table = self.table();
        let mut raw = NcScreen {
            columns: 0,
            rows: 0,
            window: 0,
        };
        // SAFETY: as every call; the answer's place lives through it.
        let code = unsafe { (table.screen)(table.context, &raw mut raw) };
        (code == 0).then_some(Screen {
            columns: raw.columns,
            rows: raw.rows,
            window: raw.window,
        })
    }

    /// Whether each of `entries` passes the filter of the panel `which` (Ctrl+I; FAR's `FileFilterControl`
    /// with `FFT_PANEL`): every one without a filter; none when the program does not answer.
    pub fn filter(&self, which: Which, entries: &[FileEntry]) -> Option<Vec<bool>> {
        let mut kept = Kept::default();
        let raw: Vec<_> = (entries.iter())
            .map(|entry| raw_entry(entry.clone(), &mut kept))
            .collect();
        let mut results = vec![0u8; raw.len()];
        let table = self.table();
        // SAFETY: as every call; the entries, their texts (in `kept`) and the results live through it.
        let code = unsafe {
            (table.filter)(
                table.context,
                which.code(),
                raw.as_ptr(),
                raw.len(),
                results.as_mut_ptr(),
            )
        };
        (code == 0).then(|| results.iter().map(|result| *result != 0).collect())
    }

    /// `text` put on the clipboard (FAR's `CopyToClipboard`).
    pub fn set_clipboard(&self, text: &str) -> bool {
        let table = self.table();
        // SAFETY: as every call; the text lives through it.
        unsafe { (table.clipboard)(table.context, NcStr::new(text)) == 0 }
    }

    /// The copy `transfer` names done as one of the program's own background tasks (API 9): its
    /// progress dialog, its questions, F12's list; the task reads and writes through the plugin's file
    /// calls. Whether it was started.
    pub fn background_transfer(&self, transfer: &Transfer) -> bool {
        let table = self.table();
        let names: Vec<NcStr> = transfer.names.iter().map(|name| NcStr::new(name)).collect();
        let flag = |on: bool, bit: u32| if on { bit } else { 0 };
        let flags = flag(transfer.upload, crate::api::TRANSFER_UPLOAD)
            | flag(transfer.moving, crate::api::TRANSFER_MOVE);
        // SAFETY: as every call; the texts and the names' array live through it.
        unsafe {
            (table.transfer)(
                table.context,
                NcStr::new(&transfer.instance),
                NcStr::new(&transfer.folder),
                names.as_ptr(),
                names.len(),
                NcStr::new(&transfer.disk),
                flags,
            ) == 0
        }
    }

    /// Whether the plugin named `plugin` (or of that folder, manifest or library) is loaded (FAR's
    /// `PCTL_FINDPLUGIN`).
    pub fn find_plugin(&self, plugin: &str) -> bool {
        self.plugins_control(crate::api::PLUGINS_FIND, plugin)
    }

    /// The plugin of the folder, manifest or library `plugin` (or a plugin's folder by its name) loaded
    /// (FAR's `PCTL_LOADPLUGIN`): started; its failure the program shows.
    pub fn load_plugin(&self, plugin: &str) -> bool {
        self.plugins_control(crate::api::PLUGINS_LOAD, plugin)
    }

    /// The plugin named `plugin` (or of that path) unloaded (FAR's `PCTL_UNLOADPLUGIN`); this one
    /// itself as its call ends.
    pub fn unload_plugin(&self, plugin: &str) -> bool {
        self.plugins_control(crate::api::PLUGINS_UNLOAD, plugin)
    }

    fn plugins_control(&self, action: u32, plugin: &str) -> bool {
        let table = self.table();
        // SAFETY: as every call; the text lives through it.
        unsafe { (table.plugins_control)(table.context, action, NcStr::new(plugin)) == 0 }
    }

    /// Whether the program asks before the work `name` (FAR's `FSSF_CONFIRMATIONS` by its names:
    /// "RemoveConnection", "Delete"...): none for a name it does not know.
    pub fn confirms(&self, name: &str) -> Option<bool> {
        let table = self.table();
        // SAFETY: as every call; the text lives through it.
        match unsafe { (table.confirmation)(table.context, NcStr::new(name)) } {
            1 => Some(true),
            0 => Some(false),
            _ => None,
        }
    }

    /// Whether the file name `name` matches `masks` in the program's masks (FAR's `ProcessName` with
    /// `PN_CMPNAMELIST`: `*.zip,*.rar`, `|` before the exclusions, `<group>`, regular expressions): none
    /// for masks with errors.
    pub fn match_masks(&self, masks: &str, name: &str) -> Option<bool> {
        let table = self.table();
        // SAFETY: as every call; the texts live through it.
        match unsafe { (table.match_masks)(table.context, NcStr::new(masks), NcStr::new(name)) } {
            1 => Some(true),
            0 => Some(false),
            _ => None,
        }
    }

    /// The stop the user asked of the call running forgotten: [`Host::cancelled`] is false again until
    /// the next Esc (arclite's "Do you want to interrupt current operation?" answered No).
    pub fn resume(&self) {
        let table = self.table();
        // SAFETY: as every call.
        unsafe { (table.resume)(table.context) }
    }

    /// The plugin's help at `topic` (FAR's `ShowHelp`): false while the program shows none.
    pub fn show_help(&self, topic: &str) -> bool {
        let table = self.table();
        // SAFETY: as every call; the text lives through it.
        unsafe { (table.show_help)(table.context, NcStr::new(topic)) == 0 }
    }
}

pub(crate) unsafe extern "C" fn synchro<P: Plugin>(raw: *mut c_void, param: u64) {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    // A panic stays here: there is no answer to fail.
    let _ = catch_unwind(AssertUnwindSafe(|| state.plugin.synchro(param)));
}

#[cfg(test)]
#[path = "program_tests.rs"]
pub(crate) mod tests;
