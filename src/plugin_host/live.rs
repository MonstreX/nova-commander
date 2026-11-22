//! Plugins loaded, unloaded and started anew while the program runs (P6.0e; FAR's `PluginsControl` with
//! `PCTL_LOADPLUGIN` / `PCTL_UNLOADPLUGIN`, the owner's own: one left hanging started anew, the plugins'
//! folder looked at again). The screen never waits: a start is told later ([`PluginHost::pump_starts`]); a
//! plugin let go has its call asked to stop, hears nothing more and is released by its own thread once
//! that call ends (one that never ends keeps its thread and its library's copy to the program's end, the
//! new one loaded from a copy of its own).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::manifest::Manifest;
use super::worker::{self, Poll};
use super::{Event, LoadFailure, PluginHost, Start};
use crate::vfs;

/// The starts not told yet, each with its deadline.
#[derive(Default)]
pub(super) struct Starts(Vec<(Start, Instant)>);

impl std::fmt::Debug for Starts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Starts({})", self.0.len())
    }
}

/// What came of a plugin started while the program runs.
#[derive(Debug)]
pub enum Started {
    /// Loaded, by its name.
    Loaded(String),
    Failed(LoadFailure),
}

impl Event {
    /// The plugin it is of.
    pub fn plugin(&self) -> &str {
        match self {
            Self::Ask { plugin, .. }
            | Self::Dialog { plugin, .. }
            | Self::Busy { plugin, .. }
            | Self::Done { plugin }
            | Self::Failed { plugin, .. }
            | Self::Ran { plugin, .. }
            | Self::Panel { plugin, .. }
            | Self::Key { plugin, .. }
            | Self::Put { plugin, .. }
            | Self::Got { plugin, .. }
            | Self::Made { plugin, .. }
            | Self::Processed { plugin, .. }
            | Self::DiskFile { plugin, .. }
            | Self::Created { plugin, .. }
            | Self::Setting { plugin, .. }
            | Self::Synchro { plugin, .. }
            | Self::PanelAsk { plugin, .. }
            | Self::EditorAsk { plugin, .. }
            | Self::Configured { plugin, .. } => plugin,
        }
    }
}

impl PluginHost {
    /// The plugin in the folder `place` started on its thread, its start told by [`Self::pump_starts`];
    /// `settings` the program's (`plugins.<name>.settings.<key>`). Refused: why.
    pub fn start_plugin(
        &mut self,
        place: &Path,
        settings: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), String> {
        let manifest_path = place.join("plugin.json");
        let manifest = Manifest::read(&manifest_path)?;
        if self.is_known(&manifest.name) {
            return Err(format!("A plugin named {} is loaded", manifest.name));
        }
        super::copies::sweep(place);
        let start = self
            .start_one(place, &manifest_path, settings, &|_| true)?
            .ok_or_else(|| format!("{} is not loaded", manifest.name))?;
        self.starts.0.push((start, Instant::now() + worker::START));
        Ok(())
    }

    /// Whether a plugin of that name is loaded (not starting, not let go).
    pub fn is_loaded(&self, name: &str) -> bool {
        self.named(name).is_some()
    }

    /// Whether a plugin of that name is loaded or starting.
    fn is_known(&self, name: &str) -> bool {
        self.named(name).is_some()
            || (self.starts.0.iter())
                .any(|(start, _)| start.manifest.name.eq_ignore_ascii_case(name))
    }

    /// The starts told since, each loaded or failed; never waits.
    pub fn pump_starts(&mut self) -> Vec<Started> {
        let mut told = Vec::new();
        for (start, deadline) in std::mem::take(&mut self.starts.0) {
            let Start {
                manifest,
                library,
                place,
                starting,
            } = start;
            match starting.poll(deadline) {
                Poll::Waiting(starting) => {
                    let start = Start {
                        manifest,
                        library,
                        place,
                        starting,
                    };
                    self.starts.0.push((start, deadline));
                }
                Poll::Told(result) => match *result {
                    Ok((worker, said)) => {
                        let plugin = Start::made(manifest, library, place, worker, said);
                        told.push(Started::Loaded(plugin.name.clone()));
                        self.plugins.push(plugin);
                    }
                    Err(reason) => told.push(Started::Failed(LoadFailure {
                        path: place,
                        reason,
                    })),
                },
            }
        }
        told
    }

    /// The names of the plugins starting.
    pub fn starting_names(&self) -> Vec<String> {
        (self.starts.0.iter())
            .map(|(start, _)| start.manifest.name.clone())
            .collect()
    }

    /// The plugin `name` let go (unloaded): its call is asked to stop, its panels' paths have no provider
    /// any more, what it sent and the program has not taken is forgotten but the values it kept. Its
    /// folder; none when no plugin of that name is loaded.
    pub fn unload(&mut self, name: &str) -> Option<PathBuf> {
        let at = (self.plugins.iter()).position(|plugin| plugin.name.eq_ignore_ascii_case(name))?;
        let super::Plugin {
            name,
            place,
            worker,
            ..
        } = self.plugins.remove(at);
        // Let go first: nothing it sends after the events are looked through is heard.
        worker.let_go();
        vfs::unregister(&name.to_ascii_lowercase());
        let mut early = self.early.borrow_mut();
        let others: VecDeque<Event> = (early.drain(..))
            .chain(self.events.try_iter())
            .filter(|event| {
                matches!(event, Event::Setting { .. })
                    || !event.plugin().eq_ignore_ascii_case(&name)
            })
            .collect();
        *early = others;
        Some(place)
    }

    /// The plugins' folders with a manifest whose plugin is neither loaded nor starting (new ones, or
    /// ones not loaded), their names as their manifests say.
    pub fn not_loaded(&self) -> Vec<(String, PathBuf)> {
        let Ok(entries) = std::fs::read_dir(&self.folder) else {
            return Vec::new();
        };
        let mut found: Vec<(String, PathBuf)> = (entries.flatten())
            .map(|entry| entry.path())
            .filter_map(|place| {
                let manifest = Manifest::read(&place.join("plugin.json")).ok()?;
                (!self.is_known(&manifest.name)).then_some((manifest.name, place))
            })
            .collect();
        found.sort();
        found
    }
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
