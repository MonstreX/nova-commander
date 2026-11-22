//! A plugin's panels in the program's panels (ADR-0016 §13): one opened where its item was chosen (the
//! panel of the drive menu, the active one for F11), its plugin told when no panel shows it any more -
//! however the panel left it (".." at its root, another drive, a folder typed, the history) - or when it
//! never came to be shown (its root refused, its reading stopped or overtaken by another folder).

use std::path::PathBuf;

use super::super::{App, Side};
use crate::vfs::VfsPath;

/// Where an edited copy not put back is kept (the extracted copies' words).
fn kept_line(copy: &std::path::Path) -> String {
    format!("The edited copy is kept: {}", copy.display())
}

/// A put sent to a plugin and not ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct Putting {
    pub plugin: String,
    /// The panel whose selection goes when it is done.
    pub selection: Option<Side>,
    /// F4's edited copy put back (FAR's `PutFiles` with `OPM_EDIT`): the program's folder of it and the
    /// copy, which goes once taken and stays (said where) when not.
    pub edited: Option<(PathBuf, PathBuf)>,
    /// A copy put back at a save in the editor (API 8): the copy and its time and length then; it and its
    /// folder stay, the editor having it open.
    pub saved: Option<(PathBuf, Made)>,
}

/// A copy's time and length.
pub(in crate::app) type Made = (Option<std::time::SystemTime>, u64);

impl App {
    /// The plugin's panel `path` opened in the panel `target` (0 the left, 1 the right).
    pub(super) fn open_plugin_panel(&mut self, plugin: &str, target: usize, path: &VfsPath) {
        let side = if target == Side::Right.index() {
            Side::Right
        } else {
            Side::Left
        };
        let title = self.plugin_title(plugin);
        self.plugins.opening.push((path.clone(), side));
        self.enter_drive_folder(side, path, &title);
    }

    /// The command line's folder (FAR's `ShowFileList`: `SetCurDir` of a plugin's panel is its
    /// `OpenPanelInfo.CurDir`): a plugin's panel's folder inside it (TmpPanel's at its root: none), else
    /// the panel's.
    pub fn prompt_folder(&self) -> String {
        let path = &self.command_panel().path;
        if !self.plugins.host.is_plugin_panel(path) {
            return path.to_string();
        }
        (path.session_path())
            .map(|(_, inner)| {
                (inner.trim_start_matches('/')).replace('/', std::path::MAIN_SEPARATOR_STR)
            })
            .unwrap_or_default()
    }

    /// Each panel sorted by what its view shows (FAR's `IgnorePaths`): names only while its first name
    /// column shows them (a provider's whole paths, a user's mode with `O`).
    pub(in crate::app) fn sync_sort_names(&mut self) {
        for panel in &mut self.panels {
            let names_only = (self.panel_modes)
                .spec_in(panel.view_mode, &panel.look)
                .names_only();
            panel.sort_by_names_only(names_only);
        }
    }

    /// The copy dialog's target when the passive panel shows a plugin's panel (FAR's `ToPlugin`: shown,
    /// a file panel; `copy.cpp` offers `<Format>:`, `::` without one).
    pub(in crate::app) fn plugin_target(
        &self,
    ) -> Option<super::super::transfer_dialog::PluginTarget> {
        let passive = self.active.other();
        let panel = &self.panels[passive.index()];
        let shown = panel.visible && self.shows_files(passive);
        (shown && self.plugins.host.is_plugin_panel(&panel.path)).then(|| {
            super::super::transfer_dialog::PluginTarget {
                text: (panel.look.format.as_ref())
                    .map_or_else(|| "::".to_string(), |f| format!("{f}:")),
                dir: panel.path.clone(),
                takes_paths: panel.look.takes_paths,
            }
        })
    }

    /// F5 / F6's target left as the plugin's format into a panel that takes paths (FAR's `PutFiles`,
    /// TmpPanel's "add", ADR-0016 §14): its plugin gets the sources' paths on their disk; the active
    /// panel's selection goes once the plugin took them. Sources on no disk are left out (none: said so).
    pub(in crate::app) fn put_into(&mut self, target: &VfsPath, sources: &[VfsPath], moving: bool) {
        let paths: Vec<PathBuf> = (sources.iter())
            .filter_map(|source| source.as_local().map(PathBuf::from))
            .collect();
        let target = target.clone();
        let plugin = target.provider().unwrap_or_default().to_string();
        if paths.is_empty() {
            let title = self.plugin_title(&plugin);
            let lines = vec!["Only files and folders on a disk can be put here".to_string()];
            return self.show_dialog(super::super::Dialog::error(&title, lines));
        }
        let selection = (!self.tree_has_keys()).then_some(self.active);
        self.plugins.putting = Some(Putting {
            plugin,
            selection,
            edited: None,
            saved: None,
        });
        // FAR's `SrcPath`: the folder of the panel they come from.
        let from = (self.active_panel().local_folder()).map(std::path::Path::to_path_buf);
        (self.plugins.host).put(&target, paths, (moving, 0), from.as_deref());
    }

    /// F4's edited `copy` (in the program's `folder`) put back into the plugin's panel `target` that
    /// takes paths (FAR's `PutFiles` with `OPM_EDIT`): the plugin takes it as it does (arclite with its
    /// dialog); the folder goes once it is done.
    pub(in crate::app) fn put_edited_back(
        &mut self,
        target: &VfsPath,
        folder: PathBuf,
        copy: PathBuf,
    ) {
        let plugin = target.provider().unwrap_or_default().to_string();
        if self.plugins.putting.is_some() {
            // One put waited for at a time: its end must not take this copy's folder for its own.
            let title = self.plugin_title(&plugin);
            let lines = vec![kept_line(&copy)];
            return self.show_dialog(super::super::Dialog::error(&title, lines));
        }
        self.plugins.putting = Some(Putting {
            plugin,
            selection: None,
            edited: Some((folder.clone(), copy.clone())),
            saved: None,
        });
        let edit = (false, crate::plugin_host::getting::GET_EDIT);
        (self.plugins.host).put(target, vec![copy], edit, Some(&folder));
    }

