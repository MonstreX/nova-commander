//! NetBox's "Synchronize" dialog (`TFullSynchronizeDialog`): the local and remote folders, " Direction/Target
//! directory " (Both, Remote, Local), " Mode " (Synchronize files, Mirror files, Synchronize timestamps),
//! " Synchronize options " (Delete files, Existing files only, Preview changes, Selected files only),
//! " Compare criteria " (Modification time, File size - "Same size only" for timestamps), "Use same options
//! next time"; its `UpdateControls` rules; the options kept as WinSCP's `SynchronizeParams` (its `sp*`
//! flags) and `SynchronizeModeAuto`.

use nc_plugin_sdk::{Controls, DialogItem, Host, Value};

use crate::placed::{at, heading, line};
use crate::sync_collect::{Mode, Settings};

/// What the dialog gives besides the comparison's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    pub settings: Settings,
    pub local: String,
    pub remote: String,
    pub preview: bool,
    pub selected_only: bool,
    pub save: bool,
}

/// WinSCP's `TTerminal::sp*` flags kept.
pub const DELETE: i64 = 0x01;
pub const EXISTING_ONLY: i64 = 0x04;
pub const PREVIEW: i64 = 0x40;
pub const TIMESTAMP: i64 = 0x100;
pub const NOT_BY_TIME: i64 = 0x200;
pub const BY_SIZE: i64 = 0x400;
pub const SELECTED_ONLY: i64 = 0x800;
pub const MIRROR: i64 = 0x1000;

/// The keys in the program's settings of the plugin (WinSCP's names).
const PARAMS_KEY: &str = "SynchronizeParams";
const MODE_KEY: &str = "SynchronizeModeAuto";

/// WinSCP's default: changes previewed, by time.
const DEFAULT_PARAMS: i64 = PREVIEW;

// The items, in their order.
const LOCAL: usize = 1;
const REMOTE: usize = 3;
pub const BOTH: usize = 5;
const TO_REMOTE: usize = 6;
const TO_LOCAL: usize = 7;
pub const FILES: usize = 9;
pub const MIRROR_ITEM: usize = 10;
pub const TIMESTAMPS: usize = 11;
pub const DELETE_ITEM: usize = 13;
const EXISTING: usize = 14;
const PREVIEW_ITEM: usize = 15;
const SELECTED: usize = 16;
pub const BY_TIME: usize = 18;
pub const BY_SIZE_ITEM: usize = 19;
const SAVE: usize = 20;
pub const OK: usize = 22;

/// WinSCP's `TSynchronizeMode` values (`smRemote`, `smLocal`, `smBoth`).
fn mode_value(mode: Mode) -> i64 {
    match mode {
        Mode::Remote => 0,
        Mode::Local => 1,
        Mode::Both => 2,
    }
}

fn mode_of(value: i64) -> Option<Mode> {
    match value {
        0 => Some(Mode::Remote),
        1 => Some(Mode::Local),
        2 => Some(Mode::Both),
        _ => None,
    }
}

/// The kept params and the kept mode (none: from the panel the command came from - WinSCP's
/// `SynchronizeModeAuto` of -1).
pub fn kept(host: &Host) -> (i64, Option<Mode>) {
    let params = host.int_setting(PARAMS_KEY, DEFAULT_PARAMS);
    (params, mode_of(host.int_setting(MODE_KEY, -1)))
}

/// The params `chosen` keeps (WinSCP's `Execute` result), `mode` kept too when asked.
pub fn keep(host: &Host, chosen: &Chosen, mode: bool) {
    host.set_setting(PARAMS_KEY, &params_of(chosen).to_string());
    if mode {
        host.set_setting(MODE_KEY, &mode_value(chosen.settings.mode).to_string());
    }
}

/// The `sp*` flags of `chosen`.
pub fn params_of(chosen: &Chosen) -> i64 {
    let settings = &chosen.settings;
    let flag = |on: bool, bit: i64| if on { bit } else { 0 };
    flag(settings.delete, DELETE)
        | flag(settings.existing_only, EXISTING_ONLY)
        | flag(chosen.preview, PREVIEW)
        | flag(chosen.selected_only, SELECTED_ONLY)
        | flag(settings.timestamps, TIMESTAMP)
        | flag(settings.mirror, MIRROR)
        | flag(!settings.by_time, NOT_BY_TIME)
        | flag(settings.by_size, BY_SIZE)
}

