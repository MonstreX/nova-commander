//! Colorer's text parser (`TextParserImpl`), a line at a time: the state at a line's start is the stack
//! of the blocks open there (what Colorer's `ParseCache` holds for the line), the line is coloured from
//! it - the innermost block's scheme until its end, then the one around it, up to the type's base scheme
//! (`TextParser::parse`) - and the state at the next line's start comes out. A state kept every so many
//! lines lets any screen be coloured from the nearest one above it (ADR-0024 §4).
//!
//! The line's regions come out as Colorer's `LineRegionsSupport` lays them out: the scheme the line
//! starts in, then each region and each scheme entered or left in the order found, a later one painted
//! over an earlier one.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

mod search;
mod vtlist;

use std::rc::Rc;

use super::hrc::{BlockNode, Library, Node, NodeRef, RegionId, SchemeId};
use super::regex::{Mask, Matches, Parse, ascii_chars};
use vtlist::VtList;

/// The deepest blocks go (`MAX_RECURSION_LEVEL`).
const MAX_DEPTH: usize = 100;
/// The most inherits searched one inside another: a schema inheriting itself round (Colorer recurses
/// until its stack overflows; ours stops: review B7).
pub(super) const MAX_INHERITS: usize = 64;
/// The most characters a scheme's search looks at past its parent's end (FarColorer's `maxblocksize`).
pub const MAX_BLOCK: usize = 300;

/// A block open at a line's start (a `ParseCache` entry).
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    /// The scheme inside it (after a virtual substitution).
    pub scheme: SchemeId,
    /// The `<block>` that opened it.
    pub block: NodeRef,
    /// Its start's match, for its end's `\y`.
    pub start: Matches,
    /// The line its start was on, when its end refers back to it.
    pub back_line: Option<Rc<[char]>>,
    /// The inherits whose virtual substitutions held when it opened (`vcache`).
    pub virtuals: Vec<NodeRef>,
}

/// The parser's state at a line's start: the blocks open, the outermost first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub levels: Vec<Level>,
}

/// One region of a line (`LineRegion`): from `start` to `end` (none: the line's end), painted in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: Option<usize>,
    /// None: the background (no region).
    pub region: Option<RegionId>,
    /// The scheme it is in (`LineRegion::scheme`; none outside every block).
    pub scheme: Option<SchemeId>,
    /// The blocks open around it (the Outliner's `curLevel`).
    pub depth: usize,
    /// A scheme's background going on (`enterScheme`, `leaveScheme`), not a region added.
    pub flow: bool,
}

/// What a search at a position found (`MATCH_NOTHING`, `MATCH_RE`, `MATCH_SCHEME`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Found {
    Nothing,
    Re,
    Scheme,
}

/// A block's end being looked for: its expression and what its `\y` refers to.
#[derive(Clone, Copy)]
struct End<'a> {
    block: &'a BlockNode,
    back: Option<(&'a [char], &'a Matches)>,
}

/// The line's regions being laid out (`LineRegionsSupport` for one line).
#[derive(Default)]
struct Regions {
    spans: Vec<Span>,
    /// The span the background of the scheme in force goes on in.
    flow: usize,
    /// The regions and the schemes of the blocks entered, the line's background first.
    schemes: Vec<(Option<RegionId>, Option<SchemeId>)>,
}

impl Regions {
    fn new() -> Regions {
        Regions {
            spans: vec![Span {
                start: 0,
                end: None,
                region: None,
                scheme: None,
                depth: 0,
                flow: true,
            }],
            flow: 0,
            schemes: vec![(None, None)],
        }
    }

    fn add(&mut self, start: i32, end: i32, region: Option<RegionId>) {
        if let (Ok(start), Some(region)) = (usize::try_from(start), region) {
            let end = usize::try_from(end).ok();
            let scheme = self.schemes.last().and_then(|(_, scheme)| *scheme);
            self.spans.push(Span {
                start,
                end,
                region: Some(region),
                scheme,
                depth: self.schemes.len() - 1,
                flow: false,
            });
        }
    }

