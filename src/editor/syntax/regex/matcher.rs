//! Colorer's matcher as it is (`CRegExp::lowParse` and `parseRE`): it walks the tree from node to node -
//! into a bracket's or an operator's chain, out to the parent at a chain's end - and keeps a frame for
//! each choice (an alternation's other branch, one more or one fewer repetition, a look-around); a
//! failure goes back to the last frame and its action says what next. The counters of the repetitions
//! live beside the tree, as Colorer keeps them in its nodes, and are not put back on a backtrack.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use std::cell::RefCell;

#[path = "matcher_take.rs"]
mod take;

use super::chars::{self, is_word};
use super::{Anchor, Matches, Meta, Op, Parse, Regex, has};

/// What a frame does when what it tried came out true or false (`ReAction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    None,
    False,
    True,
    Break,
    RangeNMStep2,
    RangeNMStep3,
    RangeNStep2,
    NGRangeNStep2,
    NGRangeNMStep2,
    NGRangeNMStep3,
}

/// A choice kept to come back to (`StackElem`).
#[derive(Clone, Copy, Debug)]
struct Frame {
    re: Option<usize>,
    prev: Option<usize>,
    to_parse: i32,
    left_enter: bool,
    if_true: Action,
    if_false: Action,
}

/// What the matcher changes in a node: a bracket's start, a repetition's counters.
#[derive(Clone, Copy, Debug, Default)]
struct State {
    start: i32,
    count0: i32,
    count1: i32,
    old_parse: i32,
}

/// The frames and the nodes' states, kept for the thread (Colorer's `RegExpStack`).
#[derive(Default)]
struct Scratch {
    stack: Vec<Frame>,
    states: Vec<State>,
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::default());
}

/// After a node: on to the next, back to the loop's top, or done.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Advance,
    Continue,
    Return(bool),
}

/// Colorer's line breaks (`isLineBreak`).
pub(super) fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// Whether a meta symbol that takes a character takes `c`.
pub(super) fn meta_takes(meta: Meta, c: char) -> bool {
    match meta {
        Meta::Digit => chars::is_digit(c),
        Meta::NDigit => !chars::is_digit(c),
        Meta::WordSymb => is_word(c),
        Meta::NWordSymb => !is_word(c),
        Meta::WSpace => chars::is_whitespace(c),
        Meta::NWSpace => !chars::is_whitespace(c),
        Meta::UCase => chars::is_upper(c),
        Meta::NUCase => chars::is_lower(c),
        _ => true,
    }
}

struct Machine<'a> {
    rx: &'a Regex,
    text: &'a [char],
    end: i32,
    found: &'a mut Matches,
    stack: &'a mut Vec<Frame>,
    states: &'a mut [State],
    back: Option<(&'a [char], &'a Matches)>,
    scheme_start: i32,
    re: Option<usize>,
    prev: Option<usize>,
    to_parse: i32,
    left_enter: bool,
    action: Action,
    steps: u32,
    exceeded: bool,
    start_change: bool,
    end_change: bool,
}

/// `parse`: whether it matched and whether it gave up at its step limit.
pub(super) fn parse(rx: &Regex, text: &[char], found: &mut Matches, how: Parse) -> (bool, bool) {
    SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        let Scratch { stack, states } = &mut *scratch;
        if states.len() < rx.nodes.len() {
            states.resize(rx.nodes.len(), State::default());
        }
        let len = text.len();
        let mut machine = Machine {
            rx,
            text,
            end: how.eol.unwrap_or(len).min(len) as i32,
            found,
            stack,
            states,
            back: how.back,
            scheme_start: how.scheme_start as i32,
            re: None,
            prev: None,
            to_parse: 0,
            left_enter: true,
            action: Action::None,
            steps: 0,
            exceeded: false,
            start_change: false,
            end_change: false,
        };
        let matched = machine.search(&how);
        (matched, machine.exceeded)
    })
}

