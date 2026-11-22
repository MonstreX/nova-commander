//! FAR's file masks (`far/filemasks.cpp`, `CmpName`): masks separated by `,` or `;` (quotes allowed),
//! exclusions after `|`, wildcards `*` and `?`, character sets `[a-z]`. As in FAR,
//! `*.*` matches every name, a trailing `.*` also matches names without an extension, and `*.` matches
//! names without one. Case is folded as the system compares names: to upper case on Windows, as FAR's
//! `upper` (`fold`).
//! `<name>` stands for the masks of that group of file masks and `%PATHEXT%` for the extensions of the
//! programs (FAR's `filemasks::assign`, `add_pathext`). `,` and `;` inside `[...]` are a set's, not
//! separators (FAR's `with_brackets`). `/regex/options` is a regular expression searched for in the
//! name (FAR's Perl style: `i` without case, `s`, `m`, `x`; `o` changes nothing); one with errors makes
//! the whole list fail, as FAR's "File-mask string contains errors" (review R38, R39; ADR-0020).

use regex::{Regex, RegexBuilder};

use crate::platform::files::NAMES_DIFFER_BY_CASE;

/// FAR's start and end of a regular expression in a mask list.
const RE_MARK: char = '/';

/// One mask: wildcards folded as the system compares names (`fold`), as characters; or a regular
/// expression.
#[derive(Debug, Clone)]
enum Pattern {
    Wild(Vec<char>),
    /// The expression as written (`/body/options`: two with other options differ) and compiled.
    Re(String, Regex),
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Wild(one), Self::Wild(other)) => one == other,
            (Self::Re(one, _), Self::Re(other, _)) => one == other,
            _ => false,
        }
    }
}

impl Eq for Pattern {}

impl Pattern {
    fn takes(&self, name: &str) -> bool {
        match self {
            Self::Wild(mask) => cmp_name(mask, name, MASKS),
            Self::Re(_, re) => re.is_match(name),
        }
    }
}

/// A compiled mask list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMask {
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
}

impl FileMask {
    /// Parses `text`; `None` when it contains no mask. No group is known: `<name>` stands for nothing.
    #[cfg(test)]
    pub fn parse(text: &str) -> Option<Self> {
        Self::parse_with(text, &|_| None, "")
    }

    /// Parses `text` with the groups `group` knows and the programs' extensions `path_ext` (`.COM;.EXE`).
    pub fn parse_with(
        text: &str,
        group: &dyn Fn(&str) -> Option<String>,
        path_ext: &str,
    ) -> Option<Self> {
        let text = with_path_ext(&expand_groups(text, group), path_ext);
        let (mut include, exclude) = read_lists(&text)?;
        if include.is_empty() {
            // FAR: exclusions alone are taken from every name.
            if exclude.is_empty() {
                return None;
            }
            include.push(Pattern::Wild(vec!['*']));
        }
        Some(Self { include, exclude })
    }

    /// FAR's `filemasks::check`: an include mask takes the name and no exclude mask does.
    pub fn matches(&self, name: &str) -> bool {
        let takes = |mask: &Pattern| mask.takes(name);
        self.include.iter().any(takes) && !self.exclude.iter().any(takes)
    }
}

/// FAR's `filemasks::assign` loop: regular expressions and the masks between them, the include list
/// then, after `|`, the exclude list (a second `|` ends it); `None` when a regular expression has errors.
fn read_lists(text: &str) -> Option<(Vec<Pattern>, Vec<Pattern>)> {
    let separators: &[char] = &[' ', ',', ';'];
    let mut lists: [(Vec<Pattern>, String); 2] = Default::default();
    let mut list = 0;
    let mut rest = text;
    while !rest.is_empty() {
        rest = rest.trim_start_matches(separators);
        if rest.starts_with(RE_MARK) {
            let (re, after) = split_re(rest);
            lists[list]
                .0
                .push(Pattern::Re(re.to_string(), compile_re(re)?));
            rest = after;
        }
        rest = rest.trim_start_matches(separators);
        let end = rest.find([RE_MARK, '|']).unwrap_or(rest.len());
        // A list of its own each: FAR runs the parts on together, which joins two masks into one.
        lists[list].1.push(',');
        lists[list].1.push_str(&rest[..end]);
        rest = &rest[end..];
        if let Some(after) = rest.strip_prefix('|') {
            if list == 1 {
                break;
            }
            list = 1;
            rest = after;
        }
    }
    let [(mut include, simple_include), (mut exclude, simple_exclude)] = lists;
    include.extend(split_masks(&simple_include).into_iter().map(Pattern::Wild));
    exclude.extend(split_masks(&simple_exclude).into_iter().map(Pattern::Wild));
    Some((include, exclude))
}

