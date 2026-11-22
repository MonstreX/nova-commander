//! What a match must look like, found once after compiling (`CRegExp::optimize`): its first node and
//! first ASCII characters, its anchors, the most characters it takes, the characters it needs, and the
//! first characters of each alternation's left branch. The matcher skips with them what cannot match.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use super::chars;
use super::{Anchor, Mask, Meta, Op, Regex};

const ALL: Mask = [u64::MAX, u64::MAX];
/// The most character sets kept that a match needs (`MAX_REQUIRED_SETS`).
const REQUIRED_SETS: usize = 4;

fn bit(mask: &mut Mask, c: u32) {
    mask[(c >> 6) as usize] |= 1 << (c & 63);
}

fn count(mask: &Mask) -> u32 {
    mask[0].count_ones() + mask[1].count_ones()
}

/// The first characters of a chain or a node (`FirstChars`).
#[derive(Clone, Copy, Default)]
struct First {
    mask: Mask,
    nullable: bool,
}

/// Fills the regex's facts.
pub(super) fn optimize(regex: &mut Regex) {
    regex.facts.first_node = first_node(regex);
    let first = chain_first(regex, Some(regex.root));
    regex.facts.first_chars = (!first.nullable && first.mask != ALL).then_some(first.mask);
    regex.facts.start = start_anchor(regex);
    regex.facts.end_anchor = end_anchor(regex);
    regex.facts.max_len = chain_max(regex, Some(regex.root));
    let mut sets = chain_required(regex, Some(regex.root));
    sets.sort_by_key(count);
    let mut required: Vec<Mask> = Vec::new();
    for set in sets {
        if required.len() == REQUIRED_SETS {
            break;
        }
        if !required.contains(&set) {
            required.push(set);
        }
    }
    regex.facts.required = required;
    branch_firsts(regex, Some(regex.root));
}

/// The first node that takes a character, past brackets, look-arounds and zero-width symbols.
fn first_node(regex: &Regex) -> Option<usize> {
    let mut found = None;
    let mut next = Some(regex.root);
    while let Some(node) = next {
        let at = &regex.nodes[node];
        match &at.op {
            Op::Brackets => next = at.param,
            Op::Ahead | Op::NAhead | Op::Behind | Op::NBehind => next = at.next,
            Op::Meta(meta) => {
                found = Some(node);
                if !zero_width(*meta) {
                    break;
                }
                next = at.next;
            }
            Op::Symb(_) | Op::Word(_) | Op::Enum(_) => return Some(node),
            _ => break,
        }
    }
    found
}

fn zero_width(meta: Meta) -> bool {
    matches!(
        meta,
        Meta::SoL
            | Meta::EoL
            | Meta::WBound
            | Meta::NWBound
            | Meta::PreNW
            | Meta::SoScheme
            | Meta::Start
            | Meta::End
    )
}

fn add_first(regex: &Regex, first: &mut First, c: char) {
    let value = c as u32;
    if value >= 128 {
        return;
    }
    bit(&mut first.mask, value);
    if regex.flags.ignore_case && c.is_ascii_alphabetic() {
        bit(&mut first.mask, value ^ 0x20);
    }
}

/// A meta symbol's ASCII characters (none for a zero-width one).
fn meta_chars(regex: &Regex, meta: Meta) -> Option<Mask> {
    let test: fn(char) -> bool = match meta {
        Meta::AnyChr => {
            let mut mask = ALL;
            if !regex.flags.single_line {
                for c in 0x0A..=0x0D_u32 {
                    mask[0] &= !(1 << c);
                }
            }
            return Some(mask);
        }
        Meta::Digit => chars::is_digit,
        Meta::NDigit => |c| !chars::is_digit(c),
        Meta::WordSymb => chars::is_word,
        Meta::NWordSymb => |c| !chars::is_word(c),
        Meta::WSpace => chars::is_whitespace,
        Meta::NWSpace => |c| !chars::is_whitespace(c),
        Meta::UCase => chars::is_upper,
        Meta::NUCase => chars::is_lower,
        _ => return None,
    };
    let mut mask = [0; 2];
    for c in 0..128_u8 {
        if test(char::from(c)) {
            bit(&mut mask, u32::from(c));
        }
    }
    Some(mask)
}

