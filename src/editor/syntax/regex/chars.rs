//! Colorer's characters (`strings/icu/Character.h`, `UnicodeTools.cpp`, `UStr::createCharClass`; Colorer
//! is MIT, Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov): its character tests, its
//! escapes and its classes `[...]` with Unicode categories `{L}`, subtraction `-[...]` and intersection
//! `&&[...]`. Colorer counts UTF-16 units; this counts characters (a difference only past the BMP).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Colorer's `BAD_WCHAR`: no character (out of the string, a bad escape).
pub const BAD: char = '\u{FFFF}';
/// ICU's `UnicodeSet::MAX_VALUE`.
const MAX: u32 = 0x10_FFFF;

/// The character at `i` of `s`, or [`BAD`] out of it (ICU's `charAt`).
pub fn at(s: &[char], i: i32) -> char {
    usize::try_from(i)
        .ok()
        .and_then(|i| s.get(i))
        .copied()
        .unwrap_or(BAD)
}

/// A set of characters as sorted, disjoint, inclusive ranges (ICU's `UnicodeSet`), with a bit per ASCII
/// character for the common case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CharClass {
    ranges: Vec<(u32, u32)>,
    ascii: [u64; 2],
}

impl CharClass {
    fn add_range(&mut self, lo: u32, hi: u32) {
        if lo <= hi {
            self.ranges.push((lo, hi.min(MAX)));
            self.normalize();
        }
    }

    pub fn add(&mut self, c: char) {
        self.add_range(c as u32, c as u32);
    }

    pub fn add_all(&mut self, other: &CharClass) {
        self.ranges.extend_from_slice(&other.ranges);
        self.normalize();
    }

    fn normalize(&mut self) {
        self.ranges.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.ranges.len());
        for &(lo, hi) in &self.ranges {
            match merged.last_mut() {
                Some(last) if lo <= last.1.saturating_add(1) => last.1 = last.1.max(hi),
                _ => merged.push((lo, hi)),
            }
        }
        self.ranges = merged;
        self.freeze();
    }

    pub fn complement(&mut self) {
        let mut out = Vec::with_capacity(self.ranges.len() + 1);
        let mut from = 0;
        for &(lo, hi) in &self.ranges {
            if lo > from {
                out.push((from, lo - 1));
            }
            from = hi + 1;
        }
        if from <= MAX {
            out.push((from, MAX));
        }
        self.ranges = out;
        self.freeze();
    }

    pub fn retain_all(&mut self, other: &CharClass) {
        let mut out = Vec::new();
        let (mut i, mut j) = (0, 0);
        while i < self.ranges.len() && j < other.ranges.len() {
            let (a, b) = (self.ranges[i], other.ranges[j]);
            let (lo, hi) = (a.0.max(b.0), a.1.min(b.1));
            if lo <= hi {
                out.push((lo, hi));
            }
            if a.1 < b.1 {
                i += 1;
            } else {
                j += 1;
            }
        }
        self.ranges = out;
        self.freeze();
    }

    pub fn remove_all(&mut self, other: &CharClass) {
        let mut rest = other.clone();
        rest.complement();
        self.retain_all(&rest);
    }

    fn freeze(&mut self) {
        self.ascii = [0; 2];
        for &(lo, hi) in &self.ranges {
            for c in lo..=hi.min(127) {
                self.ascii[(c >> 6) as usize] |= 1 << (c & 63);
            }
        }
    }

    pub fn contains(&self, c: char) -> bool {
        let c = c as u32;
        if c < 128 {
            return self.ascii[(c >> 6) as usize] & (1 << (c & 63)) != 0;
        }
        let at = self.ranges.partition_point(|&(_, hi)| hi < c);
        self.ranges.get(at).is_some_and(|&(lo, _)| lo <= c)
    }

    fn everything() -> CharClass {
        let mut all = CharClass::default();
        all.add_range(0, MAX);
        all
    }
}

/// A Unicode property as ICU's `\p{name}` (Colorer's `{name}`), from `regex-syntax`'s tables; none
/// for a name it does not know.
pub fn property(name: &str) -> Option<CharClass> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<CharClass>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache
        .entry(name.to_string())
        .or_insert_with(|| {
            let hir = regex_syntax::parse(&format!("\\p{{{name}}}")).ok()?;
            let regex_syntax::hir::HirKind::Class(regex_syntax::hir::Class::Unicode(class)) =
                hir.kind()
            else {
                return None;
            };
            let mut set = CharClass::default();
            for range in class.ranges() {
                set.ranges.push((range.start() as u32, range.end() as u32));
            }
            set.normalize();
            Some(set)
        })
        .clone()
}

