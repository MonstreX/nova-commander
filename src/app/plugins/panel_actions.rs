//! What a plugin does to a panel (API 3, FAR's `FCTL_*`), its command line, the viewer and the editor. A
//! panel's action waits for its folder to be read, as the plugin's other requests do; a folder set or
//! read again is answered once read (FAR's calls are synchronous), but on the asking plugin's own panel
//! at once: its reading needs the plugin's thread, which waits for the answer, so it starts without the
//! screen waiting for it either (ADR-0019).

use std::time::Duration;

use super::super::{App, Screen, Side};
use super::panel_control::{PanelAsk, is_own};
use crate::plugin_host::panel_requests::{PanelAction, PanelAnswer};
use crate::vfs::VfsPath;

impl App {
    /// The action done; none while its panel's folder is read.
    pub(super) fn panel_do(
        &mut self,
        ask: &mut PanelAsk,
        action: PanelAction,
    ) -> Option<PanelAnswer> {
        let side = ask.side;
        if ask.started.is_some() {
            return self.finish_reading(ask);
        }
        let panels_own = !matches!(
            action,
            PanelAction::GetCommandLine
                | PanelAction::SetCommandLine(_)
                | PanelAction::InsertCommandLine(_)
                | PanelAction::RunCommand(_)
                | PanelAction::View { .. }
                | PanelAction::Edit { .. }
        );
        if panels_own && !self.may_answer(ask) {
            return None;
        }
        let done = |ok: bool| Some(PanelAnswer::Done(ok));
        match action {
            PanelAction::SetFolder { folder, cursor } => {
                let target = (self.panels[side.index()].path).resolve_given(&folder, &ask.plugin);
                let own = is_own(&target, &ask.plugin)
                    || is_own(&self.panels[side.index()].path, &ask.plugin);
                let started = self.read_panel(side, own, |panel| {
                    panel.change_dir(&target, cursor.as_deref()).is_ok()
                });
                if !started {
                    return done(false);
                }
                ask.started = Some(Some(target));
                self.finish_reading(ask)
            }
            PanelAction::Reread { keep_selection } => {
                let own = is_own(&self.panels[side.index()].path, &ask.plugin);
                self.read_panel(side, own, |panel| {
                    if !keep_selection {
                        panel.selected.clear();
                    }
                    panel.reload();
                    true
                });
                ask.started = Some(None);
                self.finish_reading(ask)
            }
            PanelAction::SetCursor { index, name, top } => {
                self.set_panel_cursor(side, index, name.as_deref(), top);
                done(true)
            }
            PanelAction::Select { names, on, all } => {
                self.select_in_panel(side, &names, on, all);
                done(true)
            }
            PanelAction::SetSort { sort, reverse } => {
                let Some(mode) = crate::plugin_host::sort_mode(sort) else {
                    return done(false);
                };
                self.panels[side.index()].set_sort_direction(mode, reverse);
                done(true)
            }
            PanelAction::Close { folder } => done(self.close_plugin_panel(side, folder.as_deref())),
            other => self.line_or_window(other),
        }
    }

    /// `read` started on the panel `side`; for the asking plugin's own panel (`own`) without the
    /// screen waiting for it, its title telling of the reading only after the moment a wait would take.
    /// False when it could not start.
    fn read_panel(
        &mut self,
        side: Side,
        own: bool,
        read: impl FnOnce(&mut super::super::Panel) -> bool,
    ) -> bool {
        let panel = &mut self.panels[side.index()];
        let wait = panel.read_wait();
        if own {
            panel.set_read_wait(Duration::ZERO);
        }
        let started = read(panel);
        panel.set_read_wait(wait);
        if own {
            panel.quiet_reading();
        }
        started
    }

    /// The command line's actions, the viewer's and the editor's (no panel's). The viewer and the editor
    /// open only over the panels with no other work waited for: a plugin's call is no reason to close
    /// what the user has open (FAR opens a window of its own; the program has one screen of each).
    fn line_or_window(&mut self, action: PanelAction) -> Option<PanelAnswer> {
        let done = |ok: bool| Some(PanelAnswer::Done(ok));
        let free = matches!(self.screen, Screen::Panels) && self.waiting.is_none();
        match action {
            PanelAction::GetCommandLine => {
                Some(PanelAnswer::Text(self.command_line.text().to_string()))
            }
            PanelAction::SetCommandLine(text) => {
                self.command_line.set(&text);
                done(true)
            }
            PanelAction::InsertCommandLine(text) => {
                self.command_line.insert_str(&text);
                done(true)
            }
            PanelAction::RunCommand(line) if free => done(self.run_command_for_plugin(&line)),
            PanelAction::View {
                path,
                delete_on_close,
                ..
            } if free => {
                let path = std::path::PathBuf::from(path);
                self.plugins
                    .delete_on_close
                    .extend(delete_on_close.then(|| path.clone()));
                self.view_file(&path, false);
                done(true)
            }
            PanelAction::Edit {
                path,
                line,
                column,
                delete_on_close,
            } if free => {
                let path = std::path::PathBuf::from(path);
                self.plugins
                    .delete_on_close
                    .extend(delete_on_close.then(|| path.clone()));
                self.edit_file_at(&path, false, (line, column));
                done(true)
            }
            // A window up, or a panel's own (done by `panel_do`).
            _ => done(false),
        }
    }

