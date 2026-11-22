//! F7's "Make folder" (R7 review D4; FAR's `ShellMakeDir`, `mkdir.cpp`): "Create the folder" with its
//! history `NewFolder`, "Link type" (none, directory junction, symbolic link) with its "Target" (history
//! `NewFolderLinkTarget`, open only with a link type), "Process multiple names" (FAR's `MultiMakeDir`,
//! kept in the settings): names separated by `,` or `;`, quotes keeping them whole. Each name is made
//! with the folders on its way; with a link type the last one is the link (a junction to the full
//! target, a symbolic link to a folder); no name at all is FAR's "Incorrect folders list". The cursor
//! goes onto the last name's first folder.

use std::path::{Path, PathBuf};

use super::form::{Control, EditHistory, Form, Item, ListLine};
use super::line_input::LineInput;
use crate::platform::files::NewLink;

/// The name's edit line.
pub(super) const NAME: usize = 1;
/// The link type's list.
pub(super) const LINK_TYPE: usize = 4;
/// The link's target.
pub(super) const TARGET: usize = 6;
/// "Process multiple names".
pub(super) const MULTIPLE: usize = 7;
/// OK.
pub(super) const OK: usize = 9;

/// FAR's dialog: its box 3..72 by 1..10, OK the default button.
pub(super) fn form(multiple: bool) -> Form {
    let edit = |x: u16, y: u16, width: u16, history: &str| {
        let mut item = Item::new(x, y, Control::Edit(LineInput::default()));
        item.width = width;
        item.with_history(EditHistory::new(history).use_last())
    };
    let lines = ["&none", "directory &junction", "symbolic &link"]
        .map(|line| ListLine::Choice(line.to_string()))
        .to_vec();
    let mut link_type = Item::new(
        20,
        5,
        Control::DropDown {
            lines,
            chosen: 0,
            open: None,
            caption: None,
            bottom_title: None,
        },
    );
    link_type.width = 51;
    let mut target = edit(20, 6, 51, "NewFolderLinkTarget");
    target.disabled = true;
    let button = |label: &str, default: bool| {
        let label = label.to_string();
        Item::new(0, 9, Control::Button { label, default })
    };
    let check = Control::Check {
        label: "Process &multiple names".to_string(),
        checked: multiple,
    };
    let items = vec![
        Item::new(5, 2, Control::Text("Create the &folder:".to_string())),
        edit(5, 3, 66, "NewFolder"),
        Item::new(0, 4, Control::Separator),
        Item::new(5, 5, Control::Text("&Link type:".to_string())),
        link_type,
        Item::new(5, 6, Control::Text("&Target:".to_string())),
        target,
        Item::new(5, 7, check),
        Item::new(0, 8, Control::Separator),
        button("OK", true),
        button("Cancel", false),
    ];
    let mut form = Form::new("Make folder", 76, 12, items);
    form.focus = NAME;
    form
}

/// After a key: the target open only with a link type (FAR's `DN_EDITCHANGE` of the list).
pub(super) fn follow(dialog: &mut super::Dialog) {
    if let super::Dialog::Form {
        form,
        action: super::dialog::FormAction::MakeDir(_),
    } = dialog
    {
        form.items[TARGET].disabled = form.chosen(LINK_TYPE) == 0;
    }
}

/// The link the list chose: none, a junction, a symbolic link to a folder (FAR's `RP_SYMLINKDIR`).
pub(super) fn link_of(chosen: usize) -> Option<NewLink> {
    match chosen {
        1 => Some(NewLink::Junction),
        2 => Some(NewLink::SymbolicFolder),
        _ => None,
    }
}

/// The names given: with "multiple names" separated by `,` or `;` (quotes keep one whole, spaces round
/// each go), FAR's `enum_tokens_with_quotes_t<with_trim>`; else the line, its quotes dropped, as one.
pub(super) fn names(text: &str, multiple: bool) -> Vec<String> {
    if !multiple {
        let name = text.replace('"', "");
        let name = name.trim();
        return (!name.is_empty())
            .then(|| name.to_string())
            .into_iter()
            .collect();
    }
    let (mut names, mut name, mut quoted) = (Vec::new(), String::new(), false);
    for ch in text.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' | ';' if !quoted => names.push(std::mem::take(&mut name)),
            _ => name.push(ch),
        }
    }
    names.push(name);
    (names.into_iter())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// `name` made in `parent` with the folders on its way (FAR's: those there taken, the last one made, one
/// there the error), the last one a link to `target` with `link`; the path made.
pub(super) fn make_one(
    parent: &Path,
    name: &str,
    link: Option<(NewLink, &str)>,
) -> std::io::Result<PathBuf> {
    let Some((kind, target)) = link else {
        return crate::vfs::local::ops::make_dir(parent, name);
    };
    let path = parent.join(name.trim());
    if let Some(above) = path.parent() {
        std::fs::create_dir_all(above)?;
    }
    let target = target.replace('"', "");
    crate::platform::files::make_link(kind, Path::new(target.trim()), &path)?;
    Ok(path)
}

#[cfg(test)]
#[path = "make_dir_form_tests.rs"]
mod tests;