/// FAR's `extract_re`: `/.../` (a `/` after `\` is the expression's) and the letters after it.
fn split_re(text: &str) -> (&str, &str) {
    let mut end = text.len();
    let mut previous = RE_MARK;
    for (at, ch) in text.char_indices().skip(1) {
        if ch == RE_MARK && previous != '\\' {
            end = at + 1;
            break;
        }
        previous = ch;
    }
    let options = text[end..]
        .find(|ch: char| !ch.is_alphabetic())
        .map_or(text.len(), |at| end + at);
    text.split_at(options)
}

/// FAR's `RegExp::Compile` in Perl style: the expression between the first and the last `/`, the
/// options after it.
fn compile_re(text: &str) -> Option<Regex> {
    let end = text.rfind(RE_MARK).filter(|end| *end > 0)?;
    let mut builder = RegexBuilder::new(&text[1..end]);
    for option in text[end + 1..].chars() {
        match option {
            'i' => builder.case_insensitive(true),
            's' => builder.dot_matches_new_line(true),
            'm' => builder.multi_line(true),
            'x' => builder.ignore_whitespace(true),
            'o' => &mut builder,
            _ => return None,
        };
    }
    builder.build().ok()
}

/// FAR's quick-search match (`filelist.cpp`, `CmpName(exclude_sets(Name + '*'), FileName, true, false)`):
/// `prefix*` against the name after its last separator (`skippath`), brackets taken literally, without
/// case on every system, none of the legacy endings.
pub fn matches_prefix(prefix: &str, name: &str) -> bool {
    let mask: Vec<char> = (prefix.chars()).map(upper).chain(['*']).collect();
    let name = name.rsplit(std::path::is_separator).next().unwrap_or(name);
    cmp_name(&mask, name, QUICK)
}

/// FAR's `upper` of one character; one whose upper case is more than one character stays as it is.
fn upper(letter: char) -> char {
    if letter.is_ascii() {
        return letter.to_ascii_uppercase();
    }
    let mut upper = letter.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(one), None) => one,
        _ => letter,
    }
}

/// One character as the system compares names: FAR's `upper` where case does not matter.
fn fold(letter: char) -> char {
    match NAMES_DIFFER_BY_CASE {
        true => letter,
        false => upper(letter),
    }
}

/// How `cmp_name` compares: FAR's `CmpName` as the masks call it and as the quick search does.
#[derive(Debug, Clone, Copy)]
struct How {
    /// `[...]` is a set (the quick search's `exclude_sets` takes brackets literally).
    sets: bool,
    /// `CmpNameLegacyMode`: `*.*` every name, `.*` at the end also none, `.` at the end no extension.
    legacy: bool,
    fold: fn(char) -> char,
}

const MASKS: How = How {
    sets: true,
    legacy: true,
    fold,
};

const QUICK: How = How {
    sets: false,
    legacy: false,
    fold: upper,
};

/// FAR's loop over `<name>`: each is replaced, everywhere, by its group's masks; a group met again (one
/// that holds itself) and an unknown one stand for nothing.
fn expand_groups(text: &str, group: &dyn Fn(&str) -> Option<String>) -> String {
    let mut text = text.to_string();
    let mut used: Vec<String> = Vec::new();
    while let Some(open) = text.find('<') {
        let Some(close) = text[open..].find('>').map(|at| open + at) else {
            break;
        };
        let with_brackets = text[open..=close].to_string();
        let name = &with_brackets[1..with_brackets.len() - 1];
        let value = if used.iter().any(|known| known == name) {
            String::new()
        } else {
            used.push(name.to_string());
            group(name).unwrap_or_default()
        };
        text = text.replace(&with_brackets, &value);
    }
    text
}

/// FAR's `add_pathext`: `%PATHEXT%`, in any case, becomes `*.COM,*.EXE,...`.
fn with_path_ext(text: &str, path_ext: &str) -> String {
    const NAME: &str = "%pathext%";
    let Some(at) = text.to_ascii_lowercase().find(NAME) else {
        return text.to_string();
    };
    let masks: Vec<String> = (path_ext.split(';'))
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
        .map(|extension| format!("*{extension}"))
        .collect();
    let rest = with_path_ext(&text[at + NAME.len()..], path_ext);
    format!("{}{}{rest}", &text[..at], masks.join(","))
}

/// FAR's `enum_tokens_with_quotes_t<with_brackets, with_trim>`: split at `,` and `;` outside quotes
/// and outside `[...]`; quotes come first (a `[` inside them opens no set, a `"` inside a set quotes).
fn split_masks(text: &str) -> Vec<Vec<char>> {
    let mut masks = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut in_set = false;
    for ch in text.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' | ';' if !quoted && !in_set => push_mask(&mut masks, &mut current),
            _ => {
                if !quoted {
                    in_set = if in_set { ch != ']' } else { ch == '[' };
                }
                current.push(ch);
            }
        }
    }
    push_mask(&mut masks, &mut current);
    masks
}

