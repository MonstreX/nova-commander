//! The links put right once the files are read (`HrcLibraryImpl::updateLinks`): each block's and
//! inherit's scheme found by its name (loading the types it names), the virtual substitutions marked,
//! each scheme's nodes to look through flattened (`buildSearchNodes`, `mergeSearchKeywords`), and the
//! nodes that may start with each ASCII character (`SearchDispatch`).
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use std::collections::HashMap;

use super::super::regex::to_lower as chars_to_lower;
use super::names::Kind;
use super::{Dispatch, KeywordList, KeywordsNode, Library, Node, NodeRef, SchemeId};

/// Where a scheme's search list is (`kSearchUnbuilt`, `kSearchBuilding`, `kSearchDone`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Built {
    Building,
    Done,
}

impl Library {
    /// `updateLinks`.
    pub(super) fn update_links(&mut self) {
        let mut changed = vec![false; self.schemes.len()];
        while self.structure_changed {
            self.structure_changed = false;
            for id in 0..self.schemes.len() {
                let type_id = self.schemes[id].file_type;
                if !self.types[type_id].load_done {
                    continue;
                }
                let outer = self.current_type.replace(type_id);
                let linked = self.link_scheme(id);
                self.current_type = outer;
                changed.resize(self.schemes.len(), false);
                for id in linked {
                    changed[id] = true;
                }
                if self.structure_changed {
                    break;
                }
            }
        }
        let dirty = self.dirty_schemes(changed);
        // The schemes not changed keep their lists: the recursion takes them as they are.
        let mut built: HashMap<SchemeId, Built> = (0..self.schemes.len())
            .filter(|&id| !dirty[id])
            .map(|id| (id, Built::Done))
            .collect();
        for (id, _) in dirty.iter().enumerate().filter(|(_, dirty)| **dirty) {
            self.build_search(id, &mut built);
        }
        self.build_dispatch(&dirty);
        self.linked = self.schemes.len();
    }

    /// The schemes whose search lists and dispatch are built anew: those new since the last update,
    /// those whose links were put right now (`changed`), and those that inherit any of them.
    fn dirty_schemes(&self, mut dirty: Vec<bool>) -> Vec<bool> {
        let count = self.schemes.len();
        dirty.resize(count, false);
        for flag in dirty.iter_mut().skip(self.linked) {
            *flag = true;
        }
        let targets = |id: SchemeId| {
            (self.schemes[id].nodes.iter()).filter_map(|&at| match &self.nodes[at] {
                Node::Inherit(inherit) => Some(inherit),
                _ => None,
            })
        };
        loop {
            let mut grew = false;
            for id in 0..count {
                if dirty[id] {
                    continue;
                }
                let reaches = targets(id).any(|inherit| {
                    let ends = inherit
                        .virtuals
                        .iter()
                        .flat_map(|entry| [entry.virt, entry.subst]);
                    std::iter::once(inherit.scheme)
                        .chain(ends)
                        .flatten()
                        .any(|target| dirty[target])
                });
                if reaches {
                    dirty[id] = true;
                    grew = true;
                }
            }
            if !grew {
                return dirty;
            }
        }
    }

    /// A scheme name of a node looked up once (`updateSchemeLink`); whether it was looked up now.
    fn resolve(&mut self, name: &mut Option<String>, target: &mut Option<SchemeId>) -> bool {
        let Some(wanted) = name.take() else {
            return false;
        };
        if target.is_some() {
            return true;
        }
        *target = self
            .qualify_foreign_name(&wanted, Kind::Scheme, true)
            .and_then(|qualified| self.scheme_names.get(&qualified).copied());
        if target.is_none() {
            self.log
                .push(format!("cannot resolve scheme name '{wanted}'"));
        }
        true
    }

    /// The scheme's names looked up; the schemes this changed: itself when a name of it was looked up
    /// now, and the virtual targets it names (their substitutions grew).
    fn link_scheme(&mut self, id: SchemeId) -> Vec<SchemeId> {
        let mut changed = Vec::new();
        for at in self.schemes[id].nodes.clone() {
            let mut node =
                std::mem::replace(&mut self.nodes[at], Node::Inherit(Default::default()));
            let mut looked = false;
            match &mut node {
                Node::Block(block) => {
                    looked |= self.resolve(&mut block.scheme_name, &mut block.scheme);
                }
                Node::Inherit(inherit) => {
                    looked |= self.resolve(&mut inherit.scheme_name, &mut inherit.scheme);
                    for entry in &mut inherit.virtuals {
                        looked |= self.resolve(&mut entry.virt_name, &mut entry.virt);
                        if let Some(virt) = entry.virt {
                            self.schemes[virt].virtual_target = true;
                        }
                        looked |= self.resolve(&mut entry.subst_name, &mut entry.subst);
                        changed.extend(entry.virt.filter(|_| looked));
                    }
                }
                _ => {}
            }
            if looked {
                changed.push(id);
            }
            self.nodes[at] = node;
        }
        changed
    }

    /// `buildSearchNodes`: inherits with no virtual substitution (and not into a virtual target, nor
    /// into a scheme being built) put in place, then the keyword lists merged.
    fn build_search(&mut self, id: SchemeId, built: &mut HashMap<SchemeId, Built>) {
        if built.contains_key(&id) {
            return;
        }
        built.insert(id, Built::Building);
        let mut search = Vec::new();
        for &at in &self.schemes[id].nodes.clone() {
            if let Node::Inherit(inherit) = &self.nodes[at]
                && let Some(target) = inherit.scheme
                && inherit.virtuals.is_empty()
                && !self.schemes[target].virtual_target
                && built.get(&target) != Some(&Built::Building)
            {
                self.build_search(target, built);
                search.extend(self.schemes[target].search.iter().copied());
                continue;
            }
            search.push(NodeRef::Node(at));
        }
        self.merge_keywords(id, search);
        built.insert(id, Built::Done);
    }

