//! What the scheme finds at a position (`searchMatch` and its `searchKW`, `searchRE`, `searchBL`,
//! `searchIN`): the first node in the scheme's order that matches there wins - a keyword, a regular
//! expression, a block (whose scheme then takes the text up to its end, maybe past the line), an
//! inherited scheme (or its virtual substitute).
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use std::rc::Rc;

use super::super::hrc::{BlockNode, InheritNode, KeywordsNode, Node, NodeRef, ReNode, SchemeId};
use super::super::regex::{Matches, is_word};
use super::{End, Found, Level, Parser};

impl Parser<'_> {
    /// `searchMatch`: the scheme's nodes that may start with the character here, in its order.
    pub(super) fn search_match(&mut self, scheme: SchemeId, low: i32, high: i32) -> Found {
        let lib = self.lib;
        let scheme = lib.scheme(scheme);
        let here = usize::try_from(self.gx)
            .ok()
            .and_then(|gx| self.line.get(gx));
        let slice = match (&scheme.dispatch, here) {
            (Some(dispatch), Some(&c)) if c.is_ascii() => {
                let c = c as usize;
                let range = dispatch.offsets[c] as usize..dispatch.offsets[c + 1] as usize;
                Some(&dispatch.nodes[range])
            }
            _ => None,
        };
        let count = slice.map_or(scheme.search.len(), <[u16]>::len);
        for i in 0..count {
            let at = slice.map_or(i, |nodes| usize::from(nodes[i]));
            let node = scheme.search[at];
            let found = match lib.node(node) {
                Node::Inherit(inherit) => self.search_inherit(node, inherit, low, high),
                Node::Keywords(keywords) => self.search_keywords(keywords, low),
                Node::Re(re) => self.search_re(re, low, high),
                Node::Block(block) => self.search_block(node, block, low, high),
            };
            if found != Found::Nothing {
                return if matches!(lib.node(node), Node::Block(_)) {
                    Found::Scheme
                } else {
                    found
                };
            }
        }
        Found::Nothing
    }

    /// `searchIN`: the inherited scheme, or its substitute by an inherit's virtual entry.
    fn search_inherit(
        &mut self,
        node: NodeRef,
        inherit: &InheritNode,
        low: i32,
        high: i32,
    ) -> Found {
        let Some(target) = inherit.scheme else {
            return Found::Nothing;
        };
        if self.inherits >= super::MAX_INHERITS {
            return Found::Nothing;
        }
        self.inherits += 1;
        let found = self.search_inherited(node, inherit, target, (low, high));
        self.inherits -= 1;
        found
    }

    /// The inherited scheme searched (or its virtual substitute).
    fn search_inherited(
        &mut self,
        node: NodeRef,
        inherit: &InheritNode,
        target: SchemeId,
        (low, high): (i32, i32),
    ) -> Found {
        let subst = if self.lib.scheme(target).virtual_target {
            self.vt.push_virtual(self.lib, target)
        } else {
            None
        };
        match subst {
            None => {
                let pushed = self.vt.push(node, inherit);
                let found = self.search_match(target, low, high);
                if pushed {
                    self.vt.pop();
                }
                found
            }
            Some(subst) => {
                let found = self.search_match(subst, low, high);
                self.vt.pop_virtual();
                found
            }
        }
    }

    /// `searchRE`.
    fn search_re(&mut self, re: &ReNode, low: i32, high: i32) -> Found {
        let eol = if re.low_priority { low } else { high };
        let how = self.how(self.gx, eol, re.start.position_moves());
        let mut found = Matches::default();
        if !re.start.may_match(self.line.len(), &how, &self.chars)
            || !re.start.parse(self.line, &mut found, how)
        {
            return Found::Nothing;
        }
        for i in 0..found.count {
            self.regions.add(found.s[i], found.e[i], re.regions[i]);
        }
        for i in 0..found.named {
            self.regions.add(found.ns[i], found.ne[i], re.regions_n[i]);
        }
        if found.e[0] == found.s[0] {
            return Found::Nothing;
        }
        self.gx = found.e[0];
        Found::Re
    }

    /// `searchBL`: the block's start, then its scheme up to its end; a block not ended on the line is
    /// left open for the next.
    fn search_block(&mut self, node: NodeRef, block: &BlockNode, low: i32, high: i32) -> Found {
        let Some(inner) = block.scheme else {
            return Found::Nothing;
        };
        let eol = if block.low_priority { low } else { high };
        let how = self.how(self.gx, eol, block.start.position_moves());
        let mut start = Matches::default();
        if !block.start.may_match(self.line.len(), &how, &self.chars)
            || !block.start.parse(self.line, &mut start, how)
        {
            return Found::Nothing;
        }
        self.gx = start.e[0];
        let subst = if self.lib.scheme(inner).virtual_target {
            self.vt.push_virtual(self.lib, inner)
        } else {
            None
        };
        let scheme = subst.unwrap_or(inner);
        let back_line: Option<Rc<[char]>> = block.end.has_back_trace().then(|| self.line.into());
        let (was_done, old_scheme, old_start) = (self.done, self.scheme, self.scheme_start);
        let old_matchend = self.matchend.clone();
        self.scheme = scheme;
        self.scheme_start = self.gx;
        self.enter_block(&start, block);
        let virtuals = self.vt.store();
        let back = Some((back_line.as_deref().unwrap_or(self.line), &start));
        self.colorize(Some(End { block, back }), block.low_content_priority);
        if !self.done {
            let matchend = self.matchend.clone();
            self.leave_block(&matchend, block);
        }
        self.gx = self.matchend.e[0];
        let zero_length = start.s[0] == self.matchend.e[0] && was_done == self.done;
        self.matchend = old_matchend;
        self.scheme_start = old_start;
        self.scheme = old_scheme;
        if self.done && !was_done {
            self.opened.push(Level {
                scheme,
                block: node,
                start,
                back_line,
                virtuals,
            });
        }
        if subst.is_some() {
            self.vt.pop_virtual();
        }
        if zero_length {
            Found::Nothing
        } else {
            Found::Scheme
        }
    }

    /// `searchKW`: the list's word here, in the line or its lower case.
    fn search_keywords(&mut self, keywords: &KeywordsNode, low: i32) -> Found {
        let list = &keywords.list;
        let gx = self.gx as usize;
        let low = usize::try_from(low).unwrap_or(0);
        if list.words.is_empty() || list.min_len.saturating_add(gx) > low {
            return Found::Nothing;
        }
        if !list.match_case {
            self.lower();
        }
        let text = match (&self.lower, list.match_case) {
            (Some(lower), false) => lower.as_slice(),
            _ => self.line,
        };
        let Some((len, region)) = find_keyword(keywords, text, gx, low) else {
            return Found::Nothing;
        };
        self.regions.add(gx as i32, (gx + len) as i32, region);
        self.gx += len as i32;
        Found::Re
    }
}

