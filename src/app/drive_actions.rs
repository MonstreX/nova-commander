//! What the drive menu does to a drive (`far/diskmenu.cpp`): Del disconnects a network drive (asking
//! "Reconnect at logon"), deletes a SUBST drive, detaches a virtual disk or ejects a CD or a card;
//! Shift+Del removes a USB device safely (a virtual disk is detached); Ins loads the media or connects
//! a remembered network drive; Shift+F6 changes the label. A failure is retried once after the process
//! leaves the drive (FAR's `GoHome`), then offers Retry. The menu comes back after each of them, at the
//! same line.

use std::io;

use super::App;
use super::command::Side;
use super::dialog::{ConfirmAction, Dialog, FormAction, InputAction};
use super::form::{Control, Form, Item};
use super::line_input::LineInput;
use super::waiting::{Then, WAITING_FOR_DISK};
use crate::platform;
use crate::platform::volumes::DriveKind;
use crate::platform::volumes::{HotplugDevice, VolumeFacts};
use crate::vfs::local::{Drive, can_be_virtual, drive_name};

/// Something done to a drive (at its root, `C:\`), kept to be retried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveAction {
    Disconnect {
        root: String,
        forget: bool,
    },
    RemoveSubst(String),
    DetachVirtual(String),
    Eject {
        root: String,
        removable: bool,
    },
    RemoveDevice {
        root: String,
        device: HotplugDevice,
    },
    Load(String),
    /// Without a login: as the connection was saved.
    Connect {
        root: String,
        share: String,
        login: Option<login::Login>,
    },
    Label {
        root: String,
        label: String,
    },
}

/// Where the drive menu comes back: its side and line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuPlace {
    pub side: Side,
    pub selected: usize,
}

impl DriveAction {
    fn run(&self) -> io::Result<()> {
        match self {
            Self::Disconnect { root, forget } => {
                platform::volumes::disconnect_network_drive(root, *forget)
            }
            Self::RemoveSubst(root) => platform::volumes::remove_subst(root),
            Self::DetachVirtual(root) => platform::volumes::detach_virtual_disk(root),
            Self::Eject { root, removable } => platform::volumes::eject_media(root, *removable),
            Self::RemoveDevice { device, .. } => platform::volumes::remove_hotplug_device(device),
            Self::Load(root) => platform::volumes::load_media(root),
            Self::Connect { root, share, login } => {
                let login =
                    (login.as_ref()).map(|login| (login.user.as_str(), login.password.as_str()));
                platform::volumes::connect_network_drive(root, share, login)
            }
            Self::Label { root, label } => platform::volumes::set_volume_label(root, label),
        }
    }

    fn root(&self) -> &str {
        match self {
            Self::Disconnect { root, .. }
            | Self::RemoveSubst(root)
            | Self::DetachVirtual(root)
            | Self::Eject { root, .. }
            | Self::RemoveDevice { root, .. }
            | Self::Load(root)
            | Self::Connect { root, .. }
            | Self::Label { root, .. } => root,
        }
    }

    /// FAR's `Item.Operation` texts; loading, connecting and labels have none.
    fn failure(&self) -> Option<&'static str> {
        match self {
            Self::Disconnect { .. } => Some("Cannot disconnect"),
            Self::RemoveSubst(_) => Some("Cannot delete a substituted disk"),
            Self::DetachVirtual(_) => Some("Cannot detach a virtual disk"),
            Self::Eject { .. } => Some("Could not eject media from drive"),
            Self::RemoveDevice { .. } => Some("Cannot remove a device for drive"),
            _ => None,
        }
    }

    /// FAR's message while it runs: loading and connecting wait for the disk; the others show none.
    fn waiting_lines(&self) -> Vec<String> {
        match self {
            Self::Load(_) | Self::Connect { .. } => vec![WAITING_FOR_DISK.to_string()],
            _ => Vec::new(),
        }
    }

    /// The drive as the error names it: `C:`; a label's error names the root, `C:\`.
    fn drive_name(&self) -> String {
        match self {
            Self::Label { root, .. } => root.clone(),
            _ => drive_name(self.root()),
        }
    }
}

