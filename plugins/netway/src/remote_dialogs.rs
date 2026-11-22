//! NetBox's dialogs of the operations on a connected session's files (`WinSCPDialogs.cpp`
//! `CreateDirectoryDialog`, `TPropertiesDialog`, `TLinkDialog`, `RemoteTransferDialog`,
//! `RenameFileDialog`; `WinSCPFileSystem.cpp` `DeleteFilesEx`'s question): "Make folder" with its
//! attributes, "Attributes" (owner, group, permissions, recursively), "Add link" / "Edit link", the input
//! lines of "Duplicate", "Move" and "Rename", and "Do you wish to delete ...". SFTP 3 changes an owner
//! and a group by their numbers (WinSCP's `fcGroupOwnerChangingByID`): the numbers are typed, the names
//! shown beside them.

use nc_plugin_sdk::{Dialog, DialogEvent, DialogItem, Host, InputOptions, Message, Value};

use crate::copy_params::{DEFAULT_RIGHTS, rights_of_text, rights_text};
use crate::remote_ops::Properties;
use crate::rights_box::{Rights, RightsBox};

/// NetBox's histories.
pub const MOVE_TO_HISTORY: &str = "WinscpMoveTo";
const LINK_HISTORY: &str = "WinscpRemoteLink";
const LINK_TO_HISTORY: &str = "WinscpRemoteLinkPointTo";
const NEW_FOLDER_HISTORY: &str = "NewFolder";

/// NetBox's `DeleteFilesEx` question: the file's name or how many.
pub fn delete_question(names: &[String]) -> String {
    match names {
        [one] => format!("Do you wish to delete the file {one}"),
        many => format!("Do you wish to delete {} items", many.len()),
    }
}

/// The question asked: true for OK.
pub fn ask_delete(host: &Host, names: &[String]) -> bool {
    let question = delete_question(names);
    let message = Message::new("Confirmation", &[&question]).buttons(&["&Ok", "&Cancel"]);
    host.show(&message) == Some(0)
}

/// NetBox's `FileNameFormatString` of `RemoteTransferDialog`: the file's name or how many.
pub fn transfer_prompt(names: &[String], moving: bool) -> String {
    let verb = if moving { "Move" } else { "Duplicate" };
    match names {
        [one] => format!("{verb} file '{one}' to remote directory:"),
        many => format!("{verb} {} files to remote directory:", many.len()),
    }
}

/// The line's first text: the folder with the name (one file) or `*.*` (WinSCP's `TUnixPath::Join`).
pub fn transfer_value(folder: &str, names: &[String]) -> String {
    let mask = match names {
        [one] => one.as_str(),
        _ => "*.*",
    };
    format!("{}/{mask}", folder.trim_end_matches('/'))
}

/// The folder and the mask typed (WinSCP's `UnixExtractFilePath` / `UnixExtractFileName`).
pub fn split_value(value: &str) -> (String, String) {
    match value.rsplit_once('/') {
        Some(("", mask)) => ("/".to_string(), mask.to_string()),
        Some((folder, mask)) => (folder.to_string(), mask.to_string()),
        None => (String::new(), value.to_string()),
    }
}

/// NetBox's `RemoteTransferDialog`: the target's folder and mask, none on Esc or an empty line.
pub fn ask_transfer(
    host: &Host,
    folder: &str,
    names: &[String],
    moving: bool,
) -> Option<(String, String)> {
    let title = if moving { "Move" } else { "Duplicate" };
    let prompt = transfer_prompt(names, moving);
    let typed = host.input_history(
        title,
        &prompt,
        &transfer_value(folder, names),
        MOVE_TO_HISTORY,
        InputOptions::default(),
    )?;
    let typed = typed.trim();
    (!typed.is_empty()).then(|| split_value(typed))
}

/// NetBox's `RenameFileDialog`: the new name, none on Esc or an empty line.
pub fn ask_rename(host: &Host, name: &str) -> Option<String> {
    let prompt = format!("Rename file '{name}' to:");
    let typed = host.input("Rename", &prompt, name)?;
    (!typed.trim().is_empty()).then(|| typed.trim().to_string())
}

/// What "Make folder" was closed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFolder {
    pub name: String,
    pub rights: Option<u16>,
    /// "Use same settings next time".
    pub save: bool,
}

const FOLDER_NAME: usize = 1;
const SET_RIGHTS: usize = 3;
const FOLDER_BOX: RightsBox = RightsBox {
    base: 4,
    specials: true,
    buttons: true,
    add_x: false,
    three_state: false,
};

