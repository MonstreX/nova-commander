//! Searches in the background (2b step 7c, the owner's; FAR has none): Background in the results dialog
//! sends a search that goes on behind the panels. Each is a line of the status row (`Find 37` and its
//! moving block, `Find 37 done`) and of the Screens list (F12) with its masks and text; Enter there
//! brings its results dialog back with all it found, Del takes away one that is over. A search that ends
//! in the background flashes the taskbar button as a task does, and stays until its results are seen.

use super::super::{App, FindResults, Overlay};

/// What the status row and the Screens list say of a search in the background.
pub fn search_label(results: &FindResults) -> String {
    let count = results.found_count();
    if results.finalized {
        format!("Find {count} done")
    } else {
        format!("Find {count}")
    }
}

impl App {
    /// The searches in the background, in the order they went there.
    pub fn background_searches(&self) -> &[FindResults] {
        &self.find.background
    }

    /// The search goes back to its place in the list (the end for a new one); returns that place.
    pub(in crate::app) fn send_find_to_background(
        &mut self,
        mut results: Box<FindResults>,
    ) -> usize {
        let searches = &mut self.find.background;
        let place = results
            .place
            .take()
            .map_or(searches.len(), |place| place.min(searches.len()));
        searches.insert(place, *results);
        place
    }

    /// The searches in the background that still go on.
    pub(in crate::app) fn finds_running(&self) -> usize {
        (self.find.background.iter())
            .filter(|results| !results.finalized)
            .count()
    }

    /// What the searches in the background found since the last tick; whether the screen is to be drawn
    /// again (always while one goes on: its block moves).
    pub(in crate::app) fn pump_background_finds(&mut self) -> bool {
        let mut changed = false;
        for results in &mut self.find.background {
            let going = !results.finalized;
            changed |= results.pump();
            if going && results.finalized {
                self.work.task_ended_signal = true;
            }
        }
        changed || self.finds_running() > 0
    }

    /// Enter in the Screens list: the search's results dialog in front again.
    pub(in crate::app) fn bring_back_find(&mut self, index: usize) {
        if index < self.find.background.len() {
            let mut results = self.find.background.remove(index);
            results.place = Some(index);
            self.overlay = Some(Overlay::FindResults(Box::new(results)));
        }
    }

    /// Del in the Screens list: a search that is over leaves, one that goes on stays.
    pub(in crate::app) fn forget_find(&mut self, index: usize) {
        if self
            .find
            .background
            .get(index)
            .is_some_and(|results| results.finalized)
        {
            self.find.background.remove(index);
        }
    }
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