impl App {
    /// Del: FAR's `DisconnectDrive`.
    pub(super) fn drive_delete(&mut self, place: MenuPlace, drive: &Drive) {
        if drive.letter.is_none() {
            return self.back_to_drive_menu(place);
        }
        let drive = drive.clone();
        self.wait_for(Vec::new(), move || {
            let facts = DeleteFacts::read(&drive);
            Box::new(move |app: &mut App| app.drive_delete_asks(place, &drive, facts))
        });
        self.label_waiting("Disconnecting the drive…");
    }

    /// Del once the drive was looked at: the question, or the ejection at once.
    fn drive_delete_asks(&mut self, place: MenuPlace, drive: &Drive, facts: DeleteFacts) {
        let root = drive.root.clone();
        if let Some(folder) = facts.subst {
            if !self.prefs.confirm_subst {
                return self.run_drive_action(DriveAction::RemoveSubst(root), place, true);
            }
            return self.show_dialog(Dialog::Confirm {
                title: "Substituted disk deletion".to_string(),
                lines: vec![
                    format!("Delete SUBST-disk {}?", drive_name(&root)),
                    "The drive is mapped to:".to_string(),
                    folder,
                ],
                buttons: vec!["Yes", "No"],
                focused: 0,
                danger: true,
                action: ConfirmAction::Drive(DriveAction::RemoveSubst(root), place),
            });
        }
        if drive.kind == DriveKind::Remote {
            if !self.prefs.confirm_connection {
                let forget = !facts.reconnect;
                let action = DriveAction::Disconnect { root, forget };
                return self.run_drive_action(action, place, true);
            }
            return self.ask_disconnect(place, drive, facts.share, facts.reconnect);
        }
        if let Some(file) = facts.image {
            if !self.prefs.confirm_virtual {
                return self.run_drive_action(DriveAction::DetachVirtual(root), place, true);
            }
            // FAR's `remove_virtual`.
            return self.show_dialog(Dialog::Confirm {
                title: "Virtual disk detaching".to_string(),
                lines: vec![
                    format!("Detach virtual disk {}?", drive_name(&root)),
                    "The drive is mapped to:".to_string(),
                    file,
                ],
                buttons: vec!["Yes", "No"],
                focused: 0,
                danger: true,
                action: ConfirmAction::Drive(DriveAction::DetachVirtual(root), place),
            });
        }
        match drive.kind {
            DriveKind::CdRom => self.run_drive_action(
                DriveAction::Eject {
                    root,
                    removable: false,
                },
                place,
                true,
            ),
            DriveKind::Removable if facts.ejectable => self.run_drive_action(
                DriveAction::Eject {
                    root,
                    removable: true,
                },
                place,
                true,
            ),
            _ => self.back_to_drive_menu(place),
        }
    }

