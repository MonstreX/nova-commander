//! The pattern into its tree (`CRegExp::setRELow` and `setStructs`): atoms in a chain, a postfix operator
//! taking the node before it (an alternation the whole chain before it), runs of characters joined into
//! words, empty alternatives added.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use super::chars::{self, BAD, at};
use super::{Error, Facts, Flags, Meta, NAMED, Node, Op, Regex};

/// The tree being built.
struct Compiler<'a> {
    nodes: Vec<Node>,
    expr: &'a [char],
    flags: Flags,
    brackets: usize,
    names: Vec<Vec<char>>,
    back: Option<&'a Regex>,
    uses_back_trace: bool,
}

/// `/body/flags` compiled into a tree (not yet optimized).
pub(super) fn compile(expr: &[char], back: Option<&Regex>) -> Result<Regex, Error> {
    if expr.is_empty() {
        return Err(Error::Failed);
    }
    let start = expr
        .iter()
        .position(|&c| !chars::is_whitespace(c))
        .ok_or(Error::Syntax)?;
    if expr[start] != '/' {
        return Err(Error::Syntax);
    }
    let start = start + 1;
    let close = (start..expr.len())
        .rev()
        .find(|&i| expr[i] == '/')
        .ok_or(Error::Syntax)?;
    let mut flags = Flags::default();
    for &c in &expr[close + 1..] {
        match c {
            'i' => flags.ignore_case = true,
            'x' => flags.extend = true,
            's' => flags.single_line = true,
            'm' => flags.multi_line = true,
            _ => {}
        }
    }
    let body = &expr[start..close];
    let mut compiler = Compiler {
        nodes: Vec::new(),
        expr: body,
        flags,
        brackets: 1,
        names: Vec::new(),
        back,
        uses_back_trace: false,
    };
    let root = compiler.add(None);
    compiler.nodes[root].op = Op::Brackets;
    compiler.nodes[root].param0 = 0;
    let head = compiler.add(Some(root));
    let len = body.len() as i32;
    let (head, end) = compiler.structs(head, 0, len)?;
    compiler.nodes[root].param = Some(head);
    if end != len {
        return Err(Error::Brackets);
    }
    Ok(Regex {
        nodes: compiler.nodes,
        root,
        flags,
        facts: Facts::default(),
        brackets: compiler.brackets,
        names: compiler.names,
        uses_back_trace: compiler.uses_back_trace,
        step_limit: 1_000_000,
        moves: false,
    })
}

