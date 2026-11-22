//! FarColorer's editor features over the highlighter's states (S2d.7e2): the region at the cursor
//! (`cursorRegion`: "Region info", "Select region") and the pair of the bracket at the cursor
//! (`BaseEditor::searchPair`: "Match pair", "Select block", "Select pair") - each line's regions
//! parsed from the nearest state kept above it.

use std::sync::Arc;

use super::{FILL_LIMIT, Highlighter, Wait};
use crate::editor::syntax::hrc::Library;
use crate::editor::syntax::outline::Outlines;
use crate::editor::syntax::parser::{State, parse_line};
use crate::editor::syntax::regions::{self, Defs, PairEnd, Part};
use crate::editor::view::{LineSource, Missing};

/// FarColorer's backparse: the lines a pair is looked for past in a document too big to keep states
/// for (its `getLineRegions` gives nothing that far beyond what it parsed).
const BACK_PARSE: usize = 2000;
/// The lines above parsed at once when the pair is looked for upwards.
const CHUNK: usize = 64;

/// The region at the cursor: its characters, its name and its scheme's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorRegion {
    pub start: usize,
    pub end: Option<usize>,
    /// None: the background of the scheme it is in.
    pub region: Option<String>,
    pub scheme: Option<String>,
}

/// A pair found (`PairMatch`): its end at the cursor and its other end (none: not found), each with
/// its line's first byte; whether the cursor's end opens it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairMatch {
    pub start: (u64, PairEnd),
    pub end: Option<(u64, PairEnd)>,
    pub top: bool,
}

/// The lines of a document with their regions compacted.
type Lines = Vec<(u64, Vec<Part>)>;

impl Highlighter {
    /// The state at the start of the line at `line` (its first byte), parsed from the nearest above.
    fn state_at(&self, lib: &Library, view: &dyn LineSource, line: u64) -> Result<State, Missing> {
        let (mut start, mut state, _) = self.start_for(view, line)?;
        while start < line {
            let text = view.line_at(start)?;
            state = parse_line(lib, self.base, &state, &text.chars).next;
            match view.line_after(&text)? {
                Some(next) => start = next,
                None => break,
            }
        }
        Ok(state)
    }

    /// The lines from `first` to before `past` (none: on to `count` lines) with their regions.
    fn parse_lines(
        &self,
        lib: &Library,
        view: &dyn LineSource,
        (first, past): (u64, Option<u64>),
        count: usize,
    ) -> Result<Lines, Missing> {
        let defs = Defs::of(lib);
        let mut state = self.state_at(lib, view, first)?;
        let mut lines = Vec::new();
        let mut start = Some(first);
        while let Some(at) = start.filter(|at| past.is_none_or(|past| *at < past)) {
            if lines.len() >= count {
                break;
            }
            let text = view.line_at(at)?;
            let parsed = parse_line(lib, self.base, &state, &text.chars);
            lines.push((at, regions::compact(lib, &defs, &parsed.spans)));
            state = parsed.next;
            start = view.line_after(&text)?;
        }
        Ok(lines)
    }

    /// The outlines of the whole document (FarColorer's `validate(-1)` before its lists), collected on
    /// from where they stopped; none in a document too big to keep states for.
    pub fn outlines(&mut self, view: &dyn LineSource) -> Result<Outlines, Wait> {
        if view.size() > FILL_LIMIT {
            return Ok(Outlines::default());
        }
        let syntax = Arc::clone(&self.syntax);
        let Some(lib) = syntax.read_now()? else {
            return Ok(Outlines::default());
        };
        self.walk_outlines(&lib, view, None)?;
        Ok(self.outline.outlines.clone())
    }

    /// The outlines collected on from where they stopped, until the document's end or `until`
    /// (review B3: the ticks collect them ahead, so that the list asks for little); whether some are
    /// left.
    pub(super) fn walk_outlines(
        &mut self,
        lib: &Library,
        view: &dyn LineSource,
        until: Option<std::time::Instant>,
    ) -> Result<bool, Missing> {
        let defs = Defs::of(lib);
        while let Some((start, number, state)) = self.outline.next.take() {
            if until.is_some_and(|until| std::time::Instant::now() >= until) {
                self.outline.next = Some((start, number, state));
                return Ok(true);
            }
            let walked = (view.line_at(start)).and_then(|text| {
                let parsed = parse_line(lib, self.base, &state, &text.chars);
                Ok((view.line_after(&text)?, text, parsed))
            });
            let (next, text, parsed) = match walked {
                Ok(walked) => walked,
                Err(missing) => {
                    self.outline.next = Some((start, number, state));
                    return Err(missing);
                }
            };
            let line = (number, start);
            let wanted = (defs.outlined, defs.error);
            (self.outline.outlines).add_line(lib, wanted, line, (&text.chars, &parsed.spans));
            self.outline.next = next.map(|next| (next, number + 1, parsed.next));
        }
        Ok(false)
    }