    /// FAR's `MessageRemoveConnection`: "Reconnect at logon" only for a connection that is restored at
    /// logon; a drive that is not connected now is forgotten.
    fn ask_disconnect(&mut self, place: MenuPlace, drive: &Drive, share: String, reconnect: bool) {
        let question = format!("Disconnect from the drive {}?", drive_name(&drive.root));
        let lines = [question, "The drive is mapped to:".to_string(), share];
        let width = lines.iter().map(|line| line.chars().count()).max();
        let width = u16::try_from(width.unwrap_or(0)).unwrap_or(60).max(30);
        let centred = |text: &String, y| {
            let length = u16::try_from(text.chars().count()).unwrap_or(width);
            Item::new(
                5 + (width - length.min(width)) / 2,
                y,
                Control::Label(text.clone()),
            )
        };
        let mut items: Vec<Item> = lines
            .iter()
            .zip(2u16..)
            .map(|(line, y)| centred(line, y))
            .collect();
        items.push(Item::new(0, 5, Control::Separator));
        let mut y = 6;
        let reconnect_item = reconnect.then(|| {
            let check = Control::Check {
                label: "&Reconnect at logon".to_string(),
                checked: true,
            };
            items.push(Item::new(5, 6, check));
            items.push(Item::new(0, 7, Control::Separator));
            y = 8;
            items.len() - 2
        });
        let button = |label: &str, default| Control::Button {
            label: label.to_string(),
            default,
        };
        let yes = items.len();
        items.push(Item::new(0, y, button("Yes", true)));
        items.push(Item::new(0, y, button("No", false)));
        let form = Form::new("Disconnect network drive", width + 10, y + 3, items).warning();
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::Disconnect {
                root: drive.root.clone(),
                place,
                yes,
                reconnect: reconnect_item,
            },
        });
    }

    /// The disconnect dialog closed: Yes disconnects, unchecked "Reconnect at logon" forgets it.
    pub(super) fn finish_disconnect(
        &mut self,
        form: &Form,
        button: Option<usize>,
        (root, place, yes, reconnect): (String, MenuPlace, usize, Option<usize>),
    ) {
        if button != Some(yes) {
            return self.back_to_drive_menu(place);
        }
        let forget = reconnect.is_none_or(|item| !form.is_checked(item));
        self.run_drive_action(DriveAction::Disconnect { root, forget }, place, true);
    }

    /// Shift+Del: FAR's `RemoveHotplugDrive`, asking first. A virtual disk is detached instead, without
    /// a question, as FAR does (removing it as a device would leave its file locked until a restart).
    pub(super) fn drive_remove_device(&mut self, place: MenuPlace, drive: &Drive) {
        let root = drive.root.clone();
        let image = drive.image.is_some();
        let kind = drive.kind;
        self.wait_for(Vec::new(), move || {
            let image = image
                || (can_be_virtual(kind) && platform::volumes::virtual_disk_file(&root).is_some());
            let found = (!image)
                .then(|| platform::volumes::hotplug_device(&root))
                .flatten()
                .map(|device| (disks_of(&device, &root), device));
            Box::new(move |app: &mut App| match found {
                _ if image => app.run_drive_action(DriveAction::DetachVirtual(root), place, true),
                Some((letters, device)) => app.ask_remove_device(place, root, device, &letters),
                None => app.back_to_drive_menu(place),
            })
        });
        self.label_waiting("Removing the device…");
    }

    /// FAR's question before a device goes.
    fn ask_remove_device(
        &mut self,
        place: MenuPlace,
        root: String,
        device: HotplugDevice,
        letters: &[String],
    ) {
        if !self.prefs.confirm_hotplug {
            let action = DriveAction::RemoveDevice { root, device };
            return self.run_drive_action(action, place, true);
        }
        let mut lines = vec![
            "Do you want to remove the device".to_string(),
            device.description.clone(),
        ];
        if !device.friendly_name.is_empty()
            && !device
                .friendly_name
                .eq_ignore_ascii_case(&device.description)
        {
            lines.push(device.friendly_name.clone());
        }
        lines.push(format!("(disk(s): {})", letters.join(", ")));
        self.show_dialog(Dialog::Confirm {
            title: "Device Removal".to_string(),
            lines,
            buttons: vec!["R&emove", "&Cancel"],
            focused: 0,
            danger: true,
            action: ConfirmAction::Drive(DriveAction::RemoveDevice { root, device }, place),
        });
    }

    /// Ins: FAR loads the media of a CD or removable drive and connects a remembered network drive.
    pub(super) fn drive_insert(&mut self, place: MenuPlace, drive: &Drive) {
        if drive.letter.is_none() {
            return self.back_to_drive_menu(place);
        }
        let root = drive.root.clone();
        match drive.kind {
            DriveKind::Removable | DriveKind::CdRom => {
                self.run_drive_action(DriveAction::Load(root), place, false);
            }
            DriveKind::Remote if drive.disconnected => {
                let known = drive.associated_path.clone();
                self.wait_for(vec![WAITING_FOR_DISK.to_string()], move || {
                    let share = known.or_else(|| platform::volumes::network_name(&root));
                    let share = share.unwrap_or_default();
                    let action = DriveAction::Connect {
                        root,
                        share,
                        login: None,
                    };
                    Box::new(move |app: &mut App| app.run_drive_action(action, place, false))
                });
            }
            _ => self.back_to_drive_menu(place),
        }
    }

    /// Shift+F6: FAR's `rename_volume`.
    pub(super) fn drive_ask_label(&mut self, place: MenuPlace, drive: &Drive) {
        let root = drive.root.clone();
        self.wait_for(Vec::new(), move || {
            // The error is the worker's own: read before anything else can change it.
            let facts = platform::volumes::volume_facts(&root).ok_or_else(io::Error::last_os_error);
            Box::new(move |app: &mut App| app.ask_label(place, root, facts))
        });
        self.label_waiting("Reading the volume label…");
    }

    fn ask_label(&mut self, place: MenuPlace, root: String, facts: io::Result<VolumeFacts>) {
        let facts = match facts {
            Ok(facts) => facts,
            Err(error) => {
                return self.show_dialog(Dialog::Confirm {
                    title: "Error".to_string(),
                    lines: vec![error.to_string()],
                    buttons: vec!["OK"],
                    focused: 0,
                    danger: true,
                    action: ConfirmAction::BackToDriveMenu(place),
                });
            }
        };
        self.show_dialog(Dialog::Input {
            title: "Volume label".to_string(),
            prompt: String::new(),
            input: LineInput::with_text(&facts.label),
            action: InputAction::VolumeLabel { root, place },
        });
    }

    /// Does `action` on a worker; `retry_away` moves the process off the drive and tries once more
    /// first, as FAR sends its panels home. A failure asks Retry / Cancel; the menu comes back.
    pub(super) fn run_drive_action(
        &mut self,
        action: DriveAction,
        place: MenuPlace,
        retry_away: bool,
    ) {
        // Esc: the notice, then the menu back (as after the action).
        let back = ConfirmAction::BackToDriveMenu(place);
        self.wait_for_back(action.waiting_lines(), back, move || -> Then {
            let mut result = action.run();
            if result.is_err() && retry_away {
                leave_drive(action.root());
                result = action.run();
            }
            Box::new(move |app: &mut App| app.drive_action_ended(action, place, result))
        });
    }

    fn drive_action_ended(
        &mut self,
        action: DriveAction,
        place: MenuPlace,
        result: io::Result<()>,
    ) {
        match result {
            Ok(()) => self.drive_action_done(action, place),
            // FAR's `ConnectToNetworkResource`: its own dialog asks for the name and password.
            Err(error) if platform::volumes::credentials_refused(&error) => match action {
                DriveAction::Connect { root, share, .. } => {
                    self.ask_network_login(root, share, place);
                }
                action => self.drive_action_failed(action, place, &error),
            },
            Err(error) => self.drive_action_failed(action, place, &error),
        }
    }

    fn drive_action_failed(&mut self, action: DriveAction, place: MenuPlace, error: &io::Error) {
        let mut lines: Vec<String> = action.failure().map(str::to_string).into_iter().collect();
        lines.push(action.drive_name());
        lines.push(error.to_string());
        self.show_dialog(Dialog::Confirm {
            title: "Error".to_string(),
            lines,
            buttons: vec!["&Retry", "&Cancel"],
            focused: 0,
            danger: true,
            action: ConfirmAction::Drive(action, place),
        });
    }

    fn drive_action_done(&mut self, action: DriveAction, place: MenuPlace) {
        let DriveAction::RemoveDevice { device, .. } = action else {
            self.reread_panels();
            return self.back_to_drive_menu(place);
        };
        // FAR's MChangeHotPlugNotify1/2.
        self.show_dialog(Dialog::Confirm {
            title: "Device Removal".to_string(),
            lines: vec![
                "The device".to_string(),
                device.description,
                device.friendly_name,
                "can now be safely removed".to_string(),
            ],
            buttons: vec!["OK"],
            focused: 0,
            danger: false,
            action: ConfirmAction::BackToDriveMenu(place),
        });
    }

    pub(super) fn back_to_drive_menu(&mut self, place: MenuPlace) {
        self.reopen_drive_menu(place.side, place.selected);
    }

    /// A drive dialog closed without its action (Esc, No, Cancel): the menu comes back.
    pub(super) fn drive_dialog_cancelled(&mut self, dialog: &Dialog) {
        match dialog {
            Dialog::Confirm {
                action: ConfirmAction::Drive(_, place) | ConfirmAction::BackToDriveMenu(place),
                ..
            }
            | Dialog::Input {
                action: InputAction::VolumeLabel { place, .. },
                ..
            } => self.back_to_drive_menu(*place),
            // Esc on the question about tasks that do not stop: Back.
            Dialog::Confirm {
                action: ConfirmAction::QuitNotStopping,
                ..
            } => self.stay_after_not_stopping(),
            _ => {}
        }
    }
}

