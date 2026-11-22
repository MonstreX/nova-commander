//! The plugin host (ADR-0004 §7, ADR-0016): finds `plugins\<name>\plugin.json` next to the program, loads
//! each enabled plugin's library on a thread of its own (the plugin's calls run there, one at a time, as
//! FAR's run on its main thread) and adapts it to the program: what it tells about itself, its F11
//! commands and drive menu items, its requests (a message, a menu, an input line the user answers; its log). A plugin with a
//! broken manifest, another API version or a library that does not start is not loaded; the program says
//! why and keeps working.

mod calls;
mod copies;
pub mod disk_files;
pub mod editor_requests;
mod ffi;
pub mod getting;
mod items;
mod live;
mod log;
mod manifest;
mod opening;
pub mod panel_requests;
mod plugin_files;
mod provider;
mod run;
mod search_open;
pub mod windows;
mod worker;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use nc_plugin_api::API_VERSION;

use crate::platform;
pub use calls::CallProgress;
use ffi::{HostContext, PLACE_COMMANDS, PLACE_CONFIG, PLACE_DISKS};
pub use ffi::{KeyAnswer, KeyGo, KeyPress, sort_code, sort_mode};
pub use live::Started;
use log::PluginLog;
use manifest::Manifest;
pub use opening::command_prefix;
use provider::PluginProvider;
pub use run::bad_instance;
use windows::{Answer, MessageRequest, Request};
use worker::{Job, Worker};

use crate::vfs::{self, VfsPath};

/// What a plugin tells about itself (FAR's "Plugin information").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginInfo {
    pub title: String,
    pub description: String,
    pub author: String,
    pub version: String,
}

/// What a plugin asks of the program, or what happened to its call.
#[derive(Debug)]
pub enum Event {
    /// A window; the plugin waits for `reply` (a window gone without one: Esc).
    Ask {
        plugin: String,
        request: Request,
        reply: Sender<Answer>,
    },
    /// A dialog; the plugin's thread waits in it, hearing the program through `talk` (the dialog gone
    /// without its end: Esc).
    Dialog {
        plugin: String,
        request: windows::DialogRequest,
        talk: Sender<windows::DialogMessage>,
    },
    /// A message without buttons, kept while the plugin works on.
    Busy {
        plugin: String,
        message: MessageRequest,
        /// The call that showed it (Esc asks that one to stop).
        call: u64,
    },
    /// The call that showed a [`Event::Busy`] window has ended: the window goes.
    Done { plugin: String },
    /// A command failed: the plugin's reason; `serial` the program's number for a command, drive menu
    /// item or configuration it ends.
    Failed {
        plugin: String,
        error: String,
        serial: Option<u64>,
    },
    /// The command or drive menu item `serial` ended without opening a panel (ADR-0019: its wait ends).
    Ran { plugin: String, serial: u64 },
    /// A command or a drive menu item opened the plugin's panel `path` (at its root) for the panel
    /// `target` the item was run for.
    Panel {
        plugin: String,
        target: usize,
        path: VfsPath,
        serial: u64,
    },
    /// A key of the plugin's panel's own was handled for the panel `side` (0 the left): what the program
    /// does next; `serial` the program's number for the press, the keys after it held until then.
    Key {
        plugin: String,
        side: usize,
        /// The program's number for the press it answers.
        serial: u64,
        answer: ffi::KeyAnswer,
    },
    /// A put into the plugin's panel ended (`done`: the paths taken; false: the user stopped it); the
    /// panels showing it are read again.
    Put { plugin: String, done: bool },
    /// Files written out by the plugin for the press `serial` ended: `Ok(true)` done, `Ok(false)` stopped
    /// by the user, `Err` the plugin's reason (FAR's `GetFilesW` returning 1, -1, 0).
    Got {
        plugin: String,
        serial: u64,
        result: Result<bool, String>,
    },
    /// A folder made by the plugin for the press `serial` (FAR's `MakeDirectoryW` returned): its name,
    /// none when the user stopped it, else the plugin's reason.
    Made {
        plugin: String,
        serial: u64,
        result: Result<Option<String>, String>,
    },
    /// A panel's entries handed to the plugin's commands on its file for the press `serial` (FAR's
    /// `ProcessHostFileW` returned): what it did (`HOST_FILE_*` bits; 0 also when the user stopped it),
    /// else the plugin's reason.
    Processed {
        plugin: String,
        serial: u64,
        result: Result<u32, String>,
    },
    /// A disk's file offered for Shift+F2 / Shift+F3 for the press `serial` ended (`disk_files`): how,
    /// and the plugin's reason when it failed.
    DiskFile {
        plugin: String,
        serial: u64,
        end: disk_files::DiskEnd,
        error: Option<String>,
    },
    /// Shift+F1's files put into a new panel of the plugin's for the press `serial` (FAR's
    /// `PluginPutFilesToNew`): `done` false when the user stopped it.
    Created {
        plugin: String,
        serial: u64,
        done: bool,
    },
    /// The plugin set its value of `key` (FAR's `PluginSettings::Set`): the program keeps it in its
    /// settings as `plugins.<name>.settings.<key>`.
    Setting {
        plugin: String,
        key: String,
        value: String,
    },
    /// The plugin asks for its `synchro` with `param` on its thread (FAR's `ACTL_SYNCHRO`), from any of
    /// its threads.
    Synchro { plugin: String, param: u64 },
    /// The plugin asks of a panel (API 3, FAR's `PanelControl`); its thread waits for `reply` (none sent:
    /// it fails). Never asked in a panel's call: that one reads [`PluginHost::panels_seen`].
    PanelAsk {
        plugin: String,
        request: panel_requests::PanelRequest,
        reply: Sender<panel_requests::PanelAnswer>,
    },
    /// A request to the editor up (API 10, FAR's `EditorControl`); the plugin waits for `reply`.
    EditorAsk {
        plugin: String,
        request: editor_requests::EditorRequest,
        reply: Sender<editor_requests::EditorAnswer>,
    },
    /// A configuration of the plugin ended (FAR's `ConfigureW`): `Ok` with its items read again when it
    /// said the settings changed (the plugins' panels shown are read again too, FAR's
    /// `ConfigureCurrent`), none when they did not; `Err` the configuration's own failure.
    Configured {
        plugin: String,
        result: Result<Option<PluginItems>, String>,
        serial: u64,
    },
}

