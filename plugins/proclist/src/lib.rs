//! Process list (FAR's proclist, `far-source/plugins/proclist`): the local computer's processes on a
//! panel, read again each second, in FAR's ten modes with its counters; Enter brings a process's window
//! to the front, Shift+F1 and Shift+F2 lower and raise its priority, F8 (Del) ends it, Ctrl+F puts its
//! path into the command line, Ctrl+F3..F12 sort the panel by FAR's sorts and the plugin's own. Its
//! menus' item and its configuration are "Process list"; the command line's `plist:` opens it.
//!
//! An entry is named by the process's name and number (`svchost.exe 1234`): the program selects and
//! reads its entries by name, and two processes may have one name.
//!
//! Windows only: the processes are Windows' (on other systems the crate is empty and no plugin is built;
//! a Linux process list of `/proc` is the port's: ADR-0017, `docs/tech-debt.md` [S7]).
#![cfg(windows)]

pub mod columns;
pub mod counters;
pub mod entries;
pub mod facts;
pub mod friendly;
pub mod info;
mod keys;
pub mod modes;
pub mod options;
pub mod refresh;
pub mod sort;
pub mod table;
mod texts;
pub mod tree;

use std::collections::HashMap;

use nc_plugin_sdk::{
    Error, ErrorKind, FileEntry, Host, Info, KeyAnswer, KeyGo, KeyPress, PanelFlags, PanelKey,
    PanelMode, PanelState, SortBy, Which, api, export_plugin,
};

use modes::{Described, Mode, Source, Translated};
use options::{Options, SORT_MODE, START_MODE, TITLE};
use refresh::{Local, Refresher};
use table::Process;

/// The plugin's name as the panels' state names it.
const NAME: &str = "proclist";

/// How long the services' list is read again after (they rarely change).
const SERVICES_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// The status line of every mode (FAR's `StatusCols`, `StatusWidths`).
const STATUS_COLUMNS: &str = "N,S,D,T";
const STATUS_WIDTHS: &str = "0,8,0,5";

/// One panel of the plugin's.
#[derive(Debug, Clone, Default)]
struct Instance {
    /// FAR's `SortMode` of the panel.
    sort: u32,
    /// Listed once already: the panel shows its own mode and sort.
    listed: bool,
    /// The mode and the program's sort the panel was last read with.
    seen: Option<(u32, u32)>,
    /// The tree's unfolded branches (owner, 2026-11-15: folded at first), by process number and start:
    /// a number another process takes again opens folded.
    unfolded: std::collections::HashSet<tree::Started>,
}

pub struct ProcList {
    host: Host,
    options: Options,
    modes: Vec<Mode>,
    translated: Vec<Option<Translated>>,
    refresher: Option<Refresher>,
    instances: HashMap<String, Instance>,
    opened: usize,
    /// The processes' texts the viewer reads, by entry, with when each was made.
    texts: HashMap<String, (std::time::Instant, Vec<u8>)>,
    /// Shift+F3's views so far, and the temporary folders they left (removed once empty).
    viewed: usize,
    viewed_folders: Vec<std::path::PathBuf>,
    /// The services by process and when they were read.
    services: (
        Option<std::time::Instant>,
        HashMap<u32, Vec<nc_plugin_sdk::system::Service>>,
    ),
}

/// The panel of the plugin's being read: the side whose folder is read now (the published state of the
/// one being listed), or the active one.
fn read_panel(host: &Host) -> Option<PanelState> {
    let ours = |state: &PanelState| state.plugin.as_deref() == Some(NAME);
    [Which::Left, Which::Right]
        .into_iter()
        .filter_map(|which| host.panel(which))
        .find(|state| ours(state) && state.reading)
        .or_else(|| host.panel(Which::Active).filter(ours))
}

impl ProcList {
    fn start_mode(&self) -> usize {
        let mode = self.host.int_setting(START_MODE, 1);
        usize::try_from(mode)
            .ok()
            .filter(|mode| *mode < modes::MODES)
            .unwrap_or(1)
    }

    /// The saved sort; the tree before any is saved (owner, 2026-11-15; FAR's flat list is "Unsorted").
    fn saved_sort(&self) -> u32 {
        sort::loaded(self.host.int_setting(SORT_MODE, i64::from(sort::TREE)))
    }

    fn instance(&mut self, instance: &str) -> &mut Instance {
        let sort = self.saved_sort();
        (self.instances.entry(instance.to_string())).or_insert(Instance {
            sort,
            ..Instance::default()
        })
    }

    fn refresher(&mut self) -> &Refresher {
        let host = self.host;
        self.refresher.get_or_insert_with(|| {
            let synchro = host.synchro();
            let refresher = Refresher::start(Box::new(Local), move || synchro.ask(0));
            if let Some(error) = &refresher.not_started {
                host.log(&format!("The processes are not read each second: {error}"));
            }
            refresher
        })
    }