    fn flow_to(&mut self, start: usize, (region, scheme): (Option<RegionId>, Option<SchemeId>)) {
        self.spans[self.flow].end = Some(start);
        self.flow = self.spans.len();
        self.spans.push(Span {
            start,
            end: None,
            region,
            scheme,
            depth: self.schemes.len() - 1,
            flow: true,
        });
    }

    fn enter(&mut self, start: i32, region: Option<RegionId>, scheme: SchemeId) {
        self.schemes.push((region, Some(scheme)));
        if region.is_some() {
            self.flow_to(usize::try_from(start).unwrap_or(0), (region, Some(scheme)));
        }
    }

    fn leave(&mut self, end: i32) {
        let region = self.schemes.pop().and_then(|(region, _)| region);
        if region.is_some() {
            let outer = self.schemes.last().copied().unwrap_or_default();
            self.flow_to(usize::try_from(end).unwrap_or(0), outer);
        }
    }
}

/// A line parsed: its regions and the state at the next line's start.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub spans: Vec<Span>,
    pub next: State,
}

/// The parser over one line (`TextParser::Impl`).
struct Parser<'a> {
    lib: &'a Library,
    line: &'a [char],
    lower: Option<Vec<char>>,
    chars: Mask,
    gx: i32,
    len: i32,
    scheme_start: i32,
    scheme: SchemeId,
    matchend: Matches,
    vt: VtList,
    /// The line is done (Colorer's `current_parse_line` past `end_line4parse`).
    done: bool,
    depth: usize,
    /// The inherits being searched through, one inside another (`search_inherit`).
    inherits: usize,
    max_block: i32,
    regions: Regions,
    /// The blocks opened on the line and not closed, the innermost first.
    opened: Vec<Level>,
}

/// `line` parsed from `state` with `base` the type's base scheme.
pub fn parse_line(lib: &Library, base: SchemeId, state: &State, line: &[char]) -> Parsed {
    let mut parser = Parser {
        lib,
        line,
        lower: None,
        chars: ascii_chars(line),
        gx: 0,
        len: -1,
        scheme_start: -1,
        scheme: base,
        matchend: Matches::default(),
        vt: VtList::default(),
        done: false,
        depth: 0,
        inherits: 0,
        max_block: MAX_BLOCK as i32,
        regions: Regions::new(),
        opened: Vec::new(),
    };
    for level in &state.levels {
        let region = parser.block(level.block).region;
        parser.regions.enter(0, region, level.scheme);
    }
    let mut open = state.levels.len();
    for level in state.levels.iter().rev() {
        let block = parser.block(level.block);
        parser.vt.restore(&level.virtuals);
        parser.scheme = level.scheme;
        parser.depth = 0;
        let back = level.back_line.as_deref().map(|text| (text, &level.start));
        parser.colorize(Some(End { block, back }), block.low_content_priority);
        parser.vt.clear();
        if parser.done {
            break;
        }
        let matchend = parser.matchend.clone();
        parser.leave_block(&matchend, block);
        parser.gx = parser.matchend.e[0];
        open -= 1;
    }
    if open == 0 {
        parser.scheme = base;
        parser.depth = 0;
        parser.colorize(None, false);
    }
    let mut levels = state.levels[..open].to_vec();
    levels.extend(parser.opened.into_iter().rev());
    Parsed {
        spans: parser.regions.spans,
        next: State { levels },
    }
}