    /// `mergeSearchKeywords`: runs of keyword lists of the same case and no word dividers merged.
    fn merge_keywords(&mut self, id: SchemeId, search: Vec<NodeRef>) {
        let mut merged = Vec::with_capacity(search.len());
        let mut owned = Vec::new();
        let mut i = 0;
        while i < search.len() {
            let Some(first) = self.keywords_of(search[i]) else {
                merged.push(search[i]);
                i += 1;
                continue;
            };
            if first.list.words.is_empty() {
                i += 1;
                continue;
            }
            let mut run = vec![&first.list];
            let mut j = i + 1;
            while j < search.len() {
                let Some(next) = self.keywords_of(search[j]) else {
                    break;
                };
                if next.list.words.is_empty() {
                    j += 1;
                    continue;
                }
                let mergeable = next.list.match_case == first.list.match_case
                    && first.worddiv.is_none()
                    && next.worddiv.is_none();
                if !mergeable {
                    break;
                }
                run.push(&next.list);
                j += 1;
            }
            if run.len() == 1 {
                merged.push(search[i]);
            } else {
                let list = KeywordList::merged(&run);
                owned.push(Node::Keywords(Box::new(KeywordsNode {
                    list,
                    worddiv: None,
                })));
                merged.push(NodeRef::Owned(id, owned.len() - 1));
            }
            i = j;
        }
        self.schemes[id].owned = owned;
        self.schemes[id].search = merged;
    }

    fn keywords_of(&self, node: NodeRef) -> Option<&KeywordsNode> {
        match self.node(node) {
            Node::Keywords(keywords) => Some(keywords),
            _ => None,
        }
    }

    /// The schemes each virtual target may be replaced by.
    fn substitutions(&self) -> HashMap<SchemeId, Vec<SchemeId>> {
        let mut map: HashMap<SchemeId, Vec<SchemeId>> = HashMap::new();
        for scheme in &self.schemes {
            for &at in &scheme.nodes {
                let Node::Inherit(inherit) = &self.nodes[at] else {
                    continue;
                };
                for entry in &inherit.virtuals {
                    if let (Some(virt), Some(subst)) = (entry.virt, entry.subst) {
                        map.entry(virt).or_default().push(subst);
                    }
                }
            }
        }
        map
    }

    /// Each scheme's dispatch, when it narrows the search enough.
    /// The dispatch of the schemes marked `dirty` built anew (the others keep theirs).
    fn build_dispatch(&mut self, dirty: &[bool]) {
        let mut starts = Starts {
            library: self,
            cache: HashMap::new(),
            substitutions: self.substitutions(),
        };
        let mut dispatches = Vec::with_capacity(starts.library.schemes.len());
        for (id, &dirty) in dirty.iter().enumerate() {
            if !dirty {
                dispatches.push(starts.library.schemes[id].dispatch.clone());
                continue;
            }
            let search = starts.library.schemes[id].search.clone();
            if search.len() < 2 || search.len() > usize::from(u16::MAX) {
                dispatches.push(None);
                continue;
            }
            let mut dispatch = Dispatch {
                offsets: [0; 129],
                nodes: Vec::new(),
            };
            for c in 0..128_u8 {
                dispatch.offsets[usize::from(c)] = dispatch.nodes.len() as u32;
                for (i, &node) in search.iter().enumerate() {
                    if starts.node(node, char::from(c)) {
                        dispatch.nodes.push(i as u16);
                    }
                }
            }
            dispatch.offsets[128] = dispatch.nodes.len() as u32;
            let narrow = dispatch.nodes.len() * 4 <= search.len() * 128 * 3;
            dispatches.push(narrow.then_some(dispatch));
        }
        for (scheme, dispatch) in self.schemes.iter_mut().zip(dispatches) {
            scheme.dispatch = dispatch;
        }
    }
}

/// Whether a node or a scheme may start with a character (`nodeCanStartWith`, `schemeCanStartWith`).
struct Starts<'a> {
    library: &'a Library,
    /// 1: being found, 2: no, 3: yes.
    cache: HashMap<(SchemeId, u8), u8>,
    substitutions: HashMap<SchemeId, Vec<SchemeId>>,
}

impl Starts<'_> {
    fn node(&mut self, node: NodeRef, c: char) -> bool {
        match self.library.node(node) {
            Node::Re(re) => re.start.can_start_with(c),
            Node::Block(block) => block.start.can_start_with(c),
            Node::Keywords(keywords) => {
                let list = &keywords.list;
                let c = if list.match_case {
                    c
                } else {
                    chars_to_lower(c)
                };
                !list.words.is_empty() && list.first_char.contains(c)
            }
            Node::Inherit(inherit) => inherit.scheme.is_none_or(|scheme| self.scheme(scheme, c)),
        }
    }

    fn scheme(&mut self, scheme: SchemeId, c: char) -> bool {
        let key = (scheme, c as u8);
        match self.cache.get(&key) {
            Some(1 | 3) => return true,
            Some(2) => return false,
            _ => {}
        }
        self.cache.insert(key, 1);
        let library = self.library;
        let mut result = library.schemes[scheme]
            .search
            .iter()
            .any(|&node| self.node(node, c));
        if !result && let Some(substs) = self.substitutions.get(&scheme).cloned() {
            result = substs.into_iter().any(|subst| self.scheme(subst, c));
        }
        self.cache.insert(key, if result { 3 } else { 2 });
        result
    }
}
