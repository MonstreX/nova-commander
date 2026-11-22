//! "Copy contents of symbolic links" (ADR-0009 §10): a link is walked as what it leads to, the copy is
//! a file or a folder of its own. A link back to a folder being copied is reported and left out. As in
//! FAR, a move takes only the link away: what it leads to stays where it is
//! (`ScanTree::InsideReparsePoint`).

use super::engine::{Flow, Run};
use super::sides;
use crate::vfs::{Entry, VfsPath};

impl Run<'_> {
    /// Whether the link at hand is to be walked: only a disk holds links to follow.
    pub(super) fn follows(&self, source: &VfsPath, entry: &Entry) -> bool {
        self.request.keep.link_contents && entry.points_elsewhere() && sides::disk(source).is_some()
    }

    /// Whether what is at hand may be moved: never what lies behind a link.
    pub(super) fn moves(&self) -> bool {
        self.request.move_ && self.inside_links == 0
    }

    pub(super) fn link_contents(&mut self, source: &VfsPath, destination: &VfsPath) -> Flow {
        let Some(leads_to) = self.attempt(source, |_| sides::followed(source))? else {
            return Ok(());
        };
        if leads_to.is_dir && !self.may_walk(source, destination) {
            return Ok(());
        }
        let before = std::mem::take(&mut self.incomplete);
        self.inside_links += 1;
        let walked = if leads_to.is_dir {
            self.dir(source, destination, &leads_to)
        } else {
            self.file(source, destination, &leads_to)
        };
        self.inside_links -= 1;
        walked?;
        let complete = !self.incomplete;
        self.incomplete |= before;
        if complete && self.moves() {
            self.attempt(source, |run| run.remove_link(source))?;
        }
        Ok(())
    }

    /// A link back to a folder on the way here would go on for ever, one into the folder being written
    /// to would copy the copy.
    fn may_walk(&mut self, source: &VfsPath, destination: &VfsPath) -> bool {
        if sides::identity(source).is_some_and(|place| self.walking.contains(&place)) {
            self.fail(source, "leads back to a folder being copied; left out");
            return false;
        }
        if sides::leads_inside(source, destination) {
            self.fail(source, "a folder cannot be copied or moved into itself");
            return false;
        }
        true
    }
}
