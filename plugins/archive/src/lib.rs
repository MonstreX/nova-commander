//! Archives as panels (FAR's arclite, `far-source/plugins/arclite`; ADR-0021, ADR-0022): every format
//! 7-Zip's library reads, opened by Enter on a file of arclite's masks, by Ctrl+PgDn on any file it
//! reads, by `arc:<archive>` or by the Plugins menu's "Open archive"; its folders on a panel with the
//! packed sizes, ".." at its root back to the file; its files written out by the plugin (FAR's
//! `GetFilesW`: F5 / F6 and Alt+F6 by arclite's Extract dialog, F3 / F4 for the viewer and the editor).
//! Where an archive opens: `opening`, `loading`, `detect`, `library_open`; its panel: `panel`; the archive
//! itself: `opened`, `item`, `library_items`, `tree`, `library_read`; extracting: `extraction`, `extract`
//! and the windows (`extract_dialog`, `asking`, `progress`, `layout`); writing: `update*`, `deleting`,
//! `creating`; testing: `testing`; the Plugins menu's commands: `menu_commands`, a disk's archives at
//! once: `bulk`.

pub mod asking;
pub mod bulk;
pub mod command_line;
pub mod command_run;
pub mod command_update;
pub mod create_dialog;
pub mod create_dialog_read;
pub mod create_dialog_state;
pub mod create_formats;
pub mod create_options;
pub mod creating;
pub mod deleting;
pub mod detect;
pub mod extract;
pub mod extract_dialog;
pub mod extract_options;
pub mod extraction;
pub mod item;
pub mod layout;
pub mod library_items;
pub mod library_open;
pub mod library_read;
pub mod loading;
pub mod menu_commands;
pub mod names;
pub mod open_asking;
pub mod open_results;
pub mod opened;
pub mod opening;
pub mod options;
pub mod options_dialogs;
pub mod panel;
pub mod partition;
pub mod partition_panel;
pub mod progress;
pub mod properties;
pub mod props;
pub mod sfx;
pub mod sfx_convert;
pub mod sfx_dialog;
pub mod sfx_module;
pub mod sfx_rsrc;
pub mod support;
#[cfg(test)]
pub(crate) mod test_archives;
pub mod testing;
pub mod testing_library;
pub mod tree;
pub mod update;
pub mod update_dialog;
pub mod update_library;
pub mod update_options;
pub mod update_progress;
pub mod update_props;
pub mod update_volumes;
pub mod update_write;
pub mod updating;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use nc_plugin_sdk::{
    Error, ErrorKind, FileEntry, GetMode, GetRequest, Host, HostFileDone, Info, InfoLine,
    KeyAnswer, KeyGo, KeyPress, Menu, MenuItem, OpenHow, PanelFlags, PanelKey, PutRequest, SortBy,
    export_plugin,
};

use opened::Opened;
use options::{Options, TITLE};

/// arclite's word for a file that is not an archive it reads (`MSG_ERROR_NOT_ARCHIVE`).
pub(crate) const NOT_ARCHIVE: &str = "File is not recognized as archive";

pub struct ArchivePlugin {
    host: Host,
    options: Options,
    /// Each panel's archive, by the panel's instance.
    panels: HashMap<String, Opened>,
    /// Panels of no archive yet, for Shift+F1 (arclite's `Plugin` of an archive not open).
    new_panels: HashSet<String>,
    opened: usize,
    /// Shift+F2's answers for a disk's next archives (arclite's `batch_options`).
    batch: Option<extraction::Batch>,
    /// 7-Zip's library, found when first needed.
    library: std::cell::OnceCell<nc_plugin_sdk::sevenzip::Library>,
}

