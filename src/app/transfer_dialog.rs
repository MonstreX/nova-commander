//! FAR's copy, move and link dialogs (F5 / F6 / Alt+F6, `far/copy.cpp` `ShellCopy::ShellCopy`): the
//! same items at the same places. Turning the answers into an operation is `fileops`' job.

use super::App;
use super::dialog::{Dialog, FormAction};
use super::form::{Control, EditHistory, Form, Item, ListLine};
use super::line_input::LineInput;
use crate::jobs::transfer::{Keep, Rights};
use crate::platform::files::NewLink;
use crate::vfs::VfsPath;

#[path = "transfer_targets.rs"]
mod targets;

pub use targets::split_targets;
pub(super) use targets::unquote;

/// FAR's `DlgW` and `DlgH`.
const WIDTH: u16 = 76;
const HEIGHT: u16 = 17;

/// The items by index (FAR's `ID_SC_*`).
pub const TARGET: usize = 1;
pub const SECURITY_DEFAULT: usize = 4;
pub const SECURITY_COPY: usize = 5;
pub const SECURITY_INHERIT: usize = 6;
pub const EXISTING: usize = 9;
pub const PRESERVE_TIMESTAMPS: usize = 10;
pub const LINK_CONTENTS: usize = 11;
pub const MULTIPLE_TARGETS: usize = 12;
/// Not FAR's: the owner's, beside several destinations, so FAR's rows stay where they are.
pub const VERIFY: usize = 13;
pub const USE_FILTER: usize = 15;
pub const COPY_BUTTON: usize = 17;
pub const TREE: usize = 18;
pub const FILTER: usize = 19;

/// FAR's link dialog: the copy dialog without access rights, timestamps and link contents, its rows
/// moved up (`DlgH` 14).
const LINK_HEIGHT: u16 = 14;
pub const LINK_TYPE: usize = 4;
pub const LINK_MULTIPLE_TARGETS: usize = 5;
pub const LINK_USE_FILTER: usize = 7;
pub const LINK_BUTTON: usize = 9;
pub const LINK_TREE: usize = 10;
pub const LINK_FILTER: usize = 11;
/// "Link type" in FAR's order (`MLinkType*`).
pub const LINK_TYPES: [(&str, NewLink); 5] = [
    ("&hard link", NewLink::Hard),
    ("directory &junction", NewLink::Junction),
    ("&symbolic link", NewLink::Symbolic),
    ("symbolic link (&file)", NewLink::SymbolicFile),
    ("symbolic link (fol&der)", NewLink::SymbolicFolder),
];

/// What F5, F6 and Alt+F6 do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Copy,
    Move,
    Link,
}

impl Operation {
    /// The button that starts it.
    pub fn button(self) -> usize {
        match self {
            Self::Link => LINK_BUTTON,
            Self::Copy | Self::Move => COPY_BUTTON,
        }
    }

    /// Its "F10-Tree" (`tree_panel::target_tree`).
    pub fn tree_button(self) -> usize {
        match self {
            Self::Link => LINK_TREE,
            Self::Copy | Self::Move => TREE,
        }
    }

    /// The title of its dialogs and messages.
    pub fn title(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Move => "Move",
            Self::Link => "Link",
        }
    }
}

/// The lines of "Already existing files" (FAR's `CM_*`).
pub const EXISTING_LINES: [&str; 6] = [
    "&Ask",
    "&Overwrite",
    "&Skip",
    "R&ename",
    "A&ppend",
    "Only ne&wer file(s)",
];
/// The marked line after the separator: "Also ask on R/O files".
pub const ASK_READ_ONLY_LINE: usize = EXISTING_LINES.len() + 1;

/// What the dialog was answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferAnswers {
    pub targets: Vec<String>,
    pub existing: usize,
    pub ask_read_only: bool,
    /// 0 Default, 1 Copy, 2 Inherit.
    pub security: usize,
    pub preserve_timestamps: bool,
    pub link_contents: bool,
    pub verify: bool,
    /// Alt+F6's "Link type".
    pub link: Option<NewLink>,
    /// FAR's "Use filter": Copy's filter takes what goes.
    pub use_filter: bool,
}

