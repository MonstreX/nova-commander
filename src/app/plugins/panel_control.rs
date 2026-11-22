//! A plugin's requests of the panels (API 3, FAR's `PanelControl`): either panel's state and items, and
//! what is done to it (`panel_actions`). The plugin's thread waits for the answer, the screen does not
//! (ADR-0019). As FAR's calls find the panel read, a request for a panel whose folder is being read is
//! answered once it is read - but at once for the asking plugin's own panel, or one being read into it
//! (that reading needs the plugin's thread, which waits here). In a panel's own call (a folder, a file,
//! a title) the plugin reads the panels as the last tick saw them, without the program
//! (`publish_panels`): the screen may be waiting for that call.

use std::sync::mpsc::Sender;

use super::super::{App, Side};
use crate::plugin_host::panel_requests::{
    PanelAnswer, PanelItem, PanelKind, PanelRequest, PanelState, Which,
};
use crate::vfs::{Entry, VfsPath};

/// A request not answered yet.
#[derive(Debug)]
pub(in crate::app) struct PanelAsk {
    pub(super) plugin: String,
    pub(super) request: PanelRequest,
    /// Its panel, as it was when asked: a Tab meanwhile does not change it.
    pub(super) side: Side,
    reply: Sender<PanelAnswer>,
    /// A folder set or read again: done, its reading waited for; with the folder it is to show.
    pub(super) started: Option<Option<VfsPath>>,
}

#[cfg(all(test, windows))]
impl PanelAsk {
    /// A request of `plugin` about the left panel, as it comes.
    pub(super) fn for_test(
        plugin: &str,
        request: PanelRequest,
        reply: Sender<PanelAnswer>,
    ) -> Self {
        Self {
            plugin: plugin.to_string(),
            request,
            side: Side::Left,
            reply,
            started: None,
        }
    }
}

impl App {
    /// `Event::PanelAsk`: answered now, or kept until its panel is read.
    pub(super) fn plugin_panel_ask(
        &mut self,
        plugin: String,
        request: PanelRequest,
        reply: Sender<PanelAnswer>,
    ) {
        let which = match &request {
            PanelRequest::State(which)
            | PanelRequest::Items { which, .. }
            | PanelRequest::Do(which, _)
            | PanelRequest::Filter { which, .. } => *which,
            _ => Which::Active,
        };
        let ask = PanelAsk {
            side: self.side_of(which),
            plugin,
            request,
            reply,
            started: None,
        };
        self.plugins.panel_asks.push(ask);
        self.answer_panel_asks();
    }

    /// The requests that can be answered now answered, in their order; true when a panel changed.
    pub(super) fn answer_panel_asks(&mut self) -> bool {
        let mut changed = false;
        let mut kept = Vec::new();
        self.plugins.gone.clear();
        for mut ask in std::mem::take(&mut self.plugins.panel_asks) {
            // One let go meanwhile (P6.0e) is not answered: dropped, its waiting call hears nothing.
            if self.let_go_meanwhile(&ask.plugin) {
                continue;
            }
            match self.answer_panel_ask(&mut ask) {
                Some(answer) => {
                    changed = true;
                    // A plugin gone meanwhile hears nothing.
                    let _ = ask.reply.send(answer);
                }
                None => kept.push(ask),
            }
        }
        // Asked while answering (none can be: the plugin waits), kept after those already waiting.
        kept.append(&mut self.plugins.panel_asks);
        kept.retain(|ask| !self.let_go_meanwhile(&ask.plugin));
        self.plugins.gone.clear();
        self.plugins.panel_asks = kept;
        changed
    }

    /// Whether `plugin` was let go while the requests are answered.
    fn let_go_meanwhile(&self, plugin: &str) -> bool {
        (self.plugins.gone.iter()).any(|gone| gone.eq_ignore_ascii_case(plugin))
    }

    /// The answer, or none while the panel's folder is read.
    fn answer_panel_ask(&mut self, ask: &mut PanelAsk) -> Option<PanelAnswer> {
        let side = ask.side;
        match &ask.request {
            PanelRequest::State(_) => (self.may_answer(ask))
                .then(|| PanelAnswer::State(Box::new(self.panel_state(side, &ask.plugin)))),
            &PanelRequest::Items { selected, .. } => {
                (self.may_answer(ask)).then(|| PanelAnswer::Items(self.panel_items(side, selected)))
            }
            PanelRequest::Do(_, action) => {
                let action = action.clone();
                self.panel_do(ask, action)
            }
            _ => self.program_answer(ask),
        }
    }

    /// Whether the asking plugin's panel is read, or need not be waited for: it is the plugin's own, or
    /// being read into it.
    pub(super) fn may_answer(&self, ask: &PanelAsk) -> bool {
        let panel = &self.panels[ask.side.index()];
        !panel.is_reading()
            || is_own(&panel.path, &ask.plugin)
            || panel
                .reading_target()
                .is_some_and(|target| is_own(target, &ask.plugin))
    }