impl Compiler<'_> {
    fn add(&mut self, parent: Option<usize>) -> usize {
        self.nodes.push(Node::new(parent));
        self.nodes.len() - 1
    }

    fn at(&self, i: i32) -> char {
        at(self.expr, i)
    }

    fn set(&mut self, node: usize, op: Op) {
        self.nodes[node].op = op;
    }

    /// The chain of `from..to` from `head` (`setStructs`): its new head and where it stopped (at a `)`,
    /// relative to `from`; `to - from` at the end).
    fn structs(&mut self, head: usize, from: i32, to: i32) -> Result<(usize, i32), Error> {
        if from >= to {
            return Ok((head, 0));
        }
        let mut ret = -1;
        let mut next = head;
        let mut i = from;
        while i < to {
            let c = self.at(i);
            if self.flags.extend && chars::is_whitespace(c) {
                i += 1;
                continue;
            }
            if c == ')' {
                ret = i - from;
                break;
            }
            if i != from {
                let node = self.add(self.nodes[next].parent);
                self.nodes[next].next = Some(node);
                self.nodes[node].prev = Some(next);
                next = node;
            }
            i = self.atom(next, i, to)? + 1;
        }
        self.join_words_and_alternatives(head);
        let head = self.operators(head)?;
        Ok((head, if ret == -1 { to - from } else { ret }))
    }

    /// One atom or operator at `i` into `node`: the position of its last character.
    fn atom(&mut self, node: usize, i: i32, to: i32) -> Result<i32, Error> {
        let c = self.at(i);
        let meta = match c {
            '\\' => return self.escape(node, i, to),
            '.' => Some(Meta::AnyChr),
            '^' => Some(Meta::SoL),
            '$' => Some(Meta::EoL),
            '~' => Some(Meta::SoScheme),
            _ => None,
        };
        if let Some(meta) = meta {
            self.set(node, Op::Meta(meta));
            return Ok(i);
        }
        self.nodes[node].param = None;
        self.nodes[node].param0 = 0;
        if let Some(end) = self.postfix(node, i, to) {
            return Ok(end);
        }
        match c {
            '{' => self.range(node, i, to),
            '(' => self.bracket(node, i, to),
            '[' => {
                let (class, end) =
                    chars::char_class(self.expr, i, self.flags.ignore_case).ok_or(Error::Enum)?;
                self.set(node, Op::Enum(Box::new(class)));
                Ok(end)
            }
            ']' | '}' => Err(Error::Brackets),
            _ => {
                self.set(node, Op::Symb(c));
                Ok(i)
            }
        }
    }

    /// `?#N ?~N *? +? ?= ?! ?? * + ? |` at `i`: the position of its last character.
    fn postfix(&mut self, node: usize, i: i32, to: i32) -> Option<i32> {
        let (c, c1, c2) = (self.at(i), self.at(i + 1), self.at(i + 2));
        if to > i + 2 && c == '?' && (c1 == '#' || c1 == '~') && c2.is_ascii_digit() {
            self.set(node, if c1 == '#' { Op::Behind } else { Op::NBehind });
            self.nodes[node].param0 = chars::hex(c2);
            return Some(i + 2);
        }
        let two = match (c, c1) {
            _ if to <= i + 1 => None,
            ('*', '?') => Some((Op::NGRangeN, 0, 0)),
            ('+', '?') => Some((Op::NGRangeN, 1, 0)),
            ('?', '=') => Some((Op::Ahead, 0, 0)),
            ('?', '!') => Some((Op::NAhead, 0, 0)),
            ('?', '?') => Some((Op::NGRangeNM, 0, 1)),
            _ => None,
        };
        let (op, s, e, end) = match (two, c) {
            (Some((op, s, e)), _) => (op, s, e, i + 1),
            (None, '*') => (Op::RangeN, 0, 0, i),
            (None, '+') => (Op::RangeN, 1, 0, i),
            (None, '?') => (Op::RangeNM, 0, 1, i),
            (None, '|') => (Op::Or, 0, 0, i),
            _ => return None,
        };
        self.set(node, op);
        if !matches!(self.nodes[node].op, Op::Ahead | Op::NAhead | Op::Or) {
            self.nodes[node].s = s;
            if matches!(self.nodes[node].op, Op::RangeNM | Op::NGRangeNM) {
                self.nodes[node].e = e;
            }
        }
        Some(end)
    }

    /// `{n}`, `{n,}`, `{n,m}` and their lazy `?` at `i`.
    fn range(&mut self, node: usize, i: i32, to: i32) -> Result<i32, Error> {
        let (st, mut en, mut comma, mut lazy) = (i + 1, -1, -1, false);
        let mut j = i;
        while j < to {
            if to > j + 1 && self.at(j) == '}' && self.at(j + 1) == '?' {
                en = j;
                lazy = true;
                j += 1;
                break;
            }
            if self.at(j) == '}' {
                en = j;
                break;
            }
            if self.at(j) == ',' {
                comma = j;
            }
            j += 1;
        }
        if en == -1 {
            return Err(Error::Brackets);
        }
        if comma == -1 {
            comma = en;
        }
        let s = chars::number(self.expr, st, comma - st);
        let mut e = if comma == en {
            s
        } else {
            chars::number(self.expr, comma + 1, en - comma - 1)
        };
        if s < 0 {
            return Err(Error::Op);
        }
        if en - comma == 1 {
            e = -1;
        } else if e < s {
            return Err(Error::Op);
        }
        let op = match (e == -1, lazy) {
            (true, false) => Op::RangeN,
            (true, true) => Op::NGRangeN,
            (false, false) => Op::RangeNM,
            (false, true) => Op::NGRangeNM,
        };
        self.set(node, op);
        (self.nodes[node].s, self.nodes[node].e) = (s, e);
        Ok(j)
    }

    /// `(...)`, `(?:...)`, `(?{name}...)` at `i`: the position of its `)`.
    fn bracket(&mut self, node: usize, mut i: i32, to: i32) -> Result<i32, Error> {
        if to >= i + 2 && self.at(i + 1) == '?' && self.at(i + 2) == ':' {
            self.set(node, Op::NamedBrackets);
            self.nodes[node].param0 = -1;
            i += 3;
        } else if to > i + 2 && self.at(i + 1) == '?' && self.at(i + 2) == '{' {
            self.set(node, Op::NamedBrackets);
            let name = chars::curly(self.expr, i + 2).ok_or(Error::Brackets)?;
            let len = name.len() as i32;
            self.nodes[node].param0 = if name.is_empty() || self.names.len() >= NAMED {
                -1
            } else {
                self.names.push(name);
                self.names.len() as i32 - 1
            };
            i += len + 4;
        } else {
            self.set(node, Op::Brackets);
            self.nodes[node].param0 = if self.brackets < super::MATCHES {
                self.brackets += 1;
                self.brackets as i32 - 1
            } else {
                -1
            };
            i += 1;
        }
        let inner = self.add(Some(node));
        let (inner, end) = self.structs(inner, i, to)?;
        self.nodes[node].param = Some(inner);
        if to - i - end == 0 {
            return Err(Error::Brackets);
        }
        Ok(i + end)
    }

    /// An escape at `i`: the position of its last character.
    fn escape(&mut self, node: usize, mut i: i32, to: i32) -> Result<i32, Error> {
        if i + 1 >= to {
            return Err(Error::Syntax);
        }
        let c = self.at(i + 1);
        let meta = match c {
            'd' => Some(Meta::Digit),
            'D' => Some(Meta::NDigit),
            'w' => Some(Meta::WordSymb),
            'W' => Some(Meta::NWordSymb),
            's' => Some(Meta::WSpace),
            'S' => Some(Meta::NWSpace),
            'u' => Some(Meta::UCase),
            'l' => Some(Meta::NUCase),
            'b' => Some(Meta::WBound),
            'B' => Some(Meta::NWBound),
            'c' => Some(Meta::PreNW),
            'm' => Some(Meta::Start),
            'M' => Some(Meta::End),
            _ => None,
        };
        let symbol = match c {
            't' => Some('\t'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            _ => None,
        };
        match (meta, symbol, c) {
            (Some(meta), _, _) => self.set(node, Op::Meta(meta)),
            (_, Some(symbol), _) => self.set(node, Op::Symb(symbol)),
            (_, _, 'y' | 'Y') => i = self.back_trace(node, i, to)?,
            (_, _, 'p') => i = self.named_back(node, i, to)?,
            _ => i = self.numbered_back(node, i)?,
        }
        Ok(i + 1)
    }

    /// `\yN \YN \y{name} \Y{name}` at `i`: the position before its last character.
    fn back_trace(&mut self, node: usize, i: i32, to: i32) -> Result<i32, Error> {
        self.uses_back_trace = true;
        let exact = self.at(i + 1) == 'y';
        self.set(node, if exact { Op::BkTrace } else { Op::BkTraceN });
        if i + 2 >= to {
            return Err(Error::Syntax);
        }
        let no = chars::hex(self.at(i + 2));
        self.nodes[node].param0 = no;
        if no != -1 {
            return Ok(i + 1);
        }
        self.set(
            node,
            if exact {
                Op::BkTraceName
            } else {
                Op::BkTraceNName
            },
        );
        let name = chars::curly(self.expr, i + 2).ok_or(Error::Syntax)?;
        let back = self.back.ok_or(Error::Failed)?;
        let no = back.bracket_no(&name).ok_or(Error::Syntax)?;
        self.nodes[node].param0 = no as i32;
        Ok(i + name.len() as i32 + 2)
    }

    /// `\p{name}` at `i`.
    fn named_back(&mut self, node: usize, i: i32, to: i32) -> Result<i32, Error> {
        if i + 2 >= to {
            return Err(Error::Syntax);
        }
        self.set(node, Op::BkBrackName);
        let name = chars::curly(self.expr, i + 2).ok_or(Error::Syntax)?;
        let no = self
            .names
            .iter()
            .position(|known| eq_ignore_case(known, &name))
            .ok_or(Error::Syntax)?;
        self.nodes[node].param0 = no as i32;
        Ok(i + name.len() as i32 + 2)
    }

    /// `\N` (a bracket of this expression) or an escaped character.
    fn numbered_back(&mut self, node: usize, i: i32) -> Result<i32, Error> {
        let no = chars::hex(self.at(i + 1));
        if (0..=9).contains(&no) {
            self.set(node, Op::BkBrack);
            self.nodes[node].param0 = no;
            return Ok(i);
        }
        let (symbol, end) = chars::escaped(self.expr, i);
        if symbol == BAD {
            return Err(Error::Syntax);
        }
        self.set(node, Op::Symb(symbol));
        Ok(end - 1)
    }

    fn op(&self, node: Option<usize>) -> Option<&Op> {
        node.map(|node| &self.nodes[node].op)
    }

    /// Runs of characters into words (not the last before a postfix operator) and alternatives with
    /// nothing on a side given an empty node.
    fn join_words_and_alternatives(&mut self, head: usize) {
        let mut cur = Some(head);
        while let Some(node) = cur {
            if let Some(after) = self.join_word(node) {
                cur = after;
                continue;
            }
            while self.nodes[node].op == Op::Or && self.add_empty_alternative(node) {}
            cur = self.nodes[node].next;
        }
    }

    /// The characters from `node` made one word: the node after it, or none when there was no run.
    fn join_word(&mut self, node: usize) -> Option<Option<usize>> {
        let mut size: i32 = 0;
        let mut end = Some(node);
        while let Some(at) = end.filter(|&at| matches!(self.nodes[at].op, Op::Symb(_))) {
            end = self.nodes[at].next;
            size += 1;
        }
        if let Some(at) = end.filter(|&at| self.nodes[at].op.is_postfix()) {
            size -= 1;
            end = self.nodes[at].prev;
        }
        if size <= 1 {
            return None;
        }
        let mut word = Vec::with_capacity(size as usize);
        let mut at = Some(node);
        for _ in 0..size {
            let cur = at.unwrap_or(node);
            if let Op::Symb(c) = self.nodes[cur].op {
                word.push(c);
            }
            at = self.nodes[cur].next;
            self.nodes[cur].next = None;
        }
        self.nodes[node].op = Op::Word(word);
        self.nodes[node].next = end;
        if let Some(after) = end {
            self.nodes[after].prev = Some(node);
        }
        Some(end)
    }

    /// One empty alternative beside the alternation `node` where a side has nothing; false when none
    /// is needed.
    fn add_empty_alternative(&mut self, node: usize) -> bool {
        let (prev, next) = (self.nodes[node].prev, self.nodes[node].next);
        let next_op = self.op(next).cloned();
        let empty = self.add(self.nodes[node].parent);
        match (prev, next, next_op) {
            (None, _, _) => {
                self.nodes[empty].next = Some(node);
                self.nodes[node].prev = Some(empty);
            }
            (_, None, _) => {
                self.nodes[empty].prev = Some(node);
                self.nodes[node].next = Some(empty);
            }
            (_, Some(next), Some(op)) if op == Op::Or || op.is_postfix() => {
                self.nodes[empty].prev = Some(node);
                self.nodes[empty].next = Some(next);
                self.nodes[next].prev = Some(empty);
                self.nodes[node].next = Some(empty);
            }
            _ => {
                self.nodes.pop();
                return false;
            }
        }
        true
    }

    /// Each postfix operator given the node before it (an alternation the chain before it back to the
    /// previous alternation): the chain's new head.
    fn operators(&mut self, mut head: usize) -> Result<usize, Error> {
        let mut cur = Some(head);
        while let Some(node) = cur {
            if self.nodes[node].op.is_postfix() {
                let mut first = self.nodes[node].prev.ok_or(Error::Op)?;
                self.nodes[first].next = None;
                self.nodes[first].parent = Some(node);
                let or = self.nodes[node].op == Op::Or;
                while let Some(prev) = self.nodes[first].prev.filter(|_| or)
                    && self.nodes[prev].op != Op::Or
                {
                    self.nodes[first].parent = Some(node);
                    first = prev;
                }
                self.nodes[node].param = Some(first);
                match self.nodes[first].prev {
                    None => {
                        head = node;
                        self.nodes[node].prev = None;
                    }
                    Some(before) => {
                        self.nodes[node].prev = Some(before);
                        self.nodes[before].next = Some(node);
                    }
                }
                self.nodes[first].prev = None;
            }
            cur = self.nodes[node].next;
        }
        Ok(head)
    }
}

fn eq_ignore_case(a: &[char], b: &[char]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| chars::to_lower(*x) == chars::to_lower(*y))
}