impl TransferAnswers {
    /// What the operation keeps besides the contents.
    pub fn keep(&self) -> Keep {
        Keep {
            timestamps: self.preserve_timestamps,
            rights: match self.security {
                1 => Rights::Copy,
                2 => Rights::Inherit,
                _ => Rights::Default,
            },
            link_contents: self.link_contents,
            verify: self.verify,
        }
    }

    pub fn from_form(form: &Form, operation: Operation) -> Self {
        if operation == Operation::Link {
            return Self::from_link_form(form);
        }
        let move_ = operation == Operation::Move;
        let target = form.edit_text(TARGET);
        let targets = if !move_ && form.is_checked(MULTIPLE_TARGETS) {
            split_targets(target)
        } else {
            vec![unquote(target)]
        };
        let security = [SECURITY_DEFAULT, SECURITY_COPY, SECURITY_INHERIT]
            .iter()
            .position(|&index| form.is_checked(index))
            .unwrap_or(0);
        Self {
            targets,
            existing: form.chosen(EXISTING),
            ask_read_only: form.line_checked(EXISTING, ASK_READ_ONLY_LINE),
            security,
            preserve_timestamps: form.is_checked(PRESERVE_TIMESTAMPS),
            link_contents: form.is_checked(LINK_CONTENTS),
            verify: form.is_checked(VERIFY),
            link: None,
            use_filter: form.is_checked(USE_FILTER),
        }
    }

    fn from_link_form(form: &Form) -> Self {
        let target = form.edit_text(TARGET);
        let targets = if form.is_checked(LINK_MULTIPLE_TARGETS) {
            split_targets(target)
        } else {
            vec![unquote(target)]
        };
        let kind = LINK_TYPES
            .get(form.chosen(LINK_TYPE))
            .map(|(_, kind)| *kind);
        Self {
            targets,
            existing: 0,
            ask_read_only: true,
            security: 0,
            preserve_timestamps: false,
            link_contents: false,
            verify: false,
            link: kind.or(Some(NewLink::Hard)),
            use_filter: form.is_checked(LINK_USE_FILTER),
        }
    }
}

impl App {
    /// The copy, move or link dialog's button: the filters, the tree, or the operation - into a plugin's
    /// panel when its target was left as the plugin's format (FAR's `ShellCopy` `ToPlugin`).
    pub(super) fn finish_transfer_form(
        &mut self,
        action: FormAction,
        form: &Form,
        button: Option<usize>,
    ) {
        let FormAction::Transfer {
            sources,
            operation,
            to_plugin,
        } = action
        else {
            return;
        };
        if super::transfer_filter::is_filter_button(operation, button) {
            let action = FormAction::Transfer {
                sources,
                operation,
                to_plugin,
            };
            return self.open_copy_filters(form, action);
        }
        if button == Some(operation.tree_button()) {
            let action = FormAction::Transfer {
                sources,
                operation,
                to_plugin,
            };
            let dialog = Dialog::Form {
                form: form.clone(),
                action,
            };
            return self.open_target_tree(dialog, super::tree_panel::TreeMode::Active);
        }
        if button != Some(operation.button()) {
            return;
        }
        let mut answers = TransferAnswers::from_form(form, operation);
        let moving = operation == Operation::Move;
        match to_plugin.filter(|target| answers.targets == [target.text.clone()]) {
            Some(target) if target.takes_paths => {
                return self.put_into(&target.dir, &sources, moving);
            }
            Some(target) => answers.targets = vec![target.dir.to_string()],
            None => {}
        }
        self.start_transfer(&sources, &answers, operation);
    }

    pub(super) fn ask_copy(&mut self) {
        self.ask_transfer(Operation::Copy);
    }

    pub(super) fn ask_move(&mut self) {
        self.ask_transfer(Operation::Move);
    }

    pub(super) fn ask_link(&mut self) {
        self.ask_transfer(Operation::Link);
    }

    /// A drop with the mouse (FAR's `KEY_DRAGCOPY`, `KEY_DRAGMOVE`): the dialog of F5 or F6 from the
    /// active panel, for the folder under the other panel's cursor when there is one (`ToSubdir`).
    pub(super) fn ask_drop(&mut self, moving: bool) {
        let passive = self.passive_panel();
        let target_dir = match passive
            .current()
            .filter(|entry| entry.is_dir && !entry.is_parent)
        {
            Some(entry) => passive.path.join(&entry.name).to_string(),
            None => passive.path.to_string(),
        };
        let operation = if moving {
            Operation::Move
        } else {
            Operation::Copy
        };
        // FAR's `Ask = !Drag || Confirm.Drag`: without the confirmation the operation starts with what
        // its dialog would offer.
        self.ask_transfer_to(operation, target_dir, self.prefs.confirm_drag);
    }