impl ArchivePlugin {
    fn archive(&self, instance: &str) -> Result<&Opened, Error> {
        (self.panels.get(instance))
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "The archive is not open"))
    }

    /// The archive written anew (`temp`) put in its place (arclite's `close`, `MoveFileEx`, `reopen`):
    /// every panel of it closed first, then read again with its password, so that none goes on with the
    /// old file (a later update from it would undo this one); the new file gone when it is not placed.
    fn swap_in(&mut self, archive: &Path, temp: &Path) -> Result<(), Error> {
        // The same file whatever its name's case (a new archive typed in another case).
        let same = |path: &Path| {
            path.to_string_lossy().to_lowercase() == archive.to_string_lossy().to_lowercase()
        };
        let sharing: Vec<(String, Option<String>)> = (self.panels.iter())
            .filter(|(_, other)| same(&other.path))
            .map(|(other, opened)| (other.clone(), opened.password.clone()))
            .collect();
        for (other, _) in &sharing {
            self.panels.remove(other);
        }
        let placed = update_write::replace(temp, archive);
        if placed.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        // Every one read again whatever happens to another; the first failure told after.
        let mut reopening = Ok(());
        for (other, password) in sharing {
            match self.load(archive) {
                Ok(Some(mut reopened)) => {
                    reopened.password = password;
                    self.panels.insert(other, reopened);
                }
                Ok(None) => {}
                Err(error) => reopening = reopening.and(Err(error)),
            }
        }
        placed.map_err(|error| io_error(&error))?;
        reopening
    }

    /// The entries `request` asks for of the folder `path` of the panel `instance` extracted; the archive
    /// deleted after when asked (closed first: its panel gone), or (F6) written anew without them.
    fn extract(&mut self, instance: &str, request: &extraction::Request) -> Result<(), Error> {
        let host = self.host;
        let opened = (self.panels.get_mut(instance))
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "The archive is not open"))?;
        let archive = opened.path.clone();
        match extraction::run(host, opened, request, &mut self.batch)? {
            extraction::After::Nothing => Ok(()),
            extraction::After::DeleteArchive if request.mode.top_level => {
                // Shift+F2's panel is no panel shown: arclite's `Far::close_panel` of it closes none.
                self.panels.remove(instance);
                let _ = std::fs::remove_file(&archive);
                Ok(())
            }
            extraction::After::DeleteArchive => {
                self.panels.remove(instance);
                extraction::delete_archive(host, &archive);
                Ok(())
            }
            extraction::After::Replace(temp) => self.swap_in(&archive, &temp),
        }
    }

    /// arclite's Alt+F6 (`Plugin::extract`): the selected entries, or the one under the cursor, into the
    /// archive's folder by the Extract dialog; the panels read again.
    fn extract_here(
        &mut self,
        instance: &str,
        path: &str,
        press: &KeyPress,
    ) -> Result<KeyAnswer, Error> {
        let names = if press.selected.is_empty() {
            press.current.iter().cloned().collect()
        } else {
            press.selected.clone()
        };
        let archive = self.archive(instance)?.path.clone();
        let request = extraction::Request {
            path,
            names: &names,
            destination: archive.parent().map(Path::to_path_buf).unwrap_or_default(),
            mode: GetMode::default(),
            folder: archive.parent(),
            moving: false,
        };
        match self.extract(instance, &request) {
            Err(error) if error.kind == ErrorKind::Cancelled => {}
            other => other?,
        }
        Ok(KeyAnswer {
            reread: true,
            reread_passive: true,
            ..KeyAnswer::default()
        })
    }

    /// arclite's Ctrl+A (`show_attr`): the properties of the entry under the cursor, of its folder on
    /// "..", the archive's own at its root; none (or "...") leaves the key to the program.
    fn show_properties(&self, instance: &str, path: &str, press: &KeyPress) -> KeyAnswer {
        let Some(opened) = self.panels.get(instance) else {
            return KeyAnswer::pass();
        };
        let at = (path, press.current.as_deref());
        let properties = properties::shown_for(opened, at);
        if properties.is_empty() {
            return KeyAnswer::pass();
        }
        let columns = self.host.screen().map_or(80, |screen| screen.columns);
        let width = if columns >= 80 { columns - 20 } else { 60 };
        let dialog = properties::dialog(&properties, usize::try_from(width).unwrap_or(60));
        let closed = self.host.dialog(&dialog);
        let more = dialog.items.len() - 1;
        if closed.button == Some(more) {
            KeyAnswer::pass()
        } else {
            KeyAnswer::default()
        }
    }
}

