//! The C ABI between nova-commander and its plugins (ADR-0004 §7, ADR-0016): plain `#[repr(C)]` types and
//! function tables, no dependencies. A plugin library exports [`ENTRY`]; the host calls it with its own table
//! ([`NcHost`]) and gets the plugin's ([`NcPlugin`]). Both tables start with the API version and their size,
//! so either side can tell what the other knows. Strings are UTF-8 as a pointer and a length; what a side
//! hands over stays its own and valid until its next call on the same table (the plugin's strings) or until
//! the call returns (the host's; a text the host answers with: until the plugin's next call to the host).
//!
//! The plugin's functions are called on one thread of the host's for that plugin, one call at a time; the
//! host's functions are called from that thread and may block it (a message waits for the user).

#![no_std]

use core::ffi::c_void;

mod abi;
mod dialog;
mod editor_control;
mod menu;
mod messages;
mod panel;
mod panel_control;
mod transfer;

pub use abi::ABI;
pub use dialog::*;
pub use editor_control::*;
pub use menu::*;
pub use messages::*;
pub use panel::*;
pub use panel_control::*;
pub use transfer::*;

/// The version of this ABI. A plugin built for another version is not loaded. 2 (ADR-0019 §5): the host's
/// [`NcHost::progress`] and [`NcHost::cancelled`]; 3 (ADR-0016 §10, addendum P6): the panels
/// ([`NcHost::panel_state`], [`NcHost::panel_items`], [`NcHost::panel_do`]) and what API 2 left; 4 (ADR-0016,
/// addendum P6.6b): the program's confirmations ([`NcHost::confirmation`]); 5 (ADR-0021, the archives):
/// the program's masks ([`NcHost::match_masks`]), an entry's allocated size ([`NcEntry::allocated`]),
/// files written out by the plugin ([`NcPlugin::get`], [`PANEL_GIVES_FILES`]), a stop taken back
/// ([`NcHost::resume`]); 6 (ADR-0016, S6.7): Alt+F7's search in archives ([`OPEN_SEARCH`]); 7 (ADR-0023,
/// S6.4b): the plugin's data folder ([`NcHost::data_folder`]); 8 (ADR-0023, S6.5c): a dialog's box
/// ([`ITEM_BOX`]), an edited copy put back at each save ([`PANEL_PUTS_ON_SAVE`]); 9 (ADR-0023, S6.8a): a
/// copy handed to the program's background tasks ([`NcHost::transfer`]), a written file's end
/// ([`WRITE_END`]) or its abandonment ([`WRITE_ABORT`]); 10 (ADR-0026, S2d.7): the editor's plugins
/// ([`PLACE_EDITOR`], [`NcHost::editor`]); 11 (R7 review A3): the structs' fingerprint in both tables
/// ([`ABI`]).
pub const API_VERSION: u32 = 11;

/// The name of the function a plugin library exports: [`EntryFn`].
pub const ENTRY: &str = "nc_plugin_entry";

/// UTF-8 text: `len` bytes from `ptr` (any pointer when `len` is 0).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcStr {
    pub ptr: *const u8,
    pub len: usize,
}

impl NcStr {
    pub const EMPTY: Self = Self {
        ptr: core::ptr::null(),
        len: 0,
    };

    /// `text` as it is (the caller keeps it alive as long as the other side may read it).
    pub const fn new(text: &str) -> Self {
        Self {
            ptr: text.as_ptr(),
            len: text.len(),
        }
    }

    /// The text.
    ///
    /// # Safety
    /// `ptr` points to `len` bytes of valid UTF-8 that live as long as `'a` (or `len` is 0).
    pub unsafe fn as_str<'a>(self) -> &'a str {
        if self.len == 0 || self.ptr.is_null() {
            return "";
        }
        // SAFETY: the caller promises `len` readable bytes of UTF-8 at `ptr` for `'a`.
        unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.ptr, self.len)) }
    }
}

