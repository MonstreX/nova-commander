//! Network browser (FAR's Network plugin, `far-source/plugins/network`): the network's providers, their
//! domains, the domains' computers and a computer's shares on a panel; Enter on a share leaves for its
//! folder. Its menus' items are FAR's "Network browser" (the Plugins menu, the configuration) and "network"
//! (the Disks menu); the command line's `net:` opens it at a computer (`net:\\server`, its share under the
//! cursor with `net:\\server\share`), `netg:` at a domain. Where a panel opens: `opening`.
//!
//! Windows only: the network is Windows' (WNet, NetServerEnum; on other systems the crate is empty and no
//! plugin is built; a Linux network browser is the port's decision: ADR-0017, `docs/tech-debt.md` [S7]).
#![cfg(windows)]

mod asking;
pub mod browse;
mod connecting;
mod favorite_keys;
pub mod favorites;
mod keys;
pub mod mapping;
mod opening;
pub mod options;
pub mod start;
pub mod timed;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use nc_plugin_sdk::{
    Error, ErrorKind, FileEntry, Host, Info, KeyAnswer, KeyGo, KeyPress, PanelFlags, PanelKey,
    PanelMode, Which, export_plugin,
};

use browse::{Browser, System};
use opening::Pending;
use options::{DISK_ITEM, FORMAT, Options, TITLE};
use timed::Timed;

/// The plugin's name as the panels' state names it.
const NAME: &str = "network";
/// FAR's `PanelMode` setting: the mode a panel opens in, its default 3.
const PANEL_MODE: &str = "PanelMode";
const DEFAULT_MODE: usize = 3;

/// One panel of the plugin's: the folder it was last read at, the mode it was shown in.
#[derive(Debug, Clone, Default)]
struct Instance {
    path: String,
    mode: Option<u32>,
}

pub struct NetworkPlugin {
    host: Host,
    options: Options,
    net: Timed,
    browser: Browser,
    instances: HashMap<String, Instance>,
    opened: usize,
    /// No panel opened yet (FAR's `IsFirstRun`): the first goes to this computer's workgroup.
    first_run: bool,
    /// Where the next panel opens (FAR's `CommonCurResource`): the folder the last one closed at.
    last: Option<String>,
    pending: Option<Pending>,
    /// The name and password last given for a connection.
    last_logon: RefCell<asking::LastLogon>,
    /// FAR's favorites, as the plugin's setting keeps them.
    favorites: RefCell<favorites::Favorites>,
}

impl NetworkPlugin {
    /// The panel of ours being read now, as the host tells it.
    fn reading_panel(&self) -> Option<nc_plugin_sdk::PanelState> {
        [Which::Left, Which::Right]
            .into_iter()
            .filter_map(|which| self.host.panel(which))
            .find(|state| state.reading && state.plugin.as_deref() == Some(NAME))
    }

    fn saved_mode(&self) -> usize {
        (self.host.setting(PANEL_MODE))
            .and_then(|text| text.trim().parse().ok())
            .filter(|mode| *mode < 10)
            .unwrap_or(DEFAULT_MODE)
    }

    /// Ctrl+`digit` on the panel `instance`: the mode it closes in (FAR's `PInfo.ViewMode` at
    /// `FE_CLOSE`), the program's reading not asked for it. Ctrl+6..9 with no mode of ours there change
    /// nothing (the program's are 0 to 5): the mode chosen stays. A mode chosen from the menus is
    /// known at the panel's next reading.
    fn mode_chosen(&mut self, instance: &str, digit: u32) {
        if !mode_exists(digit) {
            return;
        }
        self.instances.entry(instance.to_string()).or_default().mode = Some(digit);
    }
}

/// Whether the panel has a mode `digit`: the program's 0 to 5, else one of ours.
fn mode_exists(digit: u32) -> bool {
    let index = usize::try_from(digit).unwrap_or(usize::MAX);
    index < 6 || modes().get(index).is_some_and(Option::is_some)
}

/// FAR's `PanelModesArray`: modes 3, 4 and 5 the plugin's (the network name, the disk a share is
/// connected as, its comment), the others the program's.
fn modes() -> Vec<Option<PanelMode>> {
    let titles = |count: usize| {
        ["Network name", "*:", "Comment"][..count]
            .iter()
            .map(|title| Some((*title).to_string()))
            .collect()
    };
    let mode = |columns: &str, widths: &str, count: usize| {
        Some(PanelMode {
            columns: columns.to_string(),
            widths: widths.to_string(),
            titles: titles(count),
            ..PanelMode::default()
        })
    };
    let mut modes = vec![None; 10];
    modes[3] = mode("N,C0,C1", "0,2,0", 3);
    modes[4] = mode("N,C0", "0,2", 2);
    modes[5] = mode("N,C0,C1", "0,2,0", 3);
    modes
}

impl nc_plugin_sdk::Plugin for NetworkPlugin {
    fn new(host: Host) -> Self {
        Self {
            host,
            options: Options::load(&host),
            net: Timed::new(Arc::new(System), timed::WAIT),
            browser: Browser::default(),
            instances: HashMap::new(),
            opened: 0,
            first_run: true,
            last: None,
            pending: None,
            last_logon: RefCell::default(),
            favorites: RefCell::new(favorites::Favorites::parse(
                &host.setting(favorites::SETTING).unwrap_or_default(),
            )),
        }
    }