    fn ask_transfer(&mut self, operation: Operation) {
        let target_dir = self.transfer_target_dir();
        self.ask_transfer_to(operation, target_dir, true);
    }

    /// The folder F5 / F6 offer: the passive panel's; FAR's info panel and quick view stand in the folder
    /// of the panel beside them (their `m_CurDir`), so with one of them passive that folder.
    pub(super) fn transfer_target_dir(&self) -> String {
        let passive = self.active.other();
        let beside = self.info_side() == Some(passive) || self.quick_view_side() == Some(passive);
        match beside {
            true => self.active_panel().path.to_string(),
            false => self.passive_panel().path.to_string(),
        }
    }

    /// The items of the active panel, or the tree's folder (`tree_panel::operations`).
    /// `ask`: the dialog comes up; else the operation starts as its button would start it.
    fn ask_transfer_to(&mut self, operation: Operation, target_dir: String, ask: bool) {
        let move_ = operation == Operation::Move;
        // FAR's `ToPlugin`: into a shown plugin's panel the target is its format (not for a link).
        let to_plugin = self.plugin_target();
        if let Some(folder) = self.tree_folder_below_root() {
            // FAR's tree: Alt+F6 into a plugin's panel does nothing (`treelist.cpp`).
            if operation == Operation::Link && to_plugin.is_some() {
                return;
            }
            // FAR's tree: its folder's whole path (`GetSelName`) when the other panel is in it.
            let Some(folder) = folder else {
                return;
            };
            let path = folder.display().to_string();
            let target = if crate::platform::files::same_path(&path, &target_dir) {
                quote_target(path, move_)
            } else {
                initial_target(&[], &path, &target_dir, move_)
            };
            let sources = vec![VfsPath::local(&folder)];
            let dialog = self.transfer_dialog(operation, sources, &target, false, to_plugin);
            return self.offer_transfer(dialog, ask);
        }
        // FAR: no links on a plugin panel; a plugin that writes its files out shows its own dialog.
        if operation == Operation::Link && self.active_panel().path.as_local().is_none()
            || self.get_from_plugin(operation, to_plugin.is_some())
        {
            return;
        }
        let panel = self.active_panel();
        let has_files = panel.targets_have_files();
        let (sources, source_dir) = (panel.targets(), panel.path.to_string());
        if sources.is_empty() {
            return;
        }
        let target = initial_target(&sources, &source_dir, &target_dir, move_);
        let to_plugin = to_plugin.filter(|_| operation != Operation::Link);
        let dialog = self.transfer_dialog(operation, sources, &target, has_files, to_plugin);
        self.offer_transfer(dialog, ask);
    }

    /// The dialog up, or (`ask` false) its button pressed at once.
    fn offer_transfer(&mut self, dialog: Dialog, ask: bool) {
        match dialog {
            Dialog::Form {
                form,
                action: action @ FormAction::Transfer { operation, .. },
            } if !ask => self.finish_transfer_form(action, &form, Some(operation.button())),
            dialog => self.show_dialog(dialog),
        }
    }

    /// The dialog of `operation` for `sources` with `target` in its line; `has_files`: files are among
    /// them.
    pub(super) fn ask_transfer_of(
        &mut self,
        operation: Operation,
        sources: Vec<VfsPath>,
        target: &str,
        has_files: bool,
        to_plugin: Option<PluginTarget>,
    ) {
        let dialog = self.transfer_dialog(operation, sources, target, has_files, to_plugin);
        self.show_dialog(dialog);
    }

