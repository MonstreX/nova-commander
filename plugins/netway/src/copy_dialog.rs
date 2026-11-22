//! NetBox's copy dialog (`WinSCPDialogs.cpp` `TCopyDialog`, `TWinSCPFileSystem::CopyDialog`): F5 / F6 of
//! a connected session's files to a disk and of a disk's into it - "Copy" or "Move", the files' prompt,
//! the target folder with the file mask after it, the transfer settings in three lines, "Preset" (NetBox's
//! presets and "Custom", which opens the transfer settings), "New and updated file(s) only", "Use same
//! settings next time" (the settings kept as the defaults), "Transfer settings..." (the transfer settings'
//! dialog); a disk's folder that is not there offered to be made (`CloseQuery`). For the viewer and the
//! editor (`coTempTransfer`, when the configuration asks for the dialog) without the prompt, the target
//! and "New and updated file(s) only". The queue's check boxes come with the background transfers (S6.8).

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use nc_plugin_sdk::{Controls, Dialog, DialogEvent, DialogItem, Host, Message};

use crate::copy_names::split_target;
use crate::copy_params::{Attrs, CopyParams};
use crate::options::{presets, save_copy_params};
use crate::transfer_settings;

/// The dialog's width (NetBox's `DlgLength`).
const WIDTH: usize = 78;

/// The transfer settings' lines (NetBox's lister, 3 rows) and their width (its box's less 4).
const INFO_ROWS: usize = 3;
const INFO_WIDTH: usize = WIDTH - 10;

/// The histories of the target: a disk's folders FAR's copy dialog's, the server's NetBox's own.
const LOCAL_HISTORY: &str = "Copy";
const REMOTE_HISTORY: &str = "WinscpRemoteDirectory";

/// NetBox's `NB_COPY_PARAM_CUSTOM_TITLE`.
pub const SETTINGS_TITLE: &str = "Transfer settings";

/// What is copied: its direction, moving or not, the first level's names, for the viewer or the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copying<'a> {
    /// To the server (a disk's files), not from it.
    pub upload: bool,
    pub moving: bool,
    pub names: &'a [String],
    /// NetBox's `coTempTransfer`: no target, no "New and updated file(s) only".
    pub temporary: bool,
}

impl Copying<'_> {
    /// What the direction cannot use.
    pub fn attrs(&self) -> Attrs {
        match self.upload {
            true => Attrs::upload(self.moving),
            false => Attrs::download(),
        }
    }
}

/// The dialog closed by OK: the target folder and the parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmed {
    pub folder: String,
    pub params: CopyParams,
}

/// The items' places, the temporary dialog having no prompt, target nor "New and updated".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slots {
    pub target: Option<usize>,
    pub info: usize,
    pub preset: usize,
    pub newer_only: Option<usize>,
    /// "Transfer on background (add to queue)" (not for the viewer's and the editor's copies).
    pub queue: Option<usize>,
    pub reuse: usize,
    pub settings: usize,
    pub ok: usize,
}

pub fn slots(temporary: bool) -> Slots {
    let shift = if temporary { 2 } else { 0 };
    Slots {
        target: (!temporary).then_some(1),
        info: 3 - shift,
        preset: 8 - shift,
        newer_only: (!temporary).then_some(10),
        queue: (!temporary).then_some(11),
        reuse: 12 - shift - 2 * usize::from(temporary),
        settings: 14 - shift - 2 * usize::from(temporary),
        ok: 15 - shift - 2 * usize::from(temporary),
    }
}

/// NetBox's title: "Copy" or "Move".
pub fn title(moving: bool) -> &'static str {
    if moving { "Move" } else { "Copy" }
}

/// WinSCP's `RightCutToLength`: `text` cut to `length` characters with "..." at its end.
fn cut(text: &str, length: usize) -> String {
    if text.chars().count() <= length {
        return text.to_string();
    }
    let kept: String = text.chars().take(length.saturating_sub(3)).collect();
    format!("{kept}...")
}

/// NetBox's `FormatPrompt`: "Co&py "name" to:" (the name cut to the dialog) or "Co&py N items to:".
pub fn prompt(copying: &Copying) -> String {
    let verb = if copying.moving { "Mo&ve" } else { "Co&py" };
    match copying.names {
        [one] => {
            let name = one.rsplit(['/', '\\']).next().unwrap_or(one);
            let bare = format!("{} \"\" to:", verb.replace('&', ""));
            let room = WIDTH.saturating_sub(bare.len() + 10);
            format!("{verb} \"{}\" to:", cut(name, room))
        }
        many => format!("{verb} {} items to:", many.len()),
    }
}

