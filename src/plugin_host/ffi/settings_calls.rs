//! A plugin's settings (`NcHost::setting`, `NcHost::set_setting`; FAR's `PluginSettings`): its values are
//! kept on its thread, read back at once, and each one set is sent to the program, which saves it with its
//! settings as `plugins.<name>.settings.<key>`.

use std::ffi::c_void;

use nc_plugin_api::NcStr;

use super::calls::context;
use super::text;
use crate::plugin_host::Event;

/// `NcHost::setting`.
pub unsafe extern "C" fn setting(context: *mut c_void, key: NcStr, value: *mut NcStr) -> i32 {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        // Another thread of the plugin's (review R45).
        return -1;
    };
    let Some(found) = context.settings.borrow().get(&text(key)).cloned() else {
        return -1;
    };
    let mut answer = context.answer.borrow_mut();
    *answer = found;
    if !value.is_null() {
        // SAFETY: the plugin hands a writable `NcStr` or null; the text stays in the context until the
        // next answer replaces it.
        unsafe { value.write(NcStr::new(&answer)) };
    }
    0
}

/// `NcHost::set_setting`.
pub unsafe extern "C" fn set_setting(context: *mut c_void, key: NcStr, value: NcStr) {
    // SAFETY: the host's own context, handed back by the plugin.
    let Some(context) = (unsafe { self::context(context) }) else {
        // Another thread of the plugin's (review R45).
        return;
    };
    let (key, value) = (text(key), text(value));
    (context.settings.borrow_mut()).insert(key.clone(), value.clone());
    // The program not listening (it ends): the value is the plugin's still while it runs.
    let _ = context.events.send(Event::Setting {
        plugin: context.plugin.clone(),
        key,
        value,
    });
}

#[cfg(test)]
#[path = "settings_calls_tests.rs"]
mod tests;
