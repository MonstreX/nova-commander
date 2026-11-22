//! The temporary panel (ADR-0016 §14), FAR's TmpPanel: files and folders from different places on one
//! panel, each by its whole path and with its real name, so that the program views, copies, moves and
//! deletes the files themselves. Its options (`options`, FAR's `TmpCfg.cpp`) are read at the start, at
//! every open and before its configuration dialog, and written on its OK. With the common panel (FAR's
//! default) every temporary panel shows the one current of ten common lists, kept while the program runs
//! (`TmpClass.cpp:810-813` `IfOptCommonPanel`); without it each panel opened has a list of its own, gone
//! when it closes. Copying or moving into it adds the paths (folders with their contents as the option
//! says, `put`); the names of files gone drop off as it is read, unless any elements are kept; in safe
//! mode its entries have no real names. `tmp:` on the command line and list files (Enter or Ctrl+PgDn on
//! one of its masks) open it with a list's paths or show the list as a menu (`command`, `listfile`,
//! `menu_list`, `run`); Alt+F7's "Panel" puts the found files on it (`find_list`).

use std::collections::HashMap;
use std::path::Path;

use nc_plugin_sdk::{
    Error, ErrorKind, FileEntry, Host, Info, KeyAnswer, KeyPress, PanelFlags, PanelKey, PanelMode,
    Plugin, export_plugin,
};

mod command;
mod find_list;
mod keys;
mod list;
mod listfile;
mod menu_list;
mod options;
mod put;
mod run;
mod savlst;
mod update;

pub use list::List;
pub use options::Options;

/// FAR's common panel: ten of them.
pub const PANELS: usize = 10;

/// The item's name in F11's menu and the drive menu.
const ITEM: &str = "Temporary panel";

/// Its own mode's key (FAR's TmpPanel opens in its mode 4).
const OWN_MODE: usize = 4;

/// The instance of a panel opened with the common panel: it shows the current common list.
pub const COMMON: &str = "common";
/// The instances of panels opened without it: `local<n>`, each its own list.
const LOCAL: &str = "local";

pub struct TmpPanel {
    host: Host,
    panels: [List; PANELS],
    /// The current common panel (FAR's `SharedData->CurrentCommonPanel`): the one every temporary panel
    /// shows with the common panel.
    current: usize,
    /// The lists of the panels opened without the common panel (FAR's `m_LocalPanel`), by instance.
    locals: HashMap<String, List>,
    /// How many panels were opened without it: the next one's number.
    opened: usize,
    /// As the settings had them when last read (FAR's `Opt`).
    options: Options,
    /// The common panel search results go to next when none is empty (FAR's
    /// `Opt.LastSearchResultsPanel`).
    last_search_results: usize,
}

impl TmpPanel {
    /// The list the panel `instance` shows now (FAR's `IfOptCommonPanel`): the current common one with
    /// the common panel, else its own.
    fn list(&mut self, instance: &str) -> Result<&mut List, Error> {
        let local = instance
            .strip_prefix(LOCAL)
            .is_some_and(|number| number.parse::<usize>().is_ok());
        if instance != COMMON && !local {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("No temporary panel {instance}"),
            ));
        }
        // A panel of its own that is closed (or from an earlier run) is none, whatever the option is now.
        if local && !self.locals.contains_key(instance) {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("The temporary panel {instance} is closed"),
            ));
        }
        if self.options.common_panel {
            return Ok(&mut self.panels[self.current]);
        }
        if instance == COMMON {
            // Opened with the common panel, shown on after it was turned off: a list of its own.
            return Ok(self.locals.entry(instance.to_string()).or_default());
        }
        // Only a panel `open` made has a list: one reopened by its path (the folders' history after a
        // restart) is no temporary panel any more, never another's list.
        self.locals.get_mut(instance).ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("The temporary panel {instance} is closed"),
            )
        })
    }

    /// FAR's `OpenW`: the options read again (`GetOptions`), then the panel opened: the common one, or a
    /// new one of its own.
    fn open(&mut self) -> String {
        self.options = Options::read(&self.host);
        self.new_instance()
    }

    /// A panel opened with the options as they are now: the common one, or a new one of its own.
    fn new_instance(&mut self) -> String {
        if self.options.common_panel {
            return COMMON.to_string();
        }
        self.opened += 1;
        let instance = format!("{LOCAL}{}", self.opened);
        self.locals.insert(instance.clone(), List::default());
        instance
    }

    /// FAR's title (`TmpClass.cpp:95-97`): "(R) " in safe mode, " [n]" with the common panel.
    fn title(&self) -> String {
        let safe = if self.options.safe_mode_panel {
            "(R) "
        } else {
            ""
        };
        let common = if self.options.common_panel {
            format!(" [{}]", self.current)
        } else {
            String::new()
        };
        format!("{safe}{ITEM}{common}")
    }
}

