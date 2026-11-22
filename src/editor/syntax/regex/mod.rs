//! Colorer's regular expressions (`cregexp`, ported from Colorer-library; MIT, Copyright (c) 2009 Igor
//! Russkih, 2009 and present Aleksey Dobrunov; ADR-0024): `/pattern/ixsm` with Perl's syntax and
//! Colorer's own - `~` (the scheme's start), `\m` / `\M` (the match's new start and end), `\yN` / `\YN` /
//! `\y{name}` (another expression's bracket: a block's end refers to its start), `pattern?#N` /
//! `pattern?~N` (N characters back), `pattern?=` / `pattern?!`, named brackets `(?{name} ...)` with
//! `\p{name}`, Unicode classes `[{L}{Nd}]` with subtraction and intersection.
//!
//! The matcher is Colorer's backtracking machine as it is (its quirks are the grammars' behaviour),
//! with its prefilters (first characters, anchors, the longest match before `$`, the characters a
//! match needs). It counts characters where Colorer counts UTF-16 units.

mod analyze;
mod chars;
mod compile;
mod matcher;

pub use chars::{CharClass, char_class, is_word, to_lower};

/// Numbered brackets kept (Colorer's `MATCHES_NUM`): bracket 0 is the whole match.
pub const MATCHES: usize = 16;
/// Named brackets kept (`NAMED_MATCHES_NUM`).
pub const NAMED: usize = 16;

/// A bit per ASCII character (Colorer's `AsciiCharMask`).
pub type Mask = [u64; 2];

/// Why a pattern did not compile (`EError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Empty, or `\y{name}` with no expression to name.
    Failed,
    /// Not `/.../`, a bad escape.
    Syntax,
    /// Brackets not paired.
    Brackets,
    /// A bad class `[...]`.
    Enum,
    /// An operator with nothing before it, a bad `{n,m}`.
    Op,
}

/// What a match found (`SMatches`): each bracket's start and end, -1 where it took nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matches {
    pub s: [i32; MATCHES],
    pub e: [i32; MATCHES],
    /// How many numbered brackets the expression has.
    pub count: usize,
    pub ns: [i32; NAMED],
    pub ne: [i32; NAMED],
    /// How many named ones.
    pub named: usize,
}

impl Default for Matches {
    fn default() -> Self {
        Matches {
            s: [-1; MATCHES],
            e: [-1; MATCHES],
            count: 0,
            ns: [-1; NAMED],
            ne: [-1; NAMED],
            named: 0,
        }
    }
}

impl Matches {
    fn reset(&mut self, count: usize, named: usize) {
        *self = Matches {
            count,
            named,
            ..Matches::default()
        };
    }
}

/// A special character (`EMetaSymbols`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Meta {
    AnyChr,
    SoL,
    SoScheme,
    EoL,
    Digit,
    NDigit,
    WordSymb,
    NWordSymb,
    WSpace,
    NWSpace,
    UCase,
    NUCase,
    WBound,
    NWBound,
    PreNW,
    Start,
    End,
}

/// A node's operation (`EOps`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Op {
    Empty,
    Meta(Meta),
    Symb(char),
    Word(Vec<char>),
    Enum(Box<CharClass>),
    Brackets,
    NamedBrackets,
    BkTrace,
    BkTraceN,
    BkTraceName,
    BkTraceNName,
    BkBrack,
    BkBrackName,
    RangeN,
    RangeNM,
    NGRangeN,
    NGRangeNM,
    Or,
    Behind,
    NBehind,
    Ahead,
    NAhead,
}

impl Op {
    /// An operator over what stands before it (between `ReBlockOps` and `ReSymbolOps`).
    fn is_postfix(&self) -> bool {
        matches!(
            self,
            Op::RangeN
                | Op::RangeNM
                | Op::NGRangeN
                | Op::NGRangeNM
                | Op::Or
                | Op::Behind
                | Op::NBehind
                | Op::Ahead
                | Op::NAhead
        )
    }
}

/// A node of the tree (`SRegInfo`), its links as indices; what the matcher changes in it is kept
/// apart ([`matcher`]).
#[derive(Clone, Debug)]
pub(super) struct Node {
    op: Op,
    parent: Option<usize>,
    next: Option<usize>,
    prev: Option<usize>,
    /// The bracket's, the operator's or the look-around's own chain.
    param: Option<usize>,
    /// A bracket's number (-1: none kept), a back reference's, a look-behind's characters.
    param0: i32,
    /// A range's least and most (-1: no most).
    s: i32,
    e: i32,
    /// The first ASCII characters of an alternation's left branch, when they tell.
    branch_first: Option<Mask>,
}

impl Node {
    fn new(parent: Option<usize>) -> Node {
        Node {
            op: Op::Empty,
            parent,
            next: None,
            prev: None,
            param: None,
            param0: 0,
            s: 0,
            e: 0,
            branch_first: None,
        }
    }
}

/// The flags after the closing `/`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Flags {
    ignore_case: bool,
    extend: bool,
    single_line: bool,
    multi_line: bool,
}

/// How a pattern may only start (`StartAnchor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum Anchor {
    #[default]
    None,
    LineStart,
    SchemeStart,
}