fn category(name: &'static str, cell: &'static OnceLock<CharClass>) -> &'static CharClass {
    cell.get_or_init(|| property(name).unwrap_or_default())
}

fn letters() -> &'static CharClass {
    static CELL: OnceLock<CharClass> = OnceLock::new();
    category("L", &CELL)
}

fn digits() -> &'static CharClass {
    static CELL: OnceLock<CharClass> = OnceLock::new();
    category("Nd", &CELL)
}

fn separators() -> &'static CharClass {
    static CELL: OnceLock<CharClass> = OnceLock::new();
    category("Z", &CELL)
}

fn uppers() -> &'static CharClass {
    static CELL: OnceLock<CharClass> = OnceLock::new();
    category("Lu", &CELL)
}

fn lowers() -> &'static CharClass {
    static CELL: OnceLock<CharClass> = OnceLock::new();
    category("Ll", &CELL)
}

/// ICU's `u_isspace` behind Colorer's ASCII fast path.
pub fn is_whitespace(c: char) -> bool {
    if c.is_ascii() {
        return c == ' ' || ('\t'..='\r').contains(&c);
    }
    separators().contains(c)
}

pub fn is_lower(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_lowercase();
    }
    lowers().contains(c)
}

pub fn is_upper(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_uppercase();
    }
    uppers().contains(c)
}

pub fn is_letter(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphabetic();
    }
    letters().contains(c)
}

pub fn is_digit(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    digits().contains(c)
}

pub fn is_word(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_';
    }
    digits().contains(c) || letters().contains(c)
}

/// ICU's simple `u_tolower`: one character for one.
pub fn to_lower(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    single(c.to_lowercase()).unwrap_or(c)
}

fn to_upper(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_uppercase();
    }
    single(c.to_uppercase()).unwrap_or(c)
}

