//! A plugin opened from the command line or by a file (ADR-0016 §14). FAR's `ProcessCommandLine`: a line
//! `prefix:text` of a plugin's prefix, after the history took it, is echoed on the user screen and goes to
//! the first plugin that has the prefix (FAR's own commands come before it, the system's after). FAR's
//! `OpenFilePlugin` on Enter and Ctrl+PgDn (`filelist.cpp` `ProcessEnter`): after the file's associations,
//! a file of a plugin's masks (not a program, for Enter) goes to the plugins in their order until one takes
//! it; none: the program's own way. The answer is waited for as a panel key's, the keys after it held: its
//! panel opened (over the file's panel, ".." at its root back to the file, for a file), the active panel to
//! a folder, the command line, a file opened by the system.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::{App, Overlay, Side};
use super::keys::{KeyWait, WaitKind};
use crate::plugin_host::{KeyAnswer, KeyGo, command_prefix};
use crate::vfs::VfsPath;

/// The prefix a right click goes to (the shipped emenu's).
pub(in crate::app) const CLICK_PREFIX: &str = "rclk_mouse";

/// A wait ended unanswered: the program's folder of a plugin's file's copy it was for goes. False: the
/// wait is not kept (for `retain`).
pub(super) fn let_wait_go(wait: &KeyWait) -> bool {
    use super::getting::{GetView, Getting};
    let folder = match &wait.kind {
        WaitKind::File {
            temp: Some(folder), ..
        }
        | WaitKind::Get(Getting::View(GetView { folder, .. }) | Getting::Open { folder, .. }) => {
            folder
        }
        _ => return false,
    };
    super::super::panel::forget_copy(folder.clone());
    false
}

/// The entry a plugin's panel opened by a key goes over (FAR's `OpenFilePlugin` on Enter or Ctrl+PgDn,
/// as arclite's partition is entered from its disk's panel): the one under the cursor `cursor` when the
/// key `key` was Enter or Ctrl+PgDn; none for another key (TmpPanel's switch of panels).
pub(super) fn opens_over(key: KeyEvent, cursor: &Option<String>) -> Option<String> {
    let enter = key.code == KeyCode::Enter && key.modifiers.is_empty();
    let pgdn = key.code == KeyCode::PageDown && key.modifiers == KeyModifiers::CONTROL;
    if enter || pgdn { cursor.clone() } else { None }
}

impl App {
    /// A plugin's panel opened from the entry `name` of the panel `side`: over that panel as a file's
    /// plugin is, ".." at its root back to it with the cursor on `name`.
    pub(super) fn open_over(
        &mut self,
        plugin: &str,
        side: Side,
        instance: &str,
        name: &str,
    ) -> std::io::Result<()> {
        let path = VfsPath::in_session(plugin, instance, "/");
        self.panels[side.index()].push_file_plugin(name, None);
        self.plugins.opening.push((path.clone(), side));
        self.panels[side.index()].change_dir(&path, None)
    }

    /// FAR's `ProcessCommandLine`: `line`, run in `dir`, to the plugin of its prefix; false when no plugin
    /// has it (the line is the system's then).
    pub(in crate::app) fn plugin_command_line(&mut self, line: &str, dir: &Path) -> bool {
        let Some(prefix) = command_prefix(line) else {
            return false;
        };
        let Some(plugin) = self.plugins.host.prefix_plugin(prefix) else {
            return false;
        };
        // FAR's `ConsoleActivator(true)`: the line echoed on the user screen after the prompt's folder (a
        // plugin's panel's own, as the prompt shows it).
        let shown = if self
            .plugins
            .host
            .is_plugin_panel(&self.command_panel().path)
        {
            PathBuf::from(self.prompt_folder())
        } else {
            dir.to_path_buf()
        };
        self.show_in_user_screen(&shown, line);
        let command = line.get(prefix.len() + 1..).unwrap_or_default();
        let name = self.plugins.host.plugins[plugin].name.clone();
        let side = self.active;
        let folder = self.far_folder(self.command_side());
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let serial = self.wait_for_plugin(WaitKind::Command, &name, side, enter);
        let to = (side.index(), serial);
        (self.plugins.host).command(plugin, (prefix, command), folder.as_deref(), to);
        true
    }