/// The prefilters `optimize` finds.
#[derive(Clone, Debug, Default)]
pub(super) struct Facts {
    first_node: Option<usize>,
    first_chars: Option<Mask>,
    start: Anchor,
    end_anchor: bool,
    /// The most characters a match takes, none when unbounded.
    max_len: Option<i32>,
    required: Vec<Mask>,
}

/// A compiled expression (`CRegExp`).
#[derive(Clone, Debug)]
pub struct Regex {
    nodes: Vec<Node>,
    root: usize,
    flags: Flags,
    facts: Facts,
    brackets: usize,
    names: Vec<Vec<char>>,
    uses_back_trace: bool,
    step_limit: u32,
    /// Whether a parse that does not say moves on from its position (`positionMoves`).
    moves: bool,
}

/// Where and how [`Regex::parse`] looks.
#[derive(Clone, Copy, Debug, Default)]
pub struct Parse<'a> {
    /// The first position tried.
    pub pos: usize,
    /// The end (`$`); the text's length when `None`.
    pub eol: Option<usize>,
    /// Where `~` matches: the scheme's start.
    pub scheme_start: usize,
    /// On from `pos` to the end (`positionMoves`), or at `pos` only.
    pub moves: bool,
    /// The ASCII characters of the whole line ([`ascii_chars`]): a pattern needing others fails at
    /// once.
    pub line_chars: Option<&'a Mask>,
    /// The text and the matches `\y` refers to (a block's start).
    pub back: Option<(&'a [char], &'a Matches)>,
}

/// Every ASCII character of `text` (`collectAsciiChars`).
pub fn ascii_chars(text: &[char]) -> Mask {
    let mut mask = [0; 2];
    for &c in text {
        let c = c as u32;
        if c < 128 {
            mask[(c >> 6) as usize] |= 1 << (c & 63);
        }
    }
    mask
}

fn has(mask: &Mask, c: char) -> bool {
    let c = c as u32;
    c < 128 && mask[(c >> 6) as usize] & (1 << (c & 63)) != 0
}

impl Regex {
    /// `/pattern/flags` compiled.
    pub fn new(pattern: &str) -> Result<Regex, Error> {
        Regex::with_back(pattern, None)
    }

    /// Compiled with `back` the expression whose named brackets `\y{name}` refer to.
    pub fn with_back(pattern: &str, back: Option<&Regex>) -> Result<Regex, Error> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut regex = compile::compile(&chars, back)?;
        analyze::optimize(&mut regex);
        Ok(regex)
    }

    /// A named bracket's number (names compared without case).
    pub fn bracket_no(&self, name: &[char]) -> Option<usize> {
        self.names.iter().position(|known| {
            known.len() == name.len()
                && known
                    .iter()
                    .zip(name)
                    .all(|(a, b)| chars::to_lower(*a) == chars::to_lower(*b))
        })
    }

    /// A named bracket's name.
    pub fn bracket_name(&self, no: usize) -> Option<String> {
        self.names.get(no).map(|name| name.iter().collect())
    }

    /// Whether it refers to another expression's brackets (`\y`, `\Y`).
    pub fn has_back_trace(&self) -> bool {
        self.uses_back_trace
    }

    /// Whether its parses move on from their position unless told (`setPositionMoves`).
    pub fn set_position_moves(&mut self, moves: bool) {
        self.moves = moves;
    }

    pub fn position_moves(&self) -> bool {
        self.moves
    }

    /// The most backtracking steps one parse takes before it fails (1 000 000).
    #[cfg(test)]
    pub fn set_step_limit(&mut self, limit: u32) {
        self.step_limit = limit.max(1);
    }

    /// Whether a match may start with `ch` (`canStartWith`).
    pub fn can_start_with(&self, ch: char) -> bool {
        analyze::can_start_with(self, ch)
    }

    /// Colorer's cheap check before [`parse`](Self::parse) (`mayMatch`) with the same `how`: false means
    /// it cannot match.
    pub fn may_match(&self, text_len: usize, how: &Parse, line: &Mask) -> bool {
        let facts = &self.facts;
        match facts.start {
            Anchor::LineStart if how.pos != 0 => return false,
            Anchor::SchemeStart if how.pos != how.scheme_start => return false,
            _ => {}
        }
        let eol = how.eol.unwrap_or(text_len);
        if let (true, Some(max), false) = (facts.end_anchor, facts.max_len, how.moves)
            && eol.saturating_sub(how.pos) > max as usize
        {
            return false;
        }
        facts
            .required
            .iter()
            .all(|set| (set[0] & line[0]) | (set[1] & line[1]) != 0)
    }

    /// Runs the expression over `text` as `how` says; true with `found` filled when it matched.
    pub fn parse(&self, text: &[char], found: &mut Matches, how: Parse) -> bool {
        matcher::parse(self, text, found, how).0
    }

    /// As [`parse`](Self::parse), also telling whether it gave up at its step limit.
    #[cfg(test)]
    pub fn parse_limited(&self, text: &[char], found: &mut Matches, how: Parse) -> (bool, bool) {
        matcher::parse(self, text, found, how)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_colorer;
