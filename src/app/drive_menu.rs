//! FAR's drive menu (Alt+F1 / Alt+F2, `far/diskmenu.cpp`): a line per drive with the columns of
//! `DriveMenuMode`, opened over the panel it changes. Ctrl+1..9 and Ctrl+H switch columns, F9 sets them
//! in a dialog, Ctrl+R reads the drives again, Shift+Enter opens the drive in Explorer, the letter or
//! Enter goes to the drive: to the folder the panels were last in on it, as FAR's `FarChDir("D:")`. The
//! plugins' items come under the drives (`drive_menu_plugins`).

use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::command::Side;
use super::dialog::{Dialog, FormAction};
use super::drive_actions::MenuPlace;
use super::form::{Control, Form, Item};
use super::waiting::WAITING_FOR_DISK;
use super::{App, Overlay, keyboard};
use crate::platform;
use crate::platform::volumes::DriveKind;
use crate::vfs::VfsPath;
use crate::vfs::local::{self, Drive, DriveColumns};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveMenu {
    pub side: Side,
    pub drives: Vec<Drive>,
    /// The plugins' items, on the lines after the drives and a separator.
    pub plugins: Vec<DiskItem>,
    /// The line selected, of the drives, the separator and the items.
    pub selected: usize,
    /// The columns the lines were read with.
    pub columns: DriveColumns,
    /// The first line shown (FAR's `TopPos`), kept by the renderer.
    pub top: std::cell::Cell<usize>,
}

/// The F9 dialog's check boxes: (item index, label, column).
const OPTIONS: [(&str, u16, Column); 13] = [
    ("Show disk &type", 5, Column::Kind),
    ("Show disk &label", 5, Column::Label),
    ("Use shell na&me", 9, Column::ShellName),
    ("Show &file system type", 5, Column::FileSystem),
    ("Show &size", 5, Column::Size),
    ("Show si&ze as a decimal fraction", 9, Column::SizeFloat),
    (
        "Show &network name / SUBST path / VHD name",
        5,
        Column::Path,
    ),
    ("Show &plugins", 5, Column::Plugins),
    ("Sort plugins by &hotkey", 9, Column::SortPlugins),
    ("Show &removable drive parameters", 5, Column::Removable),
    ("Show &CD drive parameters", 5, Column::Cd),
    ("Show n&etwork drive parameters", 5, Column::Network),
    ("Detect virtual disks", 5, Column::Virtual),
];
/// The boxes of the label and the size in the F9 dialog.
const LABEL: usize = 1;
const SIZE: usize = 4;
/// The OK button's item in the F9 dialog.
const OK_BUTTON: usize = OPTIONS.len() + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Kind,
    Label,
    ShellName,
    FileSystem,
    Size,
    SizeFloat,
    Path,
    Plugins,
    SortPlugins,
    Removable,
    Cd,
    Network,
    Virtual,
}

fn column(columns: &mut DriveColumns, which: Column) -> &mut bool {
    match which {
        Column::Kind => &mut columns.kind,
        Column::Label => &mut columns.label,
        Column::ShellName => &mut columns.shell_name,
        Column::FileSystem => &mut columns.file_system,
        Column::Size => &mut columns.size,
        Column::SizeFloat => &mut columns.size_float,
        Column::Path => &mut columns.associated_path,
        Column::Plugins => &mut columns.plugins,
        Column::SortPlugins => &mut columns.sort_plugins_by_hotkey,
        Column::Removable => &mut columns.removable,
        Column::Cd => &mut columns.cd,
        Column::Network => &mut columns.network,
        Column::Virtual => &mut columns.virtual_disks,
    }
}