/// What the host gives a plugin: its version, its folder and the functions the plugin may call.
#[repr(C)]
pub struct NcHost {
    pub api_version: u32,
    /// `size_of::<NcHost>()` of the host that made it.
    pub size: usize,
    /// [`ABI`] of the host's build.
    pub abi: u64,
    /// The host's own, handed back with every call.
    pub context: *mut c_void,
    /// The plugin's folder (`plugins\<name>`), valid while it is loaded: its own files are there.
    pub folder: NcStr,
    /// A message window (FAR's `Message`, always as with `FMSG_ALLINONE`): `text`'s lines and `buttons`'
    /// labels, joined by `\n` (an empty last line kept, unlike FAR's), `&` before a hotkey letter. A line
    /// starting with `\u{1}` is a separator, with `\u{2}` a double one, the rest of it a title in its
    /// middle; a line too long for
    /// the screen is a read-only line that scrolls. Blocks until the user closes it: the index of the button
    /// pressed, -1 for Esc. With no buttons the window stays while the plugin works (until its call ends or
    /// its next window) and the call returns 0 at once. -1 at once while the plugin starts (no window then).
    /// `flags`: [`MESSAGE_WARNING`], [`MESSAGE_LEFT_ALIGN`], [`MESSAGE_ERROR`], one of the button sets
    /// `MESSAGE_BUTTONS_*` (then `buttons` is not read).
    ///
    /// Safety (every host function): `context` is the table's own; called on the plugin's thread; what the
    /// pointers give lives through the call.
    pub message: unsafe extern "C" fn(
        context: *mut c_void,
        flags: u32,
        title: NcStr,
        text: NcStr,
        buttons: NcStr,
    ) -> i32,
    /// A menu (FAR's `Menu`): blocks until the user closes it. Returns the item under the cursor when it
    /// closed (-1 for Esc or no item); `key` (when not null) gets the index of the break key that closed it,
    /// -1 when Enter or Esc did.
    pub menu: unsafe extern "C" fn(context: *mut c_void, menu: *const NcMenu, key: *mut i32) -> i32,
    /// An input line (FAR's `InputBox`): `prompt` above `initial`'s text, `history` the line's history
    /// (empty: none; FAR's `HistoryName`). `flags`: `INPUT_*` (FAR's `FIB_*` of the same values). Blocks
    /// until the user closes it: 0 with the text in `text` (the host's, valid until the plugin's next call
    /// to the host), -1 for Esc - and for an empty text without [`INPUT_ENABLE_EMPTY`].
    pub input: unsafe extern "C" fn(
        context: *mut c_void,
        title: NcStr,
        prompt: NcStr,
        initial: NcStr,
        history: NcStr,
        flags: u32,
        text: *mut NcStr,
    ) -> i32,
    /// A line in the plugin's log (`settings\Logs\<plugin>.log`): what a plugin would print goes here, as
    /// its stdout is the program's screen.
    pub log: unsafe extern "C" fn(context: *mut c_void, text: NcStr),
    /// A dialog (FAR's `DialogInit` / `DialogRun`): blocks until it closes. Returns the index of the button
    /// item that closed it, -1 for Esc (or the dialog gone another way, or asked while the plugin starts).
    /// Every item's last value is written to `values` (up to `count` of them; texts the host's, valid until
    /// the plugin's next call to the host). Its handler is called on the plugin's thread meanwhile.
    pub dialog: unsafe extern "C" fn(
        context: *mut c_void,
        dialog: *const NcDialog,
        values: *mut NcDialogValue,
        count: usize,
    ) -> i32,
    /// A change of a dialog's item asked from its handler, with the `handle` the handler was given (valid
    /// during that call only). 0: taken, -1: no such item or handle.
    pub dialog_change: unsafe extern "C" fn(
        context: *mut c_void,
        handle: *mut c_void,
        change: *const NcDialogChange,
    ) -> i32,
    /// The plugin's value of `key` (FAR's `PluginSettings::Get`; values are text, kept by the program in its
    /// settings as `plugins.<name>.settings.<key>`): 0 with the text in `value` (the host's, valid until
    /// the plugin's next call to the host), -1 when there is none.
    pub setting: unsafe extern "C" fn(context: *mut c_void, key: NcStr, value: *mut NcStr) -> i32,
    /// The plugin's value of `key` set to `value` (FAR's `PluginSettings::Set`): read back at once, saved
    /// with the program's settings.
    pub set_setting: unsafe extern "C" fn(context: *mut c_void, key: NcStr, value: NcStr),
    /// `len` bytes at `bytes` in the code page `code_page` ([`CODE_PAGE_ANSI`], [`CODE_PAGE_OEM`] or the
    /// page's number) as text in `text` (the host's, valid until the plugin's next call to the host;
    /// FAR's `MultiByteToWideChar`, what a plugin cannot read in Rust): 0, -1 for a page the system has
    /// not.
    pub decode: unsafe extern "C" fn(
        context: *mut c_void,
        code_page: u32,
        bytes: *const u8,
        len: usize,
        text: *mut NcStr,
    ) -> i32,
    /// How far the call running is (ADR-0019 §5): `done` of `total` (0: not known) and `text` (the item at
    /// hand). The program's window over a call that lasts shows it; never blocks.
    pub progress: unsafe extern "C" fn(context: *mut c_void, done: u64, total: u64, text: NcStr),
    /// Whether the user asked the call running to stop (FAR's `CheckForEsc`, without blocking): 1 yes,
    /// 0 no. A call that sees it ends at once with what it did, its usual answer (for a list: what was
    /// read; for a key: what to do with what was done). One that does not is let go after a while: its
    /// late answer is let be.
    pub cancelled: unsafe extern "C" fn(context: *mut c_void) -> i32,
    /// The panel `which` (`WHICH_*`) as it is now, into `state`: 0, -1 when the program is not
    /// listening (the plugin starting) or from another thread. In a panel's call (its folder listed, a
    /// file, a title) as the program last saw it; the program may be waiting for that call, so
    /// `panel_items` and `panel_do` answer -1 there.
    pub panel_state:
        unsafe extern "C" fn(context: *mut c_void, which: u32, state: *mut NcPanelState) -> i32,
    /// The panel's items from `from`, as many as `count` into `items` (`flags`: [`ITEMS_SELECTED`]):
    /// how many there are in all (from 0), -1 as for `panel_state`.
    pub panel_items: unsafe extern "C" fn(
        context: *mut c_void,
        which: u32,
        flags: u32,
        from: usize,
        items: *mut NcPanelItem,
        count: usize,
    ) -> isize,
    /// `action` done to the panel `which` (the command line, the viewer and the editor are no panel's:
    /// `which` is not read): 0, -1 when it could not be; a text answered in `answer` (when not null).
    pub panel_do: unsafe extern "C" fn(
        context: *mut c_void,
        which: u32,
        action: *const NcPanelAction,
        answer: *mut NcStr,
    ) -> i32,
    /// The plugin's [`NcPlugin::synchro`] called with `param` on its own thread once that thread is free
    /// (FAR's `ACTL_SYNCHRO`): any thread of the plugin's may ask (a worker that saw a change); never
    /// blocks. Asked again before it came, it comes once for each.
    pub synchro: unsafe extern "C" fn(context: *mut c_void, param: u64),
    /// The keys named in `keys` (separated by spaces: "Shift+F6", "Ctrl+R") done by the program as if
    /// typed, once the plugin's call has returned (FAR's `ACTL_POSTKEYSEQUENCE`, a macro's `Keys` posted):
    /// 0, -1 when a name is not a key (nothing done) or as for `panel_state`.
    pub post_keys: unsafe extern "C" fn(context: *mut c_void, keys: NcStr) -> i32,
    /// The program's screen into `screen` (FAR's `ACTL_GETFARRECT`, `ACTL_GETFARHWND`): 0, -1 as for
    /// `panel_state`.
    pub screen: unsafe extern "C" fn(context: *mut c_void, screen: *mut NcScreen) -> i32,
    /// Whether each of `count` entries at `entries` passes the filter of the panel `which` (Ctrl+I; FAR's
    /// `FileFilterControl` with `FFT_PANEL` and `FFCTL_ISFILEINFILTER`): `results[i]` 1 or 0, every one 1
    /// without a filter. 0, -1 as for `panel_state`.
    pub filter: unsafe extern "C" fn(
        context: *mut c_void,
        which: u32,
        entries: *const NcEntry,
        count: usize,
        results: *mut u8,
    ) -> i32,
    /// `text` put on the clipboard (FAR's `CopyToClipboard`): 0, -1 as for `panel_state`.
    pub clipboard: unsafe extern "C" fn(context: *mut c_void, text: NcStr) -> i32,
    /// The plugin's help (its folder's) at `topic` shown (FAR's `ShowHelp` with its module): 0, -1 when
    /// the program shows none (it has no help yet: the call is there for plugins written now).
    pub show_help: unsafe extern "C" fn(context: *mut c_void, topic: NcStr) -> i32,
    /// A plugin found, loaded or unloaded while the program runs (FAR's `PluginsControl`, farcmds'
    /// `load:` / `unload:`): `action` `PLUGINS_*`, `text` a plugin's name (looked for first) or an
    /// absolute path of its folder, its manifest or its library. 0 done (a load: started, its failure shown by the program), -1 not (no
    /// such plugin, a name loaded already) or as for `panel_state`. A plugin unloading itself is let go
    /// as its call ends.
    pub plugins_control:
        unsafe extern "C" fn(context: *mut c_void, action: u32, text: NcStr) -> i32,
    /// Whether the program asks before the work `name` (its Confirmations; FAR's `FSSF_CONFIRMATIONS`
    /// by FAR's names: `Delete`, `DeleteFolder`, `RO`, `Drag`, `Esc`, `Exit`, `HistoryClear`,
    /// `RemoveConnection`, whatever their case): 1 it asks, 0 it does not, -1 a name this build does not
    /// know or as for `panel_state`.
    pub confirmation: unsafe extern "C" fn(context: *mut c_void, name: NcStr) -> i32,
    /// Whether the file name `name` (no folder) matches `masks` in the program's masks (FAR's
    /// `ProcessName` with `PN_CMPNAMELIST`: `*.zip,*.rar`, `|` before the exclusions, `<group>`, regular
    /// expressions): 1 it does, 0 it does not, -1 masks with errors or as for `panel_state` (API 5).
    pub match_masks: unsafe extern "C" fn(context: *mut c_void, masks: NcStr, name: NcStr) -> i32,
    /// The stop the user asked of the call running forgotten: `cancelled` answers 0 again until the next
    /// Esc (arclite's "Do you want to interrupt current operation?" answered No; API 5).
    pub resume: unsafe extern "C" fn(context: *mut c_void),
    /// The plugin's data folder beside the program's settings (`settings\Plugins\<name>`; API 7), valid
    /// while it is loaded: what the user's work with it keeps (NetWay's sessions), never in `folder`, which
    /// an update of the plugin replaces. Not made by the host: the plugin makes it when it writes there.
    pub data_folder: NcStr,
    /// The `count` entries `names` of the folder `folder` of the plugin's panel `instance` copied (moved
    /// with [`TRANSFER_MOVE`]) into the disk's folder `disk`, or with [`TRANSFER_UPLOAD`] the disk's
    /// entries `names` of `disk` into `folder`, as one of the program's own background tasks (its
    /// progress dialog, its questions, F12's list): the task reads and writes through the plugin's file
    /// calls ([`NcPlugin::read`], [`NcPlugin::write`], ...). Asked from the plugin's thread (NetBox's
    /// "Transfer on background"); 0 when the task is started, -1 when not (API 9).
    pub transfer: unsafe extern "C" fn(
        context: *mut c_void,
        instance: NcStr,
        folder: NcStr,
        names: *const NcStr,
        count: usize,
        disk: NcStr,
        flags: u32,
    ) -> i32,
    /// `action` done to the editor up (FAR's `EditorControl`, API 10), asked from a call of the
    /// plugin's made while it is up (its [`PLACE_EDITOR`] item); the answer in `answer` (when not
    /// null): 0, -1 when it could not be (no editor, a line not there, lock mode for an edit) or as for
    /// `panel_state`.
    pub editor: unsafe extern "C" fn(
        context: *mut c_void,
        action: *const NcEditorAction,
        answer: *mut NcEditorAnswer,
    ) -> i32,
}