/// The target's line: the folder with its separator, then the file mask (NetBox's `Execute`).
pub fn target_text(folder: &str, upload: bool, params: &CopyParams) -> String {
    let separator = if upload { '/' } else { '\\' };
    let folder = match folder.ends_with(['/', '\\']) || folder.is_empty() {
        true => folder.to_string(),
        false => format!("{folder}{separator}"),
    };
    format!("{folder}{}", params.file_mask)
}

/// FAR's `FarWrapText` of the settings' line into the lister's rows (the rest not shown).
pub fn info_lines(params: &CopyParams, attrs: Attrs) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in params.info(attrs).split(' ') {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= INFO_WIDTH => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines.resize(INFO_ROWS, String::new());
    lines
}

/// "Preset": NetBox's presets, then "Custom".
fn preset_lines() -> Vec<&'static str> {
    let mut lines: Vec<&str> = presets().iter().map(|(name, _)| *name).collect();
    lines.push("Custom");
    lines
}

fn titled(title: &str) -> DialogItem {
    DialogItem {
        text: title.to_string(),
        ..DialogItem::separator()
    }
}

/// The dialog's items.
pub fn items(copying: &Copying, folder: &str, params: &CopyParams) -> Vec<DialogItem> {
    let mut items = Vec::new();
    if !copying.temporary {
        let history = if copying.upload {
            REMOTE_HISTORY
        } else {
            LOCAL_HISTORY
        };
        let mut target = DialogItem::edit(&target_text(folder, copying.upload, params))
            .width(u32::try_from(WIDTH - 8).unwrap_or(70))
            .history(history, false)
            .focus();
        if !copying.upload {
            target = target.edit_path();
        }
        items.extend([DialogItem::text(&prompt(copying)), target]);
    }
    items.push(titled(" Transfer settings "));
    let attrs = copying.attrs();
    items.extend((info_lines(params, attrs).iter()).map(|line| DialogItem::text(line)));
    items.extend([
        DialogItem::separator(),
        DialogItem::text("Preset"),
        DialogItem::list(&preset_lines(), 0),
        DialogItem::separator(),
    ]);
    if !copying.temporary {
        items.push(DialogItem::check(
            "&New and updated file(s) only",
            params.newer_only,
        ));
        // NetBox's `NB_TRANSFER_QUEUE` (its "No confirmations" is the program's questions' own).
        items.push(DialogItem::check(
            "Transfer on background (add to &queue)",
            params.queue,
        ));
    }
    items.extend([
        DialogItem::check("Use &same settings next time", false),
        DialogItem::separator(),
        DialogItem::button("Transfer settin&gs...").no_close(),
        DialogItem::button("&Ok").default_button(),
        DialogItem::button("&Cancel"),
    ]);
    items
}

/// The folder and parameters the dialog's `text` and check box give over `folder`: a target typed
/// without a folder keeps the folder (NetBox's `Execute`).
pub fn confirmed(
    text: &str,
    (newer_only, queue): (bool, bool),
    upload: bool,
    folder: &str,
    params: &CopyParams,
) -> Confirmed {
    let (typed, mask) = split_target(text.trim(), upload);
    Confirmed {
        folder: if typed.is_empty() {
            folder.to_string()
        } else {
            typed
        },
        params: CopyParams {
            file_mask: mask,
            newer_only,
            queue,
            ..params.clone()
        },
    }
}

/// A disk's folder typed, from the program's folder `base` when relative.
pub fn local_folder(typed: &str, base: Option<&Path>) -> PathBuf {
    let typed = PathBuf::from(typed.trim().trim_matches('"'));
    match base {
        Some(base) if typed.is_relative() => base.join(typed),
        _ => typed,
    }
}

/// The settings shown anew in the lister.
fn show_info(controls: &mut Controls, slots: Slots, params: &CopyParams, attrs: Attrs) {
    for (row, line) in info_lines(params, attrs).iter().enumerate() {
        controls.set_text(slots.info + row, line);
    }
}

