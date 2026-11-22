//! A plugin's panel's files written out by the plugin (FAR's `GetFilesW`, API 5), its entries deleted and
//! its folders made by it (`DeleteFilesW`, `MakeDirectoryW`), handed to its commands on the panel's file
//! (`ProcessHostFileW`): sent to its thread as presses the program waits for, their ends an
//! [`Event::Got`](super::Event::Got), [`Event::Made`](super::Event::Made) or
//! [`Event::Processed`](super::Event::Processed).

use super::worker::Job;
use super::{Event, PluginHost, ffi};
use crate::vfs::VfsPath;

pub use super::ffi::{
    GET_EDIT, GET_SILENT, GET_VIEW, GetFiles, HOST_FILE_CHANGED, HOST_FILE_UNSELECT,
};

impl PluginHost {
    /// `sources` (paths on a disk, taken from `from`) copied or moved into the folder `path` of a
    /// plugin's panel that takes paths, `mode` FAR's `OPM_*` (`GET_EDIT`: the editor's copy put back), on
    /// the plugin's thread; its end comes as [`Event::Put`], a failure as [`Event::Failed`].
    pub fn put(
        &self,
        path: &VfsPath,
        sources: Vec<std::path::PathBuf>,
        (moving, mode): (bool, u32),
        from: Option<&std::path::Path>,
    ) {
        let (Some(plugin), Some((instance, folder))) = (self.plugin_of(path), path.session_path())
        else {
            return;
        };
        // Every path exactly, or none put (review R49: the plugin would keep another name).
        let exact: Result<Vec<String>, String> =
            sources.iter().map(|source| ffi::exact(source)).collect();
        let from = from.map(ffi::exact).transpose();
        match exact.and_then(|sources| Ok((sources, from?))) {
            Ok((sources, from)) => {
                plugin.worker.send(Job::Put {
                    instance: instance.to_string(),
                    path: folder.to_string(),
                    files: ffi::PutFiles {
                        sources,
                        moving,
                        mode,
                        folder: from.unwrap_or_default(),
                    },
                });
            }
            Err(error) => {
                let _ = self.sender.send(Event::Failed {
                    plugin: plugin.name.clone(),
                    error,
                    serial: None,
                });
            }
        }
    }

    /// `files` of the folder `path` of a plugin's panel written out by the plugin for the press `serial`;
    /// false when no plugin's panel is there (sent nowhere).
    pub fn get(&self, path: &VfsPath, files: GetFiles, serial: u64) -> bool {
        let (Some(plugin), Some((instance, folder))) = (self.plugin_of(path), path.session_path())
        else {
            return false;
        };
        let job = Job::Get {
            instance: instance.to_string(),
            path: folder.to_string(),
            files,
            serial,
        };
        plugin.send_waited(serial, job)
    }

    /// The entries `names` of the folder `path` of a plugin's panel deleted by the plugin for the press
    /// `serial`, its own question first; false when no plugin's panel is there.
    pub fn delete_files(&self, path: &VfsPath, names: Vec<String>, serial: u64) -> bool {
        let (Some(plugin), Some((instance, folder))) = (self.plugin_of(path), path.session_path())
        else {
            return false;
        };
        let job = Job::Delete {
            instance: instance.to_string(),
            path: folder.to_string(),
            names,
            serial,
        };
        plugin.send_waited(serial, job)
    }

    /// The entries `names` of the folder `path` of a plugin's panel opened from a file handed to the
    /// plugin's commands on that file for the press `serial` (FAR's `ProcessHostFileW`, Shift+F3); false
    /// when no plugin's panel is there.
    pub fn process_host_file(&self, path: &VfsPath, names: Vec<String>, serial: u64) -> bool {
        let (Some(plugin), Some((instance, folder))) = (self.plugin_of(path), path.session_path())
        else {
            return false;
        };
        let job = Job::HostFile {
            instance: instance.to_string(),
            path: folder.to_string(),
            names,
            serial,
        };
        plugin.send_waited(serial, job)
    }

    /// A folder made by the plugin in the folder `path` of its panel for the press `serial`, the name
    /// asked by it; false when no plugin's panel is there.
    pub fn make_folder(&self, path: &VfsPath, serial: u64) -> bool {
        let (Some(plugin), Some((instance, folder))) = (self.plugin_of(path), path.session_path())
        else {
            return false;
        };
        let job = Job::MakeFolder {
            instance: instance.to_string(),
            path: folder.to_string(),
            serial,
        };
        plugin.send_waited(serial, job)
    }
}
