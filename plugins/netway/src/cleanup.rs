//! NetBox's "Cleanup Configuration" (`TCleanupDialog`, WinSCP's `CleanupConfiguration`, stored sessions'
//! `Cleanup`, `CleanupCaches`): what the plugin keeps, each part removed when checked - its configuration
//! (the transfer, editor and endurance settings back to their defaults, the bookmarks), the stored sessions,
//! the caches (the known host keys and certificates). "Select all", "Deselect all"; OK only with a part
//! checked. NetBox's INI file and random seed file have no counterpart here.

use std::path::Path;

use nc_plugin_sdk::{Controls, Dialog, DialogEvent, DialogItem, Host, Message};

/// A part the plugin keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Configuration,
    Sessions,
    Caches,
}

impl Part {
    /// NetBox's `NB_CLEANUP_CONFIG`, `NB_CLEANUP_SESSIONS`, `NB_CLEANUP_CACHES`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Configuration => "&Configuration",
            Self::Sessions => "&Stored sessions",
            Self::Caches => "&Caches",
        }
    }

    /// WinSCP's `CLEANUP_CONFIG_ERROR`, `CLEANUP_SESSIONS_ERROR`, `CLEANUP_CACHES_ERROR`.
    fn failure(self) -> &'static str {
        match self {
            Self::Configuration => "Error cleaning up general configuration.",
            Self::Sessions => "Error cleaning up stored sessions.",
            Self::Caches => "Error cleaning up caches.",
        }
    }
}

/// The caches' files in the data folder.
const CACHES: [&str; 2] = [crate::connecting::KNOWN_HOSTS, "known_certificates"];

/// The parts there are in `data` (the plugin's data folder): the configuration always (WinSCP's), the
/// sessions and the caches when kept.
pub fn present(data: &Path) -> Vec<Part> {
    let sessions = data.join(crate::SESSIONS_FOLDER);
    let any_session = std::fs::read_dir(&sessions).is_ok_and(|mut read| read.next().is_some());
    let any_cache = CACHES.iter().any(|name| data.join(name).exists());
    let mut parts = vec![Part::Configuration];
    parts.extend(any_session.then_some(Part::Sessions));
    parts.extend(any_cache.then_some(Part::Caches));
    parts
}

/// The dialog's items: a check box per part, "Select all" and "Deselect all", OK and Cancel.
pub fn items(parts: &[Part]) -> Vec<DialogItem> {
    let mut items: Vec<DialogItem> = (parts.iter())
        .map(|part| DialogItem::check(part.label(), false))
        .collect();
    items.extend([
        DialogItem::separator(),
        DialogItem::button("Select &all").no_close(),
        DialogItem::button("&Deselect all").same_row().no_close(),
        DialogItem::separator(),
        DialogItem::button("OK").default_button().disabled(true),
        DialogItem::button("Cancel").same_row(),
    ]);
    items
}

/// The part `part` removed from `data`.
fn clean(host: &Host, data: &Path, part: Part) -> std::io::Result<()> {
    let gone = |path: &Path| match path.exists() {
        true if path.is_dir() => std::fs::remove_dir_all(path),
        true => std::fs::remove_file(path),
        false => Ok(()),
    };
    match part {
        Part::Configuration => {
            crate::options::save_copy_params(host, &crate::copy_params::CopyParams::default());
            crate::options::save_editor_options(host, crate::options::EditorOptions::default());
            crate::endurance::save_reconnect(host, crate::endurance::Reconnect::default());
            gone(&data.join(crate::bookmarks::FILE))
        }
        Part::Sessions => gone(&data.join(crate::SESSIONS_FOLDER)),
        Part::Caches => CACHES.iter().try_for_each(|name| gone(&data.join(name))),
    }
}

/// The check boxes all set to `on`, OK following them.
fn check_all(controls: &mut Controls, count: usize, on: bool) {
    for item in 0..count {
        controls.set_checked(item, on);
    }
    controls.enable(count + 4, on);
}

/// NetBox's dialog, then the parts checked removed (a failure told, the others still done).
pub fn cleanup(host: &Host) {
    let data = host.data_folder();
    let parts = present(&data);
    let count = parts.len();
    let ok = count + 4;
    let dialog = Dialog::new("Cleanup Configuration", items(&parts));
    let closed = host.dialog_with(&dialog, |event, controls| {
        match event {
            DialogEvent::Button(button) if button == count + 1 => check_all(controls, count, true),
            DialogEvent::Button(button) if button == count + 2 => check_all(controls, count, false),
            DialogEvent::Changed(item) if item < count => {
                let any = (0..count).any(|item| controls.value(item).checked());
                controls.enable(ok, any);
            }
            _ => {}
        }
        true
    });
    if closed.button != Some(ok) {
        return;
    }
    for (at, part) in parts.iter().enumerate() {
        let checked = closed.values.get(at).is_some_and(|value| value.checked());
        if checked && clean(host, &data, *part).is_err() {
            host.show(
                &Message::new("Error", &[part.failure()])
                    .warning()
                    .buttons(&["&OK"]),
            );
        }
    }
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