    /// FAR's Apps and Shift+Apps, or a right click (`click`, the screen's cell), on a panel or on a drive of
    /// the drive menu (`root`), which call EMenu (`KnownIDs.Emenu` with `OPEN_FILEPANEL`,
    /// `OPEN_LEFTDISKMENU`): `rclk_apps:` (`rclk_mouse:X Y`) with the drive's root to the plugin that has
    /// the prefix (the shipped emenu), not echoed, for the panel of `side`: that plugin's name; none when no
    /// plugin has the prefix.
    pub(in crate::app) fn context_menu_key(
        &mut self,
        root: Option<&str>,
        side: Side,
        click: Option<(u16, u16)>,
    ) -> Option<String> {
        let prefix = match click {
            Some(_) => CLICK_PREFIX,
            None => "rclk_apps",
        };
        let plugin = self.plugins.host.prefix_plugin(prefix)?;
        let name = self.plugins.host.plugins[plugin].name.clone();
        let folder = self.far_folder(side);
        let key = KeyEvent::new(KeyCode::Menu, KeyModifiers::NONE);
        let serial = self.wait_for_plugin(WaitKind::Command, &name, side, key);
        let to = (side.index(), serial);
        let root = root.unwrap_or_default();
        let text = match click {
            Some((x, y)) => format!("{x} {y} {root}"),
            None => root.to_string(),
        };
        (self.plugins.host).command(plugin, (prefix, text.trim_end()), folder.as_deref(), to);
        Some(name)
    }

    /// Enter (or, `alternative`, Ctrl+PgDn) on the active panel's file after its associations: to the
    /// plugins whose masks it has, in their order (FAR's `OpenFilePlugin`); false when none has it.
    pub(in crate::app) fn open_file_by_plugins(&mut self, alternative: bool) -> bool {
        let panel = self.active_panel();
        let Some(entry) = panel
            .current()
            .filter(|entry| !entry.is_dir && !entry.is_parent)
        else {
            return false;
        };
        let Some(path) = panel.current_path() else {
            return false;
        };
        let entry_name = entry.name.clone();
        let name = path.file_name().map_or_else(
            || entry_name.clone(),
            |name| name.to_string_lossy().into_owned(),
        );
        let plugins = self.plugins_for(&name);
        self.ask_file_plugin(plugins, (path, entry_name, None), alternative)
    }

    /// The plugins whose masks the file name `name` has, in their order.
    pub(super) fn plugins_for(&self, name: &str) -> Vec<usize> {
        (self.plugins_with_masks().into_iter())
            .filter(|(_, masks)| masks.iter().any(|mask| mask.matches(name)))
            .map(|(index, _)| index)
            .collect()
    }

    /// The plugins that open files, in their order, each with its masks as the program reads them
    /// (Enter's and Ctrl+PgDn's, Alt+F7's search in archives).
    pub(in crate::app) fn plugins_with_masks(&self) -> Vec<(usize, Vec<crate::vfs::FileMask>)> {
        (self.plugins.host.plugins.iter().enumerate())
            .map(|(index, plugin)| {
                let masks = (plugin.file_masks.iter())
                    .filter_map(|masks| self.file_mask(masks))
                    .collect::<Vec<_>>();
                (index, masks)
            })
            .filter(|(_, masks)| !masks.is_empty())
            .collect()
    }

    /// Enter (Ctrl+PgDn: `alternative`) on a plugin's file, its copy at `path` in the program's folder
    /// `temp` (FAR's `ProcessEnter` with `PluginMode`): its association first (`ProcessLocalFileTypes`
    /// on the copy, run in its folder), then - not a program - the plugins of its masks, a panel over the
    /// archive's (an archive inside an archive); none taking it, Enter opens the copy by the system (the
    /// folder kept for it), Ctrl+PgDn nothing (the folder gone).
    pub(super) fn open_copy_by_plugins(
        &mut self,
        (path, name): (PathBuf, String),
        temp: PathBuf,
        alternative: bool,
    ) {
        let file_name = (path.file_name()).map_or_else(String::new, |n| n.to_string_lossy().into());
        let mode = if alternative {
            super::super::associations::AssocMode::AltExecute
        } else {
            super::super::associations::AssocMode::Execute
        };
        let target = super::super::associations::AssocTarget {
            name: file_name.clone(),
            current: file_name.clone(),
            dir: Some(temp.clone()),
        };
        if self.open_target_by_association(target, mode) {
            return;
        }
        let program = crate::platform::process::program_start(&file_name, Default::default());
        let plugins = match program {
            Some(_) => Vec::new(),
            None => self.plugins_for(&file_name),
        };
        if !self.ask_file_plugin(
            plugins,
            (path.clone(), name, Some(temp.clone())),
            alternative,
        ) {
            self.copy_not_taken(path, &temp, alternative);
        }
    }

    /// A plugin's file's copy no plugin took: Enter opens it by the system (its folder kept for the
    /// program it starts, tech-debt), Ctrl+PgDn lets it go.
    fn copy_not_taken(&mut self, path: PathBuf, temp: &Path, alternative: bool) {
        if alternative {
            super::super::panel::forget_copy(temp.to_path_buf());
        } else {
            self.open_by_system(path, "Open");
        }
    }