    fn transfer_dialog(
        &self,
        operation: Operation,
        sources: Vec<VfsPath>,
        target: &str,
        has_files: bool,
        to_plugin: Option<PluginTarget>,
    ) -> Dialog {
        let move_ = operation == Operation::Move;
        let target = to_plugin
            .as_ref()
            .map_or(target, |plugin| plugin.text.as_str());
        // FAR's tree names its folder by its whole path.
        let prompt = prompt(&sources, operation, self.tree_has_keys());
        let mut form = if operation == Operation::Link {
            // FAR: a hard link when files are among the items, a junction for folders alone.
            link_form(&prompt, target, has_files)
        } else {
            transfer_form(&prompt, target, move_)
        };
        // FAR: no filter for a single file.
        let single_file = sources.len() == 1 && has_files;
        super::transfer_filter::prepare(&mut form, operation, single_file);
        // FAR's `no_tree`: the tree's button gone with the tree turned off.
        let no_tree = self.prefs.tree.turn_off_completely;
        let tree = &mut form.items[operation.tree_button()];
        (tree.hidden, tree.disabled) = (no_tree, no_tree);
        Dialog::Form {
            form,
            action: FormAction::Transfer {
                sources,
                operation,
                to_plugin,
            },
        }
    }
}

/// The passive panel a plugin's as the copy dialog's target (FAR's `ToPlugin`, `copy.cpp`): the text
/// offered (`<format>:`, `::` without one), the folder it stands for and whether its plugin takes paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginTarget {
    pub text: String,
    pub dir: VfsPath,
    pub takes_paths: bool,
}

/// FAR's `MCopyFile` / `MCopyFiles`, the move and the link ones: `Copy "a.txt" to:`, `Copy 3 items
/// to:`, `Create link to "a.txt" in:`.
/// `whole`: the item by its whole path, cut at its end with FAR's ellipsis where the line ends (FAR's
/// `truncate_right` in its dialog's box).
fn prompt(sources: &[VfsPath], operation: Operation, whole: bool) -> String {
    let (verb, plural, to) = match operation {
        Operation::Copy => ("Copy", "Copy", "t&o"),
        Operation::Move => ("Rename or move", "Rename or move", "t&o"),
        Operation::Link => ("Create link to", "Create links to", "&in"),
    };
    match sources {
        [single] if whole => {
            // The box's inside less the words around the name (`&` draws nothing).
            let room = usize::from(WIDTH - 4 - 3 - 2).saturating_sub(verb.len() + to.len() + 4);
            let path = single.to_string();
            let name = crate::app::text_cut::truncate_right(&path, room);
            // FAR's `escape_ampersands`: a name's `&` is shown, not a hotkey.
            format!("{verb} \"{}\" {to}:", name.replace('&', "&&"))
        }
        [single] => format!("{verb} \"{}\" {to}:", name_of(single).replace('&', "&&")),
        _ => format!("{plural} {} items {to}:", sources.len()),
    }
}

/// FAR: the other panel's folder with a slash; the item's own name when that folder is this one and one
/// item is copied. A copy target with `,` or `;` is quoted, so "several destinations" keeps it whole.
pub(super) fn initial_target(
    sources: &[VfsPath],
    source_dir: &str,
    target_dir: &str,
    move_: bool,
) -> String {
    let same = crate::platform::files::same_path(source_dir, target_dir);
    let target = match sources {
        [single] if same => name_of(single),
        _ => {
            let mut dir = target_dir.to_string();
            // A provider's folder (a plugin's panel) takes `/`, a disk's the system's separator.
            let separator = if VfsPath::parse(target_dir).as_local().is_some() {
                std::path::MAIN_SEPARATOR
            } else {
                '/'
            };
            if !dir.ends_with(separator) {
                dir.push(separator);
            }
            dir
        }
    };
    quote_target(target, move_)
}

/// A target as the line offers it: quoted when a copy's has `,` or `;` (several destinations would split
/// it) or it has spaces at an end (they go when the line is read: `a ` must not become its sibling `a`).
pub(super) fn quote_target(target: String, move_: bool) -> String {
    if (!move_ && target.contains([',', ';'])) || target != target.trim() {
        format!("\"{target}\"")
    } else {
        target
    }
}

/// FAR's target line (`ID_SC_TARGETEDIT`): its history "Copy", the last line when it is empty.
fn copy_line(input: LineInput) -> Item {
    (Item::new(5, 3, Control::Edit(input)).wide(66))
        .with_history(EditHistory::new("Copy").use_last())
}

