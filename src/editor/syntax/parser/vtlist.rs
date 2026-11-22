//! The virtual substitutions in force (`VTList`): each inherit being searched that has `<virtual>`
//! entries; a scheme that is a virtual target is replaced by the substitute of the innermost such entry,
//! and the entries above it are hidden while the substitute is searched.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use super::super::hrc::{InheritNode, Library, Node, NodeRef, SchemeId};

#[derive(Clone, Copy, Debug)]
struct Entry {
    inherit: NodeRef,
    shadow_last: Option<usize>,
}

#[derive(Debug, Default)]
pub(super) struct VtList {
    entries: Vec<Entry>,
    /// How many entries are in force (`last_index + 1`).
    live: usize,
}

impl VtList {
    /// An inherit with virtual entries put in force (`push`); false for one without.
    pub(super) fn push(&mut self, node: NodeRef, inherit: &InheritNode) -> bool {
        if inherit.virtuals.is_empty() {
            return false;
        }
        self.entries.insert(
            self.live,
            Entry {
                inherit: node,
                shadow_last: None,
            },
        );
        self.live += 1;
        true
    }

    pub(super) fn pop(&mut self) {
        if self.live > 0 {
            self.live -= 1;
            self.entries.remove(self.live);
        }
    }

    /// The scheme's substitute (`pushvirt`): the entries from the one that gave it hidden.
    pub(super) fn push_virtual(&mut self, lib: &Library, scheme: SchemeId) -> Option<SchemeId> {
        let mut result = scheme;
        let mut from = None;
        for i in (0..self.live).rev() {
            let Node::Inherit(inherit) = lib.node(self.entries[i].inherit) else {
                continue;
            };
            for entry in &inherit.virtuals {
                if entry.virt == Some(result)
                    && let Some(subst) = entry.subst
                {
                    result = subst;
                    from = Some(i);
                }
            }
        }
        let from = from?;
        self.entries[from].shadow_last = Some(self.live);
        self.live = from;
        Some(result)
    }

    /// The entries hidden by the last substitute back in force (`popvirt`).
    pub(super) fn pop_virtual(&mut self) {
        let that = self.live;
        if let Some(entry) = self.entries.get_mut(that) {
            self.live = entry.shadow_last.take().unwrap_or(self.live);
        }
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.live = 0;
    }

    /// The inherits in force (`store`).
    pub(super) fn store(&self) -> Vec<NodeRef> {
        self.entries[..self.live]
            .iter()
            .map(|entry| entry.inherit)
            .collect()
    }

    /// The inherits a block opened with back in force (`restore`), when none are.
    pub(super) fn restore(&mut self, inherits: &[NodeRef]) {
        if self.live > 0 || inherits.is_empty() {
            return;
        }
        self.entries = inherits
            .iter()
            .map(|&inherit| Entry {
                inherit,
                shadow_last: None,
            })
            .collect();
        self.live = self.entries.len();
    }
}
