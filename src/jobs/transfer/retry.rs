//! "Retry skipped" (owner's, from TeraCopy): only what an error left out, each to the place it was going
//! to, with the operation's settings.

use super::engine::Run;
use super::{Skipped, VfsPath, sides};

impl Run<'_> {
    pub(super) fn retry_skipped(&mut self) {
        let request = self.request;
        for item in &request.retrying {
            self.top = Some(item.top.clone());
            self.inside_links = u32::from(item.inside_link);
            if self.selected(&item.source, &item.destination).is_err() {
                return;
            }
            self.remove_emptied(item);
        }
    }

    /// A move: the folders the object was in went away with the rest of it, except the ones it kept.
    fn remove_emptied(&mut self, item: &Skipped) {
        if !self.moves() || item.source == item.top || sides::stat(&item.source).is_ok() {
            return;
        }
        let mut folder = item.source.parent();
        while let Some(current) = folder {
            let stop = self.host.stop_flag();
            let empty = sides::names(&current, &stop).is_ok_and(|names| names.is_empty());
            if !empty || sides::remove_empty_dir(&current).is_err() || current == item.top {
                return;
            }
            folder = current.parent();
        }
    }

    /// The object at hand was left out after an error.
    pub(super) fn remember_skipped(&mut self) {
        let (Some((source, destination)), Some(top)) = (&self.current, &self.top) else {
            return;
        };
        let item = Skipped {
            source: source.clone(),
            destination: destination.clone(),
            top: top.clone(),
            inside_link: self.inside_links > 0,
        };
        if self.report.skipped.last() != Some(&item) {
            self.report.skipped.push(item);
        }
    }

    /// `item` with the object at hand known for what goes wrong in it; the outer one again afterwards.
    pub(super) fn at_object<T>(
        &mut self,
        source: &VfsPath,
        destination: &VfsPath,
        walk: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let outer = self.current.replace((source.clone(), destination.clone()));
        let done = walk(self);
        self.current = outer;
        done
    }
}
