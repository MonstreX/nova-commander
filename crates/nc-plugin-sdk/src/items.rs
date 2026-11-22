//! A plugin's items in the program's menus through the table (FAR's `GetPluginInfo` lists, `OpenW` from a
//! menu): their counts and labels by place (`PLACE_*`), one of them run.

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::api::{
    NcStr, PLACE_COMMANDS, PLACE_CONFIG, PLACE_DISKS, PLACE_EDITOR, PLACE_EDITOR_KEYS,
    PLACE_FILE_MASKS, PLACE_FIND_LIST, PLACE_PREFIXES,
};
use crate::{CHANGED, Plugin, State, panic_text, state};

/// The labels of `place` (`PLACE_*`), none for a place this build does not know.
fn place_labels<P>(state: &mut State<P>, place: u32) -> Option<&mut Vec<String>> {
    state.labels.get_mut(usize::try_from(place).ok()?)
}

pub(crate) unsafe extern "C" fn item_count<P: Plugin>(raw: *mut c_void, place: u32) -> usize {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    let plugin = &state.plugin;
    let told = catch_unwind(AssertUnwindSafe(|| match place {
        PLACE_COMMANDS => plugin.commands(),
        PLACE_DISKS => plugin.disks(),
        PLACE_CONFIG => plugin.config_items(),
        PLACE_PREFIXES => plugin.prefixes(),
        PLACE_FILE_MASKS => plugin.file_masks(),
        PLACE_FIND_LIST if plugin.takes_find_list() => vec![String::new()],
        PLACE_EDITOR => plugin.editor_items(),
        PLACE_EDITOR_KEYS if plugin.takes_editor_keys() => vec![String::new()],
        _ => Vec::new(),
    }))
    .unwrap_or_default();
    place_labels(state, place).map_or(0, |labels| {
        *labels = told;
        labels.len()
    })
}

pub(crate) unsafe extern "C" fn item<P: Plugin>(
    raw: *mut c_void,
    place: u32,
    index: usize,
) -> NcStr {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    let label = place_labels(state, place)
        .and_then(|labels| labels.get(index).cloned())
        .unwrap_or_default();
    state.keep(label)
}

pub(crate) unsafe extern "C" fn run_item<P: Plugin>(
    raw: *mut c_void,
    place: u32,
    index: usize,
    panel: *mut NcStr,
) -> i32 {
    // SAFETY: the host passes back the state of this table.
    let state = unsafe { state::<P>(raw) };
    state.kept.clear();
    let plugin = &mut state.plugin;
    let result = catch_unwind(AssertUnwindSafe(|| match place {
        PLACE_COMMANDS => plugin.run_command(index),
        PLACE_DISKS => plugin.open_disk(index),
        PLACE_EDITOR => plugin.run_editor_item(index).map(|()| None),
        // Changed: a non-empty answer (the ABI's `PLACE_CONFIG`).
        PLACE_CONFIG => Ok(plugin.configure(index).then(|| CHANGED.to_string())),
        _ => Err(format!("The plugin has no place {place}")),
    }));
    state.error = match result {
        Ok(Ok(opened)) => {
            if let Some(instance) = opened.filter(|instance| !instance.is_empty())
                && !panel.is_null()
            {
                let instance = state.keep(instance);
                // SAFETY: the host's place for the answer, alive through the call; the text is kept
                // until the plugin's next call.
                unsafe { *panel = instance };
            }
            return 0;
        }
        Ok(Err(error)) => error,
        Err(_) => panic_text(),
    };
    1
}