fn single(mut chars: impl Iterator<Item = char>) -> Option<char> {
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

/// `UnicodeTools::getHex`: a hexadecimal digit's value, -1 for anything else.
pub fn hex(c: char) -> i32 {
    c.to_digit(16).map_or(-1, |d| d as i32)
}

/// `UnicodeTools::getNumber`: decimal digits from `from`, `len` of them; -1 if any is not one.
pub fn number(s: &[char], from: i32, len: i32) -> i32 {
    let mut value: i32 = 0;
    for i in from..from + len {
        let Some(d) = at(s, i).to_digit(10) else {
            return -1;
        };
        value = value.wrapping_mul(10).wrapping_add(d as i32);
    }
    value
}

fn hex_number(s: &[char]) -> i32 {
    let mut value: i32 = 0;
    for &c in s {
        let d = hex(c);
        if d < 0 {
            return -1;
        }
        value = value.wrapping_shl(4).wrapping_add(d);
    }
    value
}

/// `UnicodeTools::getCurlyContent`: what `{...}` at `pos` holds (printable characters only).
pub fn curly(s: &[char], pos: i32) -> Option<Vec<char>> {
    if at(s, pos) != '{' {
        return None;
    }
    let from = (pos + 1) as usize;
    for (end, &c) in s.iter().enumerate().skip(from) {
        if c == '}' {
            return Some(s[from..end].to_vec());
        }
        if c.is_whitespace() || c.is_control() {
            return None;
        }
    }
    None
}

/// `UnicodeTools::getEscapedChar`: the character at `pos` (`\x41`, `\x{2028}`, `\c` for `c`) and the
/// position of its last character.
pub fn escaped(s: &[char], pos: i32) -> (char, i32) {
    if at(s, pos) != '\\' {
        return (at(s, pos), pos);
    }
    let ret = pos + 1;
    if at(s, pos + 1) != 'x' {
        return (at(s, pos + 1), ret);
    }
    if at(s, pos + 2) == '{' {
        let Some(value) = curly(s, pos + 2) else {
            return (BAD, ret);
        };
        let code = hex_number(&value);
        return match u32::try_from(code)
            .ok()
            .filter(|&c| c <= 0xFFFF)
            .and_then(char::from_u32)
        {
            Some(c) => (c, ret + value.len() as i32 + 2),
            None => (BAD, ret),
        };
    }
    let digits: Vec<char> = (pos + 2..pos + 4)
        .map(|i| at(s, i))
        .filter(|&c| c != BAD)
        .collect();
    let code = hex_number(&digits);
    if s.len() as i32 <= pos + 2 || code < 0 {
        return (BAD, ret);
    }
    (char::from_u32(code as u32).unwrap_or(BAD), ret + 2)
}

/// A single character of a class with its other cases under `/i`.
fn add_cased(class: &mut CharClass, c: char, ignore_case: bool) {
    class.add(c);
    if ignore_case {
        class.add(to_lower(c));
        class.add(to_upper(c));
    }
}

/// `\d \D \w \W \s \S \l \u` inside a class; false for another escape.
fn add_escape_class(class: &mut CharClass, c: char, ignore_case: bool) -> bool {
    let mut set = CharClass::default();
    match c {
        'd' | 'D' => set.add_all(digits()),
        'w' | 'W' => {
            set.add_all(letters());
            set.add_all(digits());
            set.add('_');
        }
        's' | 'S' => {
            set.add_all(separators());
            "\t\n\r\x0c".chars().for_each(|c| set.add(c));
        }
        'l' | 'u' => {
            set.add_all(if c == 'l' { lowers() } else { uppers() });
            if ignore_case {
                set.add_all(if c == 'l' { uppers() } else { lowers() });
            }
        }
        _ => return false,
    }
    if matches!(c, 'D' | 'W' | 'S') {
        let mut all = CharClass::everything();
        all.remove_all(&set);
        set = all;
    }
    class.add_all(&set);
    true
}

/// `UStr::createCharClass`: the class `[...]` at `pos` and the position of its `]`.
pub fn char_class(s: &[char], mut pos: i32, ignore_case: bool) -> Option<(CharClass, i32)> {
    if at(s, pos) != '[' {
        return None;
    }
    let mut class = CharClass::default();
    pos += 1;
    let inverse = at(s, pos) == '^';
    if inverse {
        pos += 1;
    }
    let len = s.len() as i32;
    let mut prev = BAD;
    while pos < len {
        let c = s[pos as usize];
        if c == ']' {
            if inverse {
                class.complement();
            }
            return Some((class, pos));
        }
        let (next, new_prev) = class_item(s, pos, &mut class, (prev, ignore_case))?;
        pos = next + 1;
        prev = new_prev;
    }
    None
}

/// One item of a class at `pos`: the position of its last character and the single character it was
/// (for a range after it); none for a bad one that spoils the class.
fn class_item(
    s: &[char],
    pos: i32,
    class: &mut CharClass,
    (prev, ignore_case): (char, bool),
) -> Option<(i32, char)> {
    let c = s[pos as usize];
    let len = s.len() as i32;
    if c == '{' {
        let name: String = curly(s, pos)?.into_iter().collect();
        if !name.is_empty() {
            class.add_all(&property(&name).unwrap_or_default());
        }
        return Some((pos + name.chars().count() as i32 + 1, BAD));
    }
    if c == '\\' && pos + 1 < len {
        if add_escape_class(class, s[pos as usize + 1], ignore_case) {
            return Some((pos + 1, BAD));
        }
        let (escaped, end) = escaped(s, pos);
        if escaped == BAD {
            return Some((pos + 1, BAD));
        }
        add_cased(class, escaped, ignore_case);
        return Some((end, escaped));
    }
    if c == '-' && at(s, pos + 1) == '[' {
        let (sub, end) = char_class(s, pos + 1, false)?;
        class.remove_all(&sub);
        return Some((end, BAD));
    }
    if c == '&' && at(s, pos + 1) == '&' && at(s, pos + 2) == '[' {
        let (sub, end) = char_class(s, pos + 2, false)?;
        class.retain_all(&sub);
        return Some((end, BAD));
    }
    if c == '[' {
        let (sub, end) = char_class(s, pos, ignore_case)?;
        class.add_all(&sub);
        return Some((end, BAD));
    }
    if c == '-' && prev != BAD && pos + 1 < len && s[pos as usize + 1] != ']' {
        let (last, end) = escaped(s, pos + 1);
        if last == BAD {
            return None;
        }
        class.add_range(prev as u32, last as u32);
        return Some((end, prev));
    }
    add_cased(class, c, ignore_case);
    Some((pos, c))
}