/// What Del needs to know of a drive before it asks: the SUBST folder, the share and whether the
/// connection is restored at logon, the image file of a virtual disk, whether the media can be ejected.
struct DeleteFacts {
    subst: Option<String>,
    image: Option<String>,
    share: String,
    reconnect: bool,
    ejectable: bool,
}

impl DeleteFacts {
    /// Reads only what the drive's kind asks for (on a worker: a network drive may take its time).
    fn read(drive: &Drive) -> Self {
        let subst = (drive.subst.clone()).or_else(|| platform::volumes::subst_target(&drive.root));
        let remote = subst.is_none() && drive.kind == DriveKind::Remote;
        let share = (drive.associated_path.clone())
            .filter(|_| remote)
            .or_else(|| {
                remote
                    .then(|| platform::volumes::network_name(&drive.root))
                    .flatten()
            });
        // FAR looks for an image on Del whatever the menu's options.
        let image = (subst.is_none() && can_be_virtual(drive.kind))
            .then(|| {
                (drive.image.clone()).or_else(|| platform::volumes::virtual_disk_file(&drive.root))
            })
            .flatten();
        Self {
            reconnect: remote
                && !drive.disconnected
                && platform::volumes::is_persistent_network_drive(&drive.root),
            ejectable: subst.is_none()
                && drive.kind == DriveKind::Removable
                && platform::volumes::is_ejectable_media(&drive.root),
            share: share.unwrap_or_default(),
            subst,
            image,
        }
    }
}

/// The whole device goes: every drive on it is named (FAR's `GetDisksForDevice`).
fn disks_of(device: &HotplugDevice, root: &str) -> Vec<String> {
    platform::volumes::volume_roots()
        .into_iter()
        .filter(|other| other.letter.is_some())
        .filter(|other| {
            platform::files::same_path(&other.path, root)
                || platform::volumes::hotplug_device(&other.path).as_ref() == Some(device)
        })
        .map(|other| drive_name(&other.path))
        .collect()
}

/// FAR's `GoHome`: its panels hold the process's folder; here the process's own folder is moved to the
/// root of the program's drive when it is on the drive at `root`.
fn leave_drive(root: &str) {
    let on_drive = std::env::current_dir().is_ok_and(|dir| {
        super::drive_menu::root_key(&dir) == super::drive_menu::root_key(root.as_ref())
    });
    let home = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.ancestors().last().map(std::path::Path::to_path_buf));
    if on_drive && let Some(home) = home {
        // Only a better chance for the retry; nothing to report when it fails.
        let _ = std::env::set_current_dir(home);
    }
}

#[path = "drive_login.rs"]
mod login;

#[cfg(test)]
#[path = "drive_actions_tests.rs"]
mod tests;