/// [`NcHost::transfer`]'s flags: from the disk into the panel.
pub const TRANSFER_UPLOAD: u32 = 1;
/// Moved: the sources removed once copied.
pub const TRANSFER_MOVE: u32 = 2;

/// [`NcHost::plugins_control`]'s actions: whether the plugin is loaded (FAR's `PCTL_FINDPLUGIN`).
pub const PLUGINS_FIND: u32 = 0;
/// The plugin loaded (`PCTL_LOADPLUGIN`).
pub const PLUGINS_LOAD: u32 = 1;
/// The plugin unloaded (`PCTL_UNLOADPLUGIN`).
pub const PLUGINS_UNLOAD: u32 = 2;

/// [`NcHost::decode`]'s page of the system's ANSI code (Windows' `CP_ACP`).
pub const CODE_PAGE_ANSI: u32 = 0;
/// The system's OEM code page, the console's (Windows' `CP_OEMCP`).
pub const CODE_PAGE_OEM: u32 = 1;

/// What a plugin tells about itself (FAR's "Plugin information": title, description, author, version).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcPluginInfo {
    pub title: NcStr,
    pub description: NcStr,
    pub author: NcStr,
    pub version: NcStr,
}

/// The program's menus a plugin puts items in ([`NcPlugin::item_count`]): "Plugin commands" (F11).
pub const PLACE_COMMANDS: u32 = 0;
/// The drive menu (Alt+F1 / Alt+F2, FAR's `DiskMenuStrings`), under the drives.
pub const PLACE_DISKS: u32 = 1;
/// Options > Plugins configuration (FAR's `PluginConfig`): running an item runs the plugin's configuration
/// (FAR's `ConfigureW`), and a non-empty `panel` answer of [`NcPlugin::run_item`] means "the settings
/// changed" (`ConfigureW` returning TRUE: the program reads the plugin's items and its panels again).
pub const PLACE_CONFIG: u32 = 2;
/// Not a menu: its command line prefixes, an item each, without the `:` (FAR's `CommandPrefix`, there
/// split by `:`); `prefix:text` typed in the command line goes to [`NcPlugin::open_command`]. Not run.
pub const PLACE_PREFIXES: u32 = 3;
/// Not a menu: the masks of the files it opens, an item each, in the program's masks (`*.temp,*.lst`;
/// FAR's plugins check them in `AnalyseW`); Enter or Ctrl+PgDn on such a file goes to
/// [`NcPlugin::open_file`]. Not run.
pub const PLACE_FILE_MASKS: u32 = 4;
/// Not a menu: one item (its text not read) when the plugin takes Alt+F7's found files (FAR's
/// `SetFindListW`): the results' "Panel" hands them to [`NcPlugin::find_list`]. Not run.
pub const PLACE_FIND_LIST: u32 = 5;
/// The editor's "Plugin commands" (F11 in the editor; FAR's `PF_EDITOR` items, `OpenW` with
/// `OPEN_EDITOR`; API 10): its item runs with the editor up, which [`NcHost::editor`] reads and changes.
pub const PLACE_EDITOR: u32 = 6;
/// Not a menu: one item (its text not read) when the plugin takes the editor's keys (FAR's
/// `ProcessEditorInputW`, API 10): each key the editor has done goes to [`NcPlugin::editor_key`], the
/// keys after it waiting for its end. Not run.
pub const PLACE_EDITOR_KEYS: u32 = 7;