impl Machine<'_> {
    /// The text's character at `i`; a NUL out of it.
    fn at(&self, i: i32) -> char {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.text.get(i))
            .copied()
            .unwrap_or('\0')
    }

    fn same(&self, a: char, b: char) -> bool {
        a == b || self.rx.flags.ignore_case && chars::to_lower(a) == chars::to_lower(b)
    }

    /// `parseRE`: the prefilters, then a match tried at each position from `how.pos` (one only unless
    /// it moves).
    fn search(&mut self, how: &Parse) -> bool {
        let facts = &self.rx.facts;
        let mut pos = how.pos as i32;
        if let Some(line) = how.line_chars
            && !facts
                .required
                .iter()
                .all(|set| (set[0] & line[0]) | (set[1] & line[1]) != 0)
        {
            return false;
        }
        let anchored = facts.start != Anchor::None;
        match facts.start {
            Anchor::LineStart if pos != 0 => return false,
            Anchor::SchemeStart if pos != self.scheme_start => return false,
            _ => {}
        }
        if let (true, Some(max)) = (facts.end_anchor, facts.max_len) {
            if how.moves && !anchored && self.end - max > pos {
                pos = self.end - max;
            }
            if self.end - pos > max {
                return false;
            }
        }
        if !how.moves && !self.may_start(pos) {
            return false;
        }
        self.found.reset(self.rx.brackets, self.rx.names.len());
        self.positions(pos, how.moves, anchored)
    }

    /// The match tried at `pos` and on (when it moves).
    fn positions(&mut self, mut pos: i32, moves: bool, anchored: bool) -> bool {
        loop {
            if moves && !self.may_start(pos) {
                if anchored {
                    return false;
                }
            } else {
                self.start_change = false;
                self.end_change = false;
                self.found.reset(self.rx.brackets, self.rx.names.len());
                if self.run(pos) {
                    return true;
                }
                if self.exceeded || !moves || anchored {
                    return false;
                }
            }
            pos += 1;
            if pos > self.end {
                return false;
            }
        }
    }

    /// The first characters and the first node allow a match at `pos` (`quickCheck`).
    fn may_start(&mut self, pos: i32) -> bool {
        let facts = &self.rx.facts;
        if let Some(mask) = facts.first_chars {
            if pos >= self.end {
                return false;
            }
            let c = self.at(pos);
            if c.is_ascii() && !has(&mask, c) {
                return false;
            }
        }
        let Some(node) = facts.first_node else {
            return true;
        };
        let taken = pos < self.end;
        match &self.rx.nodes[node].op {
            Op::Symb(c) => taken && self.same(self.at(pos), *c),
            Op::Word(word) => taken && word.first().is_some_and(|c| self.same(self.at(pos), *c)),
            Op::Enum(class) => taken && class.contains(self.at(pos)),
            Op::Meta(Meta::Start | Meta::End) => true,
            Op::Meta(meta) => {
                let mut at = pos;
                self.check_meta(*meta, &mut at, false)
            }
            _ => true,
        }
    }

    fn check_stack(&mut self, res: bool) {
        match self.stack.pop() {
            None => self.action = if res { Action::True } else { Action::False },
            Some(frame) => {
                self.action = if res { frame.if_true } else { frame.if_false };
                self.re = frame.re;
                self.prev = frame.prev;
                self.to_parse = frame.to_parse;
                self.left_enter = frame.left_enter;
            }
        }
    }

    fn insert_stack(
        &mut self,
        (if_true, if_false): (Action, Action),
        re: Option<usize>,
        prev: Option<usize>,
        to_parse: i32,
    ) {
        self.stack.push(Frame {
            re: self.re,
            prev: self.prev,
            to_parse: self.to_parse,
            left_enter: self.left_enter,
            if_true,
            if_false,
        });
        self.prev = prev;
        self.re = re;
        self.to_parse = to_parse;
        self.left_enter = true;
        if self.re.is_none()
            && let Some(prev) = self.prev
        {
            self.re = self.rx.nodes[prev].parent;
            self.left_enter = false;
        }
    }

    fn fail(&mut self) -> Flow {
        self.check_stack(false);
        Flow::Continue
    }

    /// `lowParse` from the root at `to_parse`.
    fn run(&mut self, to_parse: i32) -> bool {
        self.stack.clear();
        self.re = Some(self.rx.root);
        self.prev = None;
        self.to_parse = to_parse;
        self.left_enter = true;
        self.action = Action::None;
        loop {
            while self.re.is_some() || self.action != Action::None {
                self.steps += 1;
                if self.steps > self.rx.step_limit {
                    self.exceeded = true;
                    return false;
                }
                let flow = match (self.re, self.action) {
                    (Some(re), Action::None) => self.step(re),
                    _ => Flow::Advance,
                };
                let flow = match flow {
                    Flow::Advance => self.after_action(),
                    other => other,
                };
                match flow {
                    Flow::Return(result) => return result,
                    Flow::Continue => continue,
                    Flow::Advance => {}
                }
                let Some(re) = self.re else {
                    return false;
                };
                let node = &self.rx.nodes[re];
                (self.re, self.left_enter) = match node.next {
                    None => (node.parent, false),
                    Some(next) => (Some(next), true),
                };
            }
            self.check_stack(true);
        }
    }

    /// What the action a frame gave asks.
    fn after_action(&mut self) -> Flow {
        let action = self.action;
        if action == Action::None {
            return Flow::Advance;
        }
        if matches!(action, Action::False | Action::True) {
            if self.stack.is_empty() {
                return Flow::Return(action == Action::True);
            }
            self.check_stack(action == Action::True);
            return Flow::Continue;
        }
        self.action = Action::None;
        if action == Action::Break {
            return Flow::Advance;
        }
        let Some(re) = self.re else {
            return Flow::Return(false);
        };
        let (next, param) = (self.rx.nodes[re].next, self.rx.nodes[re].param);
        let to_parse = self.to_parse;
        match action {
            Action::RangeNStep2 => {
                self.insert_stack((Action::True, Action::False), next, Some(re), to_parse);
            }
            Action::RangeNMStep2 => {
                let then = (Action::True, Action::RangeNMStep3);
                self.insert_stack(then, next, Some(re), to_parse);
            }
            Action::RangeNMStep3 | Action::NGRangeNMStep3 => {
                self.states[re].count1 += 1;
                self.check_stack(false);
            }
            Action::NGRangeNStep2 => {
                if self.states[re].count0 != 0 {
                    self.states[re].count0 -= 1;
                }
                self.enter(param);
            }
            _ => {
                let then = (Action::True, Action::NGRangeNMStep3);
                self.insert_stack(then, param, None, to_parse);
            }
        }
        Flow::Continue
    }

    fn enter(&mut self, chain: Option<usize>) {
        self.re = chain;
        self.left_enter = true;
    }

    /// One node, its action none.
    fn step(&mut self, re: usize) -> Flow {
        match &self.rx.nodes[re].op {
            Op::Empty => Flow::Advance,
            Op::Brackets | Op::NamedBrackets => self.bracket(re),
            Op::Ahead | Op::NAhead | Op::Behind | Op::NBehind => self.look(re),
            Op::Or => self.or(re),
            Op::RangeN | Op::NGRangeN => self.range_n(re),
            Op::RangeNM | Op::NGRangeNM => self.range_nm(re),
            _ => {
                if self.consume(re) {
                    Flow::Advance
                } else {
                    self.fail()
                }
            }
        }
    }

    fn bracket(&mut self, re: usize) -> Flow {
        let node = &self.rx.nodes[re];
        if self.left_enter {
            self.states[re].start = self.to_parse;
            self.re = node.param;
            return Flow::Continue;
        }
        let Ok(no) = usize::try_from(node.param0) else {
            return Flow::Advance;
        };
        let (start, end) = (self.states[re].start, self.to_parse);
        let found = &mut *self.found;
        if node.op == Op::Brackets {
            if no != 0 || !self.start_change {
                found.s[no] = start;
            }
            if no != 0 || !self.end_change {
                found.e[no] = end;
            }
            if found.e[no] < found.s[no] {
                found.s[no] = found.e[no];
            }
        } else {
            found.ns[no] = start;
            found.ne[no] = end;
            if found.ne[no] < found.ns[no] {
                found.ns[no] = found.ne[no];
            }
        }
        Flow::Advance
    }

    fn look(&mut self, re: usize) -> Flow {
        let node = &self.rx.nodes[re];
        if !self.left_enter {
            self.check_stack(true);
            return Flow::Continue;
        }
        let (param, back) = (node.param, self.to_parse - node.param0);
        let to_parse = self.to_parse;
        match node.op {
            Op::Ahead => self.insert_stack((Action::Break, Action::False), param, None, to_parse),
            Op::NAhead => self.insert_stack((Action::False, Action::Break), param, None, to_parse),
            Op::Behind if back < 0 => return self.fail(),
            Op::Behind => self.insert_stack((Action::Break, Action::False), param, None, back),
            _ if back >= 0 => {
                self.insert_stack((Action::False, Action::Break), param, None, back);
            }
            _ => return Flow::Advance,
        }
        Flow::Continue
    }

    fn or(&mut self, mut re: usize) -> Flow {
        if !self.left_enter {
            while let Some(next) = self.rx.nodes[re].next {
                re = next;
            }
            self.re = Some(re);
            return Flow::Advance;
        }
        let node = &self.rx.nodes[re];
        if let Some(mask) = node.branch_first
            && self.to_parse < self.end
        {
            let c = self.at(self.to_parse);
            if c.is_ascii() && !has(&mask, c) {
                return Flow::Advance;
            }
        }
        let (param, to_parse) = (node.param, self.to_parse);
        self.insert_stack((Action::True, Action::Break), param, None, to_parse);
        Flow::Continue
    }

    /// `*`, `+`, `{n,}` and their lazy forms.
    fn range_n(&mut self, re: usize) -> Flow {
        let node = &self.rx.nodes[re];
        let lazy = node.op == Op::NGRangeN;
        let (param, next) = (node.param, node.next);
        if self.left_enter {
            self.states[re].count0 = node.s;
            self.states[re].old_parse = -1;
        }
        let state = &mut self.states[re];
        if state.count0 == 0 && state.old_parse == self.to_parse {
            return Flow::Advance;
        }
        state.old_parse = self.to_parse;
        if state.count0 != 0 {
            state.count0 -= 1;
            self.enter(param);
            return Flow::Continue;
        }
        let to_parse = self.to_parse;
        if lazy {
            self.insert_stack(
                (Action::True, Action::NGRangeNStep2),
                next,
                Some(re),
                to_parse,
            );
        } else {
            self.insert_stack((Action::True, Action::RangeNStep2), param, None, to_parse);
        }
        Flow::Continue
    }

    /// `?`, `{n,m}` and their lazy forms.
    fn range_nm(&mut self, re: usize) -> Flow {
        let node = &self.rx.nodes[re];
        let lazy = node.op == Op::NGRangeNM;
        let (param, next) = (node.param, node.next);
        if self.left_enter {
            let state = &mut self.states[re];
            state.count0 = node.s;
            state.count1 = node.e - node.s;
            state.old_parse = -1;
        }
        let state = &mut self.states[re];
        if state.count0 != 0 {
            state.count0 -= 1;
            self.enter(param);
            return Flow::Continue;
        }
        let to_parse = self.to_parse;
        if state.count1 == 0 {
            self.insert_stack((Action::True, Action::False), next, Some(re), to_parse);
            return Flow::Continue;
        }
        state.count1 -= 1;
        if lazy {
            let then = (Action::True, Action::NGRangeNMStep2);
            self.insert_stack(then, next, Some(re), to_parse);
        } else {
            self.insert_stack((Action::True, Action::RangeNMStep2), param, None, to_parse);
        }
        Flow::Continue
    }
}