impl App {
    /// Alt+F1 / Alt+F2: the menu with the panel's drive selected; on a plugin's panel the drive of the
    /// folder it was opened from (FAR's `ChangeDisk` takes `GetCurDir`); the drives read on a worker.
    pub(super) fn open_drive_menu(&mut self, side: Side) {
        let columns = self.config.drive_columns;
        let panel = &self.panels[side.index()];
        let path =
            (panel.local_folder()).map_or_else(String::new, |path| path.display().to_string());
        self.wait_for(Vec::new(), move || {
            let drives = local::list_drives(&columns);
            let selected = (drives.iter())
                .position(|drive| {
                    starts_with_ignoring_case(&path, drive.root.trim_end_matches('\\'))
                })
                .unwrap_or(0);
            Box::new(move |app: &mut App| app.show_drive_menu(side, drives, selected))
        });
        self.label_waiting("Reading the drives…");
    }

    /// The menu again with the drives read anew, the selection kept (FAR's `RetCode = SelPos`).
    pub(super) fn reopen_drive_menu(&mut self, side: Side, selected: usize) {
        let columns = self.config.drive_columns;
        self.wait_for(Vec::new(), move || {
            let drives = local::list_drives(&columns);
            Box::new(move |app: &mut App| app.show_drive_menu(side, drives, selected))
        });
        self.label_waiting("Reading the drives…");
    }

    /// The menu with `drives` and the plugins' items, line `selected` (or the nearest one) selected.
    fn show_drive_menu(&mut self, side: Side, drives: Vec<Drive>, selected: usize) {
        self.plugins.host.refresh_items(super::plugins::ITEMS_WAIT);
        let plugins = self.disk_items(&drives);
        let mut menu = DriveMenu {
            side,
            drives,
            plugins,
            selected,
            columns: self.config.drive_columns,
            top: std::cell::Cell::new(0),
        };
        menu.selected = menu.fit(selected);
        self.overlay = Some(Overlay::DriveMenu(menu));
    }

