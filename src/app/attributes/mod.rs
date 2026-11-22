//! Ctrl+A, Files > File attributes and F4 in the drive menu: FAR's "Attributes" dialog
//! (`far/setattr.cpp`, ADR-0011 step 2) for the item under the cursor or the selection, or a drive's root.
//! What it reads comes from `platform::files::file_properties`; the dialog's own rules (FAR's
//! `SetAttrDlgProc`) are here: three-state boxes for a selection, UTC, Original / Current / Blank, `*`
//! in a time, compressed and encrypted never both, "More" for the other ten attributes. "Set" makes the
//! changes as a task (`jobs::attributes`), with "Process subfolders" everything inside the folders too.

mod apply;
pub mod layout;
mod rules;

use std::path::PathBuf;
use std::time::SystemTime;

use self::layout::{BLANK, CURRENT, FIRST_FLAG, MORE, MORE_OK, ORIGINAL, SET, SYSTEM};
use self::rules::{Seen, Touched, got_focus, remember};
use super::App;
use super::dialog::{ConfirmAction, Dialog, FormAction};
use super::drive_actions::MenuPlace;
use super::form::Form;
use crate::jobs::attributes::times::{self, TimeFormat, row_times};
use crate::platform::files::{self, FileProperties, Flag, LinkFacts};

/// The attribute boxes of the dialog itself; the rest are in "More".
pub const MAIN_FLAGS: usize = 10;
const COMPRESSED: usize = 4;
const ENCRYPTED: usize = 5;
const REPARSE_POINT: usize = 10;

/// FAR's `DIALOGMODE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    File,
    Folder,
    Multiple,
}

/// One item the dialog changes, as it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    pub properties: FileProperties,
}

/// What the dialog was opened with and what it keeps between keys (FAR's `SetAttrDlgParam`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub targets: Vec<Target>,
    pub mode: Mode,
    /// The boxes as they start, "More" last: `None` is `[?]` (some of the selection have it).
    pub flags: [Option<bool>; 20],
    /// Which boxes go round through `[?]` (FAR's `DIF_3STATE`).
    pub three: [bool; 20],
    /// The "More" boxes as the user left them.
    pub advanced: [Option<bool>; 10],
    /// The dialog's boxes as the last key left them (compressed and encrypted watch each other).
    pub shown: [Option<bool>; 10],
    /// Write, creation, access, change, as they start; `None` is blank.
    pub times: [Option<SystemTime>; 4],
    /// Whether the time fields are in UTC now.
    pub utc: bool,
    /// "More"'s reparse point box as the last key left it.
    pub more_reparse: Option<bool>,
    /// The item that had the focus after the last key.
    pub focus: usize,
    /// "Process subfolders" as the last key left it.
    pub subfolders: bool,
    /// What the user changed by hand.
    pub touched: Touched,
    /// The fields as the last key left them.
    pub seen: Seen,
    pub owner: String,
    pub link: Option<LinkFacts>,
    /// A drive's device, from the drive menu (FAR's "Device:").
    pub device: Option<String>,
    pub shown_name: String,
    pub format: TimeFormat,
    /// The drive menu's place when F4 there opened the dialog: the menu comes back after it.
    pub from_menu: Option<MenuPlace>,
}