/// What a plugin gives the host: its version, its state and its functions. Every function is called with
/// the table's own `state`, one call at a time, never after `release`: that is what makes each call safe.
#[repr(C)]
pub struct NcPlugin {
    pub api_version: u32,
    /// `size_of::<NcPlugin>()` of the plugin's build.
    pub size: usize,
    /// [`ABI`] of the plugin's build.
    pub abi: u64,
    /// The plugin's own, handed back with every call.
    pub state: *mut c_void,
    pub info: unsafe extern "C" fn(state: *mut c_void) -> NcPluginInfo,
    /// How many items it puts in the program's menu `place` (`PLACE_*`; another place: none).
    pub item_count: unsafe extern "C" fn(state: *mut c_void, place: u32) -> usize,
    /// The item `index` of `place` (below the last [`NcPlugin::item_count`] of that place).
    pub item: unsafe extern "C" fn(state: *mut c_void, place: u32, index: usize) -> NcStr,
    /// Runs the item `index` of `place`, chosen by the user. 0: done, other: it failed (a panic caught, an
    /// error it reported), why in [`NcPlugin::last_error`]. Done, a non-empty `panel` (written by the
    /// plugin, the host's empty before) names the instance of a panel of the plugin to open at its root
    /// (FAR's `OpenW` returning a panel); for [`PLACE_CONFIG`] it means the settings changed.
    pub run_item: unsafe extern "C" fn(
        state: *mut c_void,
        place: u32,
        index: usize,
        panel: *mut NcStr,
    ) -> i32,
    /// Why the last call failed.
    pub last_error: unsafe extern "C" fn(state: *mut c_void) -> NcStr,
    /// The last call: the plugin frees its state and this table, and ends any thread it started.
    pub release: unsafe extern "C" fn(state: *mut c_void),
    /// The folder `path` (`/`-separated, absolute) of the panel `instance`: `*count` entries at `*entries`,
    /// valid until the plugin's next call. 0: read, else an `ERROR_*` code and why in `last_error`.
    pub list: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        entries: *mut *const NcEntry,
        count: *mut usize,
    ) -> i32,
    /// What the panel `instance` shows at `path` besides its entries (FAR's `GetOpenPanelInfo`), valid
    /// until the plugin's next call.
    pub panel_info:
        unsafe extern "C" fn(state: *mut c_void, instance: NcStr, path: NcStr) -> NcPanelInfo,
    /// The panel `instance` is no longer shown (FAR's `ClosePanel`).
    pub close_panel: unsafe extern "C" fn(state: *mut c_void, instance: NcStr),
    /// One entry of the panel `instance`: `*entry` (its name valid until the plugin's next call). The
    /// root `/` is a folder with an empty name.
    pub stat: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        entry: *mut NcEntry,
    ) -> i32,
    /// Up to `size` bytes of the file `path` from `offset` into `buffer` (the host's), `*done` of them;
    /// fewer only at the file's end (0 past it).
    pub read: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        offset: u64,
        buffer: *mut u8,
        size: usize,
        done: *mut usize,
    ) -> i32,
    /// `size` bytes of `data` written to the file `path`: `WRITE_NEW` makes it (never over an existing
    /// entry: `ERROR_EXISTS`), `WRITE_APPEND` adds them at its end. A file copied in comes as one
    /// `WRITE_NEW` (maybe of no bytes) and `WRITE_APPEND`s, then (API 9) one call of no bytes: `WRITE_END`
    /// when the file is whole - a plugin that streams it to a server finishes it there, its failure the
    /// copy's - or `WRITE_ABORT` when its copy failed or stopped - what was kept for it let go, never
    /// sent as the file (the program removes what was made).
    pub write: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        mode: u32,
        data: *const u8,
        size: usize,
    ) -> i32,
    /// Makes the folder `path` (its parent exists; an entry of that name: `ERROR_EXISTS`).
    pub make_dir: unsafe extern "C" fn(state: *mut c_void, instance: NcStr, path: NcStr) -> i32,
    /// Renames `from` to `to` in the panel `instance`; an existing `to` is replaced only with a non-zero
    /// `replace` (else `ERROR_EXISTS`).
    pub rename: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        from: NcStr,
        to: NcStr,
        replace: u32,
    ) -> i32,
    /// Removes one entry: `REMOVE_FILE` a file, `REMOVE_FOLDER` an empty folder (FAR's `DeleteFiles`).
    pub remove:
        unsafe extern "C" fn(state: *mut c_void, instance: NcStr, path: NcStr, how: u32) -> i32,
    /// A key the panel `instance` names in [`NcPanelInfo::keys`] pressed on it at `path` (FAR's
    /// `ProcessPanelInput`, the program never waiting for it), as `*event` tells it. Not a quiet call:
    /// the plugin may show windows. What the program then does is in `*answer` (`KEY_*`).
    pub panel_key: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        event: *const NcKeyEvent,
        answer: *mut NcKeyAnswer,
    ) -> i32,
    /// The files and folders on a disk `*put` names copied or moved into the folder `path` of a panel
    /// that takes paths ([`PANEL_TAKES_PATHS`], FAR's `PutFiles`): the plugin takes them as it does
    /// (TmpPanel adds references, an archive is written anew with them); the host removes nothing after
    /// it. Not a panel's quiet call: the plugin may show windows. 0 done, [`ERROR_CANCELLED`] stopped by
    /// the user, else why in `last_error` (API 5: the request in a struct, with FAR's operation mode).
    pub put: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        put: *const NcPut,
    ) -> i32,
    /// `prefix:command` run from the command line, `prefix` one of its [`PLACE_PREFIXES`] (FAR's `OpenW`
    /// with `OPEN_COMMANDLINE`, `command` the text after the `:`); `folder` the program's folder for
    /// names given without one (FAR's current folder; empty: none). Not a quiet call. Done, what the
    /// program does next is in `*answer`: [`KEY_OPEN`] its panel `path` in the active panel (FAR's
    /// `ProcessPluginPanel`), [`KEY_ACTIVE_FOLDER`], [`KEY_COMMAND_LINE`], [`KEY_SHELL_OPEN`]; none:
    /// nothing.
    pub open_command: unsafe extern "C" fn(
        state: *mut c_void,
        prefix: NcStr,
        command: NcStr,
        folder: NcStr,
        answer: *mut NcKeyAnswer,
    ) -> i32,
    /// The file `path` on a disk, one of its [`PLACE_FILE_MASKS`], offered as `alternative` says
    /// (`OPEN_*`: Enter, Ctrl+PgDn, Shift+F2, Shift+F3; FAR's `AnalyseW`, then `OpenW`); `folder` as for
    /// [`NcPlugin::open_command`]. Not a quiet call. Done, in `*answer`: [`KEY_PASS`] not its file (the
    /// next plugin's, then the program's own, as `AnalyseW` returning FALSE), [`KEY_OPEN`] its panel
    /// `path` over the file's (FAR's `PushPrev`: ".." at its root back to the file), the other places as
    /// for a command; none: done by the plugin itself (FAR's `PANEL_STOP`).
    pub open_file: unsafe extern "C" fn(
        state: *mut c_void,
        path: NcStr,
        alternative: u32,
        folder: NcStr,
        answer: *mut NcKeyAnswer,
    ) -> i32,
    /// Alt+F7's found files and folders (`count` entries at `found`, each named by its absolute path,
    /// with what the search saw of it: size, times, flags; FAR's `SetFindList` gets the search's
    /// `PluginPanelItem`s) for a panel of the plugin's (FAR's `OpenW` with `OPEN_FINDLIST`, then
    /// `SetFindList`). Not a quiet call. Done, in `*answer`: [`KEY_OPEN`] its panel `path` in the active
    /// panel; [`KEY_PASS`] not taken (the next plugin's).
    pub find_list: unsafe extern "C" fn(
        state: *mut c_void,
        found: *const NcEntry,
        count: usize,
        answer: *mut NcKeyAnswer,
    ) -> i32,
    /// What the plugin asked with [`NcHost::synchro`], on its own thread (FAR's `ProcessSynchroEventW`):
    /// not a panel's quiet call, it may do anything a command may (a panel read again).
    pub synchro: unsafe extern "C" fn(state: *mut c_void, param: u64),
    /// The file or folder `path` of the panel `instance`, just put in by a copy or a move (a file once
    /// written, a folder once what went into it is there), given what it keeps of its source: `*times`
    /// (FAR's `PutFiles` items' times and attributes). [`ERROR_UNSUPPORTED`]: the plugin keeps none.
    pub set_times: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        times: *const NcTimes,
    ) -> i32,
    /// The entries `*get` names in the folder `path` of the panel `instance` written out to a disk (FAR's
    /// `GetFilesW`, API 5; a panel that sets [`PANEL_GIVES_FILES`]). Not a panel's quiet call: the plugin
    /// shows its windows (none with [`GET_FIND`] or [`GET_QUICK_VIEW`]). 0 done, [`ERROR_CANCELLED`]
    /// stopped by the user, else why in `last_error`.
    pub get: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        get: *const NcGet,
    ) -> i32,
    /// The `count` entries named at `names` in the folder `path` of the panel `instance` deleted by the
    /// plugin as it does it (FAR's `DeleteFilesW`, API 5; a panel that sets [`PANEL_DELETES`]): the
    /// plugin asks itself, the program shows no question; `mode` `GET_*` as FAR's `OPM_*` (0: F8). Not
    /// a panel's quiet call. 0 done, [`ERROR_CANCELLED`] stopped by the user, else why in `last_error`.
    pub delete_files: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        names: *const NcStr,
        count: usize,
        mode: u32,
    ) -> i32,
    /// A folder made in the folder `path` of the panel `instance` by the plugin as it does it (FAR's
    /// `MakeDirectoryW`, API 5; a panel that sets [`PANEL_MAKES_FOLDERS`]): `*name` comes as the name
    /// asked for (empty: F7, the plugin asks it itself), and goes back as the name made (the plugin's,
    /// valid until its next call), which the cursor goes to. Not a panel's quiet call. 0 done,
    /// [`ERROR_CANCELLED`] stopped by the user, else why in `last_error`.
    pub make_folder: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        mode: u32,
        name: *mut NcStr,
    ) -> i32,
    /// A panel of the plugin's with nothing in it yet, for Shift+F1 (FAR's `OpenFilePlugin` with no
    /// file, `OFP_CREATE`: `AnalyseW` without a name, then `OpenW` with `OPEN_ANALYSE`; API 5): its
    /// instance in `*instance` (the plugin's, valid until its next call), which the program puts the
    /// active panel's files into ([`NcPlugin::put`], its folder `/`) and then closes; empty: the plugin
    /// makes none (the next plugin's). Not a panel's quiet call. 0 done, else why in `last_error`.
    pub open_new: unsafe extern "C" fn(state: *mut c_void, instance: *mut NcStr) -> i32,
    /// The `count` entries named at `names` in the folder `path` of the panel `instance` handed to the
    /// plugin's commands on the file the panel was opened from (FAR's `ProcessHostFileW`, API 5:
    /// Shift+F3, "Archive commands"): the plugin asks what to do itself (arclite's menu "Test archive
    /// files"); `mode` `GET_*` as FAR's `OPM_*` (0: the panel's key). What it did goes in `*done`
    /// ([`HOST_FILE_CHANGED`], [`HOST_FILE_UNSELECT`]; 0: nothing). Not a panel's quiet call. 0 done,
    /// [`ERROR_CANCELLED`] stopped by the user, else why in `last_error`.
    pub process_host_file: unsafe extern "C" fn(
        state: *mut c_void,
        instance: NcStr,
        path: NcStr,
        names: *const NcStr,
        count: usize,
        mode: u32,
        done: *mut u32,
    ) -> i32,
    /// The key `key` (its name as [`NcHost::post_keys`] takes it) just done by the editor, for a plugin
    /// with a [`PLACE_EDITOR_KEYS`] item (FAR's `ProcessEditorInputW`, which here comes after the
    /// editor's own handling, as AutoWrap's `ECTL_PROCESSINPUT` makes it; API 10): it may change the
    /// editor through [`NcHost::editor`]; the keys typed after it wait for its end.
    pub editor_key: unsafe extern "C" fn(state: *mut c_void, key: NcStr) -> i32,
}