pub use items::PluginItems;

/// A plugin that could not be loaded (FAR's "Error loading plugin").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadFailure {
    pub path: PathBuf,
    pub reason: String,
}

/// A plugin loaded and running on its thread.
#[derive(Debug)]
pub struct Plugin {
    pub name: String,
    pub info: PluginInfo,
    /// Its items in the plugin commands menu (F11).
    pub commands: Vec<String>,
    /// Its items in the drive menu (Alt+F1 / Alt+F2).
    pub disks: Vec<String>,
    /// Its items in Options > Plugins configuration.
    pub configs: Vec<String>,
    /// Its command line prefixes, without the `:`.
    pub prefixes: Vec<String>,
    /// The masks of the files it opens.
    pub file_masks: Vec<String>,
    /// It takes Alt+F7's found files.
    pub find_list: bool,
    /// Its items in the editor's plugin commands menu.
    pub editor: Vec<String>,
    /// It takes the editor's keys.
    pub editor_keys: bool,
    /// Its library's file (FAR's "Module path").
    pub library: PathBuf,
    /// Its folder (`plugins\<name>`), where it is loaded again from.
    pub place: PathBuf,
    worker: Worker,
    /// Its items asked again as a menu opens, not answered yet (`items`).
    items_asked: Option<std::sync::mpsc::Receiver<PluginItems>>,
    /// The call the program waits for: its press's number and its call's (ADR-0019 §5).
    waited: std::sync::Mutex<Option<(u64, u64)>>,
}

impl Plugin {
    /// A call the program waits for as the press `serial`, sent; false when the plugin is gone.
    fn send_waited(&self, serial: u64, job: Job) -> bool {
        let Some(id) = self.worker.send(job) else {
            return false;
        };
        *self.waited_now() = Some((serial, id));
        true
    }

    fn waited_now(&self) -> std::sync::MutexGuard<'_, Option<(u64, u64)>> {
        (self.waited.lock()).unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The number of the call the program waits for as the press `serial`.
    fn waited_call(&self, serial: u64) -> Option<u64> {
        (*self.waited_now()).and_then(|(waited, id)| (waited == serial).then_some(id))
    }
}