    /// FarColorer's `cursorRegion` on the line at `line` with the cursor at `pos`.
    pub fn cursor_region(
        &self,
        view: &dyn LineSource,
        line: u64,
        pos: usize,
    ) -> Result<Option<CursorRegion>, Wait> {
        let syntax = Arc::clone(&self.syntax);
        let Some(lib) = syntax.read_now()? else {
            return Ok(None);
        };
        let lines = self.parse_lines(&lib, view, (line, None), 1)?;
        let length = view.line_at(line)?.chars.len();
        let Some(part) =
            (lines.first()).and_then(|(_, list)| regions::cursor_part(list, pos, length))
        else {
            return Ok(None);
        };
        let span = part.span;
        Ok(Some(CursorRegion {
            start: span.start,
            end: span.end,
            region: span.region.map(|region| lib.region(region).name.clone()),
            scheme: span.scheme.map(|scheme| lib.scheme(scheme).name.clone()),
        }))
    }

    /// `searchGlobalPair` from the cursor at `pos` of the line at `line`.
    pub fn search_pair(
        &self,
        view: &dyn LineSource,
        line: u64,
        pos: usize,
    ) -> Result<Option<PairMatch>, Wait> {
        let syntax = Arc::clone(&self.syntax);
        let Some(lib) = syntax.read_now()? else {
            return Ok(None);
        };
        let defs = Defs::of(&lib);
        let limit = match view.size() > FILL_LIMIT {
            true => BACK_PARSE,
            false => usize::MAX,
        };
        let here = self.parse_lines(&lib, view, (line, None), 1)?;
        let Some((_, list)) = here.first() else {
            return Ok(None);
        };
        let ends = regions::pair_ends(&lib, &defs, list);
        let Some(at) = regions::pair_at(&ends, pos) else {
            return Ok(None);
        };
        let start = ends[at];
        let mut found = PairMatch {
            start: (line, start),
            end: None,
            top: start.opens,
        };
        found.end = match start.opens {
            true => self.pair_below(&lib, view, (line, &ends[at + 1..]), limit)?,
            false => self.pair_above(&lib, view, (line, &ends[..at]), limit)?,
        };
        Ok(Some(found))
    }

    /// The pair's end below an opening one: after it on its line, then on the lines after.
    fn pair_below(
        &self,
        lib: &Library,
        view: &dyn LineSource,
        (line, after): (u64, &[PairEnd]),
        limit: usize,
    ) -> Result<Option<(u64, PairEnd)>, Missing> {
        let defs = Defs::of(lib);
        let mut balance = 1;
        let mut take = |at: u64, end: &PairEnd| {
            balance += if end.opens { 1 } else { -1 };
            (balance == 0).then_some((at, *end))
        };
        if let Some(found) = after.iter().find_map(|end| take(line, end)) {
            return Ok(Some(found));
        }
        let Some(next) = view.line_after(&view.line_at(line)?)? else {
            return Ok(None);
        };
        // Line by line, stopping at the pair (review B2: the rest of the document was parsed first).
        let mut state = self.state_at(lib, view, next)?;
        let (mut at, mut seen) = (Some(next), 0);
        while let Some(start) = at.filter(|_| seen < limit) {
            let text = view.line_at(start)?;
            let parsed = parse_line(lib, self.base, &state, &text.chars);
            let list = regions::compact(lib, &defs, &parsed.spans);
            let ends = regions::pair_ends(lib, &defs, &list);
            if let Some(found) = ends.iter().find_map(|end| take(start, end)) {
                return Ok(Some(found));
            }
            state = parsed.next;
            at = view.line_after(&text)?;
            seen += 1;
        }
        Ok(None)
    }

    /// The pair's end above a closing one: before it on its line, then on the lines above.
    fn pair_above(
        &self,
        lib: &Library,
        view: &dyn LineSource,
        (line, before): (u64, &[PairEnd]),
        limit: usize,
    ) -> Result<Option<(u64, PairEnd)>, Missing> {
        let defs = Defs::of(lib);
        let mut balance = -1;
        let mut take = |at: u64, end: &PairEnd| {
            balance += if end.opens { 1 } else { -1 };
            (balance == 0).then_some((at, *end))
        };
        if let Some(found) = before.iter().rev().find_map(|end| take(line, end)) {
            return Ok(Some(found));
        }
        let (mut past, mut seen) = (line, 0);
        while seen < limit {
            let mut first = past;
            for _ in 0..CHUNK {
                match view.line_above(first)? {
                    Some(above) => first = above,
                    None => break,
                }
            }
            if first == past {
                return Ok(None);
            }
            let lines = self.parse_lines(lib, view, (first, Some(past)), usize::MAX)?;
            for (at, list) in lines.iter().rev() {
                let ends = regions::pair_ends(lib, &defs, list);
                if let Some(found) = ends.iter().rev().find_map(|end| take(*at, end)) {
                    return Ok(Some(found));
                }
            }
            seen += lines.len();
            past = first;
        }
        Ok(None)
    }
}