    pub(super) fn handle_drive_menu_key(&mut self, mut menu: DriveMenu, key: KeyEvent) {
        if self.drive_key(&menu, key) {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let (side, selected) = (menu.side, menu.selected);
        match key.code {
            KeyCode::Esc | KeyCode::F(10) => return,
            // FAR's `KEY_CTRLPGUP` there: closes it with "Use Ctrl+PgUp to change drive".
            KeyCode::PageUp if ctrl && self.prefs.ctrl_pg_up_changes_drive != 0 => return,
            KeyCode::Up | KeyCode::Left => menu.selected = menu.step(selected, false),
            KeyCode::Down | KeyCode::Right => menu.selected = menu.step(selected, true),
            KeyCode::Home | KeyCode::PageUp => menu.selected = menu.fit(0),
            KeyCode::End | KeyCode::PageDown => menu.selected = menu.fit(usize::MAX),
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                // The menu stays; a failure, quick or slow, comes over it.
                let root = menu.drives.get(selected).map(|drive| drive.root.clone());
                self.overlay = Some(Overlay::DriveMenu(menu));
                if let Some(root) = root {
                    self.open_in_explorer(&root);
                }
                return;
            }
            KeyCode::Enter => match menu.plugin_at(selected) {
                Some(item) => return self.open_plugin_disk(item, side),
                None => return self.choose_drive(side, menu.drives.get(selected)),
            },
            KeyCode::F(9) => return self.ask_drive_columns(side, selected),
            // FAR's `ShowPluginInfo` on a plugin's item; on a drive nothing.
            KeyCode::F(3) => {
                if let Some(item) = menu.plugin_at(selected).cloned() {
                    let place = (super::plugins::HotkeyMenu::Disks, selected, side);
                    return self.show_plugin_info((item.plugin, &item.label), place);
                }
            }
            // FAR's `SetHotKeyDialog` on a plugin's item, F4 or Ctrl+A (`diskmenu.cpp`).
            KeyCode::F(4) | KeyCode::Char('a' | 'A') if key.code == KeyCode::F(4) || ctrl => {
                if let Some(item) = menu.plugin_at(selected).cloned() {
                    return self.ask_disk_hotkey(&item, side, selected);
                }
            }
            KeyCode::Char(ch) if ctrl => {
                if self.switch_drive_column(ch) {
                    return self.reopen_drive_menu(side, selected);
                }
            }
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::ALT)
                    && self.drive_menu_hotkey(&menu, ch) =>
            {
                return;
            }
            _ => {}
        }
        self.overlay = Some(Overlay::DriveMenu(menu));
    }

    /// A drive's letter or a plugin item's hotkey: chosen (true).
    fn drive_menu_hotkey(&mut self, menu: &DriveMenu, key: char) -> bool {
        if let Some(letter) = latin_letter(key)
            && let Some(drive) = menu.drives.iter().find(|d| d.letter == Some(letter))
        {
            self.choose_drive(menu.side, Some(&drive.clone()));
            return true;
        }
        // The character itself first: a layout whose digit row types other characters, by its key.
        let item = (menu.hotkey_line(key))
            .or_else(|| menu.hotkey_line(latin_key(key)))
            .and_then(|line| menu.plugin_at(line));
        item.inspect(|item| self.open_plugin_disk(item, menu.side))
            .is_some()
    }

    /// Del, Shift+Del, Ins and Shift+F6 on the selected drive (`drive_actions`); false for other keys.
    fn drive_key(&mut self, menu: &DriveMenu, key: KeyEvent) -> bool {
        let Some(drive) = menu.drives.get(menu.selected) else {
            return false;
        };
        let place = MenuPlace {
            side: menu.side,
            selected: menu.selected,
        };
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Delete if shift => self.drive_remove_device(place, drive),
            KeyCode::Delete => self.drive_delete(place, drive),
            KeyCode::Insert => self.drive_insert(place, drive),
            KeyCode::Menu => return self.drive_context_menu(menu, &drive.root, key.modifiers),
            KeyCode::F(6) if shift => self.drive_ask_label(place, drive),
            // FAR's `ShellSetFileAttributes` for the drive's root.
            KeyCode::F(4) if !drive.disconnected => {
                // The menu under the wait for the root's properties: Esc on it brings it back
                // (ADR-0019; review of 19-5).
                self.overlay = Some(Overlay::DriveMenu(menu.clone()));
                self.ask_root_attributes(&drive.root, place);
            }
            _ => return false,
        }
        true
    }

    /// FAR's Ctrl+1..Ctrl+9, Ctrl+H and Ctrl+R in the menu; false for other keys.
    fn switch_drive_column(&mut self, key: char) -> bool {
        let columns = &mut self.config.drive_columns;
        match key {
            '1' => columns.kind = !columns.kind,
            '2' => columns.associated_path = !columns.associated_path,
            // Label, then the shell's name, then nothing.
            '3' if columns.label && columns.shell_name => {
                (columns.label, columns.shell_name) = (false, false);
            }
            '3' if columns.label => columns.shell_name = true,
            '3' => columns.label = true,
            '4' => columns.file_system = !columns.file_system,
            // Megabytes, then "1.50 G", then nothing.
            '5' if columns.size => (columns.size, columns.size_float) = (false, true),
            '5' if columns.size_float => columns.size_float = false,
            '5' => columns.size = true,
            '6' => columns.removable = !columns.removable,
            '7' => columns.plugins = !columns.plugins,
            '8' => columns.cd = !columns.cd,
            '9' => columns.network = !columns.network,
            'h' => columns.unmounted = !columns.unmounted,
            'r' => {}
            _ => return false,
        }
        true
    }

    /// Enter or a drive's letter: the panel goes to the drive.
    fn choose_drive(&mut self, side: Side, drive: Option<&Drive>) {
        if let Some(drive) = drive {
            self.go_to_drive(side, drive.root.clone(), drive.letter.is_some(), drive.kind);
        }
    }

    /// The panel goes to the drive at `root`, looked at on a worker; one that cannot be read asks to
    /// retry (`drive_retry`).
    pub(super) fn go_to_drive(&mut self, side: Side, root: String, letter: bool, kind: DriveKind) {
        // FAR loads the media of a CD or removable drive found empty (closes the tray).
        let load = matches!(kind, DriveKind::Removable | DriveKind::CdRom) && letter;
        self.remember_drive_folder();
        let folder = self.drive_folder(side, &root, letter);
        self.wait_for(Vec::new(), move || {
            if !(load && platform::volumes::volume_facts(&root).is_none()) {
                return entered(side, folder, root);
            }
            // Empty: FAR's message while the media loads; a drive still empty after it asks to retry.
            Box::new(move |app: &mut App| {
                app.wait_for(vec![WAITING_FOR_DISK.to_string()], move || {
                    let _ = platform::volumes::load_media(&root);
                    entered(side, folder, root)
                });
            })
        });
        self.label_waiting("Reading the drive…");
    }

    /// The panel goes to `target`; already there, it is read again with the cursor and selection kept
    /// (FAR's `Update(UPDATE_KEEP_SELECTION)`).
    /// `title`: the error's ("Change drive" from the menu, "Change directory" from the command line).
    pub(super) fn enter_drive_folder(&mut self, side: Side, target: &VfsPath, title: &str) {
        self.close_info_panel(side);
        self.close_tree_panel(side);
        self.close_quick_view(side);
        let panel = &mut self.panels[side.index()];
        if panel.path == *target {
            return panel.reload();
        }
        match panel.change_dir(target, None) {
            // Read in the background: a late failure under the same title.
            Ok(()) => panel.title_failure(title),
            Err(error) => self.show_error(title, &error),
        }
    }

    /// FAR's `FarChDir("D:")`: the folder the panels were last in on that drive, or its root; a volume
    /// with no letter has no such folder in FAR and opens at its root.
    fn drive_folder(&self, side: Side, drive_root: &str, letter: bool) -> DriveFolder {
        let root = VfsPath::local(drive_root);
        if !letter {
            return DriveFolder::Known(root);
        }
        let key = root_key(Path::new(drive_root));
        // The panel's own drive keeps its folder; then the other panel's; then the remembered one.
        for panel in [side, side.other()] {
            let path = &self.panels[panel.index()].path;
            if path.as_local().map(root_key).as_ref() == Some(&key) {
                return DriveFolder::Known(path.clone());
            }
        }
        match self.remembered.drive_folders.get(&key) {
            Some(folder) => DriveFolder::Remembered {
                folder: folder.clone(),
                root,
            },
            None => DriveFolder::Known(root),
        }
    }

    /// FAR's per-drive folder (`=D:`, set by `FarChDir` whenever the active panel's folder changes): the
    /// active panel's folder is its drive's. Called on every tick; costs no disk access.
    pub(super) fn remember_drive_folder(&mut self) {
        let Some(dir) = self.command_panel().path.as_local() else {
            return;
        };
        let key = root_key(dir);
        if self
            .remembered
            .drive_folders
            .get(&key)
            .map(std::path::PathBuf::as_path)
            != Some(dir)
        {
            let dir = dir.to_path_buf();
            self.remembered.drive_folders.insert(key, dir);
        }
    }

    /// Volumes that arrived or left (FAR's `update_devices`). True on a change.
    pub(super) fn pump_devices(&mut self) -> bool {
        let owed = self.devices_pending && !self.is_waiting();
        if owed {
            self.devices_pending = false;
            self.reread_open_drive_menu();
        }
        let roots = platform::volumes::take_device_changes();
        if roots.is_empty() {
            return owed;
        }
        self.devices_changed(&roots);
        true
    }

    /// An open drive menu reads the drives again at the same line, as FAR's Ctrl+R (it stays on screen
    /// meanwhile; while other work runs the reading is owed until it ends). A panel on one of the drives
    /// reads its folder again (ours: FAR's panels do not listen); one on a drive that left shows the
    /// error.
    pub(super) fn devices_changed(&mut self, roots: &[String]) {
        if self.is_waiting() {
            self.devices_pending = true;
        } else {
            self.reread_open_drive_menu();
        }
        let keys: Vec<String> = roots.iter().map(|root| root_key(Path::new(root))).collect();
        for panel in &mut self.panels {
            if panel
                .path
                .as_local()
                .is_some_and(|dir| keys.contains(&root_key(dir)))
            {
                panel.reload();
            }
        }
    }

    fn reread_open_drive_menu(&mut self) {
        if let Some(Overlay::DriveMenu(menu)) = &self.overlay {
            let (side, selected) = (menu.side, menu.selected);
            self.reopen_drive_menu(side, selected);
        }
    }

    /// `D:` typed on the command line: FAR's `FarChDir("D:")`, as the drive menu enters a drive.
    pub(super) fn change_to_drive(&mut self, drive_root: &str) {
        let side = self.command_side();
        self.remember_drive_folder();
        let folder = self.drive_folder(side, drive_root, true);
        self.wait_for(Vec::new(), move || {
            let target = folder.resolve();
            Box::new(move |app: &mut App| {
                app.enter_drive_folder(side, &target, "Change directory");
            })
        });
        self.label_waiting("Reading the drive…");
    }

    fn open_in_explorer(&mut self, root: &str) {
        self.open_by_system(std::path::PathBuf::from(root), "Change drive");
    }

    /// F9: FAR's "Change Drive Menu Options" dialog (`ConfigureChangeDriveMode`).
    fn ask_drive_columns(&mut self, side: Side, selected: usize) {
        let mut columns = self.config.drive_columns;
        let mut items: Vec<Item> = OPTIONS
            .iter()
            .zip(2u16..)
            .map(|(&(label, x, which), y)| {
                let checked = match which {
                    Column::Size => columns.shows_size(),
                    _ => *column(&mut columns, which),
                };
                let check = Control::Check {
                    label: label.to_string(),
                    checked,
                };
                let item = Item::new(x, y, check);
                // FAR's `LinkFlags`: the shell's name with the label, the fraction with the size.
                match which {
                    Column::ShellName => item.follows(LABEL),
                    Column::SizeFloat => item.follows(SIZE),
                    _ => item,
                }
            })
            .collect();
        let buttons_y = 3 + u16::try_from(OPTIONS.len()).unwrap_or(0);
        let button = |label: &str, default| Control::Button {
            label: label.to_string(),
            default,
        };
        items.push(Item::new(0, buttons_y - 1, Control::Separator));
        items.push(Item::new(0, buttons_y, button("OK", true)));
        items.push(Item::new(0, buttons_y, button("Cancel", false)));
        let form = Form::new("Change Drive Menu Options", 55, buttons_y + 3, items);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::DriveColumns { side, selected },
        });
    }

    /// The F9 dialog closed: OK keeps its choices; the menu comes back either way.
    pub(super) fn finish_drive_columns(
        &mut self,
        form: &Form,
        button: Option<usize>,
        side: Side,
        selected: usize,
    ) {
        if button == Some(OK_BUTTON) {
            let mut columns = self.config.drive_columns;
            for (index, &(_, _, which)) in OPTIONS.iter().enumerate() {
                *column(&mut columns, which) = form.is_checked(index);
            }
            // "Show size" with "decimal fraction" is the float style, without it megabytes.
            let any_size = columns.size;
            columns.size = any_size && !columns.size_float;
            columns.size_float &= any_size;
            self.config.drive_columns = columns;
        }
        self.reopen_drive_menu(side, selected);
    }
}

