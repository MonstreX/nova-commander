//! A disk's file offered to a plugin for Shift+F2 or Shift+F3 (FAR's `OpenPluginForFile` with
//! `OFP_EXTRACT` / `OFP_COMMANDS`, then `GetFindData` with `OPM_TOPLEVEL`, `GetFiles` or
//! `ProcessHostFile`, `ClosePanel`; API 5): on the plugin's thread in one job, the plugin's panel never
//! shown; a plugin that does not take the file passes the press on ([`Event::Key`] with `pass`), else
//! the end comes as [`Event::DiskFile`].

use std::path::{Path, PathBuf};

use super::ffi::{GetFiles, KeyAnswer, Loaded};
use super::worker::Job;
use super::{Event, KeyGo, PluginHost};
use nc_plugin_api::{GET_SILENT, GET_TOP_LEVEL, HOST_FILE_CHANGED, OPEN_COMMANDS, OPEN_EXTRACT};

/// What the file is offered for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskUse {
    /// Shift+F2: its panel's root got into `destination` (FAR's `GetFiles` with `OPM_TOPLEVEL`;
    /// `silent` after the plugin's first file, `OPM_SILENT`).
    Extract { destination: String, silent: bool },
    /// Shift+F3: its panel's root handed to its commands (`ProcessHostFile` with `OPM_TOPLEVEL`).
    Commands,
}

/// How a disk's file's call ended (FAR's `ProcessOneHostFile`: -1, 1, 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskEnd {
    /// No panel of the plugin's over it, or none read: the next file.
    NotOpened,
    /// Got, or its commands done (FAR's `GetFiles` 1, `ProcessHostFile` TRUE): the file unselected.
    Done,
    /// Stopped, nothing done or failed: no file after it.
    NotDone,
}

impl PluginHost {
    /// The file `path` on a disk offered to the plugin `plugin` for `purpose`, for the press `serial` on
    /// the panel `side`; `folder` the program's folder (FAR's current one). False when the path cannot
    /// be handed over exactly or the plugin is gone (nothing sent).
    pub fn disk_file(
        &self,
        plugin: usize,
        (path, purpose): (&Path, DiskUse),
        folder: Option<&Path>,
        (side, serial): (usize, u64),
    ) -> bool {
        let Some(plugin) = self.plugins.get(plugin) else {
            return false;
        };
        if path.to_str().is_none() {
            return false;
        }
        let job = Job::DiskFile {
            path: PathBuf::from(path),
            purpose,
            folder: folder.map(Path::to_path_buf),
            side,
            serial,
        };
        plugin.send_waited(serial, job)
    }
}

/// The file offered on the plugin's thread: opened (`OFP_EXTRACT` / `OFP_COMMANDS`), its panel's root
/// listed, got or handed to its commands, the panel closed.
pub(super) fn offered(
    loaded: &Loaded,
    plugin: &str,
    (path, purpose, folder): (&Path, &DiskUse, Option<&Path>),
    (side, serial): (usize, u64),
) -> Event {
    let ended = |end: DiskEnd, error: Option<String>| Event::DiskFile {
        plugin: plugin.to_string(),
        serial,
        end,
        error,
    };
    let how = match purpose {
        DiskUse::Extract { .. } => OPEN_EXTRACT,
        DiskUse::Commands => OPEN_COMMANDS,
    };
    let answer = match loaded.open_file(path, how, folder) {
        Ok(answer) => answer,
        Err(error) => return ended(DiskEnd::NotOpened, Some(error)),
    };
    if answer.pass {
        return Event::Key {
            plugin: plugin.to_string(),
            side,
            serial,
            answer: KeyAnswer {
                pass: true,
                ..KeyAnswer::default()
            },
        };
    }
    let Some(KeyGo::Open(instance)) = answer.go else {
        // FAR's `PANEL_STOP`, or an answer that opens nothing: this file is done with.
        return ended(DiskEnd::NotOpened, None);
    };
    let (end, error) = use_root(loaded, &instance, purpose, folder);
    loaded.close_panel(&instance);
    ended(end, error)
}

/// The plugin's panel `instance` read at its root and its entries got or handed to its commands.
fn use_root(
    loaded: &Loaded,
    instance: &str,
    purpose: &DiskUse,
    folder: Option<&Path>,
) -> (DiskEnd, Option<String>) {
    // FAR's `GetFindData` failing: nothing done with this file, the next one's turn.
    let Ok(entries) = loaded.list(instance, "/") else {
        return (DiskEnd::NotOpened, None);
    };
    let names: Vec<String> = (entries.into_iter())
        .map(|entry| entry.name)
        .filter(|name| name != "..")
        .collect();
    let done = match purpose {
        DiskUse::Extract {
            destination,
            silent,
        } => {
            // A plugin that writes no files out gets nothing (FAR's `GetFiles` not 1: the loop ends).
            if !loaded.panel_look(instance, "/").gives_files {
                return (DiskEnd::NotDone, None);
            }
            let files = GetFiles {
                names,
                destination: destination.clone(),
                moving: false,
                mode: GET_TOP_LEVEL | if *silent { GET_SILENT } else { 0 },
                folder: folder
                    .and_then(Path::to_str)
                    .unwrap_or_default()
                    .to_string(),
            };
            loaded.get(instance, "/", &files)
        }
        DiskUse::Commands => loaded
            .process_host_file(instance, "/", &names, GET_TOP_LEVEL)
            .map(|bits| bits & HOST_FILE_CHANGED != 0),
    };
    match done {
        Ok(true) => (DiskEnd::Done, None),
        Ok(false) => (DiskEnd::NotDone, None),
        Err(error) => (DiskEnd::NotDone, Some(error)),
    }
}
