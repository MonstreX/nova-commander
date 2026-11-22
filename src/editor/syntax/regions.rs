//! A line's regions as FarColorer's editor keeps them (Colorer's `LineRegionsCompactSupport`, which
//! `BaseEditor` uses): each region added cuts or hides the ones under it, so the list is the line's
//! parts in their order, the regions under `def:Special` (pairs, outline) kept apart after them in the
//! order found. The region at the cursor (FarColorer's `cursorRegion`) and the pairs' ends come from it.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use super::hrc::{Library, RegionId};
use super::parser::Span;

/// A region of the compacted list (`LineRegion`): its characters (`end` none: to the line's end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
    pub span: Span,
    /// Under `def:Special` (`LineRegion::special`).
    pub special: bool,
}

/// The library's `def:` regions the editor looks for.
#[derive(Clone, Copy, Debug, Default)]
pub struct Defs {
    pub special: Option<RegionId>,
    pub pair_start: Option<RegionId>,
    pub pair_end: Option<RegionId>,
    pub outlined: Option<RegionId>,
    pub error: Option<RegionId>,
}

impl Defs {
    pub fn of(lib: &Library) -> Defs {
        Defs {
            special: lib.region_named("def:Special"),
            pair_start: lib.region_named("def:PairStart"),
            pair_end: lib.region_named("def:PairEnd"),
            outlined: lib.region_named("def:Outlined"),
            error: lib.region_named("def:Error"),
        }
    }
}

/// Whether `region` is `parent` or under it (`Region::hasParent`).
pub fn has_parent(lib: &Library, region: RegionId, parent: Option<RegionId>) -> bool {
    parent.is_some_and(|parent| lib.region_has_parent(region, parent))
}

/// `end` as Colorer compares it: -1 for none.
fn end_of(part: &Part) -> i64 {
    part.span.end.map_or(-1, |end| end as i64)
}

/// The line's spans (in the order the parser found them) compacted as `LineRegionsCompactSupport`.
pub fn compact(lib: &Library, defs: &Defs, spans: &[Span]) -> Vec<Part> {
    let mut list: Vec<Part> = Vec::new();
    for span in spans {
        let special = span
            .region
            .is_some_and(|region| has_parent(lib, region, defs.special));
        add(
            &mut list,
            Part {
                span: *span,
                special,
            },
        );
    }
    list
}

/// Where `part` goes in `list` (`addLineRegion`'s search).
fn place_of(list: &[Part], part: &Part) -> usize {
    let start = part.span.start;
    if list[0].span.start >= start {
        return 0;
    }
    for (at, here) in list.iter().enumerate() {
        if here.span.start == start {
            return at;
        }
        match list.get(at + 1) {
            None if here.span.start < start => return at + 1,
            Some(next) if here.span.start < start && (next.span.start > start || next.special) => {
                return at + 1;
            }
            _ => {}
        }
    }
    list.len()
}

/// `LineRegionsCompactSupport::addLineRegion`.
fn add(list: &mut Vec<Part>, part: Part) {
    if part.special || list.is_empty() {
        list.push(part);
        return;
    }
    let mut at = place_of(list, &part);
    list.insert(at, part);
    let (start, end) = (part.span.start as i64, end_of(&part));
    // The region before it, cut where it starts (split in two when it goes on past it).
    if at > 0 {
        let before = list[at - 1];
        let before_end = end_of(&before);
        if before_end > start || before_end == -1 {
            if (before_end > end || before_end == -1) && end != -1 {
                let mut rest = before;
                rest.span.start = end as usize;
                list.insert(at + 1, rest);
            }
            list[at - 1].span.end = Some(start as usize);
            if list[at - 1].span.end == Some(list[at - 1].span.start) {
                list.remove(at - 1);
                at -= 1;
            }
        }
    }
    // The regions after it, cut past its end or hidden by it.
    let mut next = at + 1;
    while next < list.len() {
        if list[next].special {
            next += 1;
            continue;
        }
        let next_end = end_of(&list[next]);
        if (next_end == -1 || next_end > end) && end != -1 && (list[next].span.start as i64) < end {
            list[next].span.start = end as usize;
        }
        if (next_end <= end && next_end != -1) || end == -1 {
            list.remove(next);
            continue;
        }
        next += 1;
    }
}

/// FarColorer's `cursorRegion`: the last region of the list that is not special, not empty, with the
/// cursor's position in it (its end counted in; none: the line's `length`).
pub fn cursor_part(list: &[Part], pos: usize, length: usize) -> Option<Part> {
    (list.iter())
        .filter(|part| !part.special && Some(part.span.start) != part.span.end)
        .rfind(|part| part.span.start <= pos && pos <= part.span.end.unwrap_or(length))
        .copied()
}

/// A pair's end on a line (a region under `def:PairStart` or `def:PairEnd`): its characters, whether
/// it opens the pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairEnd {
    pub start: usize,
    pub end: usize,
    pub opens: bool,
}

/// The line's pairs' ends in the list's order.
pub fn pair_ends(lib: &Library, defs: &Defs, list: &[Part]) -> Vec<PairEnd> {
    (list.iter())
        .filter_map(|part| {
            let region = part.span.region?;
            let opens = has_parent(lib, region, defs.pair_start);
            let closes = has_parent(lib, region, defs.pair_end);
            (opens || closes).then(|| PairEnd {
                start: part.span.start,
                end: part.span.end.unwrap_or(part.span.start),
                opens,
            })
        })
        .collect()
}

/// `getPairMatch`: the last pair's end with the cursor's position in it (its end counted in).
pub fn pair_at(ends: &[PairEnd], pos: usize) -> Option<usize> {
    (0..ends.len())
        .rev()
        .find(|&at| ends[at].start <= pos && pos <= ends[at].end)
}

#[cfg(test)]
#[path = "regions_tests.rs"]
mod tests;
