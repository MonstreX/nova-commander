//! A plugin opened by the command line or by a file (ADR-0016 §14): FAR's `ProcessCommandLine` (the text
//! before the first `:` one of a plugin's prefixes, `GetPluginPrefixPath`) and `OpenFilePlugin` (a file of
//! a plugin's masks). With FAR's default confirmations (`PluginConfirm.Prefix` and `OpenFilePlugin` off)
//! the first plugin in the plugins' order takes it, no menu asks.

use std::path::{Path, PathBuf};

use super::PluginHost;
use super::worker::Job;

/// FAR's `GetPluginPrefixPath` (`pathmix.cpp`): the text before the first `:` of a command line, when it
/// can be a plugin's prefix - not empty, not after a separator at the start, not a drive's letter.
/// (FAR also leaves out a one-character name the system has a DOS device for, `QueryDosDevice`: only
/// drive letters are such names on today's systems.)
pub fn command_prefix(line: &str) -> Option<&str> {
    if line.starts_with(['\\', '/']) {
        return None;
    }
    let (prefix, _) = line.split_once(':')?;
    let mut chars = prefix.chars();
    let one_letter = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.next().is_none();
    (!prefix.is_empty() && !one_letter).then_some(prefix)
}

impl PluginHost {
    /// The first plugin one of whose prefixes is `prefix`, as FAR compares them (ignoring case).
    pub fn prefix_plugin(&self, prefix: &str) -> Option<usize> {
        (self.plugins.iter())
            .position(|plugin| (plugin.prefixes.iter()).any(|own| own.eq_ignore_ascii_case(prefix)))
    }

    /// The plugins that take Alt+F7's found files, in their order (FAR's `OpenFindListPlugin`).
    pub fn find_list_plugins(&self) -> Vec<usize> {
        (self.plugins.iter().enumerate())
            .filter(|(_, plugin)| plugin.find_list)
            .map(|(index, _)| index)
            .collect()
    }

    /// Alt+F7's `found` objects for the plugin `plugin`, its panel for the panel `side`; its answer comes
    /// as [`super::Event::Key`] for the press `serial`.
    pub fn find_list(
        &self,
        plugin: usize,
        found: Vec<crate::vfs::Entry>,
        (side, serial): (usize, u64),
    ) {
        if let Some(plugin) = self.plugins.get(plugin) {
            plugin.send_waited(
                serial,
                Job::FindList {
                    found,
                    side,
                    serial,
                },
            );
        }
    }

    /// `prefix:command` for the plugin `plugin`, from the panel `side`; its answer comes as
    /// [`super::Event::Key`] for the press `serial`.
    pub fn command(
        &self,
        plugin: usize,
        (prefix, command): (&str, &str),
        folder: Option<&Path>,
        (side, serial): (usize, u64),
    ) {
        if let Some(plugin) = self.plugins.get(plugin) {
            let job = Job::Command {
                prefix: prefix.to_string(),
                command: command.to_string(),
                folder: folder.map(Path::to_path_buf),
                side,
                serial,
            };
            plugin.send_waited(serial, job);
        }
    }

    /// The file `path` for the plugin `plugin`, Enter or (`alternative`) Ctrl+PgDn on it in the panel
    /// `side`; its answer comes as [`super::Event::Key`] for the press `serial`.
    pub fn open_file(
        &self,
        plugin: usize,
        (path, alternative): (&Path, bool),
        folder: Option<&Path>,
        (side, serial): (usize, u64),
    ) {
        if let Some(plugin) = self.plugins.get(plugin) {
            let job = Job::OpenFile {
                path: PathBuf::from(path),
                alternative,
                folder: folder.map(Path::to_path_buf),
                side,
                serial,
            };
            plugin.send_waited(serial, job);
        }
    }

    /// Shift+F1's `sources` (paths on a disk, taken from `from`) put into a new panel of the plugin
    /// `plugin` for the press `serial` on the panel `side` (FAR's `PluginPutFilesToNew`); its end comes
    /// as [`super::Event::Created`], none made as [`super::Event::Key`] passing it on. False when a path
    /// cannot be handed over exactly (nothing sent) or the plugin is gone.
    pub fn create(
        &self,
        plugin: usize,
        (sources, from): (&[PathBuf], &Path),
        (side, serial): (usize, u64),
    ) -> bool {
        let Some(plugin) = self.plugins.get(plugin) else {
            return false;
        };
        // Every path exactly, or none put (review R49: the plugin would keep another name).
        let exact: Result<Vec<String>, String> = sources
            .iter()
            .map(|source| super::ffi::exact(source))
            .collect();
        let (Ok(sources), Ok(folder)) = (exact, super::ffi::exact(from)) else {
            return false;
        };
        let files = super::ffi::PutFiles {
            sources,
            moving: false,
            mode: 0,
            folder,
        };
        let job = Job::Create {
            files,
            side,
            serial,
        };
        plugin.send_waited(serial, job)
    }
}

#[cfg(test)]
#[path = "opening_tests.rs"]
mod tests;