/// The loaded plugins and what they ask.
#[derive(Debug)]
pub struct PluginHost {
    // First: a plugin waiting for the program's answer when it ends is answered (Esc) as this goes.
    events: Receiver<Event>,
    sender: Sender<Event>,
    /// Where the plugins' logs are written (`settings\Logs`).
    logs: PathBuf,
    pub plugins: Vec<Plugin>,
    pub failures: Vec<LoadFailure>,
    /// The panels as the program last saw them, which a plugin reads in a panel's call (the program
    /// may be waiting for that call, so it cannot be asked).
    pub panels_seen: panel_requests::PanelsSeen,
    /// The plugins' folder (`plugins` next to the program), looked at again for new ones.
    folder: PathBuf,
    /// The plugins started while the program runs, their starts not told yet (P6.0e).
    starts: live::Starts,
    /// The events taken out of the line as a plugin was let go, but for its own: taken first.
    early: std::cell::RefCell<std::collections::VecDeque<Event>>,
}

impl Default for PluginHost {
    fn default() -> Self {
        let (sender, events) = channel();
        Self {
            events,
            sender,
            logs: PathBuf::new(),
            plugins: Vec::new(),
            failures: Vec::new(),
            panels_seen: panel_requests::PanelsSeen::default(),
            folder: PathBuf::new(),
            starts: live::Starts::default(),
            early: std::cell::RefCell::default(),
        }
    }
}

/// A plugin whose thread was started, its start not told yet.
struct Start {
    manifest: Manifest,
    library: PathBuf,
    place: PathBuf,
    starting: worker::Starting,
}

impl Start {
    /// The plugin once it has told its start (by `deadline`), its panels' paths reaching it from then on
    /// (a plugin of that name loaded before is replaced).
    fn loaded(self, deadline: std::time::Instant) -> Result<Plugin, String> {
        let (worker, told) = self.starting.told(deadline)?;
        Ok(Self::made(
            self.manifest,
            self.library,
            self.place,
            worker,
            told,
        ))
    }

    /// The plugin that told `told` as it started on its thread `worker`.
    fn made(
        manifest: Manifest,
        library: PathBuf,
        place: PathBuf,
        worker: Worker,
        told: run::Told,
    ) -> Plugin {
        let provider = PluginProvider::new(&manifest.name, worker.jobs.clone());
        vfs::register(Arc::new(provider));
        Plugin {
            name: manifest.name,
            info: told.info,
            commands: told.items.commands,
            disks: told.items.disks,
            configs: told.items.configs,
            prefixes: told.items.prefixes,
            file_masks: told.items.file_masks,
            find_list: told.items.find_list,
            editor: told.items.editor,
            editor_keys: told.items.editor_keys,
            library,
            place,
            worker,
            items_asked: None,
            waited: std::sync::Mutex::new(None),
        }
    }
}

impl PluginHost {
    /// The plugins in `folder` (the `plugins` folder next to the program) that `enabled` lets load, in
    /// the order of their folders' names; their logs go to `logs`, their values come from the program's
    /// `settings` (`plugins.<name>.settings.<key>`).
    pub fn load(
        folder: &Path,
        logs: &Path,
        settings: &serde_json::Map<String, serde_json::Value>,
        enabled: &dyn Fn(&str) -> bool,
    ) -> Self {
        let mut host = Self {
            logs: logs.to_path_buf(),
            folder: folder.to_path_buf(),
            ..Self::default()
        };
        let Ok(entries) = std::fs::read_dir(folder) else {
            return host;
        };
        let mut folders: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        folders.sort();
        // Every plugin's thread first, then their starts waited for together (review R17): one that
        // takes its time holds the others no longer than itself.
        let mut starting = Vec::new();
        for place in folders {
            let manifest_path = place.join("plugin.json");
            if !manifest_path.is_file() {
                continue;
            }
            copies::sweep(&place);
            match host.start_one(&place, &manifest_path, settings, enabled) {
                Ok(Some(start)) => starting.push((place, start)),
                Ok(None) => {}
                Err(reason) => host.failures.push(LoadFailure {
                    path: place,
                    reason,
                }),
            }
        }
        let deadline = std::time::Instant::now() + worker::START;
        for (place, start) in starting {
            // One name to a plugin, among those that started (review R48: case aside).
            let taken = (host.plugins.iter())
                .any(|loaded| loaded.name.eq_ignore_ascii_case(&start.manifest.name));
            let loaded = if taken {
                Err(format!("Another plugin is named {}", start.manifest.name))
            } else {
                start.loaded(deadline)
            };
            match loaded {
                Ok(plugin) => host.plugins.push(plugin),
                Err(reason) => host.failures.push(LoadFailure {
                    path: place,
                    reason,
                }),
            }
        }
        host
    }

