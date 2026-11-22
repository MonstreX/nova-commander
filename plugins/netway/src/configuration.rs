//! NetBox's configuration (`WinSCPPlugin.cpp` `ConfigureEx`, `WinSCPDialogs.cpp`
//! `TransferConfigurationDialog`, `TTransferEditorConfigurationDialog`): Options > Plugins configuration >
//! NetWay opens NetBox's menu titled by the plugin, again after each item until Esc: "Transfer settings"
//! (the default transfer settings in NetBox's dialog, titled "NetWay - Transfer settings") and
//! "Editor/viewer settings" ("Upload after every save", the viewer's and the editor's download with the
//! default settings or the copy dialog, the editor's upload with the same settings or the copy dialog,
//! not asked while every save uploads), between them "Endurance settings" (`endurance`). NetBox's other
//! items come with their steps (S6.8's background); "Allow more than one edited file" is not there: the program's editor has one file.

use nc_plugin_sdk::{Controls, Dialog, DialogEvent, DialogItem, Host, Menu, MenuItem, Value};

use crate::copy_params::Attrs;
use crate::options::{self, EditorOptions};
use crate::sessions::TITLE;
use crate::transfer_settings;

/// NetBox's menu items in their order.
pub const ITEMS: [&str; 3] = [
    "&Transfer settings",
    "En&durance settings",
    "&Editor/viewer settings",
];

/// NetBox's dialogs' titles: "NetWay - <item>".
pub fn title_of(item: &str) -> String {
    format!("{TITLE} - {}", item.replace('&', ""))
}

// The editor dialog's items.
const UPLOAD_ON_SAVE: usize = 0;
const DOWNLOAD_DEFAULT: usize = 2;
const DOWNLOAD_OPTIONS: usize = 3;
const UPLOAD_SAME: usize = 5;
const UPLOAD_OPTIONS: usize = 6;
const OK: usize = 8;

fn titled(title: &str) -> DialogItem {
    DialogItem {
        text: title.to_string(),
        ..DialogItem::separator()
    }
}

/// The editor dialog's items for `options`.
pub fn editor_items(options: EditorOptions) -> Vec<DialogItem> {
    let same_enabled = !options.upload_on_save;
    vec![
        DialogItem::check("Upload after every &save", options.upload_on_save),
        titled(" When downloading to editor/viewer "),
        DialogItem::radio("Use default &download options", options.download_default).group(),
        DialogItem::radio(
            "Display download &options dialog",
            !options.download_default,
        ),
        titled(" When uploading from editor "),
        DialogItem::radio("Use sa&me options as for download", options.upload_same)
            .group()
            .disabled(!same_enabled),
        DialogItem::radio("Display &upload options dialog", !options.upload_same)
            .disabled(!same_enabled),
        DialogItem::separator(),
        DialogItem::button("&Ok").default_button(),
        DialogItem::button("&Cancel"),
    ]
}

/// The options the editor dialog's `values` give.
pub fn editor_options_of(values: &[Value]) -> EditorOptions {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    EditorOptions {
        upload_on_save: value(UPLOAD_ON_SAVE).checked(),
        download_default: !value(DOWNLOAD_OPTIONS).checked() || value(DOWNLOAD_DEFAULT).checked(),
        upload_same: !value(UPLOAD_OPTIONS).checked() || value(UPLOAD_SAME).checked(),
    }
}

/// NetBox's `UpdateControls`: the upload's choice only while saves do not upload.
fn follow(controls: &mut Controls) {
    let enabled = !controls.value(UPLOAD_ON_SAVE).checked();
    controls.enable(UPLOAD_SAME, enabled);
    controls.enable(UPLOAD_OPTIONS, enabled);
}

/// The editor dialog: true when it was closed by OK (the options kept).
fn editor_dialog(host: &Host) -> bool {
    let options = options::editor_options(host);
    let dialog = Dialog::new(&title_of(ITEMS[2]), editor_items(options));
    let closed = host.dialog_with(&dialog, |event, controls| {
        if let DialogEvent::Changed(_) = event {
            follow(controls);
        }
        true
    });
    if closed.button != Some(OK) {
        return false;
    }
    options::save_editor_options(host, editor_options_of(&closed.values));
    true
}

/// NetBox's `TransferConfigurationDialog`: the default transfer settings, all of them open.
fn transfer_dialog(host: &Host) -> bool {
    let params = options::copy_params(host);
    let title = title_of(ITEMS[0]);
    match transfer_settings::ask(host, &title, &params, Attrs::default(), false) {
        Some(changed) => {
            options::save_copy_params(host, &changed);
            true
        }
        None => false,
    }
}

/// NetBox's `ConfigureEx`: the menu again after each item until Esc; true when something was kept.
pub fn configure(host: &Host) -> bool {
    let mut changed = false;
    let mut selected = 0;
    loop {
        let items: Vec<MenuItem> = ITEMS.iter().map(|item| MenuItem::new(item)).collect();
        let menu = Menu::new(TITLE, items).wrap().selected(selected);
        let Some(item) = host.menu(&menu).and_then(|chosen| chosen.item) else {
            return changed;
        };
        selected = item;
        changed |= match item {
            0 => transfer_dialog(host),
            1 => crate::endurance::ask(host, &title_of(ITEMS[1])),
            _ => editor_dialog(host),
        };
    }
}

#[cfg(test)]
#[path = "configuration_tests.rs"]
mod tests;