/// [`NcPlugin::write`]'s modes.
pub const WRITE_NEW: u32 = 0;
pub const WRITE_APPEND: u32 = 1;
/// No more is written to it: it is whole (API 9).
pub const WRITE_END: u32 = 2;
/// No more is written to it: its copy failed or stopped (API 9).
pub const WRITE_ABORT: u32 = 3;

/// [`NcPlugin::remove`]'s kinds.
pub const REMOVE_FILE: u32 = 0;
pub const REMOVE_FOLDER: u32 = 1;

/// A panel's call failed ([`NcPlugin::list`] and the like; 0: done), as the program's errors tell apart.
pub const ERROR_OTHER: i32 = 1;
pub const ERROR_NOT_FOUND: i32 = 2;
pub const ERROR_DENIED: i32 = 3;
pub const ERROR_EXISTS: i32 = 4;
pub const ERROR_UNSUPPORTED: i32 = 5;
/// The user stopped it (Esc at the plugin's question): nothing done.
pub const ERROR_CANCELLED: i32 = 6;

/// The function a plugin library exports as [`ENTRY`]: the plugin's table for the host's, null when it
/// cannot start. The table lives until its `release`.
pub type EntryFn = unsafe extern "C" fn(host: *const NcHost) -> *const NcPlugin;