    fn info(&self) -> Info {
        Info {
            title: TITLE.to_string(),
            description: "The network's computers and shares on a panel".to_string(),
            author: "nova-commander (after FAR's Network)".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<String> {
        (self.options.add_to_plugins.then(|| TITLE.to_string()))
            .into_iter()
            .collect()
    }

    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(Some(self.open(None)))
    }

    fn disks(&self) -> Vec<String> {
        (self.options.add_to_disks.then(|| DISK_ITEM.to_string()))
            .into_iter()
            .collect()
    }

    fn open_disk(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(Some(self.open(None)))
    }

    fn config_items(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    fn configure(&mut self, _index: usize) -> bool {
        let closed = self.host.dialog(&self.options.dialog());
        let Some(options) = self.options.from_dialog(&closed) else {
            return false;
        };
        self.options = options;
        options.save(&self.host);
        true
    }

    fn prefixes(&self) -> Vec<String> {
        vec![
            "net".to_string(),
            "netg".to_string(),
            start::SHARE_UP.to_string(),
        ]
    }

    /// FAR's `SetOpenFromCommandLine`: `net:\\server[\share]` (a bare name a computer's unless the
    /// settings say domains, `%NAME%` expanded), `netg:name` a domain; nothing named, or a name the
    /// network does not know, opens it as its menu's item does (FAR's panel opens whatever
    /// `GotoComputer` finds). `net_up:\\server\share` (the program's, from the share's root; FAR's
    /// `OPEN_FILEPANEL`): the computer with the share under the cursor, a share not Microsoft's left to
    /// the program (it opens the drive menu then).
    fn open_command(
        &mut self,
        prefix: &str,
        command: &str,
        _folder: Option<&std::path::Path>,
    ) -> Result<KeyAnswer, Error> {
        let up = prefix.eq_ignore_ascii_case(start::SHARE_UP);
        if up && !start::goes_up_from(&self.net, command.trim()) {
            return Ok(KeyAnswer::pass());
        }
        let expanded = if up {
            command.trim().to_string()
        } else {
            start::expand_variables(command)
        };
        let found = start::target(prefix, &expanded, &self.options).and_then(|target| {
            let favorites = self.favorites.borrow();
            start::computer(&self.net, &self.options, &favorites, &target).ok()
        });
        let Some(found) = found else {
            // From a share whose computer is not found: FAR's panel stays at its top.
            let start = up.then(|| ("/".to_string(), None));
            return Ok(KeyAnswer::go(KeyGo::Open(self.open(start))));
        };
        for (path, resource) in found.known {
            self.browser.remember(&path, resource);
        }
        let instance = self.open(Some((found.path, found.cursor)));
        Ok(KeyAnswer::go(KeyGo::Open(instance)))
    }

    fn list(&mut self, instance: &str, path: &str) -> Result<Vec<FileEntry>, Error> {
        let mode = self.reading_panel().map(|state| state.view_mode);
        let listed = (self.browser)
            .list(
                &crate::favorites::favored(&self.net, &self.favorites.borrow()),
                &self.options,
                path,
            )
            .map_err(|error| Error::new(ErrorKind::NotFound, keys::system_text(&error)))?;
        let state = self.instances.entry(instance.to_string()).or_default();
        state.path = path.to_string();
        state.mode = mode.or(state.mode);
        self.opened_at(instance, path);
        Ok(listed)
    }

    /// FAR's `PanelTitle` " Net: <folder> ".
    fn panel_title(&self, _instance: &str, path: &str) -> String {
        let folder = match path {
            "/" | "" => "",
            _ => browse::split(path).1,
        };
        format!("{FORMAT}: {folder}")
    }

    fn panel_format(&self, _instance: &str, _path: &str) -> String {
        FORMAT.to_string()
    }

    fn panel_modes(&self, _instance: &str, _path: &str) -> Vec<Option<PanelMode>> {
        modes()
    }

    fn panel_start_mode(&self, _instance: &str, _path: &str) -> Option<usize> {
        Some(self.saved_mode())
    }

    /// FAR's `OPIF_SHOWPRESERVECASE`.
    fn panel_flags(&self, _instance: &str) -> PanelFlags {
        PanelFlags {
            preserve_case: true,
            ..PanelFlags::default()
        }
    }

    /// The keys, labelled as FAR's for the folder the panel was last read at.
    fn panel_keys(&self, instance: &str) -> Vec<PanelKey> {
        let path = self.instances.get(instance).map(|one| one.path.as_str());
        let display = path.and_then(|path| self.browser.display_of(path));
        let mut keys = keys::keys();
        let favorites = path.is_some_and(favorite_keys::in_folder);
        keys.extend(connecting::connection_keys(display, favorites));
        keys
    }

    fn panel_key(
        &mut self,
        instance: &str,
        path: &str,
        press: &KeyPress,
    ) -> Result<KeyAnswer, Error> {
        Ok(self.key(instance, path, press))
    }

    /// FAR's `FE_CLOSE`: the mode saved, the folder kept for the next panel.
    fn close_panel(&mut self, instance: &str) {
        if let Some(closed) = self.instances.remove(instance) {
            if let Some(mode) = closed.mode {
                self.host.set_setting(PANEL_MODE, &mode.to_string());
            }
            if !closed.path.is_empty() {
                self.last = Some(closed.path);
            }
        }
    }

    /// The synchro of a panel just opened (its instance): taken to its first folder.
    fn synchro(&mut self, param: u64) {
        self.go_to_start(param);
    }
}

export_plugin!(NetworkPlugin);
