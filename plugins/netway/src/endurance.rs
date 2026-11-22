//! NetBox's "Endurance settings" (`WinSCPDialogs.cpp` `EnduranceConfigurationDialog`; WinSCP's
//! `TConfiguration` `SessionReopenAuto`, `SessionReopenAutoMaximumNumberOfRetries`,
//! `SessionReopenAutoStall` and the default copy parameters' `ResumeSupport`, `ResumeThreshold`): "Enable
//! transfer resume/transfer to temporary filename for" all files, files above a size or none; "Automatic
//! reconnect" after some seconds with at most some retries; "Timeouts": the wait for a reply dialog's
//! (kept; the program's own wait for a host that does not answer is its own, ADR-0019).

use nc_plugin_sdk::{Dialog, DialogEvent, DialogItem, Host, Value};

use crate::copy_params::{CopyParams, Resume};

/// WinSCP's reconnecting options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reconnect {
    /// Milliseconds before a lost session is opened again unasked; 0: asked.
    pub auto: u32,
    /// The most tries in a row (0: no end).
    pub retries: u32,
    /// The reply dialog's wait, in milliseconds.
    pub stall: u32,
}

impl Default for Reconnect {
    /// WinSCP's `TConfiguration::Default` (`CONST_DEFAULT_NUMBER_OF_RETRIES` 2).
    fn default() -> Self {
        Self {
            auto: 5000,
            retries: 2,
            stall: 10_000,
        }
    }
}

const AUTO: &str = "SessionReopenAuto";
const RETRIES: &str = "SessionReopenAutoMaximumNumberOfRetries";
const STALL: &str = "SessionReopenAutoStall";

fn number(host: &Host, key: &str, default: u32) -> u32 {
    u32::try_from(host.int_setting(key, i64::from(default))).unwrap_or(default)
}

pub fn reconnect(host: &Host) -> Reconnect {
    let base = Reconnect::default();
    Reconnect {
        auto: number(host, AUTO, base.auto),
        retries: number(host, RETRIES, base.retries),
        stall: number(host, STALL, base.stall),
    }
}

pub fn save_reconnect(host: &Host, reconnect: Reconnect) {
    host.set_int(AUTO, i64::from(reconnect.auto));
    host.set_int(RETRIES, i64::from(reconnect.retries));
    host.set_int(STALL, i64::from(reconnect.stall));
}

// The items, in their order (FAR's: the radio buttons together, the threshold after them).
const ALL: usize = 1;
const SMART: usize = 2;
const OFF: usize = 3;
const THRESHOLD: usize = 4;
const AUTO_CHECK: usize = 7;
const AUTO_SECONDS: usize = 8;
const RETRIES_EDIT: usize = 11;
const STALL_EDIT: usize = 15;
pub const OK: usize = 18;

fn at(item: DialogItem, x: i32, y: i32, width: i32) -> DialogItem {
    item.at(x, y, x + width.max(1) - 1, y)
}

fn titled(title: &str, y: i32) -> DialogItem {
    DialogItem {
        text: title.to_string(),
        ..DialogItem::separator()
    }
    .at(-1, y, -1, y)
}

/// The dialog's items for the resume of `params` and `reconnect`.
pub fn items(params: &CopyParams, reconnect: Reconnect) -> Vec<DialogItem> {
    let smart = params.resume == Resume::Smart;
    let auto = reconnect.auto > 0;
    let seconds = if auto { reconnect.auto / 1000 } else { 5 };
    vec![
        titled(
            " Enable transfer resume/transfer to temporary filename for ",
            2,
        ),
        at(
            DialogItem::radio("A&ll files (not recommended)", params.resume == Resume::On).group(),
            5,
            3,
            0,
        ),
        at(DialogItem::radio("Files abo&ve", smart), 5, 4, 0),
        at(
            DialogItem::radio("Di&sable", params.resume == Resume::Off),
            5,
            5,
            0,
        ),
        at(
            DialogItem::fixed("9999999", &(params.resume_threshold / 1024).to_string()),
            22,
            4,
            9,
        )
        .disabled(!smart),
        at(DialogItem::text("KB"), 32, 4, 2).disabled(!smart),
        titled(" Automatic reconnect ", 7),
        at(DialogItem::check("&Reconnect after", auto), 5, 8, 0),
        at(DialogItem::fixed("999", &seconds.to_string()), 26, 8, 5).disabled(!auto),
        at(DialogItem::text("seconds"), 32, 8, 7).disabled(!auto),
        at(DialogItem::text("Ma&ximum number of retries: "), 8, 9, 28).disabled(!auto),
        at(
            DialogItem::fixed("999", &reconnect.retries.min(99).to_string()),
            37,
            9,
            5,
        )
        .disabled(!auto),
        at(DialogItem::text("(0-99)"), 43, 9, 6).disabled(!auto),
        titled(" Timeouts ", 11),
        at(
            DialogItem::text("Wait for reply dialog timeout: "),
            5,
            12,
            31,
        ),
        at(
            DialogItem::fixed("999", &(reconnect.stall / 1000).to_string()),
            37,
            12,
            5,
        ),
        at(DialogItem::text("seconds"), 43, 12, 7),
        DialogItem::separator().at(-1, 13, -1, 13),
        at(DialogItem::button("&Ok").default_button(), 29, 14, 6),
        at(DialogItem::button("&Cancel"), 36, 14, 10),
    ]
}

fn read(value: &Value) -> u32 {
    value.text.trim().parse().unwrap_or(0)
}

/// The resume and the reconnecting the dialog's `values` give over `params`.
pub fn read_values(values: &[Value], params: &CopyParams) -> (CopyParams, Reconnect) {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let resume = match (value(ALL).checked(), value(OFF).checked()) {
        (true, _) => Resume::On,
        (_, true) => Resume::Off,
        _ => Resume::Smart,
    };
    let params = CopyParams {
        resume,
        resume_threshold: u64::from(read(&value(THRESHOLD))) * 1024,
        ..params.clone()
    };
    let auto = value(AUTO_CHECK).checked();
    let reconnect = Reconnect {
        auto: if auto {
            read(&value(AUTO_SECONDS)) * 1000
        } else {
            0
        },
        retries: if auto {
            read(&value(RETRIES_EDIT)).min(99)
        } else {
            2
        },
        stall: read(&value(STALL_EDIT)) * 1000,
    };
    (params, reconnect)
}

/// The dialog titled `title`: true when it was closed by OK (both kept).
pub fn ask(host: &Host, title: &str) -> bool {
    let params = crate::options::copy_params(host);
    let dialog = Dialog::new(title, items(&params, reconnect(host))).size(76, 17);
    let closed = host.dialog_with(&dialog, |event, controls| {
        if let DialogEvent::Changed(_) = event {
            let smart = controls.value(SMART).checked();
            controls.enable(THRESHOLD, smart);
            controls.enable(THRESHOLD + 1, smart);
            let auto = controls.value(AUTO_CHECK).checked();
            for item in AUTO_SECONDS..=RETRIES_EDIT + 1 {
                controls.enable(item, auto);
            }
        }
        true
    });
    if closed.button != Some(OK) {
        return false;
    }
    let (params, reconnect) = read_values(&closed.values, &params);
    crate::options::save_copy_params(host, &params);
    save_reconnect(host, reconnect);
    true
}

#[cfg(test)]
#[path = "endurance_tests.rs"]
mod tests;