    /// The panels as a plugin's panel call reads them (`PanelsSeen`): kept as they are now.
    pub(super) fn publish_panels(&self) {
        // No call's panel there: a panel's call is for its own panel, which the plugin knows.
        let seen = [Side::Left, Side::Right].map(|side| PanelState {
            call_target: false,
            ..self.panel_state(side, "")
        });
        let mut kept = self
            .plugins
            .host
            .panels_seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *kept != seen {
            *kept = seen;
        }
    }

    pub(super) fn side_of(&self, which: Which) -> Side {
        match which {
            Which::Active => self.active,
            Which::Passive => self.active.other(),
            Which::Left => Side::Left,
            Which::Right => Side::Right,
        }
    }

    /// FAR's `PanelInfo` and `FCTL_GETPANELDIRECTORY` of the panel `side`.
    pub(super) fn panel_state(&self, side: Side, plugin: &str) -> PanelState {
        let panel = &self.panels[side.index()];
        let kind = if self.info_side() == Some(side) {
            PanelKind::Info
        } else if self.tree_side() == Some(side) {
            PanelKind::Tree
        } else if self.quick_view_side() == Some(side) {
            PanelKind::QuickView
        } else {
            PanelKind::Files
        };
        let shows_files = kind == PanelKind::Files && panel.visible;
        // A panel showing no files has no columns (the ABI's word).
        let (columns, widths) = if shows_files {
            let (columns, widths) = panel.drawn_columns();
            (columns.to_string(), widths.to_string())
        } else {
            (String::new(), String::new())
        };
        let provider = panel.path.provider().map(str::to_string);
        let folder = match &panel.path {
            VfsPath::Local(path) => path.display().to_string(),
            mounted => {
                (mounted.session_path()).map_or_else(String::new, |(_, path)| path.to_string())
            }
        };
        PanelState {
            left: side == Side::Left,
            active: self.active == side,
            visible: panel.visible,
            real_names: panel.has_real_names(),
            plugin: provider,
            show_hidden: self.show_hidden,
            reverse_sort: panel.sort.reverse,
            selected_first: panel.sort.selected_first,
            call_target: self.call_side(plugin) == side,
            reading: panel.is_reading(),
            kind,
            folder,
            disk_folder: (panel.local_folder())
                .map_or_else(String::new, |dir| dir.display().to_string()),
            items: panel.entries.len(),
            selected: self.selected_count(side),
            current: panel.cursor,
            top: panel.offset,
            sort: crate::plugin_host::sort_code(panel.sort.mode),
            view_mode: (panel.view_mode.number())
                .map_or(0, |mode| u32::try_from(mode).unwrap_or(0)),
            columns,
            widths,
        }
    }

    /// FAR's `SelectedItemsNumber`: the selection, or the current item when none is and it is no `..`.
    fn selected_count(&self, side: Side) -> usize {
        let panel = &self.panels[side.index()];
        match panel.selected.len() {
            0 => usize::from(panel.current().is_some_and(|entry| !entry.is_parent)),
            count => count,
        }
    }

    /// The panel's items (FAR's `FCTL_GETPANELITEM`), or (`selected`) FAR's `FCTL_GETSELECTEDPANELITEM`:
    /// the selected ones in the panel's order, or the current one when none is and it is no `..`.
    fn panel_items(&self, side: Side, selected: bool) -> Vec<PanelItem> {
        let panel = &self.panels[side.index()];
        let item = |entry: &Entry| PanelItem {
            name: entry.name.clone(),
            selected: panel.selected.contains(&entry.name),
            folder: entry.is_dir,
            parent: entry.is_parent,
            attributes: entry.attributes.windows_flags(),
            size: entry.size,
            allocated: entry.allocated.unwrap_or(0),
            modified: entry.modified,
            created: entry.created,
            accessed: entry.accessed,
            changed: entry.changed,
            links: 0,
            path: (panel.item_path(entry).as_local()).map(|path| path.display().to_string()),
            description: (entry.texts.as_ref())
                .map_or_else(String::new, |texts| texts.description.clone()),
            owner: String::new(),
        };
        if !selected {
            return panel.entries.iter().map(item).collect();
        }
        if panel.selected.is_empty() {
            return (panel.current().filter(|entry| !entry.is_parent))
                .map(item)
                .into_iter()
                .collect();
        }
        (panel.entries.iter())
            .filter(|entry| panel.selected.contains(&entry.name))
            .map(item)
            .collect()
    }
}

/// Whether the panel shows the plugin's own panel.
pub(super) fn is_own(path: &VfsPath, plugin: &str) -> bool {
    path.provider()
        .is_some_and(|provider| provider.eq_ignore_ascii_case(plugin))
}

#[cfg(test)]
#[path = "panel_control_tests.rs"]
mod tests;