/// "Make folder"'s items: the name, "Set permissions" with the rights.
pub fn folder_items(name: &str, rights: Option<u16>) -> Vec<DialogItem> {
    let shown = Rights::of(rights.unwrap_or(DEFAULT_RIGHTS));
    let mut items = vec![
        DialogItem::text("New &folder name:"),
        DialogItem::edit(name)
            .history(NEW_FOLDER_HISTORY, false)
            .focus(),
        DialogItem {
            text: " Attributes ".to_string(),
            ..DialogItem::separator()
        },
        DialogItem::check("Set pe&rmissions", rights.is_some()),
    ];
    items.extend(FOLDER_BOX.items(shown, false, rights.is_some()));
    items.extend([
        DialogItem::check("Use &same settings next time", false),
        DialogItem::separator(),
        DialogItem::button("&Ok").default_button(),
        DialogItem::button("&Cancel"),
    ]);
    items
}

/// "Make folder"'s values read.
pub fn folder_of(values: &[Value]) -> NewFolder {
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let rights = FOLDER_BOX.rights_of(&value).set;
    NewFolder {
        name: value(FOLDER_NAME).text.trim().to_string(),
        rights: value(SET_RIGHTS).checked().then_some(rights),
        save: value(FOLDER_BOX.end()).checked(),
    }
}

/// NetBox's `CreateDirectoryDialog` over `name` and the rights kept for new folders.
pub fn ask_folder(host: &Host, name: &str, rights: Option<u16>) -> Option<NewFolder> {
    let dialog = Dialog::new("Make folder", folder_items(name, rights));
    let ok = FOLDER_BOX.end() + 2;
    let closed = host.dialog_with(&dialog, |event, controls| {
        match event {
            DialogEvent::Changed(SET_RIGHTS) => {
                let on = controls.value(SET_RIGHTS).checked();
                FOLDER_BOX.enable(controls, on);
            }
            DialogEvent::Changed(item) => {
                FOLDER_BOX.follow(controls, item, false);
            }
            DialogEvent::Button(item) => {
                FOLDER_BOX.follow(controls, item, true);
            }
            _ => {}
        }
        true
    });
    let made = (closed.button == Some(ok)).then(|| folder_of(&closed.values))?;
    (!made.name.is_empty()).then_some(made)
}

/// The rights kept for new folders as text (`rw-r--r--`), none when not set.
pub fn folder_rights_of_text(text: &str) -> Option<u16> {
    rights_of_text(text)
}

pub fn folder_rights_text(rights: u16) -> String {
    rights_text(rights)
}

/// What "Attributes" shows of the items: the first's name or how many, a link's target, the owner's
/// and the group's numbers and names when all share them, the rights all have, any folder.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Current {
    pub names: Vec<String>,
    pub link_to: Option<String>,
    pub uid: Option<u32>,
    pub owner: Option<String>,
    pub gid: Option<u32>,
    pub group: Option<String>,
    pub rights: Rights,
    pub any_folder: bool,
}

/// The "Attributes" dialog's items' places: the owner's and the group's numbers, the rights' box, the
/// recursive box and OK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttributeSlots {
    pub uid: usize,
    pub gid: usize,
    pub rights: RightsBox,
    pub recursive: Option<usize>,
    pub ok: usize,
}

fn attribute_slots(current: &Current) -> AttributeSlots {
    let link = usize::from(current.link_to.is_some()) * 2;
    let uid = 4 + link;
    let gid = uid + 3;
    let rights = RightsBox {
        base: gid + 3,
        specials: true,
        buttons: true,
        add_x: current.any_folder,
        three_state: current.names.len() > 1,
    };
    let recursive = current.any_folder.then(|| rights.end() + 1);
    let ok = recursive.map_or(rights.end(), |item| item + 1) + 1;
    AttributeSlots {
        uid,
        gid,
        rights,
        recursive,
        ok,
    }
}

fn number_text(number: Option<u32>) -> String {
    number.map(|number| number.to_string()).unwrap_or_default()
}

fn named(name: Option<&String>) -> String {
    name.map(|name| format!("[ {name} ]")).unwrap_or_default()
}