    /// A folder set or read again: done once read, there (the folder it was to show) or not. The asking
    /// plugin's own panel still read is done already: its reading waits for this answer.
    fn finish_reading(&self, ask: &PanelAsk) -> Option<PanelAnswer> {
        if !self.may_answer(ask) {
            return None;
        }
        let panel = &self.panels[ask.side.index()];
        let target = ask.started.as_ref().and_then(Option::as_ref);
        let there = panel.is_reading() || target.is_none_or(|target| panel.path.same_place(target));
        Some(PanelAnswer::Done(there))
    }

    /// FAR's `FCTL_REDRAWPANEL` with a `PanelRedrawInfo`: the cursor on the item (by name when given),
    /// the top where asked.
    fn set_panel_cursor(
        &mut self,
        side: Side,
        index: usize,
        name: Option<&str>,
        top: Option<usize>,
    ) {
        let panel = &mut self.panels[side.index()];
        match name {
            Some(name) => panel.go_to_name(name),
            None => panel.cursor = index.min(panel.entries.len().saturating_sub(1)),
        }
        if let Some(top) = top {
            panel.offset = top.min(panel.cursor);
        }
        panel.correct_position();
    }

    /// FAR's `FCTL_SETSELECTION` and `FCTL_ENDSELECTION`: the items named (`all`: every one but `..`)
    /// selected or not, sorted again when the selected go first.
    fn select_in_panel(&mut self, side: Side, names: &[String], on: bool, all: bool) {
        let panel = &mut self.panels[side.index()];
        // A whole panel's names may come (Advanced compare): looked up at once each.
        let named: std::collections::HashSet<&str> = names.iter().map(String::as_str).collect();
        let chosen: Vec<String> = (panel.entries.iter())
            .filter(|entry| !entry.is_parent)
            .filter(|entry| all || named.contains(entry.name.as_str()))
            .map(|entry| entry.name.clone())
            .collect();
        for name in chosen {
            if on {
                panel.selected.insert(name);
            } else {
                panel.selected.remove(&name);
            }
        }
        if panel.sort.selected_first {
            panel.set_sort_order(panel.sort);
        }
    }

    /// FAR's `FCTL_CLOSEPANEL`: a plugin's panel left for the disk's `folder` (relative: to the one under
    /// it), or the one it came from; a disk's panel has none to close (done, as FAR's).
    pub(super) fn close_plugin_panel(&mut self, side: Side, folder: Option<&str>) -> bool {
        let panel = &mut self.panels[side.index()];
        if panel.path.provider().is_none() {
            return true;
        }
        let Some(under) = panel.local_folder().map(VfsPath::local) else {
            return false;
        };
        let target = folder.map_or_else(|| under.clone(), |folder| under.resolve(folder));
        panel.change_dir(&target, None).is_ok()
    }

    /// A file the user saved in its editor is theirs: not deleted as its window closes (FAR clears
    /// `FFILEEDIT_DELETEONCLOSE` on a save, fileedit.cpp; R7 review C7).
    pub(in crate::app) fn keep_saved_file(&mut self, path: &std::path::Path) {
        (self.plugins.delete_on_close).retain(|file| file != path);
    }

    /// The files a plugin's viewer or editor were to delete as they closed: gone once the panels are back
    /// and nothing is being opened.
    pub(super) fn delete_closed_files(&mut self) {
        if !matches!(self.screen, Screen::Panels)
            || self.waiting.is_some()
            || self.plugins.delete_on_close.is_empty()
        {
            return;
        }
        // One a window behind still has open stays until it closes (R7 review C7: deleted under it).
        let (open, closed): (Vec<_>, Vec<_>) = std::mem::take(&mut self.plugins.delete_on_close)
            .into_iter()
            .partition(|file| self.window_open_on(file));
        self.plugins.delete_on_close = open;
        for file in closed {
            match std::fs::remove_file(&file) {
                // A file gone already is what was wanted.
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    self.show_error("Delete", &error);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "panel_actions_tests.rs"]
mod tests;