/// `firstCharsForNode`.
fn node_first(regex: &Regex, node: Option<usize>) -> First {
    let mut first = First::default();
    let Some(node) = node else {
        first.nullable = true;
        return first;
    };
    let at = &regex.nodes[node];
    match &at.op {
        Op::Symb(c) => add_first(regex, &mut first, *c),
        Op::Word(word) => match word.first() {
            Some(c) => add_first(regex, &mut first, *c),
            None => first.nullable = true,
        },
        Op::Enum(class) => {
            for c in 0..128_u8 {
                if class.contains(char::from(c)) {
                    bit(&mut first.mask, u32::from(c));
                }
            }
        }
        Op::Meta(meta) => match meta_chars(regex, *meta) {
            Some(mask) => first.mask = mask,
            None => first.nullable = true,
        },
        Op::Brackets | Op::NamedBrackets => first = chain_first(regex, at.param),
        Op::Or => {
            first = chain_first(regex, at.param);
            let right = chain_first(regex, at.next);
            first.mask = [first.mask[0] | right.mask[0], first.mask[1] | right.mask[1]];
            first.nullable |= right.nullable;
        }
        Op::RangeN | Op::RangeNM | Op::NGRangeN | Op::NGRangeNM => {
            first = chain_first(regex, at.param);
            first.nullable |= at.s == 0;
        }
        Op::Ahead | Op::NAhead | Op::Behind | Op::NBehind | Op::Empty => first.nullable = true,
        _ => first.mask = ALL,
    }
    first
}

/// `analyzeFirstChars`: a chain's first characters, through its nullable heads.
fn chain_first(regex: &Regex, head: Option<usize>) -> First {
    let mut first = First {
        nullable: true,
        ..First::default()
    };
    let mut node = head;
    while let Some(at) = node.filter(|_| first.nullable) {
        let current = node_first(regex, Some(at));
        first.mask = [
            first.mask[0] | current.mask[0],
            first.mask[1] | current.mask[1],
        ];
        first.nullable = current.nullable;
        if regex.nodes[at].op == Op::Or {
            break;
        }
        node = regex.nodes[at].next;
    }
    first
}

fn start_anchor(regex: &Regex) -> Anchor {
    let mut node = Some(regex.root);
    while let Some(at) =
        node.filter(|&at| matches!(regex.nodes[at].op, Op::Brackets | Op::NamedBrackets))
    {
        node = regex.nodes[at].param;
    }
    match node.map(|at| &regex.nodes[at].op) {
        Some(Op::Meta(Meta::SoL)) if !regex.flags.multi_line => Anchor::LineStart,
        Some(Op::Meta(Meta::SoScheme)) => Anchor::SchemeStart,
        _ => Anchor::None,
    }
}

fn end_anchor(regex: &Regex) -> bool {
    if regex.flags.multi_line {
        return false;
    }
    let mut node = regex.root;
    loop {
        let at = &regex.nodes[node];
        if at.op == Op::Or {
            return false;
        }
        match (at.next, &at.op, at.param) {
            (Some(next), _, _) => node = next,
            (None, Op::Brackets | Op::NamedBrackets, Some(param)) => node = param,
            _ => break,
        }
    }
    regex.nodes[node].op == Op::Meta(Meta::EoL)
}

fn add_bounded(a: Option<i32>, b: Option<i32>) -> Option<i32> {
    a?.checked_add(b?)
}

/// `maxLenOfNode`: none when unbounded.
fn node_max(regex: &Regex, node: usize) -> Option<i32> {
    let at = &regex.nodes[node];
    match &at.op {
        Op::Symb(_) | Op::Enum(_) => Some(1),
        Op::Word(word) => Some(word.len() as i32),
        Op::Meta(Meta::AnyChr) => Some(1),
        Op::Meta(meta) if zero_width(*meta) => Some(0),
        Op::Meta(_) => Some(1),
        Op::Brackets | Op::NamedBrackets => chain_max(regex, at.param),
        Op::RangeNM | Op::NGRangeNM => {
            let inner = chain_max(regex, at.param)?;
            if at.e < 0 {
                return None;
            }
            inner.checked_mul(at.e)
        }
        Op::Ahead | Op::NAhead | Op::Behind | Op::NBehind | Op::Empty => Some(0),
        _ => None,
    }
}