    /// The editor's saved `copy` (in the program's `folder`, `made` its time and length now) put back
    /// into the plugin's panel `target` while the editor stays (API 8's `PANEL_PUTS_ON_SAVE`). With
    /// another put not ended it waits: leaving the editor puts it back.
    pub(in crate::app) fn put_saved_back(
        &mut self,
        target: &VfsPath,
        folder: PathBuf,
        copy: PathBuf,
        made: Made,
    ) {
        if self.plugins.putting.is_some() {
            return;
        }
        self.plugins.putting = Some(Putting {
            plugin: target.provider().unwrap_or_default().to_string(),
            selection: None,
            edited: None,
            saved: Some((copy.clone(), made)),
        });
        let edit = (false, crate::plugin_host::getting::GET_EDIT);
        (self.plugins.host).put(target, vec![copy], edit, Some(&folder));
    }

    /// The editor's saved `copy` is being put back (its put not ended).
    pub(in crate::app) fn saving_put(&self, copy: &std::path::Path) -> bool {
        (self.plugins.putting.as_ref())
            .and_then(|putting| putting.saved.as_ref())
            .is_some_and(|(saved, _)| saved == copy)
    }

    /// A put into a panel of `plugin` ended: the panels showing one of its panels are read again; when
    /// it was done the selection it took goes (FAR's, after `PutFiles`) and an edited copy's folder goes,
    /// when not the copy is kept and said where.
    pub(in crate::app) fn put_ended(&mut self, plugin: &str, done: bool) {
        let ours = |putting: &mut Putting| putting.plugin.eq_ignore_ascii_case(plugin);
        if let Some(putting) = self.plugins.putting.take_if(ours) {
            if let (Some(side), true) = (putting.selection, done) {
                let panel = &mut self.panels[side.index()];
                panel.selected.clear();
                panel.selection_changed();
            }
            match putting.edited {
                Some((folder, _)) if done => {
                    let _ = std::fs::remove_dir_all(folder);
                }
                Some((_, copy)) => {
                    let title = self.plugin_title(plugin);
                    let lines = vec![kept_line(&copy)];
                    self.show_dialog(super::super::Dialog::error(&title, lines));
                }
                None => {}
            }
            if let (Some((copy, made)), true) = (putting.saved, done) {
                self.saved_copy_taken(&copy, made);
            }
        }
        self.reload_plugin_panels(plugin);
    }

    /// A put of `plugin` failed: it takes no selection; the line telling where an edited copy is kept.
    pub(super) fn put_failed(&mut self, plugin: &str) -> Option<String> {
        let ours = |putting: &mut Putting| putting.plugin.eq_ignore_ascii_case(plugin);
        let putting = self.plugins.putting.take_if(ours)?;
        putting.edited.map(|(_, copy)| kept_line(&copy))
    }

    /// Every panel showing a plugin's panel read again (FAR's `ConfigureCurrent`).
    pub(super) fn reload_all_plugin_panels(&mut self) {
        for side in 0..self.panels.len() {
            if self.plugins.host.is_plugin_panel(&self.panels[side].path) {
                self.panels[side].reload();
            }
        }
    }

    /// The panels showing a panel of plugin `plugin` read again.
    pub(super) fn reload_plugin_panels(&mut self, plugin: &str) {
        for panel in &mut self.panels {
            if panel
                .path
                .provider()
                .is_some_and(|id| id.eq_ignore_ascii_case(plugin))
            {
                panel.reload();
            }
        }
    }

    /// The plugins' panels shown now; those no panel shows any more, and those opened whose panel's
    /// reading ended elsewhere, are closed for their plugins.
    pub(super) fn close_left_plugin_panels(&mut self) {
        let mut shown: Vec<VfsPath> = Vec::new();
        for panel in &self.panels {
            // A plugin's panel under another stays open for when ".." comes back to it (FAR's stack).
            let under = (0..).map_while(|at| panel.under(at));
            for path in std::iter::once(&panel.path).chain(under) {
                if self.plugins.host.is_plugin_panel(path)
                    && !shown.iter().any(|known| known.same_mount(path))
                {
                    shown.push(path.clone());
                }
            }
        }
        let shows = |path: &VfsPath| shown.iter().any(|known| known.same_mount(path));
        let mut gone: Vec<VfsPath> = Vec::new();
        for old in std::mem::take(&mut self.plugins.panels) {
            if !shows(&old) {
                gone.push(old);
            }
        }
        let mut still_opening = Vec::new();
        for (path, side) in std::mem::take(&mut self.plugins.opening) {
            let panel = &self.panels[side.index()];
            if panel.path.same_mount(&path) {
                continue;
            }
            if panel.is_reading() {
                still_opening.push((path, side));
            } else if !shows(&path) && !gone.iter().any(|known| known.same_mount(&path)) {
                gone.push(path);
            }
        }
        for path in &gone {
            self.plugins.host.close_panel(path);
        }
        self.plugins.opening = still_opening;
        self.plugins.panels = shown;
    }
}

#[cfg(test)]
#[path = "panels_tests.rs"]
mod tests;
