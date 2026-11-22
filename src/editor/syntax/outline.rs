//! Colorer's `Outliner` over a document's lines (FarColorer's "Outliner list" and "Errors list"): each
//! line's first region under `def:Outlined` (or `def:Error`) makes an item - its line, its start, the
//! blocks open around it, its text - and the line's next ones add their text to it.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use super::hrc::{Library, RegionId};
use super::parser::Span;

/// An item of an outline (`OutlineItem`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineItem {
    /// Its line's number (from 0) and first byte.
    pub line: u64,
    pub start: u64,
    /// Its first character on the line.
    pub pos: usize,
    /// The blocks open around it (`level`).
    pub level: usize,
    pub token: String,
    /// Its region's name.
    pub region: String,
}

/// The two outlines FarColorer keeps: the structure's and the errors'.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outlines {
    pub functions: Vec<OutlineItem>,
    pub errors: Vec<OutlineItem>,
}

impl Outlines {
    /// The items from the line starting at `start` on dropped (`modifyEvent`).
    pub fn drop_from(&mut self, start: u64) {
        self.functions.retain(|item| item.start < start);
        self.errors.retain(|item| item.start < start);
    }

    /// A line's spans added (`addRegion` of each region, the line's first making the item).
    pub fn add_line(
        &mut self,
        lib: &Library,
        (outlined, error): (Option<RegionId>, Option<RegionId>),
        (line, start): (u64, u64),
        (chars, spans): (&[char], &[Span]),
    ) {
        for (list, wanted) in [(&mut self.functions, outlined), (&mut self.errors, error)] {
            let Some(wanted) = wanted else {
                continue;
            };
            let mut first = true;
            for span in spans.iter().filter(|span| !span.flow) {
                let Some(region) = span.region.filter(|&r| lib.region_has_parent(r, wanted)) else {
                    continue;
                };
                let end = span.end.unwrap_or(chars.len()).min(chars.len());
                let text: String = chars
                    .get(span.start..end)
                    .unwrap_or_default()
                    .iter()
                    .collect();
                if first {
                    list.push(OutlineItem {
                        line,
                        start,
                        pos: span.start,
                        level: span.depth,
                        token: text,
                        region: lib.region(region).name.clone(),
                    });
                    first = false;
                } else if let Some(item) = list.last_mut().filter(|item| item.line == line) {
                    item.token.push_str(&text);
                }
            }
        }
    }
}

/// `Outliner::manageTree`: an item's level in the tree from the levels of the items above it.
pub fn tree_level(stack: &mut Vec<usize>, level: usize) -> usize {
    while stack.last().is_some_and(|&top| level < top) {
        stack.pop();
    }
    match stack.last() {
        Some(&top) if top == level => stack.len() - 1,
        Some(&top) if top > level => 0,
        _ => {
            stack.push(level);
            stack.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_level_follows_the_items_above() {
        let mut stack = Vec::new();
        let levels: Vec<usize> = [1, 2, 2, 3, 1, 4]
            .iter()
            .map(|&level| tree_level(&mut stack, level))
            .collect();
        assert_eq!(levels, [0, 1, 1, 2, 0, 1]);
    }
}