    /// The data folder of the plugin `name` (API 7): `settings\Plugins\<name>`, beside the logs'
    /// `settings\Logs`.
    fn data_folder(&self, name: &str) -> PathBuf {
        self.logs.with_file_name("Plugins").join(name)
    }

    /// The plugin in `place` started on its thread, unless `enabled` says no.
    fn start_one(
        &self,
        place: &Path,
        manifest_path: &Path,
        settings: &serde_json::Map<String, serde_json::Value>,
        enabled: &dyn Fn(&str) -> bool,
    ) -> Result<Option<Start>, String> {
        let manifest = Manifest::read(manifest_path)?;
        if !enabled(&manifest.name) {
            return Ok(None);
        }
        if manifest.api != API_VERSION {
            return Err(format!(
                "The plugin needs API version {}; this program has {API_VERSION}",
                manifest.api
            ));
        }
        let library = place.join(platform::libraries::file_name(&manifest.library));
        let log = PluginLog::new(&self.logs, &manifest.name);
        let context = HostContext::new(&manifest.name, self.sender.clone(), place, log)
            .with_data_folder(&self.data_folder(&manifest.name))
            .with_settings(plugin_settings(settings, &manifest.name))
            .with_panels_seen(self.panels_seen.clone());
        let (loaded_from, copied) = copies::loadable(&library);
        let starting = Worker::spawn(loaded_from, copied, context)?;
        Ok(Some(Start {
            manifest,
            library,
            place: place.to_path_buf(),
            starting,
        }))
    }

    /// Runs command `index` of plugin `plugin` on its thread for the panel `target` (a panel it opens
    /// comes as [`Event::Panel`] with it); what it asks comes as events.
    pub fn run_command(&self, plugin: usize, index: usize, target: usize, serial: u64) -> bool {
        self.run(plugin, PLACE_COMMANDS, index, (target, serial))
    }

    /// The drive menu's item `index` of plugin `plugin` chosen for the panel `target`: it runs on the
    /// plugin's thread.
    pub fn open_disk(&self, plugin: usize, index: usize, target: usize, serial: u64) -> bool {
        self.run(plugin, PLACE_DISKS, index, (target, serial))
    }

    /// The configuration item `index` of plugin `plugin` chosen (Options > Plugins configuration, F11's
    /// Shift+F9) for the panel `target`: it runs on the plugin's thread and ends as [`Event::Configured`]
    /// (a failure as [`Event::Failed`]).
    pub fn configure(&self, plugin: usize, index: usize, target: usize, serial: u64) -> bool {
        self.run(plugin, PLACE_CONFIG, index, (target, serial))
    }

    /// Whether the call reached the plugin's thread; its end carries `serial`.
    fn run(&self, plugin: usize, place: u32, index: usize, (target, serial): (usize, u64)) -> bool {
        self.plugins.get(plugin).is_some_and(|plugin| {
            let job = Job::Run {
                place,
                index,
                target,
                serial,
            };
            plugin.send_waited(serial, job)
        })
    }

    /// The loaded plugin named `name`.
    fn named(&self, name: &str) -> Option<&Plugin> {
        (self.plugins.iter()).find(|plugin| plugin.name.eq_ignore_ascii_case(name))
    }

    /// What asks the call of `plugin` the program waits for as the press `serial` to stop (the plugin
    /// hears it through `cancelled()`, ADR-0019 §5); none when no such call was sent.
    pub fn stopper(&self, plugin: &str, serial: u64) -> Option<Box<dyn FnOnce() + Send>> {
        let plugin = self.named(plugin)?;
        let id = plugin.waited_call(serial)?;
        let state = std::sync::Arc::clone(&plugin.worker.jobs.state);
        Some(Box::new(move || state.stop(id)))
    }