/// NetBox's `PresetComboChange` (as the list is left) and `CustomCopyParam`: a preset's settings, or
/// "Custom"'s dialog over the settings now.
fn choose_preset(host: &Host, chosen: usize, params: &mut CopyParams, copying: &Copying) {
    let presets = presets();
    match presets.get(chosen) {
        Some((_, preset)) => *params = preset.clone(),
        None => custom(host, params, copying),
    }
}

/// NetBox's `CustomCopyParam`: the transfer settings' dialog.
fn custom(host: &Host, params: &mut CopyParams, copying: &Copying) {
    let attrs = copying.attrs();
    if let Some(changed) =
        transfer_settings::ask(host, SETTINGS_TITLE, params, attrs, copying.temporary)
    {
        *params = changed;
    }
}

/// The copy dialog of `copying` to `folder`: what it was closed with on OK, none on Cancel or Esc. A
/// disk's folder `base` takes relative folders from. "Use same settings next time" keeps the settings.
pub fn ask(
    host: &Host,
    copying: &Copying,
    folder: &str,
    params: &CopyParams,
    base: Option<&Path>,
) -> Option<Confirmed> {
    let slots = slots(copying.temporary);
    let attrs = copying.attrs();
    let dialog = Dialog::new(title(copying.moving), items(copying, folder, params));
    let current = RefCell::new(params.clone());
    // The preset as the list was entered: NetBox's takes the line chosen at each leaving, which set the
    // "Default" preset back over custom settings when the list was only passed by.
    let entered = std::cell::Cell::new(0);
    let closed = host.dialog_with(&dialog, |event, controls| match event {
        DialogEvent::Changed(item) if Some(item) == slots.target => {
            let typed = !controls.value(item).text.trim().is_empty();
            controls.enable(slots.ok, typed);
            true
        }
        DialogEvent::Focus(item) if item == slots.preset => {
            entered.set(controls.value(item).state);
            true
        }
        DialogEvent::Unfocus(item) if item == slots.preset => {
            let state = controls.value(item).state;
            if state != entered.get() {
                let chosen = usize::try_from(state).unwrap_or(0);
                choose_preset(host, chosen, &mut current.borrow_mut(), copying);
                show_info(controls, slots, &current.borrow(), attrs);
            }
            true
        }
        DialogEvent::Button(item) if item == slots.settings => {
            custom(host, &mut current.borrow_mut(), copying);
            show_info(controls, slots, &current.borrow(), attrs);
            true
        }
        DialogEvent::Close(Some(button)) if button == slots.ok && !copying.upload => slots
            .target
            .is_none_or(|target| folder_made(host, controls, target, base)),
        _ => true,
    });
    if closed.button != Some(slots.ok) {
        return None;
    }
    let value = |item: usize| closed.values.get(item).cloned().unwrap_or_default();
    let newer_only = slots.newer_only.is_some_and(|item| value(item).checked());
    let queue = slots.queue.is_some_and(|item| value(item).checked());
    let text = slots
        .target
        .map(|item| value(item).text)
        .unwrap_or_default();
    let done = confirmed(
        &text,
        (newer_only, queue),
        copying.upload,
        folder,
        &current.into_inner(),
    );
    if value(slots.reuse).checked() {
        save_copy_params(host, &done.params);
    }
    Some(done)
}

/// NetBox's `CloseQuery` of a download: a folder that is not there made after "Directory '...' doesn't
/// exist. Create?"; the dialog stays on Cancel and when it cannot be made.
fn folder_made(host: &Host, controls: &Controls, target: usize, base: Option<&Path>) -> bool {
    let (typed, _) = split_target(controls.value(target).text.trim(), false);
    if typed.is_empty() {
        return true;
    }
    let folder = local_folder(&typed, base);
    if folder.is_dir() {
        return true;
    }
    let shown = typed.trim_end_matches(['\\', '/']);
    let question = format!("Directory '{shown}' doesn't exist. Create?");
    let message = Message::new("Confirmation", &[&question]).buttons(&["&Ok", "&Cancel"]);
    if host.show(&message) != Some(0) {
        return false;
    }
    if std::fs::create_dir_all(&folder).is_err() {
        let text = format!("Can't create folder '{shown}'.");
        let _ = host.show(&Message::new("Error", &[&text]).warning().buttons(&["&Ok"]));
        return false;
    }
    true
}

#[cfg(test)]
#[path = "copy_dialog_tests.rs"]
mod tests;