    /// A new panel's instance.
    fn open(&mut self) -> String {
        self.opened += 1;
        let instance = self.opened.to_string();
        self.instance(&instance);
        instance
    }

    /// The mode the panel is listed in and its columns' widths: the panel's, or before the panel shows
    /// its own mode the start mode with its widths as given (read again at once after).
    fn view(&mut self, instance: &str) -> (usize, Vec<usize>) {
        let panel = read_panel(&self.host);
        let start = self.start_mode();
        let state = self.instance(instance);
        let first = !state.listed;
        state.listed = true;
        match panel.filter(|_| !first) {
            Some(panel) => {
                if panel.sort != api::SORT_UNSORTED {
                    state.sort = (sort::far_mode(panel.sort)).unwrap_or(state.sort);
                }
                state.seen = Some((panel.view_mode, panel.sort));
                let mode = usize::try_from(panel.view_mode).unwrap_or(start);
                (mode, entries::widths(&panel.widths))
            }
            None => {
                self.host.synchro().ask(0);
                let widths = (self.modes.get(start)).map_or("", |mode| mode.widths.as_str());
                (start, entries::widths(widths))
            }
        }
    }

    /// What the description shows of `process` in `kind`.
    fn description(
        &mut self,
        process: &Process,
        kind: Described,
        windows: &HashMap<u32, nc_plugin_sdk::system::Window>,
    ) -> Option<String> {
        match kind {
            Described::Window => windows.get(&process.pid).map(|window| window.title()),
            // Read by the refresh thread: none until then.
            Described::FileDescription => {
                let path = process.path.as_deref()?;
                self.refresher.as_ref()?.description(path).flatten()
            }
            _ => entries::described(process, kind),
        }
    }

    /// The friendly name of `process` (`XN`), indented by its tree's `row`.
    fn friendly(&mut self, process: &Process, row: Option<tree::Row>) -> String {
        let services = self.services_of(process.pid);
        let windows = HashMap::new();
        let description = self.description(process, Described::FileDescription, &windows);
        let name = friendly::name(process, &services, description.as_deref());
        match row {
            Some(row) => tree::shown(&name, row),
            None => name,
        }
    }

    /// The services running in `pid`, as read at most every few seconds.
    fn services_of(&mut self, pid: u32) -> Vec<nc_plugin_sdk::system::Service> {
        let stale = (self.services.0).is_none_or(|read| read.elapsed() >= SERVICES_EVERY);
        if stale {
            self.services = (
                Some(std::time::Instant::now()),
                nc_plugin_sdk::system::services_by_process(),
            );
        }
        self.services.1.get(&pid).cloned().unwrap_or_default()
    }

    fn entries(&mut self, instance: &str) -> Vec<FileEntry> {
        let (mode, widths) = self.view(instance);
        let processes: Vec<Process> = self.refresher().table().processes.clone();
        let state = self.instance(instance);
        let ordered = tree::ordered(&processes, state.sort, &mut state.unfolded);
        let translated = self
            .translated
            .get(mode)
            .cloned()
            .flatten()
            .unwrap_or_default();
        let windows = match translated.described {
            Some(Described::Window) => nc_plugin_sdk::system::top_windows(),
            _ => HashMap::new(),
        };
        let friendly = (translated.custom.iter()).any(|(source, _)| *source == Source::Friendly);
        let shown = view(&translated, &widths);
        (ordered.into_iter())
            .map(|(at, row)| {
                let process = &processes[at];
                let described = (translated.described)
                    .and_then(|kind| self.description(process, kind, &windows));
                let name = friendly.then(|| self.friendly(process, row));
                entries::entry(process, &shown, described, name.as_deref())
            })
            .collect()
    }

    /// The modes as the settings have them, and in the program's types.
    fn load_modes(&mut self) {
        self.modes = modes::load(|key| self.host.setting(key));
        self.translated = (self.modes.iter())
            .map(|mode| modes::translate_sized(&mode.columns, &mode.widths))
            .collect();
    }
}

fn view<'a>(mode: &'a Translated, widths: &'a [usize]) -> entries::View<'a> {
    entries::View {
        mode,
        widths,
        decimal: nc_plugin_sdk::system::decimal_separator(),
    }
}

impl nc_plugin_sdk::Plugin for ProcList {
    fn new(host: Host) -> Self {
        let mut plugin = Self {
            host,
            options: Options::load(&host),
            modes: Vec::new(),
            translated: Vec::new(),
            refresher: None,
            instances: HashMap::new(),
            opened: 0,
            texts: HashMap::new(),
            viewed: 0,
            viewed_folders: Vec::new(),
            services: (None, HashMap::new()),
        };
        plugin.load_modes();
        plugin
    }