impl<'a> Parser<'a> {
    fn block(&self, node: NodeRef) -> &'a BlockNode {
        match self.lib.node(node) {
            Node::Block(block) => block,
            other => unreachable!("a level's node is a block: {other:?}"),
        }
    }

    fn lower(&mut self) -> &[char] {
        if self.lower.is_none() {
            let lower = self
                .line
                .iter()
                .map(|&c| super::regex::to_lower(c))
                .collect();
            self.lower = Some(lower);
        }
        self.lower.as_deref().unwrap_or(self.line)
    }

    fn how(&self, pos: i32, eol: i32, moves: bool) -> Parse<'_> {
        Parse {
            pos: usize::try_from(pos).unwrap_or(0),
            eol: Some(usize::try_from(eol).unwrap_or(0)),
            scheme_start: usize::try_from(self.scheme_start).unwrap_or(usize::MAX),
            moves,
            line_chars: Some(&self.chars),
            back: None,
        }
    }

    /// `enterScheme` of a block: its region (inside its start with `inner-region`), its start's regions.
    fn enter_block(&mut self, found: &Matches, block: &BlockNode) {
        if !block.inner_region {
            self.regions.enter(found.s[0], block.region, self.scheme);
        }
        for i in 0..found.count {
            self.regions.add(found.s[i], found.e[i], block.regions[i]);
        }
        for i in 0..found.named {
            self.regions
                .add(found.ns[i], found.ne[i], block.regions_n[i]);
        }
        if block.inner_region {
            self.regions.enter(found.e[0], block.region, self.scheme);
        }
    }

    /// `leaveScheme` of a block: its end's regions, then its region left.
    fn leave_block(&mut self, found: &Matches, block: &BlockNode) {
        if block.inner_region {
            self.regions.leave(found.s[0]);
        }
        for i in 0..found.count {
            self.regions.add(found.s[i], found.e[i], block.regione[i]);
        }
        for i in 0..found.named {
            self.regions
                .add(found.ns[i], found.ne[i], block.regionen[i]);
        }
        if !block.inner_region {
            self.regions.leave(found.e[0]);
        }
    }

    /// The end of the block around, looked for from `gx`.
    fn end_found(&mut self, end: Option<End>) -> bool {
        let Some(End { block, back }) = end else {
            return false;
        };
        let mut how = self.how(self.gx, self.len, block.end.position_moves());
        how.back = back;
        if !block.end.may_match(self.line.len(), &how, &self.chars) {
            return false;
        }
        let mut found = Matches::default();
        let matched = block.end.parse(self.line, &mut found, how);
        if matched {
            self.matchend = found;
        }
        matched
    }

    /// `colorize`: the current scheme over the line from `gx` until the block's end (or the line's).
    fn colorize(&mut self, end: Option<End>, low_content_priority: bool) {
        self.len = -1;
        if self.depth > MAX_DEPTH {
            return;
        }
        self.depth += 1;
        while !self.done {
            if self.len < 0 {
                self.len = self.line.len() as i32;
            }
            let ended = self.end_found(end);
            if !ended {
                let bound = (self.gx + self.max_block).min(self.len);
                (self.matchend.s[0], self.matchend.e[0]) = (bound, bound);
            }
            let parent_len = self.len;
            if low_content_priority {
                self.len = self.matchend.s[0];
            }
            if self.search_line(parent_len) {
                continue;
            }
            if ended {
                self.scheme_start = -1;
                self.depth -= 1;
                return;
            }
            // Colorer's `chunkLongLines` (FarColorer's default; review B1): the block's end not found,
            // the line goes on in its next window - past about 300 characters it was left uncoloured.
            if self.gx < self.len {
                continue;
            }
            self.scheme_start = -1;
            self.len = -1;
            self.done = true;
            self.gx = 0;
        }
        self.depth -= 1;
    }

    /// The scheme searched at each position up to the end found; true when the line is to be looked at
    /// again (a match went past that end, or a block took the rest of the line).
    fn search_line(&mut self, parent_len: i32) -> bool {
        while self.gx <= self.matchend.s[0] {
            let was_done = self.done;
            let low = self.matchend.s[0];
            let high = (low + self.max_block).min(self.len);
            let found = self.search_match(self.scheme, low, high);
            let past = self.matchend.s[0] < self.gx;
            let moved = was_done != self.done;
            if (found == Found::Scheme && (moved || past)) || (found == Found::Re && past) {
                self.len = -1;
                return true;
            }
            if !moved {
                self.len = parent_len;
            }
            if found == Found::Nothing {
                self.gx += 1;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests;