/// "Attributes"'s items for `current`.
pub fn attribute_items(current: &Current) -> Vec<DialogItem> {
    let slots = attribute_slots(current);
    let target = match &current.names[..] {
        [one] => one.clone(),
        many => format!("selected objects ({})", many.len()),
    };
    let mut items = vec![
        DialogItem::text("Change file attributes for"),
        DialogItem::text(&target),
    ];
    if let Some(link_to) = &current.link_to {
        items.push(DialogItem::text("Link to:"));
        items.push(DialogItem::edit(link_to).read_only().same_row());
    }
    items.extend([
        DialogItem::separator(),
        DialogItem::text("&Owner:"),
        DialogItem::fixed("99999999", &number_text(current.uid)).same_row(),
        DialogItem::text(&named(current.owner.as_ref()))
            .same_row()
            .disabled(true),
        DialogItem::text("&Group:"),
        DialogItem::fixed("99999999", &number_text(current.gid)).same_row(),
        DialogItem::text(&named(current.group.as_ref()))
            .same_row()
            .disabled(true),
        DialogItem {
            text: " Permissions ".to_string(),
            ..DialogItem::separator()
        },
    ]);
    debug_assert_eq!(items.len(), slots.rights.base);
    items.extend(slots.rights.items(current.rights, false, true));
    if current.any_folder {
        items.push(DialogItem::separator());
        items.push(DialogItem::check("Set attributes &recursively", false));
    }
    items.extend([
        DialogItem::separator(),
        DialogItem::button("&Ok").default_button().disabled(true),
        DialogItem::button("&Cancel"),
    ]);
    items
}

/// The properties that changed (WinSCP's `ChangedProperties`); none when nothing did.
pub fn changed(current: &Current, values: &[Value]) -> Option<Properties> {
    let slots = attribute_slots(current);
    let value = |item: usize| values.get(item).cloned().unwrap_or_default();
    let number = |item: usize| value(item).text.trim().parse::<u32>().ok();
    let rights = slots.rights.rights_of(&value);
    let add_x = slots
        .rights
        .add_x()
        .is_some_and(|item| value(item).checked());
    let recursive = slots.recursive.is_some_and(|item| value(item).checked());
    let properties = Properties {
        rights: (rights != current.rights || add_x).then_some(rights),
        add_x,
        recursive,
        uid: number(slots.uid).filter(|uid| Some(*uid) != current.uid),
        gid: number(slots.gid).filter(|gid| Some(*gid) != current.gid),
    };
    let something = properties.rights.is_some()
        || properties.uid.is_some()
        || properties.gid.is_some()
        || recursive;
    something.then_some(properties)
}

/// NetBox's `PropertiesDialog`: the properties changed on OK (enabled once something changed).
pub fn ask_attributes(host: &Host, current: &Current) -> Option<Properties> {
    let slots = attribute_slots(current);
    let dialog = Dialog::new("Attributes", attribute_items(current));
    let closed = host.dialog_with(&dialog, |event, controls| {
        match event {
            DialogEvent::Button(item) => {
                slots.rights.follow(controls, item, true);
            }
            DialogEvent::Changed(item) => {
                slots.rights.follow(controls, item, false);
            }
            _ => return true,
        }
        let values: Vec<Value> = (0..slots.ok).map(|item| controls.value(item)).collect();
        controls.enable(slots.ok, changed(current, &values).is_some());
        true
    });
    (closed.button == Some(slots.ok)).then(|| changed(current, &closed.values))?
}

/// What the link dialog was closed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub name: String,
    pub point_to: String,
    pub symbolic: bool,
}

const LINK_NAME: usize = 1;
const LINK_TO: usize = 3;
const SYMBOLIC: usize = 5;
const LINK_OK: usize = 7;

/// NetBox's `TLinkDialog`: "Edit link" of a link (its name fixed) or "Add link".
pub fn link_items(link: &Link, edit: bool) -> Vec<DialogItem> {
    let filled = !link.name.is_empty() && !link.point_to.is_empty();
    vec![
        DialogItem::text("&Link file:").disabled(edit),
        DialogItem::edit(&link.name)
            .history(LINK_HISTORY, false)
            .disabled(edit),
        DialogItem::text("&Point link to:"),
        DialogItem::edit(&link.point_to).history(LINK_TO_HISTORY, false),
        DialogItem::separator(),
        DialogItem::check("Sy&mbolic link", link.symbolic).disabled(edit),
        DialogItem::separator(),
        DialogItem::button("&Ok").default_button().disabled(!filled),
        DialogItem::button("&Cancel"),
    ]
}

pub fn ask_link(host: &Host, link: &Link, edit: bool) -> Option<Link> {
    let title = if edit { "Edit link" } else { "Add link" };
    let dialog = Dialog::new(title, link_items(link, edit));
    let closed = host.dialog_with(&dialog, |event, controls| {
        if let DialogEvent::Changed(_) = event {
            let filled = !controls.value(LINK_NAME).text.is_empty()
                && !controls.value(LINK_TO).text.is_empty();
            controls.enable(LINK_OK, filled);
        }
        true
    });
    if closed.button != Some(LINK_OK) {
        return None;
    }
    let value = |item: usize| closed.values.get(item).cloned().unwrap_or_default();
    Some(Link {
        name: value(LINK_NAME).text,
        point_to: value(LINK_TO).text,
        symbolic: value(SYMBOLIC).checked(),
    })
}

#[cfg(test)]
#[path = "remote_dialogs_tests.rs"]
mod tests;