/// The key `ch` is typed with, from any keyboard layout (FAR finds menu hotkeys by the key): a digit's
/// key on a layout whose digit row types other characters too.
fn latin_key(ch: char) -> char {
    (keyboard::SYSTEM.key_for_char)(ch).map_or(ch, |(latin, _)| latin)
}

/// The letter of a key, from any keyboard layout.
fn latin_letter(ch: char) -> Option<char> {
    let latin = latin_key(ch);
    latin
        .is_ascii_alphabetic()
        .then(|| latin.to_ascii_uppercase())
}

/// The title of the menu's errors.
const CHANGE_DRIVE: &str = "Change drive";

/// The folder a drive is entered at.
enum DriveFolder {
    Known(VfsPath),
    /// The folder the panels last left the drive at, if it is still there; else the root.
    Remembered {
        folder: std::path::PathBuf,
        root: VfsPath,
    },
}

impl DriveFolder {
    /// Looks on the disk for a remembered folder: not on the UI thread.
    fn resolve(self) -> VfsPath {
        match self {
            Self::Known(path) => path,
            Self::Remembered { folder, .. } if folder.is_dir() => VfsPath::local(folder),
            Self::Remembered { root, .. } => root,
        }
    }
}

/// What a worker that looked at a drive gives the UI: the panel at its folder (FAR's `FarChDir`), or the
/// question to retry when it cannot be read.
fn entered(side: Side, folder: DriveFolder, root: String) -> super::waiting::Then {
    match super::drive_retry::reachable(folder.resolve(), &root) {
        Ok(target) => Box::new(move |app: &mut App| {
            app.enter_drive_folder(side, &target, CHANGE_DRIVE);
        }),
        Err(error) => Box::new(move |app: &mut App| app.ask_drive_retry(side, root, &error)),
    }
}