fn io_error(error: &std::io::Error) -> Error {
    let kind = match error.kind() {
        std::io::ErrorKind::NotFound => ErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied => ErrorKind::Denied,
        _ => ErrorKind::Other,
    };
    Error::new(kind, error.to_string())
}

impl nc_plugin_sdk::Plugin for ArchivePlugin {
    fn new(host: Host) -> Self {
        Self {
            host,
            options: Options::load(&host),
            panels: HashMap::new(),
            new_panels: HashSet::new(),
            batch: None,
            opened: 0,
            library: std::cell::OnceCell::new(),
        }
    }

    fn info(&self) -> Info {
        Info {
            title: TITLE.to_string(),
            description: "Archives as panels: 7-Zip's formats".to_string(),
            author: "nova-commander (after FAR's arclite)".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    /// arclite's menu (`OPEN_PLUGINSMENU`): "Open archive" (the first way the file opens) and "Open
    /// (format menu)" (every way, chosen in a menu when there are several),
    /// "Create archive", "Extract archive(s)", "Test archive(s)", "Convert archive into SFX".
    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        let items = vec![
            MenuItem::new("&Open archive"),
            MenuItem::new("Open (&format menu)"),
            MenuItem::new("&Create archive"),
            MenuItem::new("&Extract archive(s)"),
            MenuItem::new("&Test archive(s)"),
            MenuItem::new("C&onvert archive into SFX"),
        ];
        let chosen = self.host.menu(&Menu::new(TITLE, items));
        match chosen.and_then(|chosen| chosen.item) {
            Some(item @ (0 | 1)) => self.open_current(item == 1),
            Some(2) => self.create_from_panel().map(|()| None),
            Some(item @ (3 | 4)) => self.bulk(item == 3).map(|()| None),
            Some(5) => {
                self.convert_to_sfx();
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn config_items(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    fn configure(&mut self, _index: usize) -> bool {
        let library = self.library();
        let (formats, modules) = (library.formats(), library.modules());
        let context = options::Context {
            host: self.host,
            formats: &formats,
            modules: &modules,
        };
        let closed = (self.host).dialog_with(&self.options.dialog(), |event, controls| {
            Options::handle(&event, controls, &context)
        });
        let Some(options) = self.options.from_dialog(&closed) else {
            return false;
        };
        options.save(&self.host);
        // arclite's `Patch7zCP::SetCP` after its settings dialog.
        library.set_code_pages(options.oem_cp, options.ansi_cp);
        self.options = options;
        true
    }

    fn prefixes(&self) -> Vec<String> {
        vec![self.options.plugin_prefix.clone()]
    }

    /// Every file, whatever the configuration (the program reads the masks again only with its menus):
    /// arclite checks its masks and "Handle archive commands" itself, Ctrl+PgDn without the masks.
    fn file_masks(&self) -> Vec<String> {
        vec!["*".to_string()]
    }

    /// arclite's `OPEN_COMMANDLINE`: `arc:<archive>` opens it on a panel; `c u x e d t` run their command
    /// (no panel; both panels read again after it); a command not understood shows its syntax.
    fn open_command(
        &mut self,
        _prefix: &str,
        command: &str,
        folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        let exists = |name: &str| opening::full_path(name, folder).exists();
        let command = match command_line::parse(command, &exists) {
            Ok(command_line::Command::Open(command)) => command,
            Ok(other) => {
                self.run_line(other, folder)?;
                return Ok(KeyAnswer {
                    reread: true,
                    reread_passive: true,
                    ..KeyAnswer::default()
                });
            }
            Err(bad) => {
                command_run::show_usage(&self.host, bad);
                return Ok(KeyAnswer::default());
            }
        };
        let path = opening::full_path(&command.path, folder);
        match self.open(&path, command.password)? {
            Some(instance) => Ok(KeyAnswer::go(KeyGo::Open(instance))),
            None => Err(Error::new(ErrorKind::Other, NOT_ARCHIVE)),
        }
    }

    fn open_file(
        &mut self,
        path: &Path,
        how: OpenHow,
        _folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        Ok(self.open_offered(path, how))
    }

    fn list(&mut self, instance: &str, path: &str) -> Result<Vec<FileEntry>, Error> {
        let opened = self.archive(instance)?;
        panel::entries(opened, path)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("{path} is not there")))
    }

    fn stat(&mut self, instance: &str, path: &str) -> Result<FileEntry, Error> {
        let opened = self.archive(instance)?;
        if path.trim_matches('/').is_empty() {
            return Ok(FileEntry::folder(""));
        }
        let node = (opened.tree.find(path))
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("{path} is not there")))?;
        Ok(panel::entry(opened, node))
    }

    fn panel_title(&self, instance: &str, path: &str) -> String {
        (self.panels.get(instance)).map_or_else(String::new, |opened| panel::title(opened, path))
    }

    fn panel_start_mode(&self, _instance: &str, _path: &str) -> Option<usize> {
        (self.options.own_panel_view_mode).then_some(self.options.panel_view_mode)
    }

    fn panel_start_sort(&self, _instance: &str, _path: &str) -> Option<(SortBy, bool)> {
        (self.options.own_panel_view_mode).then_some((SortBy::Name, false))
    }

    fn panel_info_lines(&self, instance: &str, _path: &str) -> Vec<InfoLine> {
        (self.panels.get(instance)).map_or_else(Vec::new, panel::info_lines)
    }

    fn panel_format(&self, _instance: &str, _path: &str) -> String {
        self.options.plugin_prefix.clone()
    }

    fn close_panel(&mut self, instance: &str) {
        self.panels.remove(instance);
        self.new_panels.remove(instance);
    }

    /// FAR's `OpenFilePlugin` with no file (arclite's `AnalyseW` without a name: an archive not open,
    /// unless "Handle archive creation" is off).
    fn open_new(&mut self) -> Result<Option<String>, Error> {
        if !self.options.handle_create {
            return Ok(None);
        }
        self.opened += 1;
        let instance = self.opened.to_string();
        self.new_panels.insert(instance.clone());
        Ok(Some(instance))
    }

    /// Its files written out by it (FAR's arclite has `GetFilesW`), a disk's taken in by their paths
    /// (its `PutFilesW`), its entries deleted and folders made by it (`DeleteFilesW`, `MakeDirectoryW`)
    /// when its format is written (arclite's `updatable`).
    fn panel_flags(&self, instance: &str) -> PanelFlags {
        let writes = (self.panels.get(instance)).is_none_or(|opened| opened.writes.is_some());
        PanelFlags {
            gives_files: true,
            takes_paths: writes,
            deletes: writes,
            makes_folders: writes,
            ..PanelFlags::default()
        }
    }

    /// FAR's `PutFilesW` (arclite's `put_files`): the sources added to the panel's folder by the "Add
    /// files to archive" dialog; the archive written anew beside itself, closed, put in its place and
    /// read again under the same panel.
    fn put(&mut self, instance: &str, path: &str, request: &PutRequest) -> Result<(), Error> {
        if self.new_panels.contains(instance) {
            return self.put_new(request);
        }
        let opened = self.archive(instance)?;
        let update = updating::Request {
            path,
            sources: &request.sources,
            folder: request.folder.as_deref(),
            moving: request.moving,
            mode: request.mode,
        };
        let Some(written) = updating::write_update(self.host, opened, &update)? else {
            return Ok(());
        };
        let archive = opened.path.clone();
        self.swap_in(&archive, &written.temp)?;
        updating::after(self.host, &archive, written)
    }

    /// FAR's `DeleteFilesW` (arclite's `delete_files`): its question, then the archive written anew
    /// without the entries and what is under them.
    fn delete_files(
        &mut self,
        instance: &str,
        path: &str,
        names: &[String],
        mode: GetMode,
    ) -> Result<(), Error> {
        let opened = self.archive(instance)?;
        let Some(temp) = deleting::delete(self.host, opened, (path, names), mode)? else {
            return Ok(());
        };
        let archive = opened.path.clone();
        self.swap_in(&archive, &temp)
    }

    /// FAR's `MakeDirectoryW` (arclite's `create_dir`): its "Enter directory name:", then the archive
    /// written anew with the folder.
    fn make_folder(
        &mut self,
        instance: &str,
        path: &str,
        name: &str,
        mode: GetMode,
    ) -> Result<String, Error> {
        let opened = self.archive(instance)?;
        let (temp, made) = deleting::make_folder(self.host, opened, (path, name), mode)?;
        let archive = opened.path.clone();
        self.swap_in(&archive, &temp)?;
        Ok(made)
    }

    /// FAR's `ProcessHostFileW` (Shift+F3): arclite's menu of one command, "Test archive files", then
    /// `test_files`; passed, the entries unselected (FAR's call from its panel) or, for a disk's
    /// archives at once, the call done (`OPM_TOPLEVEL`: TRUE).
    fn process_host_file(
        &mut self,
        instance: &str,
        path: &str,
        names: &[String],
        mode: GetMode,
    ) -> Result<HostFileDone, Error> {
        let menu = Menu::new(TITLE, vec![MenuItem::new("Test archive files")]);
        if self.host.menu(&menu).and_then(|chosen| chosen.item) != Some(0) {
            return Ok(HostFileDone::default());
        }
        let host = self.host;
        let opened = (self.panels.get_mut(instance))
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "The archive is not open"))?;
        if !bulk::test_entries(host, opened, (path, names))? {
            return Ok(HostFileDone::default());
        }
        let from_panel = mode == GetMode::default();
        Ok(HostFileDone {
            changed: !from_panel,
            unselect: from_panel,
        })
    }

    /// arclite's own keys: Ctrl+A, Alt+F6; a disk's Enter and Ctrl+PgDn entering a partition.
    fn panel_keys(&self, instance: &str) -> Vec<PanelKey> {
        let disk = (self.panels.get(instance)).is_some_and(|opened| opened.part_mode.is_some());
        let partitions = if disk {
            &partition_panel::KEYS[..]
        } else {
            &[]
        };
        (["Ctrl+A", "Alt+F6"].iter().chain(partitions))
            .map(|key| PanelKey::new(key, ""))
            .collect()
    }

    /// arclite's own keys: Ctrl+A its properties, Alt+F6 its extraction, Enter / Ctrl+PgDn a partition.
    fn panel_key(
        &mut self,
        instance: &str,
        path: &str,
        press: &KeyPress,
    ) -> Result<KeyAnswer, Error> {
        if press.key == "Ctrl+A" {
            return Ok(self.show_properties(instance, path, press));
        }
        if partition_panel::KEYS.contains(&press.key.as_str()) {
            return self.enter_partition(instance, path, press);
        }
        self.extract_here(instance, path, press)
    }

    /// FAR's `GetFilesW` (arclite's `get_files`): F5 / F6 with the Extract dialog, F3 / F4 without it.
    fn get(&mut self, instance: &str, path: &str, request: &GetRequest) -> Result<(), Error> {
        let request = extraction::Request {
            path,
            names: &request.names,
            destination: request.destination.clone(),
            mode: request.mode,
            folder: request.folder.as_deref(),
            moving: request.moving,
        };
        self.extract(instance, &request)
    }

    fn read(
        &mut self,
        instance: &str,
        path: &str,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, Error> {
        let opened = (self.panels.get_mut(instance))
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "The archive is not open"))?;
        let item = (opened.tree.find(path))
            .filter(|&node| !opened.tree.nodes[node].is_dir)
            .and_then(|node| opened.tree.nodes[node].item)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("{path} is not there")))?;
        opened
            .read(item, offset, buffer)
            .map_err(|error| io_error(&error))
    }
}

export_plugin!(ArchivePlugin);
