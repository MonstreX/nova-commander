//! Options > Groups of file masks as FAR's `Options::MaskGroupsSettings`: named lists of masks that a
//! mask anywhere takes as `<name>` (the selection's, the associations', the filters'). The list shows
//! each group's name in ten columns, `│` and its masks, in the order of the names; Ins adds, Enter or
//! F4 changes ("&Name:", "A file &mask or several file masks:"), Del asks "Do you wish to delete", F7
//! shows only the groups whose masks take a name, Ctrl+R puts FAR's default groups back. With none at
//! all the default ones are there, as FAR's `InitConfig` puts them.

use super::App;
use super::dialog::{ConfirmAction, Dialog, FormAction};
use super::edit_list::EditListKind;
use super::form::{Control, EditHistory, Form, Item};
use super::line_input::LineInput;
use crate::settings::MaskGroup;
use crate::vfs::FileMask;

pub(super) const TITLE: &str = "Groups of file masks";
const NAME: usize = 1;
const MASKS: usize = 3;
/// FAR's `NameWidth`.
const NAME_WIDTH: usize = 10;
/// FAR's `AddEditField(.., 60)`.
const EDIT_WIDTH: u16 = 60;

/// FAR's `ApplyDefaultMaskGroups`.
const DEFAULTS: [(&str, &str); 3] = [
    (
        "arc",
        "*.zip,*.rar,*.[7bgxl]z,*.[bg]zip,*.tar,*.t[agbxl]z,*.z,*.ar[cj],*.r[0-9][0-9],*.a[0-9][0-9],\
         *.bz2,*.cab,*.jar,*.lha,*.lzh,*.ha,*.ac[bei],*.pa[ck],*.rk,*.cpio,*.rpm,*.zoo,*.hqx,*.sit,\
         *.ice,*.uc2,*.ain,*.imp,*.777,*.ufa,*.boa,*.bs[2a],*.sea,*.[ah]pk,*.ddi,*.x2,*.rkv,*.[lw]sz,\
         *.h[ay]p,*.lim,*.sqz,*.chz,*.aa[br],*.zst",
    ),
    ("temp", "*.bak,*.tmp"),
    ("exec", "*.exe,*.cmd,*.bat,*.com,%PATHEXT%"),
];

/// The saved groups in the order of their names; none: FAR's default ones.
pub(super) fn restored(saved: &[MaskGroup]) -> Vec<MaskGroup> {
    let mut groups = saved.to_vec();
    if groups.is_empty() {
        put_defaults(&mut groups);
    }
    groups.sort_by(|a, b| a.name.cmp(&b.name));
    groups
}

/// The default groups put in, each in place of one of its name.
fn put_defaults(groups: &mut Vec<MaskGroup>) {
    for (name, masks) in DEFAULTS {
        put(
            groups,
            MaskGroup {
                name: name.to_string(),
                masks: masks.to_string(),
            },
        );
    }
}

/// `group` in place of the one of its name, else added; the order of the names kept.
fn put(groups: &mut Vec<MaskGroup>, group: MaskGroup) {
    groups.retain(|known| known.name != group.name);
    let at = groups.partition_point(|known| known.name < group.name);
    groups.insert(at, group);
}

/// FAR's line: the name cut or padded to ten columns.
fn line(group: &MaskGroup) -> String {
    let name: String = group.name.chars().take(NAME_WIDTH).collect();
    let padding = " ".repeat(NAME_WIDTH - name.chars().count());
    format!("{name}{padding} \u{2502} {}", group.masks)
}

fn edit(y: u16, text: &str) -> Item {
    let mut item = Item::new(5, y, Control::Edit(LineInput::with_text(text)));
    item.width = EDIT_WIDTH;
    item
}

fn button(y: u16, label: &str, default: bool) -> Item {
    Item::new(
        0,
        y,
        Control::Button {
            label: label.to_string(),
            default,
        },
    )
}

/// FAR's `DialogBuilder` of the group: its name and its masks, OK and Cancel.
fn group_form(group: &MaskGroup) -> Form {
    let items = vec![
        Item::new(5, 2, Control::Text("&Name:".to_string())),
        edit(3, &group.name),
        Item::new(
            5,
            4,
            Control::Text("A file &mask or several file masks:".to_string()),
        ),
        edit(5, &group.masks),
        Item::new(0, 6, Control::Separator),
        button(7, "OK", true),
        button(7, "Cancel", false),
    ];
    Form::new(TITLE, EDIT_WIDTH + 10, 10, items)
}

/// F7's dialog: the name to look for.
fn find_form() -> Form {
    let items = vec![
        Item::new(5, 2, Control::Text("&Mask:".to_string())),
        edit(3, "").with_history(EditHistory::new("MaskGroupsFindMask")),
        Item::new(0, 4, Control::Separator),
        button(5, "OK", true),
        button(5, "Cancel", false),
    ];
    Form::new("Filter", EDIT_WIDTH + 10, 8, items)
}

