//! A plugin's settings (FAR's `PluginSettings`): text values by key, kept by the program in its settings
//! (`plugins.<name>.settings.<key>`) and saved with them. Typed helpers read and write FAR's numbers and
//! flags as text: a flag "1" or "0", a number in decimal.

use crate::Host;
use crate::api::NcStr;

impl Host {
    /// The plugin's value of `key` (FAR's `PluginSettings::Get`); `None` when there is none.
    pub fn setting(&self, key: &str) -> Option<String> {
        let mut value = NcStr::EMPTY;
        let table = self.table();
        // SAFETY: the host's own function with its own context, on the plugin's thread; the key lives
        // through the call and the host's answer is copied before the next call to the host.
        unsafe {
            let found = (table.setting)(table.context, NcStr::new(key), &raw mut value);
            (found == 0).then(|| value.as_str().to_string())
        }
    }

    /// The plugin's value of `key` set to `value` (FAR's `PluginSettings::Set`).
    pub fn set_setting(&self, key: &str, value: &str) {
        let table = self.table();
        // SAFETY: as in `setting`; the key and the value live through the call.
        unsafe { (table.set_setting)(table.context, NcStr::new(key), NcStr::new(value)) }
    }

    /// A flag ("1" on, "0" off); `default` when there is none or it is neither.
    pub fn bool_setting(&self, key: &str, default: bool) -> bool {
        match self.setting(key).as_deref().map(str::trim) {
            Some("1") => true,
            Some("0") => false,
            _ => default,
        }
    }

    /// A number; `default` when there is none or it is not one.
    pub fn int_setting(&self, key: &str, default: i64) -> i64 {
        (self.setting(key))
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(default)
    }

    /// A flag kept as "1" or "0".
    pub fn set_bool(&self, key: &str, value: bool) {
        self.set_setting(key, if value { "1" } else { "0" });
    }

    /// A number kept in decimal.
    pub fn set_int(&self, key: &str, value: i64) {
        self.set_setting(key, &value.to_string());
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
