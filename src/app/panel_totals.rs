//! A panel's totals kept between draws (review R32: FAR keeps counters, never a walk of every entry at
//! every draw): worked out again only after its entries or its selection changed.

use std::borrow::Borrow;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::ops::Deref;

use super::Panel;
use crate::vfs::{Entry, FolderSize};

/// The names selected on a panel; each change is counted, so that what is worked out of them is kept
/// until they change. Read as the set it is.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    names: BTreeSet<String>,
    version: u64,
}

impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        self.names == other.names
    }
}

impl Eq for Selection {}

impl Deref for Selection {
    type Target = BTreeSet<String>;

    fn deref(&self) -> &BTreeSet<String> {
        &self.names
    }
}

impl<'a> IntoIterator for &'a Selection {
    type Item = &'a String;
    type IntoIter = std::collections::btree_set::Iter<'a, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.names.iter()
    }
}

impl Selection {
    fn changed(&mut self, changed: bool) -> bool {
        if changed {
            self.version += 1;
        }
        changed
    }

    pub fn insert(&mut self, name: String) -> bool {
        let added = self.names.insert(name);
        self.changed(added)
    }

    pub fn remove<Name>(&mut self, name: &Name) -> bool
    where
        String: Borrow<Name>,
        Name: Ord + ?Sized,
    {
        let removed = self.names.remove(name);
        self.changed(removed)
    }

    pub fn clear(&mut self) {
        let any = !self.names.is_empty();
        self.names.clear();
        self.changed(any);
    }

    pub fn retain(&mut self, keep: impl FnMut(&String) -> bool) {
        let before = self.names.len();
        self.names.retain(keep);
        self.changed(self.names.len() != before);
    }

    pub fn extend(&mut self, names: impl IntoIterator<Item = String>) {
        let before = self.names.len();
        self.names.extend(names);
        self.changed(self.names.len() != before);
    }

    /// The selection becomes `names`.
    pub fn replace(&mut self, names: BTreeSet<String>) {
        let other = names != self.names;
        self.names = names;
        self.changed(other);
    }

    /// The names as a set of their own.
    pub fn to_set(&self) -> BTreeSet<String> {
        self.names.clone()
    }
}

/// Bytes, files and folders of some entries ("..": none).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub bytes: u64,
    pub files: usize,
    pub folders: usize,
}

impl Totals {
    /// `folder_sizes`: what F3 counted goes in (FAR adds it to the selection's size, not the panel's).
    fn of<'a>(entries: impl Iterator<Item = &'a Entry>, folder_sizes: bool) -> Self {
        let mut totals = Self::default();
        for entry in entries.filter(|entry| !entry.is_parent) {
            if entry.is_dir {
                totals.folders += 1;
                if folder_sizes {
                    totals.bytes += entry.size;
                }
            } else {
                totals.files += 1;
                totals.bytes += entry.size;
            }
        }
        totals
    }
}

/// The selection's totals, and whether a folder among it is still counted (a total that grows).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelectedTotals {
    pub totals: Totals,
    pub counting: bool,
}

/// What was worked out, with the changes it was worked out at.
#[derive(Debug, Default)]
pub(super) struct Kept {
    entries: Cell<Option<(u64, Totals)>>,
    selected: Cell<Option<((u64, u64), SelectedTotals)>>,
}

impl Panel {
    /// The entries changed (read, sorted, counted): what is kept of them is worked out again.
    pub(super) fn entries_changed(&mut self) {
        self.entries_version += 1;
    }

    /// The panel's totals (its bottom frame): files' bytes, files and folders.
    pub fn totals(&self) -> Totals {
        if let Some((version, totals)) = self.kept.entries.get()
            && version == self.entries_version
        {
            return totals;
        }
        let totals = Totals::of(self.entries.iter(), false);
        self.kept.entries.set(Some((self.entries_version, totals)));
        totals
    }

    /// The selection's totals, F3's folder sizes in.
    pub fn selected_totals(&self) -> SelectedTotals {
        let versions = (self.entries_version, self.selected.version);
        if let Some((kept, totals)) = self.kept.selected.get()
            && kept == versions
        {
            return totals;
        }
        let selected =
            || (self.entries.iter()).filter(|entry| self.selected.contains(entry.name.as_str()));
        let totals = SelectedTotals {
            totals: Totals::of(selected(), true),
            counting: selected().any(|entry| entry.folder_size == FolderSize::Counting),
        };
        self.kept.selected.set(Some((versions, totals)));
        totals
    }
}

#[cfg(test)]
#[path = "panel_totals_tests.rs"]
mod tests;