    /// Whether `plugin`'s thread is in a call now.
    pub fn in_call(&self, plugin: &str) -> bool {
        self.named(plugin)
            .is_some_and(|plugin| plugin.worker.jobs.state.running() != 0)
    }

    /// The call `call` of `plugin` asked to stop (Esc on the window without buttons it showed).
    pub fn stop_call(&self, plugin: &str, call: u64) {
        if let Some(plugin) = self.named(plugin) {
            plugin.worker.jobs.state.stop(call);
        }
    }

    /// What the call of `plugin` waited for as the press `serial` told of its progress, while it runs.
    pub fn progress(&self, plugin: &str, serial: u64) -> Option<CallProgress> {
        let plugin = self.named(plugin)?;
        plugin
            .worker
            .jobs
            .state
            .progress(plugin.waited_call(serial)?)
    }

    /// The loaded plugin whose panel `path` is in.
    fn plugin_of(&self, path: &VfsPath) -> Option<&Plugin> {
        let provider = path.provider()?;
        (self.plugins.iter()).find(|plugin| plugin.name.eq_ignore_ascii_case(provider))
    }

    /// Whether `path` is in a panel of a loaded plugin.
    pub fn is_plugin_panel(&self, path: &VfsPath) -> bool {
        path.session_path().is_some() && self.plugin_of(path).is_some()
    }

    /// The plugin `plugin`'s `synchro` called with `param` on its thread (FAR's `ProcessSynchroEventW`).
    pub fn synchro(&self, plugin: &str, param: u64) {
        if let Some(found) = (self.plugins.iter()).find(|found| found.name == plugin) {
            found.worker.send(Job::Synchro(param));
        }
    }

    /// The panel `path` is in is no longer shown: its plugin hears it (FAR's `ClosePanel`).
    pub fn close_panel(&self, path: &VfsPath) {
        if let (Some(plugin), Some((instance, _))) = (self.plugin_of(path), path.session_path()) {
            plugin.worker.send(Job::Close(instance.to_string()));
        }
    }

    /// `press` on the panel `side` showing the plugin's panel `path`, on the plugin's thread; its answer
    /// comes as [`Event::Key`], a failure as [`Event::Failed`].
    pub fn panel_key(
        &self,
        path: &VfsPath,
        press: ffi::KeyPress,
        side: usize,
        serial: u64,
    ) -> bool {
        let (Some(plugin), Some((instance, folder))) = (self.plugin_of(path), path.session_path())
        else {
            return false;
        };
        let job = Job::Key {
            instance: instance.to_string(),
            path: folder.to_string(),
            press,
            side,
            serial,
        };
        plugin.send_waited(serial, job)
    }

    /// The next thing a plugin asks or reports.
    pub fn take_event(&self) -> Option<Event> {
        let early = self.early.borrow_mut().pop_front();
        early.or_else(|| self.events.try_recv().ok())
    }

    /// An event as a plugin's thread sends it (the program's tests).
    #[cfg(test)]
    pub(crate) fn send_event(&self, event: Event) {
        let _ = self.sender.send(event);
    }
}

/// The values of the plugin `name` among the program's `settings`: `plugins.<name>.settings.<key>`, a
/// text as it is, a number or a flag (the file edited by hand) as its text.
fn plugin_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> BTreeMap<String, String> {
    let prefix = format!("plugins.{name}.settings.");
    (settings.iter())
        .filter_map(|(key, value)| {
            let key = key.strip_prefix(&prefix)?;
            let value = match value {
                serde_json::Value::String(text) => text.clone(),
                serde_json::Value::Number(_) | serde_json::Value::Bool(_) => value.to_string(),
                _ => return None,
            };
            Some((key.to_string(), value))
        })
        .collect()
}

#[cfg(test)]
#[path = "tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "windows_tests.rs"]
mod windows_tests;

#[cfg(test)]
#[path = "config_tests.rs"]
mod config_tests;

#[cfg(test)]
#[path = "start_tests.rs"]
mod start_tests;