fn push_mask(masks: &mut Vec<Vec<char>>, current: &mut String) {
    // The same fold as each character of a name gets.
    let mask: Vec<char> = current.trim().chars().map(fold).collect();
    if !mask.is_empty() {
        masks.push(mask);
    }
    current.clear();
}

/// FAR's `CmpName` (`processname.cpp`): the name read in place, each of its characters folded (`how`)
/// against the folded mask; `*` backtracking to the last star, `[...]` a set, the legacy endings (`How`).
fn cmp_name(pattern: &[char], name: &str, how: How) -> bool {
    if pattern == ['*'] || (how.legacy && pattern == ['*', '.', '*']) {
        return true;
    }
    if pattern.is_empty() || name.is_empty() {
        return pattern.is_empty() && name.is_empty();
    }
    let mut walk = Walk {
        pattern,
        name,
        how,
        pi: 0,
        si: 0,
        star: None,
    };
    let mut has_dot = false;
    loop {
        let current = walk.current();
        has_dot |= current == Some('.');
        let Some(&pc) = pattern.get(walk.pi) else {
            if current.is_none() {
                return true;
            }
            if !walk.backtrack() {
                return false;
            }
            continue;
        };
        match walk.step(pc, current) {
            Step::Went => continue,
            Step::Failed => return false,
            Step::Other => {}
        }
        let rest = &pattern[walk.pi..];
        let ending = rest == ['.', '*'] || (!has_dot && rest == ['.']);
        if how.legacy && current.is_none() && ending {
            return true;
        }
        if !walk.backtrack() {
            return false;
        }
    }
}

/// Where `cmp_name` is: `pi` in the mask, `si` (a byte offset) in the name, the last star's places.
struct Walk<'a> {
    pattern: &'a [char],
    name: &'a str,
    how: How,
    pi: usize,
    si: usize,
    star: Option<(usize, usize)>,
}

/// What one mask element did.
enum Step {
    Went,
    Failed,
    /// No match here: the endings are looked at, then the last star.
    Other,
}

impl Walk<'_> {
    fn current(&self) -> Option<char> {
        self.name[self.si..].chars().next()
    }

    /// FAR's `try_backtrack`: the last star takes one more character.
    fn backtrack(&mut self) -> bool {
        let Some((star_pi, star_si)) = self.star else {
            return false;
        };
        let Some(taken) = self.name[star_si..].chars().next() else {
            return false;
        };
        let next = star_si + taken.len_utf8();
        self.star = Some((star_pi, next));
        self.si = next;
        self.pi = star_pi;
        true
    }

    fn step(&mut self, pc: char, current: Option<char>) -> Step {
        match pc {
            '*' => {
                self.pi += 1;
                self.star = Some((self.pi, self.si));
                Step::Went
            }
            '?' => match current {
                Some(ch) => self.take(1, ch),
                None => Step::Failed,
            },
            '[' if self.how.sets => self.set(current),
            _ => match current {
                Some(ch) if pc == (self.how.fold)(ch) => self.take(1, ch),
                _ => Step::Other,
            },
        }
    }

    fn take(&mut self, mask: usize, ch: char) -> Step {
        self.pi += mask;
        self.si += ch.len_utf8();
        Step::Went
    }

    /// `[...]`: an empty one is skipped, an unclosed one is an ordinary `[`.
    fn set(&mut self, current: Option<char>) -> Step {
        let start = self.pi + 1;
        let Some(length) = self.pattern[start..].iter().position(|ch| *ch == ']') else {
            return match current {
                Some('[') => self.take(1, '['),
                _ => Step::Other,
            };
        };
        if length == 0 {
            self.pi = start + 1;
            return Step::Went;
        }
        let Some(ch) = current else {
            return Step::Failed;
        };
        if set_contains(&self.pattern[start..start + length], (self.how.fold)(ch)) {
            self.take(length + 2, ch)
        } else if self.backtrack() {
            Step::Went
        } else {
            Step::Failed
        }
    }
}

/// FAR's `match_char_in_set`: `[abc]`, `[a-z]` and their combinations.
fn set_contains(set: &[char], ch: char) -> bool {
    let mut index = 0;
    while index < set.len() {
        if index + 2 < set.len() && set[index + 1] == '-' {
            if (set[index]..=set[index + 2]).contains(&ch) {
                return true;
            }
            index += 3;
        } else {
            if set[index] == ch {
                return true;
            }
            index += 1;
        }
    }
    false
}

#[cfg(test)]
#[path = "mask_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mask_far_tests.rs"]
mod far_tests;