/// The root of `path`'s drive (`C:\`, `\\?\Volume{...}\`) as the remembered folders are keyed: case
/// aside where it does not matter.
pub(super) fn root_key(path: &Path) -> String {
    let root = path.ancestors().last().unwrap_or(path);
    platform::files::fold_case(&root.to_string_lossy()).into_owned()
}

fn starts_with_ignoring_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// The columns as the settings file keeps them.
pub fn column_ids(columns: &DriveColumns) -> Vec<String> {
    COLUMN_IDS
        .iter()
        .filter(|(_, get)| get(columns))
        .map(|(id, _)| (*id).to_string())
        .collect()
}

pub fn columns_from_ids(ids: &[String]) -> DriveColumns {
    let on = |id: &str| ids.iter().any(|saved| saved == id);
    DriveColumns {
        kind: on("type"),
        associated_path: on("path"),
        label: on("label"),
        shell_name: on("shellName"),
        file_system: on("fileSystem"),
        size: on("size"),
        size_float: on("sizeFloat"),
        removable: on("removable"),
        cd: on("cd"),
        network: on("network"),
        plugins: on("plugins"),
        sort_plugins_by_hotkey: on("sortPluginsByHotkey"),
        virtual_disks: on("virtual"),
        unmounted: on("unmounted"),
    }
}

type ColumnGetter = fn(&DriveColumns) -> bool;

const COLUMN_IDS: [(&str, ColumnGetter); 14] = [
    ("type", |c| c.kind),
    ("path", |c| c.associated_path),
    ("label", |c| c.label),
    ("shellName", |c| c.shell_name),
    ("fileSystem", |c| c.file_system),
    ("size", |c| c.size),
    ("sizeFloat", |c| c.size_float),
    ("removable", |c| c.removable),
    ("cd", |c| c.cd),
    ("network", |c| c.network),
    ("plugins", |c| c.plugins),
    ("sortPluginsByHotkey", |c| c.sort_plugins_by_hotkey),
    ("virtual", |c| c.virtual_disks),
    ("unmounted", |c| c.unmounted),
];

#[path = "drive_menu_plugins.rs"]
mod plugins;
pub use plugins::DiskItem;

#[cfg(test)]
#[path = "drive_menu_tests.rs"]
mod tests;
