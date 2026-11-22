//! Alt+F7's "Panel" (FAR's TmpPanel `SetFindList`, `TmpClass.cpp:264-315`): the found files and folders put
//! on a temporary panel opened for them, its list emptied first whatever Replace says. With the common panel
//! and "New panel for search results" that panel is the first empty common one, or else the next in turn
//! (`FindSearchResultsPanel`, `Opt.LastSearchResultsPanel`), and it becomes the current one; without the
//! option the current one; without the common panel a panel of its own.

use nc_plugin_sdk::{Error, FileEntry, KeyAnswer, KeyGo};

use crate::{ITEM, Options, PANELS, TmpPanel};

/// FAR's `FindSearchResultsPanel`: the first empty of the common panels, or else `last` in turn (and
/// `last` moved on).
pub fn results_panel(empty: &[bool], last: &mut usize) -> usize {
    if let Some(free) = empty.iter().position(|empty| *empty) {
        return free;
    }
    let panel = *last % PANELS;
    *last = (panel + 1) % PANELS;
    panel
}

impl TmpPanel {
    /// FAR's `OpenW` with `OPEN_FINDLIST` and `SetFindList`: the `found` objects on a panel opened for
    /// them.
    pub(crate) fn take_find_list(&mut self, found: &[FileEntry]) -> Result<KeyAnswer, Error> {
        self.options = Options::read(&self.host);
        // `BeginPutFiles`: its message while they go in.
        self.host.busy(ITEM, &["Sending files to temporary panel"]);
        if self.options.common_panel && self.options.new_panel_for_search_results {
            let empty: Vec<bool> = self
                .panels
                .iter()
                .map(|list| list.paths().is_empty())
                .collect();
            self.current = results_panel(&empty, &mut self.last_search_results);
        }
        let instance = self.new_instance();
        let list = self.list(&instance)?;
        // In place of what it held, as given, each once (`RemoveDups`), and shown at first as the search saw them (FAR's
        // `m_UpdateNotNeeded`: no looking at the disks then; the next reading of the panel looks).
        list.add_found(found);
        Ok(KeyAnswer::go(KeyGo::Open(instance)))
    }
}

#[cfg(test)]
#[path = "find_list_tests.rs"]
mod tests;