impl Session {
    /// Reads every path; one that cannot be read is left out of a selection, and fails a single item.
    fn read(
        paths: Vec<PathBuf>,
        shown_name: String,
        from_menu: Option<MenuPlace>,
        format: TimeFormat,
    ) -> std::io::Result<Self> {
        let single = paths.len() == 1;
        let mut targets = Vec::new();
        let mut last_error = None;
        for path in paths {
            match files::file_properties(&path) {
                Ok(properties) => targets.push(Target { path, properties }),
                Err(error) if single => return Err(error),
                Err(error) => last_error = Some(error),
            }
        }
        let Some(first) = targets.first() else {
            return Err(last_error.unwrap_or_else(|| std::io::Error::other("nothing to change")));
        };
        let is_dir = first.properties.attributes.is_directory();
        let mode = match (targets.len(), is_dir) {
            (1, true) => Mode::Folder,
            (1, false) => Mode::File,
            _ => Mode::Multiple,
        };
        let (flags, three) = starting_flags(&targets);
        let mut advanced = [None; 10];
        advanced.copy_from_slice(&flags[MAIN_FLAGS..]);
        let mut shown = [None; 10];
        shown.copy_from_slice(&flags[..MAIN_FLAGS]);
        let link = (targets.len() == 1)
            .then(|| first.properties.link.clone())
            .flatten();
        // FAR asks only from the drive menu (`!SrcPanel`).
        let device = from_menu
            .and_then(|_| crate::platform::volumes::device_name(&first.path.to_string_lossy()));
        Ok(Self {
            times: common_times(&targets),
            owner: common_owner(&targets),
            mode,
            flags,
            three,
            advanced,
            shown,
            utc: false,
            more_reparse: None,
            focus: FIRST_FLAG,
            subfolders: false,
            touched: Touched::default(),
            seen: Seen::default(),
            link,
            device,
            shown_name,
            format,
            from_menu,
            targets,
        })
    }
}

/// FAR's starting boxes: one item shows what it has; a selection shows `[?]` where some have it, and
/// its boxes go round through `[?]` when some do or a folder is among them.
fn starting_flags(targets: &[Target]) -> ([Option<bool>; 20], [bool; 20]) {
    let mut flags = [None; 20];
    let mut three = [false; 20];
    let folders = targets.len() > 1
        && (targets.iter()).any(|target| target.properties.attributes.is_directory());
    for (at, flag) in Flag::ALL.iter().enumerate() {
        let count = (targets.iter())
            .filter(|target| target.properties.attributes.has(*flag))
            .count();
        flags[at] = match count {
            0 => Some(false),
            all if all == targets.len() => Some(true),
            _ => None,
        };
        three[at] = folders || flags[at].is_none();
    }
    (flags, three)
}

/// A time the whole selection shares, blank otherwise.
fn common_times(targets: &[Target]) -> [Option<SystemTime>; 4] {
    let mut common = row_times(&targets[0].properties.times);
    for target in &targets[1..] {
        let times = row_times(&target.properties.times);
        for (shared, own) in common.iter_mut().zip(times) {
            if *shared != own {
                *shared = None;
            }
        }
    }
    common
}

/// An owner the whole selection shares (FAR's `CheckOwner`), blank otherwise.
fn common_owner(targets: &[Target]) -> String {
    let first = targets[0].properties.owner.clone().unwrap_or_default();
    let shared = (targets.iter()).all(|target| target.properties.owner.as_deref() == Some(&first));
    if shared { first } else { String::new() }
}