    fn info(&self) -> Info {
        Info {
            title: TITLE.to_string(),
            description: "The computer's processes on a panel".to_string(),
            author: "nova-commander (after FAR's proclist)".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<String> {
        (self.options.add_to_plugins.then(|| TITLE.to_string()))
            .into_iter()
            .collect()
    }

    fn run_command(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(Some(self.open()))
    }

    fn disks(&self) -> Vec<String> {
        (self.options.add_to_disks.then(|| TITLE.to_string()))
            .into_iter()
            .collect()
    }

    fn open_disk(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(Some(self.open()))
    }

    fn config_items(&self) -> Vec<String> {
        vec![TITLE.to_string()]
    }

    /// FAR's `Config`: its options and the panel modes saved by "Ok".
    fn configure(&mut self, _index: usize) -> bool {
        let closed = self.host.dialog(&self.options.dialog());
        let Some(options) = Options::from_dialog(&closed) else {
            return false;
        };
        self.options = options;
        options.save(&self.host);
        modes::save(&self.modes, |key, value| self.host.set_setting(key, value));
        true
    }

    fn prefixes(&self) -> Vec<String> {
        vec!["plist".to_string()]
    }

    /// `plist:` opens the panel; another computer's (`plist:\\name`) is not shown yet.
    fn open_command(
        &mut self,
        _prefix: &str,
        command: &str,
        _folder: Option<&std::path::Path>,
    ) -> Result<KeyAnswer, Error> {
        if command.trim_start().starts_with("\\\\") {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Another computer's processes are not shown yet",
            ));
        }
        Ok(KeyAnswer::go(KeyGo::Open(self.open())))
    }

    fn list(&mut self, instance: &str, path: &str) -> Result<Vec<FileEntry>, Error> {
        if path != "/" {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("The process list has no folder {path}"),
            ));
        }
        Ok(self.entries(instance))
    }

    /// An entry as the viewer sees it: its text's size (FAR's `GetFiles` for F3 and the quick view).
    fn stat(&mut self, _instance: &str, path: &str) -> Result<FileEntry, Error> {
        self.text_entry(path)
    }

    fn read(
        &mut self,
        _instance: &str,
        path: &str,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, Error> {
        self.read_text(path, offset, buffer)
    }

    fn panel_title(&self, _instance: &str, _path: &str) -> String {
        TITLE.to_string()
    }

    fn panel_modes(&self, _instance: &str, _path: &str) -> Vec<Option<PanelMode>> {
        (self.modes.iter().zip(&self.translated))
            .map(|(mode, translated)| {
                let translated = translated.as_ref()?;
                Some(PanelMode {
                    columns: translated.columns.clone(),
                    widths: mode.widths.clone(),
                    titles: translated.titles.clone(),
                    status_columns: STATUS_COLUMNS.to_string(),
                    status_widths: STATUS_WIDTHS.to_string(),
                    full_screen: mode.full_screen,
                    ..PanelMode::default()
                })
            })
            .collect()
    }

    fn panel_start_mode(&self, _instance: &str, _path: &str) -> Option<usize> {
        Some(self.start_mode())
    }

    fn panel_start_sort(&self, instance: &str, _path: &str) -> Option<(SortBy, bool)> {
        let mode = (self.instances.get(instance)).map_or_else(|| self.saved_sort(), |one| one.sort);
        sort::start(mode)
    }

    /// FAR's `OPIF_SHOWNAMESONLY`.
    fn panel_flags(&self, _instance: &str) -> PanelFlags {
        PanelFlags {
            names_only: true,
            ..PanelFlags::default()
        }
    }

    fn panel_keys(&self, _instance: &str) -> Vec<PanelKey> {
        keys::keys()
    }

    fn panel_key(
        &mut self,
        instance: &str,
        _path: &str,
        press: &KeyPress,
    ) -> Result<KeyAnswer, Error> {
        Ok(self.key(instance, press))
    }

    /// FAR's `FE_CLOSE`: the mode and the sort the panel had saved (a panel never read in its own mode
    /// keeps the saved mode); the reading thread ends with the last panel.
    fn close_panel(&mut self, instance: &str) {
        if let Some(closed) = self.instances.remove(instance) {
            let panel = closed.seen.map_or(api::SORT_UNSORTED, |(_, panel)| panel);
            self.host
                .set_int(SORT_MODE, i64::from(sort::saved(closed.sort, panel)));
            if let Some((mode, _)) = closed.seen {
                self.host.set_int(START_MODE, i64::from(mode));
            }
        }
        if self.instances.is_empty() {
            self.refresher = None;
        }
    }

    /// The processes read again: each panel of the plugin's read again (FAR's `ProcessSynchroEvent`).
    fn synchro(&mut self, _param: u64) {
        for which in [Which::Left, Which::Right] {
            let ours =
                (self.host.panel(which)).is_some_and(|state| state.plugin.as_deref() == Some(NAME));
            if ours {
                self.host.reread(which, true);
            }
        }
    }
}

export_plugin!(ProcList);