/// A binary search of the list for the word at `gx` with its word boundaries (`searchKW`): its length
/// and its region.
fn find_keyword(
    keywords: &KeywordsNode,
    text: &[char],
    gx: usize,
    low: usize,
) -> Option<(usize, Option<super::super::hrc::RegionId>)> {
    let list = &keywords.list;
    let bound = |c: char| match &keywords.worddiv {
        None => !is_word(c),
        Some(div) => div.contains(c),
    };
    let left_bad = list.has_non_symbols && gx > 0 && !bound(text[gx - 1]);
    if left_bad && !list.has_symbols {
        return None;
    }
    let (mut left, mut right) = (0, list.words.len());
    if gx < low {
        let c = text[gx];
        if c.is_ascii() {
            let c = c as usize;
            if list.first_ascii[c >> 6] & (1 << (c & 63)) == 0 {
                return None;
            }
            (left, right) = (list.ascii_begin[c], list.ascii_end[c]);
        } else if !list.first_char.contains(c) {
            return None;
        }
    }
    if left >= right {
        return None;
    }
    loop {
        let pos = left + (right - left) / 2;
        let word = &list.words[pos];
        let len = word.word.len().min(low - gx);
        let mut order = word.word.as_slice().cmp(&text[gx..gx + len]);
        if order.is_eq() && right - left == 1 {
            let after_bad = gx + len < low && !bound(text[gx + len]);
            if !word.is_symbol && (left_bad || after_bad) {
                order = std::cmp::Ordering::Less;
            }
            if order.is_eq() {
                return Some((len, word.region));
            }
        }
        if right - left == 1 {
            let shorter = word.shorter?;
            (left, right) = (shorter, shorter + 1);
        } else if order.is_gt() {
            right = pos;
        } else {
            left = pos;
        }
    }
}