impl App {
    /// `text` as a mask, its `<name>` the groups' masks and `%PATHEXT%` the programs' extensions.
    pub(super) fn file_mask(&self, text: &str) -> Option<FileMask> {
        let group = |name: &str| {
            (self.config.mask_groups.iter())
                .find(|group| group.name == name)
                .map(|group| group.masks.clone())
        };
        let path_ext = self.environment.get("PATHEXT").unwrap_or_default();
        FileMask::parse_with(text, &group, &path_ext)
    }

    /// Options > Groups of file masks.
    pub(super) fn open_mask_groups(&mut self) {
        self.show_edit_list(EditListKind::MaskGroups, 0);
    }

    /// The lines and the groups they show: those whose masks take every name of `found` (F7).
    pub(super) fn mask_group_lines(&self, found: &[String]) -> (Vec<String>, Vec<usize>) {
        (self.config.mask_groups.iter().enumerate())
            .filter(|(_, group)| {
                let mask = self.file_mask(&group.masks);
                (found.iter()).all(|name| mask.as_ref().is_some_and(|mask| mask.matches(name)))
            })
            .map(|(at, group)| (line(group), at))
            .unzip()
    }

    /// Ins (`None`), Enter or F4: the group's dialog.
    pub(super) fn edit_mask_group(&mut self, at: Option<usize>) {
        let group = at
            .and_then(|at| self.config.mask_groups.get(at))
            .cloned()
            .unwrap_or_default();
        self.show_dialog(Dialog::Form {
            form: group_form(&group),
            action: FormAction::MaskGroup(at),
        });
    }

    /// OK: the group in place of the one it was (a new name takes the place of a group of that name).
    pub(super) fn finish_mask_group(
        &mut self,
        form: &Form,
        button: Option<usize>,
        at: Option<usize>,
    ) {
        if button != Some(form.items.len() - 2) {
            return;
        }
        let group = MaskGroup {
            name: form.edit_text(NAME).to_string(),
            masks: form.edit_text(MASKS).to_string(),
        };
        // FAR keeps a group with no name, which then cannot be changed; none is kept here.
        if group.name.is_empty() {
            return;
        }
        if let Some(at) = at.filter(|&at| at < self.config.mask_groups.len()) {
            self.config.mask_groups.remove(at);
        }
        // FAR's list is filled again with the cursor at the same line.
        put(&mut self.config.mask_groups, group);
        // FAR's `UpdateHighlighting(true)`: the groups' masks made again.
        self.make_highlighting();
    }

    pub(super) fn ask_remove_mask_group(&mut self, at: usize) {
        let Some(group) = self.config.mask_groups.get(at) else {
            return;
        };
        self.show_dialog(Dialog::Confirm {
            title: TITLE.to_string(),
            lines: vec!["Do you wish to delete".to_string(), group.name.clone()],
            buttons: vec!["Delete", "Cancel"],
            focused: 0,
            danger: false,
            action: ConfirmAction::RemoveMaskGroup(at),
        });
    }

    pub(super) fn remove_mask_group(&mut self, at: usize) {
        if at < self.config.mask_groups.len() {
            self.config.mask_groups.remove(at);
            self.make_highlighting();
        }
    }

    pub(super) fn ask_restore_mask_groups(&mut self) {
        self.show_dialog(Dialog::Confirm {
            title: TITLE.to_string(),
            lines: vec!["Do you wish to restore default mask sets?".to_string()],
            buttons: vec!["Yes", "Cancel"],
            focused: 0,
            danger: true,
            action: ConfirmAction::RestoreMaskGroups,
        });
    }

    /// Ctrl+R: FAR's default groups in place of those of their names; the others stay.
    pub(super) fn restore_mask_groups(&mut self) {
        put_defaults(&mut self.config.mask_groups);
        self.make_highlighting();
    }

    pub(super) fn ask_find_mask_group(&mut self) {
        self.show_dialog(Dialog::Form {
            form: find_form(),
            action: FormAction::FindMaskGroup,
        });
    }

    /// F7's OK: the list shows the groups whose masks take the name, and the names looked for before
    /// (FAR hides more each time).
    pub(super) fn finish_find_mask_group(&mut self, form: &Form, button: Option<usize>) {
        if button != Some(form.items.len() - 2) {
            return;
        }
        if let Some(list) = &mut self.edit_lists.behind {
            list.found.push(form.edit_text(1).to_string());
            list.selected = 0;
        }
    }
}

#[cfg(test)]
#[path = "mask_groups_tests.rs"]
mod tests;
