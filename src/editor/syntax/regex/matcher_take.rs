//! What the matcher's nodes take (part of `CRegExp::lowParse` and `checkMetaSymbol`): characters,
//! words, classes, meta symbols and back references, each moving past what it took.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use super::super::chars::{self, is_word};
use super::super::{MATCHES, Meta, NAMED, Op};
use super::{Machine, is_line_break, meta_takes};

impl Machine<'_> {
    pub(super) fn word_boundary(&self, at: i32) -> bool {
        let after = at < self.end && is_word(self.at(at));
        let before = at > 0 && is_word(self.at(at - 1));
        before != after
    }

    /// `checkMetaSymbol`: whether the symbol holds at `at`, moving past what it takes; `mark` lets `\m`
    /// and `\M` set the match's bounds.
    pub(super) fn check_meta(&mut self, meta: Meta, at: &mut i32, mark: bool) -> bool {
        let pos = *at;
        let multi = self.rx.flags.multi_line;
        match meta {
            Meta::AnyChr => {
                let ok =
                    pos < self.end && (self.rx.flags.single_line || !is_line_break(self.at(pos)));
                self.took(ok, at)
            }
            Meta::SoL => pos == 0 || multi && is_line_break(self.at(pos - 1)),
            Meta::EoL => {
                pos == self.end
                    || multi && pos != 0 && pos < self.end && is_line_break(self.at(pos - 1))
            }
            Meta::WBound => self.word_boundary(pos),
            Meta::NWBound => !self.word_boundary(pos),
            Meta::PreNW => pos == 0 || pos >= self.end || !chars::is_letter(self.at(pos - 1)),
            Meta::SoScheme => self.scheme_start == pos,
            Meta::Start | Meta::End => {
                if mark {
                    self.mark(meta == Meta::Start, pos);
                }
                true
            }
            _ => {
                let ok = pos < self.end && meta_takes(meta, self.at(pos));
                self.took(ok, at)
            }
        }
    }

    pub(super) fn took(&self, ok: bool, at: &mut i32) -> bool {
        if ok {
            *at += 1;
        }
        ok
    }

    pub(super) fn mark(&mut self, start: bool, pos: i32) {
        if start {
            self.found.s[0] = pos;
            self.start_change = true;
        } else {
            self.found.e[0] = pos;
            self.end_change = true;
        }
    }

    /// A node that takes characters or holds at a position: whether it does, moving past them.
    pub(super) fn consume(&mut self, re: usize) -> bool {
        let rx = self.rx;
        let mut at = self.to_parse;
        let ok = match &rx.nodes[re].op {
            Op::Symb(c) => at < self.end && self.same(self.at(at), *c) && self.took(true, &mut at),
            Op::Meta(meta) => self.check_meta(*meta, &mut at, true),
            Op::Word(word) => self.word(word, &mut at),
            Op::Enum(class) => {
                at < self.end && class.contains(self.at(at)) && self.took(true, &mut at)
            }
            _ => self.back_reference(re, &mut at),
        };
        if ok {
            self.to_parse = at;
        }
        ok
    }

    pub(super) fn word(&self, word: &[char], at: &mut i32) -> bool {
        let len = word.len() as i32;
        if *at + len > self.end {
            return false;
        }
        let from = *at as usize;
        let text = &self.text[from..from + word.len()];
        let ok = if self.rx.flags.ignore_case {
            text.iter()
                .zip(word)
                .all(|(a, b)| chars::to_lower(*a) == chars::to_lower(*b))
        } else {
            text == word
        };
        if ok {
            *at += len;
        }
        ok
    }

    /// `\yN \YN \y{name} \Y{name} \N \p{name}`.
    pub(super) fn back_reference(&self, re: usize, at: &mut i32) -> bool {
        let node = &self.rx.nodes[re];
        let no = node.param0;
        let Ok(index) = usize::try_from(no) else {
            return false;
        };
        match node.op {
            Op::BkTrace | Op::BkTraceN => {
                let Some((text, trace)) = self.back.filter(|_| index < MATCHES) else {
                    return false;
                };
                let range = (trace.s[index], trace.e[index]);
                self.copied(text, range, at, node.op == Op::BkTraceN)
            }
            Op::BkTraceName | Op::BkTraceNName => {
                let Some((text, trace)) = self.back.filter(|_| index < NAMED) else {
                    return false;
                };
                let range = (trace.ns[index], trace.ne[index]);
                if node.op == Op::BkTraceNName
                    && (trace.named <= index || range.0 == -1 || range.1 == -1)
                {
                    return false;
                }
                self.copied(text, range, at, node.op == Op::BkTraceNName)
            }
            Op::BkBrackName if index < self.rx.names.len() => {
                let range = (self.found.ns[index], self.found.ne[index]);
                range.0 != -1 && range.1 != -1 && self.copied(self.text, range, at, false)
            }
            Op::BkBrack if index < self.rx.brackets => {
                let range = (self.found.s[index], self.found.e[index]);
                range.0 != -1 && range.1 != -1 && self.copied(self.text, range, at, false)
            }
            _ => false,
        }
    }

    /// `matchCopiedRange`: `from..to` of `source` at `at` (nothing when it took nothing).
    pub(super) fn copied(
        &self,
        source: &[char],
        (from, to): (i32, i32),
        at: &mut i32,
        icase: bool,
    ) -> bool {
        if from < 0 || to < 0 || from >= to {
            return true;
        }
        let mut pos = *at;
        for i in from..to {
            let Some(&c) = source.get(i as usize) else {
                return false;
            };
            if pos >= self.end {
                return false;
            }
            let here = self.at(pos);
            let same = if icase {
                chars::to_lower(here) == chars::to_lower(c)
            } else {
                here == c
            };
            if !same {
                return false;
            }
            pos += 1;
        }
        *at = pos;
        true
    }
}