    /// The first of `plugins` asked to open `path` (its entry `name`, for ".." back to it; the folder of
    /// its copy), the others after it if it does not; false for none.
    fn ask_file_plugin(
        &mut self,
        mut plugins: Vec<usize>,
        (path, name, temp): (PathBuf, String, Option<PathBuf>),
        alternative: bool,
    ) -> bool {
        if plugins.is_empty() {
            return false;
        }
        let plugin = plugins.remove(0);
        let plugin_name = self.plugins.host.plugins[plugin].name.clone();
        let side = self.active;
        let folder = self.far_folder(side);
        let key = if alternative {
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL)
        } else {
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        };
        let kind = WaitKind::File {
            path: path.clone(),
            name,
            alternative,
            next: plugins,
            temp,
        };
        let serial = self.wait_for_plugin(kind, &plugin_name, side, key);
        let to = (side.index(), serial);
        (self.plugins.host).open_file(plugin, (&path, alternative), folder.as_deref(), to);
        true
    }

    /// FAR's `OpenFindListPlugin`: Alt+F7's `found` objects to the first plugin that takes them, its panel
    /// in place of the active one's (`ChangePanel`).
    pub(in crate::app) fn find_list_to_plugins(&mut self, found: Vec<crate::vfs::Entry>) {
        let plugins = self.plugins.host.find_list_plugins();
        self.ask_find_list(plugins, found);
    }

    /// The first of `plugins` asked to take `found`, the others after it if it does not.
    fn ask_find_list(&mut self, mut plugins: Vec<usize>, found: Vec<crate::vfs::Entry>) {
        if plugins.is_empty() {
            return;
        }
        let plugin = plugins.remove(0);
        let name = self.plugins.host.plugins[plugin].name.clone();
        let side = self.active;
        let kind = WaitKind::FindList {
            found: found.clone(),
            next: plugins,
        };
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let serial = self.wait_for_plugin(kind, &name, side, enter);
        (self.plugins.host).find_list(plugin, found, (side.index(), serial));
    }

    /// FAR's current folder for the panel `side`: its folder on a disk, or the one its plugins' panels
    /// were entered from.
    pub(super) fn far_folder(&self, side: Side) -> Option<PathBuf> {
        let panel = &self.panels[side.index()];
        (panel.path.as_local())
            .or_else(|| panel.before_plugins()?.from.as_local())
            .map(Path::to_path_buf)
    }

    /// A file's plugin said it is not its file: the next plugin's, then the program's own for Enter
    /// (FAR's `ProcessEnter`; Ctrl+PgDn nothing more), only while the panels are as they were; a copy
    /// none took goes (Enter: opened by the system).
    fn file_passed(&mut self, wait: &KeyWait) {
        let WaitKind::File {
            path,
            name,
            alternative,
            next,
            temp,
        } = &wait.kind
        else {
            return;
        };
        let file = (path.clone(), name.clone(), temp.clone());
        let taken =
            self.still_where(wait) && self.ask_file_plugin(next.clone(), file, *alternative);
        match temp {
            Some(temp) if !taken && self.still_where(wait) => {
                self.copy_not_taken(path.clone(), temp, *alternative);
            }
            Some(temp) if !taken => super::super::panel::forget_copy(temp.clone()),
            None if !taken && !*alternative && self.still_where(wait) => {
                self.open_without_association();
            }
            _ => {}
        }
    }

    /// A plugin that did not take what `wait` offered it (its answer `pass`): what comes next for that
    /// kind of offer; false for a kind that passing does not move on (a command's).
    fn passed_on(&mut self, wait: &KeyWait) -> bool {
        match &wait.kind {
            WaitKind::File { .. } => self.file_passed(wait),
            // Not a share it goes up from: FAR's `ChangeDisk`, while the panels are as they were.
            WaitKind::ShareUp if self.still_where(wait) => self.open_drive_menu(wait.side),
            WaitKind::ShareUp => {}
            WaitKind::ToNew {
                sources,
                folder,
                next,
            } => {
                // No new panel of its own: the next plugin's (FAR's `OpenFilePlugin` goes on to it),
                // only while the panels are as they were and nothing else is waited for.
                if self.still_where(wait) && self.plugins.key_wait.is_none() {
                    let files = (sources.clone(), folder.clone());
                    self.ask_to_new(next.clone(), files, wait.side);
                }
            }
            WaitKind::DiskFile { .. } => self.disk_file_passed(wait),
            // Not taken: the next plugin's (FAR's `OpenFindListPlugin` goes on to it).
            WaitKind::FindList { found, next } => self.ask_find_list(next.clone(), found.clone()),
            _ => return false,
        }
        true
    }

    /// FAR's Go to on an object found in an archive (`FD_BUTTON_GOTO` with its `ArcItem`): with the
    /// active panel on the archive's file `archive`, the archive opened over it by the plugin `plugin`
    /// as Enter opens it (its own windows asked), then its folder `folder` inside, the cursor on
    /// `cursor`; nothing more when the plugin does not open it.
    pub(in crate::app) fn open_found_in_archive(
        &mut self,
        plugin: &str,
        archive: &Path,
        (folder, cursor): (String, Option<String>),
    ) {
        let hosted = &self.plugins.host.plugins;
        let Some(index) = hosted
            .iter()
            .position(|found| found.name.eq_ignore_ascii_case(plugin))
        else {
            return;
        };
        let Some(name) = archive.file_name() else {
            return;
        };
        let name = name.to_string_lossy().into_owned();
        let plugin_name = hosted[index].name.clone();
        let side = self.active;
        let far_folder = self.far_folder(side);
        let kind = WaitKind::Found {
            name,
            folder,
            cursor,
        };
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let serial = self.wait_for_plugin(kind, &plugin_name, side, enter);
        let to = (side.index(), serial);
        (self.plugins.host).open_file(index, (archive, false), far_folder.as_deref(), to);
    }

    /// The archive of a found object opened: over the archive's entry `name`, in `folder` inside it
    /// with the cursor on `cursor`.
    fn enter_found(
        &mut self,
        (plugin, instance): (&str, &str),
        side: Side,
        (name, folder, cursor): (&str, &str, Option<&str>),
    ) -> std::io::Result<()> {
        let path = VfsPath::in_session(plugin, instance, folder);
        let panel = &mut self.panels[side.index()];
        panel.push_file_plugin(name, None);
        self.plugins.opening.push((path.clone(), side));
        // A folder no longer in the archive: its root, as FAR's `SetPluginDirectory` failing leaves it.
        (panel.change_dir(&path, cursor))
            .or_else(|_| panel.change_dir(&VfsPath::in_session(plugin, instance, "/"), None))
    }

    /// The plugin's answer to Go to on a found object (`WaitKind::Found`); false for another wait.
    fn found_opened(&mut self, plugin: &str, wait: &KeyWait, go: Option<KeyGo>) -> bool {
        let WaitKind::Found {
            name,
            folder,
            cursor,
        } = &wait.kind
        else {
            return false;
        };
        let went = match go {
            Some(KeyGo::Open(instance)) => {
                let place = (name.as_str(), folder.as_str(), cursor.as_deref());
                self.enter_found((plugin, &instance), wait.side, place)
            }
            Some(go) => self.go_after_key(wait.side, go),
            None => Ok(()),
        };
        if let Err(error) = went {
            self.show_error(&self.plugin_title(plugin), &error);
        }
        true
    }

    /// The plugin's answer to a command or a file waited for, then the keys held after it.
    pub(super) fn opening_done(&mut self, plugin: &str, wait: KeyWait, answer: KeyAnswer) {
        if let Some((_, menu)) = (self.plugins.drive_menu_under).take_if(|(name, _)| name == plugin)
            && self.overlay.is_none()
        {
            self.overlay = Some(Overlay::DriveMenu(menu));
        }
        let held = wait.held.keys.clone();
        if answer.pass && self.passed_on(&wait) {
            return self.release_plugin_keys(held);
        }
        if answer.reread {
            self.panels[wait.side.index()].reload();
        }
        if self.found_opened(plugin, &wait, answer.go.clone()) {
            return self.release_plugin_keys(held);
        }
        let file = match &wait.kind {
            WaitKind::File { name, temp, .. } => Some((name.clone(), temp.clone())),
            _ => None,
        };
        if !matches!(answer.go, Some(KeyGo::Open(_)))
            && let Some((_, Some(temp))) = &file
        {
            // Neither passed on nor opened over: the copy goes.
            super::super::panel::forget_copy(temp.clone());
        }
        let went = match (answer.go, file) {
            // FAR's `OpenFilePlugin` with `PushPrev`: over the file's panel, even the same plugin's.
            (Some(KeyGo::Open(instance)), Some((file, temp))) => {
                let path = VfsPath::in_session(plugin, &instance, "/");
                self.panels[wait.side.index()].push_file_plugin(&file, temp);
                self.plugins.opening.push((path.clone(), wait.side));
                self.panels[wait.side.index()].change_dir(&path, None)
            }
            (Some(KeyGo::Open(instance)), None) => {
                let path = VfsPath::in_session(plugin, &instance, "/");
                self.open_plugin_panel(plugin, wait.side.index(), &path);
                Ok(())
            }
            (Some(go), _) => self.go_after_key(wait.side, go),
            (None, _) => Ok(()),
        };
        if let Err(error) = went {
            self.show_error(&self.plugin_title(plugin), &error);
        }
        self.release_plugin_keys(held);
    }
}

#[cfg(test)]
#[path = "opening_tests.rs"]
mod tests;