/// Where this run's `local<n>` numbers start: by the clock, so a path from an earlier run (the folders'
/// history) never names a panel of this one.
fn first_number() -> usize {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    usize::try_from(seconds % 1_000_000).map_or(0, |seconds| seconds * 1000)
}

/// Its item, when the option shows it.
fn item_if(shown: bool) -> Vec<String> {
    if shown {
        vec![ITEM.to_string()]
    } else {
        Vec::new()
    }
}

/// The item's path in the panel: its whole path after the root's `/`.
fn item_path(path: &str) -> &Path {
    Path::new(path.strip_prefix('/').unwrap_or(path))
}

impl Plugin for TmpPanel {
    fn new(host: Host) -> Self {
        Self {
            host,
            panels: Default::default(),
            current: 0,
            locals: HashMap::new(),
            opened: first_number(),
            // FAR's `SetStartupInfoW`: `GetOptions`.
            options: Options::read(&host),
            last_search_results: 0,
        }
    }

    fn info(&self) -> Info {
        Info {
            title: ITEM.to_string(),
            description: "Files and folders from different places on one panel".to_string(),
            author: "nova-commander".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// TmpPanel's `GetPluginInfoW`: its item with AddToPluginsMenu.
    fn commands(&self) -> Vec<String> {
        item_if(self.options.add_to_plugins_menu)
    }

    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(Some(self.open()))
    }

    /// With AddToDisksMenu.
    fn disks(&self) -> Vec<String> {
        item_if(self.options.add_to_disks_menu)
    }

    fn open_disk(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(Some(self.open()))
    }

    /// Always its item (`PluginConfig`).
    fn config_items(&self) -> Vec<String> {
        vec![ITEM.to_string()]
    }

    /// Its command line prefixes: the option's, split by `:` (FAR's `CommandPrefix`, `plugins.cpp`
    /// `enum_tokens`).
    fn prefixes(&self) -> Vec<String> {
        (self.options.prefix.split(':'))
            .filter(|prefix| !prefix.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// The list files' masks (FAR's `AnalyseW`: `Opt.Mask`).
    fn file_masks(&self) -> Vec<String> {
        Some(self.options.mask.clone())
            .filter(|mask| !mask.trim().is_empty())
            .into_iter()
            .collect()
    }

    fn open_command(
        &mut self,
        _prefix: &str,
        command: &str,
        folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        self.command(command, folder)
    }

    /// A list is not opened for Alt+F7's search in archives (`OpenHow::Search`): its menu would be
    /// asked from the search and "Replace" would empty the common panel (FAR's TmpPanel does both).
    fn open_file(
        &mut self,
        path: &Path,
        how: nc_plugin_sdk::OpenHow,
        folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        if how == nc_plugin_sdk::OpenHow::Search {
            return Ok(KeyAnswer::pass());
        }
        self.open_list(path, folder)
    }

    /// FAR's TmpPanel exports `SetFindListW`.
    fn takes_find_list(&self) -> bool {
        true
    }

    fn find_list(&mut self, found: &[FileEntry]) -> Result<KeyAnswer, Error> {
        self.take_find_list(found)
    }

    /// FAR's `Config` (`TmpCfg.cpp:37-110`): the options read again, the dialog, saved on OK.
    fn configure(&mut self, _index: usize) -> bool {
        let options = Options::read(&self.host);
        let closed = self.host.dialog(&options::dialog(&options));
        if closed.button != Some(options::OK) {
            self.options = options;
            return false;
        }
        self.options = options::from_dialog(&options, &closed.values);
        self.options.write(&self.host);
        true
    }

    fn list(&mut self, instance: &str, path: &str) -> Result<Vec<FileEntry>, Error> {
        if path != "/" {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("The temporary panel has no folder {path}"),
            ));
        }
        let (any, safe) = (self.options.any_in_panel, self.options.safe_mode_panel);
        Ok(self.list(instance)?.entries(any, safe))
    }

    fn stat(&mut self, instance: &str, path: &str) -> Result<FileEntry, Error> {
        if path == "/" {
            return Ok(FileEntry::folder(""));
        }
        let item = item_path(path);
        let (any, safe) = (self.options.any_in_panel, self.options.safe_mode_panel);
        let found = self.list(instance)?.holds(item);
        (found.then(|| list::shown(item, any, safe)).flatten()).ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("{} is not there", item.display()),
            )
        })
    }

    fn panel_title(&self, _instance: &str, _path: &str) -> String {
        self.title()
    }

    /// Its own mode from the options (`TmpClass.cpp:101-111`).
    fn panel_modes(&self, _instance: &str, _path: &str) -> Vec<Option<PanelMode>> {
        let mut modes = vec![None; OWN_MODE + 1];
        modes[OWN_MODE] = Some(self.options.mode());
        modes
    }

    fn panel_start_mode(&self, _instance: &str, _path: &str) -> Option<usize> {
        Some(OWN_MODE)
    }

    /// TmpPanel's `OPIF_ADDDOTS | OPIF_SHOWNAMESONLY` (`TmpClass.cpp` `GetOpenPanelInfo`): names only in
    /// the program's modes, its own mode showing the whole paths; it takes paths (`PutFiles`). Its real
    /// names (`OPIF_REALNAMES`, not in safe mode) are the entries'.
    fn panel_flags(&self, _instance: &str) -> PanelFlags {
        PanelFlags {
            takes_paths: true,
            names_only: true,
            ..PanelFlags::default()
        }
    }

    /// TmpPanel's `ProcessPanelInput` keys and its key bar (`TmpClass.cpp`), as the options have them.
    fn panel_keys(&self, _instance: &str) -> Vec<PanelKey> {
        keys::keys(&self.options)
    }

    fn panel_key(
        &mut self,
        instance: &str,
        _path: &str,
        press: &KeyPress,
    ) -> Result<KeyAnswer, Error> {
        self.key(instance, press)
    }

    /// TmpPanel's `Info.Format = MTempPanel`: the copy dialog's target is "Temporary panel:".
    fn panel_format(&self, _instance: &str, _path: &str) -> String {
        ITEM.to_string()
    }

    fn put(
        &mut self,
        instance: &str,
        _path: &str,
        request: &nc_plugin_sdk::PutRequest,
    ) -> Result<(), Error> {
        self.put_paths(instance, &request.sources)
    }

    /// The panel's close (FAR's `ClosePanel`): a list of its own goes with it.
    fn close_panel(&mut self, instance: &str) {
        self.locals.remove(instance);
    }

    fn remove(&mut self, instance: &str, path: &str, _folder: bool) -> Result<(), Error> {
        let item = item_path(path);
        if self.list(instance)?.remove(item) {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::NotFound,
                format!("{} is not there", item.display()),
            ))
        }
    }
}

export_plugin!(TmpPanel);