/// The dialog's items for `mode` and `params` over `local` and `remote`; `timestamps_too`: the server can set
/// times (NetBox's `fsoDisableTimestamp` otherwise), `selection`: the panel has items selected.
pub fn items(
    (local, remote): (&str, &str),
    (mode, params): (Mode, i64),
    (timestamps_too, selection): (bool, bool),
) -> Vec<DialogItem> {
    let set = |bit: i64| params & bit != 0;
    let timestamps = set(TIMESTAMP) && timestamps_too;
    vec![
        at(DialogItem::text("Lo&cal directory:"), 5, 2),
        line(
            DialogItem::edit(local).history("WinscpSynchronizeLocal", false),
            5,
            3,
            66,
        ),
        at(DialogItem::text("Remo&te directory:"), 5, 4),
        line(
            DialogItem::edit(remote).history("WinscpSynchronizeRemote", false),
            5,
            5,
            66,
        ),
        heading("Direction/Target directory", 4, 6, 68),
        at(DialogItem::radio("&Both", mode == Mode::Both).group(), 5, 7),
        at(DialogItem::radio("&Remote", mode == Mode::Remote), 24, 7),
        at(DialogItem::radio("&Local", mode == Mode::Local), 41, 7),
        heading("Mode", 4, 8, 68),
        at(
            DialogItem::radio("Synchronize &files", !timestamps && !set(MIRROR)).group(),
            5,
            9,
        ),
        at(
            DialogItem::radio("&Mirror files", set(MIRROR) && !timestamps),
            27,
            9,
        ),
        at(
            DialogItem::radio("Synchronize &timestamps", timestamps).disabled(!timestamps_too),
            44,
            9,
        ),
        heading("Synchronize options", 4, 10, 68),
        at(DialogItem::check("&Delete files", set(DELETE)), 5, 11),
        at(
            DialogItem::check("&Existing files only", set(EXISTING_ONLY)),
            37,
            11,
        ),
        at(DialogItem::check("Pre&view changes", set(PREVIEW)), 5, 12),
        at(
            DialogItem::check("Selected files o&nly", set(SELECTED_ONLY) && selection)
                .disabled(!selection),
            37,
            12,
        ),
        heading("Compare criteria", 4, 13, 68),
        at(
            DialogItem::check("&Modification time", !set(NOT_BY_TIME)),
            5,
            14,
        ),
        at(DialogItem::check("File si&ze", set(BY_SIZE)), 37, 14),
        at(
            DialogItem::check("Use same &options next time", false),
            5,
            16,
        ),
        DialogItem::separator().at(-1, 17, -1, 17),
        at(DialogItem::button("OK").default_button(), 28, 18),
        at(DialogItem::button("Cancel"), 35, 18),
    ]
}

/// What the dialog's `values` give.
pub fn chosen(values: &[Value]) -> Chosen {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let checked = |item: usize| value(item).checked();
    let mode = match () {
        () if checked(BOTH) => Mode::Both,
        () if checked(TO_LOCAL) => Mode::Local,
        () if checked(TO_REMOTE) => Mode::Remote,
        () => Mode::default(),
    };
    Chosen {
        settings: Settings {
            mode,
            delete: checked(DELETE_ITEM),
            existing_only: checked(EXISTING),
            timestamps: checked(TIMESTAMPS),
            mirror: checked(MIRROR_ITEM),
            by_time: checked(BY_TIME),
            by_size: checked(BY_SIZE_ITEM),
        },
        local: value(LOCAL).text.trim().to_string(),
        remote: value(REMOTE).text.trim().to_string(),
        preview: checked(PREVIEW_ITEM),
        selected_only: checked(SELECTED),
        save: checked(SAVE),
    }
}

/// NetBox's `UpdateControls`: timestamps take existing files by time and delete nothing; both ways take
/// no size and no mirroring; mirroring compares times; one criterion at least.
pub fn update(controls: &mut Controls) {
    let checked = |controls: &Controls, item: usize| controls.value(item).checked();
    let (both, timestamps) = (checked(controls, BOTH), checked(controls, TIMESTAMPS));
    if timestamps {
        controls.set_checked(EXISTING, true);
        controls.set_checked(DELETE_ITEM, false);
        controls.set_checked(BY_TIME, true);
    }
    if both {
        controls.set_checked(BY_SIZE_ITEM, false);
        if checked(controls, MIRROR_ITEM) {
            controls.set_checked(FILES, true);
            controls.set_checked(MIRROR_ITEM, false);
        }
    }
    let mirror = checked(controls, MIRROR_ITEM);
    if mirror {
        controls.set_checked(BY_TIME, true);
    }
    controls.enable(MIRROR_ITEM, !both);
    controls.enable(DELETE_ITEM, !both && !timestamps);
    controls.enable(EXISTING, !timestamps);
    controls.enable(BY_TIME, !both && !timestamps && !mirror);
    controls.enable(BY_SIZE_ITEM, !both);
    let size_label = if timestamps {
        "Same si&ze only"
    } else {
        "File si&ze"
    };
    controls.set_text(BY_SIZE_ITEM, size_label);
    if !checked(controls, BY_SIZE_ITEM) && !checked(controls, BY_TIME) {
        controls.set_checked(BY_TIME, true);
    }
}

/// NetBox's `NB_SAVE_SYNCHRONIZE_MODE`.
pub const SAVE_MODE: &str = "You have overridden pre-selected synchronization direction. By default the direction is determined from file panel that was active before you run the synchronize function.\n \nDo you want to make direction you have selected the default?";

#[cfg(test)]
#[path = "sync_dialog_tests.rs"]
mod tests;