/// FAR's `QuoteOuterSpace`.
fn quote_outer_space(name: &str) -> String {
    if name.starts_with(' ') || name.ends_with(' ') {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

impl App {
    /// Ctrl+A on a shown panel: the selection or the item under the cursor, never "..". A provider's
    /// panel has no real names (FAR: everything disabled): nothing happens.
    pub(super) fn ask_attributes(&mut self) {
        // On the tree its folder (FAR's `TreeList::GetSelName`), the root too; an empty tree has none.
        if let Some(folder) = self.tree_target() {
            if let Some(folder) = folder {
                let name = folder.file_name().unwrap_or(folder.as_os_str());
                let shown = quote_outer_space(&name.to_string_lossy());
                self.open_attributes(vec![folder], shown, None);
            }
            return;
        }
        let panel = self.active_panel();
        if panel.path.as_local().is_none() {
            return;
        }
        let paths: Vec<PathBuf> = (panel.targets().iter())
            .filter_map(|path| path.as_local().map(std::path::Path::to_path_buf))
            .collect();
        let name = match (paths.len(), panel.current()) {
            (1, _) if !panel.selected.is_empty() => panel.selected.iter().next().cloned(),
            (1, Some(entry)) => Some(entry.name.clone()),
            _ => None,
        };
        if paths.is_empty() {
            return;
        }
        let shown = quote_outer_space(&name.unwrap_or_default());
        self.open_attributes(paths, shown, None);
    }

    /// F4 in the drive menu: the dialog for the drive's root; the menu comes back after it.
    pub(super) fn ask_root_attributes(&mut self, root: &str, place: MenuPlace) {
        self.open_attributes(vec![PathBuf::from(root)], root.to_string(), Some(place));
    }

    /// The items' properties (each owner's name by `LookupAccountSidW`) are read off the screen's thread,
    /// a window with Esc past 300 ms (ADR-0019, review R16).
    fn open_attributes(&mut self, paths: Vec<PathBuf>, shown: String, menu: Option<MenuPlace>) {
        let format = TimeFormat::current();
        self.wait_for(Vec::new(), move || {
            let read = Session::read(paths, shown, menu, format);
            Box::new(move |app: &mut App| app.attributes_read(read, menu))
        });
        self.label_waiting("Reading the attributes…");
    }

    /// The dialog on what was read, or what kept it from being read.
    fn attributes_read(&mut self, read: std::io::Result<Session>, menu: Option<MenuPlace>) {
        match read {
            Ok(session) => self.show_attributes(layout::main_form(&session), session),
            Err(error) => match menu {
                // The message first; the menu comes back after it.
                Some(place) => self.show_dialog(Dialog::Confirm {
                    title: "Attributes".to_string(),
                    lines: vec![error.to_string()],
                    buttons: vec!["OK"],
                    focused: 0,
                    danger: true,
                    action: ConfirmAction::BackToDriveMenu(place),
                }),
                None => self.show_error("Attributes", &error),
            },
        }
    }

    fn show_attributes(&mut self, form: Form, mut session: Session) {
        remember(&form, &mut session);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::Attributes(Box::new(session)),
        });
    }

    /// A button of the dialog (`None`: Esc). Original, Current, Blank and More keep it open (FAR's
    /// `DIF_BTNNOCLOSE`).
    pub(super) fn finish_attributes(
        &mut self,
        session: Session,
        form: &Form,
        button: Option<usize>,
    ) {
        let mut form = form.clone();
        match button {
            Some(SET) => {
                let environment = &self.environment;
                let request = apply::request(&session, &form, &|text| environment.expand(text));
                self.start_attributes(request, session.from_menu);
            }
            // FAR: Windows' properties window, and the dialog closes having set nothing.
            Some(SYSTEM) => {
                let target = &session.targets[0];
                let folder = target.properties.attributes.is_directory();
                if let Err(error) = files::show_properties(&target.path, folder) {
                    self.show_error("Attributes", &error);
                } else if let Some(place) = session.from_menu {
                    self.back_to_drive_menu(place);
                }
            }
            Some(MORE) => {
                let mut session = session;
                session.more_reparse = session.advanced[REPARSE_POINT - MAIN_FLAGS];
                self.show_dialog(Dialog::Form {
                    form: layout::more_form(&session),
                    action: FormAction::AttributesMore {
                        session: Box::new(session),
                        main: Box::new(form),
                    },
                });
            }
            Some(button @ (ORIGINAL | CURRENT | BLANK)) => {
                let mut session = session;
                rules::set_times(&mut form, &mut session, button);
                form.focus = layout::date(0);
                got_focus(&mut form, &mut session);
                self.show_attributes(form, session);
            }
            _ => {
                if let Some(place) = session.from_menu {
                    self.back_to_drive_menu(place);
                }
            }
        }
    }

    /// "More" closed: OK keeps its boxes; the main dialog comes back either way.
    pub(super) fn finish_more(
        &mut self,
        mut session: Session,
        main: Form,
        form: &Form,
        button: Option<usize>,
    ) {
        if button == Some(MORE_OK) {
            for (at, state) in session.advanced.iter_mut().enumerate() {
                let now = form.tristate(at);
                session.touched.flags[MAIN_FLAGS + at] |= now != *state;
                *state = now;
            }
        }
        self.show_attributes(main, session);
    }
}

pub(super) use rules::{follow, intercept, text_click};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_links;
#[cfg(test)]
mod tests_subfolders;