/// The dialog's items as FAR places them (`copy.cpp`, `MakeDialogItems<ID_SC_COUNT>`).
fn transfer_form(prompt: &str, target: &str, move_: bool) -> Form {
    let mut input = LineInput::with_text(target);
    input.select_all();
    let text = |s: &str| Control::Text(s.to_string());
    let check = |label: &str, checked| Control::Check {
        label: label.to_string(),
        checked,
    };
    let radio = |label: &str, checked, group| Control::Radio {
        label: label.to_string(),
        checked,
        group,
    };
    let button = |label: &str, default| Control::Button {
        label: label.to_string(),
        default,
    };
    let mut lines: Vec<ListLine> = EXISTING_LINES
        .iter()
        .map(|line| ListLine::Choice((*line).to_string()))
        .collect();
    lines.push(ListLine::Separator);
    lines.push(ListLine::Toggle {
        label: "Also ask on &R/O files".to_string(),
        checked: true,
    });
    // The radio buttons follow their title, each after the previous label and four spaces.
    let default_x = 5 + 14 + 1;
    let copy_x = default_x + 7 + 5;
    let inherit_x = copy_x + 4 + 5;
    let items = vec![
        Item::new(5, 2, text(prompt)),
        copy_line(input),
        Item::new(0, 4, Control::Separator),
        Item::new(5, 5, text("&Access rights:")),
        Item::new(default_x, 5, radio("Defau&lt", true, true)),
        Item::new(copy_x, 5, radio("Co&py", false, false)),
        Item::new(inherit_x, 5, radio("&Inherit", false, false)),
        Item::new(0, 6, Control::Separator),
        Item::new(5, 7, text("Already e&xisting files:")),
        Item::new(
            29,
            7,
            Control::DropDown {
                lines,
                chosen: 0,
                open: None,
                caption: None,
                bottom_title: None,
            },
        )
        .wide(42),
        Item::new(5, 8, check("Preserve all timestamps", false)),
        Item::new(5, 9, check("Cop&y contents of symbolic links", false)),
        Item::new(5, 10, check("Process &multiple destinations", false)).disabled(move_),
        Item::new(42, 10, check("&Verify the copies", false)),
        Item::new(0, 11, Control::Separator),
        Item::new(5, 12, check("&Use filter", false)),
        Item::new(0, 13, Control::Separator),
        Item::new(0, 14, button(if move_ { "&Rename" } else { "&Copy" }, true)),
        Item::new(0, 14, button("F10-&Tree", false)),
        Item::new(0, 14, button("Filt&er", false)).disabled(true),
        Item::new(0, 14, button("Ca&ncel", false)),
    ];
    let title = if move_ { "Rename/Move" } else { "Copy" };
    Form::new(title, WIDTH, HEIGHT, items)
}

/// FAR's link dialog (`copy.cpp` with `Link`): "Link type" where "Already existing files" was,
/// several destinations, the filter, `{ Create link }`.
fn link_form(prompt: &str, target: &str, files: bool) -> Form {
    let mut input = LineInput::with_text(target);
    input.select_all();
    let text = |s: &str| Control::Text(s.to_string());
    let check = |label: &str| Control::Check {
        label: label.to_string(),
        checked: false,
    };
    let button = |label: &str, default| Control::Button {
        label: label.to_string(),
        default,
    };
    let lines = LINK_TYPES
        .iter()
        .map(|(line, _)| ListLine::Choice((*line).to_string()))
        .collect();
    let chosen = usize::from(!files);
    let items = vec![
        Item::new(5, 2, text(prompt)),
        copy_line(input),
        Item::new(0, 4, Control::Separator),
        Item::new(5, 5, text("Link t&ype:")),
        Item::new(
            29,
            5,
            Control::DropDown {
                lines,
                chosen,
                open: None,
                caption: None,
                bottom_title: None,
            },
        )
        .wide(42),
        Item::new(5, 7, check("Process &multiple destinations")),
        Item::new(0, 8, Control::Separator),
        Item::new(5, 9, check("&Use filter")),
        Item::new(0, 10, Control::Separator),
        Item::new(0, 11, button("Create &link", true)),
        Item::new(0, 11, button("F10-&Tree", false)),
        Item::new(0, 11, button("Filt&er", false)).disabled(true),
        Item::new(0, 11, button("Ca&ncel", false)),
    ];
    Form::new("Link", WIDTH, LINK_HEIGHT, items)
}

fn name_of(path: &VfsPath) -> String {
    path.file_name().unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
#[path = "transfer_dialog_tests.rs"]
mod tests;