/// `maxLenOfChain`.
fn chain_max(regex: &Regex, head: Option<usize>) -> Option<i32> {
    let mut total = Some(0);
    let mut node = head;
    while let Some(at) = node {
        if regex.nodes[at].op == Op::Or {
            let left = chain_max(regex, regex.nodes[at].param)?;
            let right = chain_max(regex, regex.nodes[at].next)?;
            return add_bounded(total, Some(left.max(right)));
        }
        total = add_bounded(total, node_max(regex, at));
        total?;
        node = regex.nodes[at].next;
    }
    total
}

fn add_required(regex: &Regex, out: &mut Vec<Mask>, c: char) {
    let value = c as u32;
    if value >= 128 || regex.flags.ignore_case && c.is_ascii_alphabetic() {
        return;
    }
    let mut set = [0; 2];
    bit(&mut set, value);
    out.push(set);
}

/// `requiredCharsForNode`.
fn node_required(regex: &Regex, node: usize) -> Vec<Mask> {
    let at = &regex.nodes[node];
    let mut out = Vec::new();
    match &at.op {
        Op::Symb(c) => add_required(regex, &mut out, *c),
        Op::Word(word) => word.iter().for_each(|c| add_required(regex, &mut out, *c)),
        Op::Brackets | Op::NamedBrackets | Op::Ahead | Op::Behind => {
            out = chain_required(regex, at.param);
        }
        Op::RangeN | Op::RangeNM | Op::NGRangeN | Op::NGRangeNM if at.s > 0 => {
            out = chain_required(regex, at.param);
        }
        _ => {}
    }
    out
}

/// `requiredCharsForChain`.
fn chain_required(regex: &Regex, head: Option<usize>) -> Vec<Mask> {
    let mut out = Vec::new();
    let mut node = head;
    while let Some(at) = node {
        if regex.nodes[at].op == Op::Or {
            let left = chain_required(regex, regex.nodes[at].param);
            let right = chain_required(regex, regex.nodes[at].next);
            for l in &left {
                for r in &right {
                    out.push([l[0] | r[0], l[1] | r[1]]);
                }
            }
            if out.len() > REQUIRED_SETS {
                out.sort_by_key(count);
                out.truncate(REQUIRED_SETS);
            }
            break;
        }
        out.extend(node_required(regex, at));
        node = regex.nodes[at].next;
    }
    out
}

/// `analyzeBranchFirstChars`.
fn branch_firsts(regex: &mut Regex, head: Option<usize>) {
    let mut node = head;
    while let Some(at) = node {
        let param = regex.nodes[at].param;
        match regex.nodes[at].op {
            Op::Or => {
                let first = chain_first(regex, param);
                let mut atom = param;
                while let Some(inner) = atom.filter(|&inner| {
                    matches!(regex.nodes[inner].op, Op::Brackets | Op::NamedBrackets)
                }) {
                    atom = regex.nodes[inner].param;
                }
                let leading_nullable = atom.is_none() || node_first(regex, atom).nullable;
                let useful = !leading_nullable && !first.nullable && first.mask != ALL;
                regex.nodes[at].branch_first = useful.then_some(first.mask);
                branch_firsts(regex, param);
            }
            Op::Brackets
            | Op::NamedBrackets
            | Op::Ahead
            | Op::NAhead
            | Op::Behind
            | Op::NBehind
            | Op::RangeN
            | Op::RangeNM
            | Op::NGRangeN
            | Op::NGRangeNM => branch_firsts(regex, param),
            _ => {}
        }
        node = regex.nodes[at].next;
    }
}

/// `canStartWith`.
pub(super) fn can_start_with(regex: &Regex, c: char) -> bool {
    if let Some(mask) = regex.facts.first_chars.filter(|_| c.is_ascii()) {
        return super::has(&mask, c);
    }
    let Some(node) = regex.facts.first_node else {
        return true;
    };
    let ignore = regex.flags.ignore_case;
    let same = |other: char| c == other || ignore && chars::to_lower(c) == chars::to_lower(other);
    match &regex.nodes[node].op {
        Op::Symb(other) => same(*other),
        Op::Word(word) => word.first().is_some_and(|other| same(*other)),
        Op::Enum(class) => class.contains(c),
        Op::Meta(Meta::AnyChr) => regex.flags.single_line || !super::matcher::is_line_break(c),
        Op::Meta(meta) => match meta_chars(regex, *meta) {
            Some(_) => super::matcher::meta_takes(*meta, c),
            None => true,
        },
        _ => true,
    }
}
