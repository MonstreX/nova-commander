//! Brackets matching as FAR's `Brackets.cpp`: what is under the cursor (a single bracket, a double one as
//! `/*`, a bracket just before the cursor, a "bracket" that is its own pair as a quotation mark), the
//! search for its pair over the lines (nested pairs counted), and the block "Select" marks.

/// FAR's options (`Brackets.hpp`), its settings' names and defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `IgnoreQuotes`: a bracket between quotation marks (`'('`) is not counted.
    pub ignore_quotes: bool,
    /// `IgnoreAfter`: a bracket just before the cursor is not taken.
    pub ignore_after: bool,
    /// `BracketPrior`: the double brackets looked at first.
    pub bracket_prior: bool,
    /// `JumpToPair`: "Select" puts the cursor on the pair too.
    pub jump_to_pair: bool,
    pub beep: bool,
    /// `QuotesType`, `Brackets1`, `Brackets2`: pairs of quotation marks, pairs of single brackets, fours
    /// of double ones (an opening pair, a closing pair).
    pub quotes: String,
    pub brackets1: String,
    pub brackets2: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            ignore_quotes: false,
            ignore_after: false,
            bracket_prior: true,
            jump_to_pair: true,
            beep: false,
            quotes: "''\"\"`'``„”".to_string(),
            brackets1: "<>{}[]()\"\"''%%«»".to_string(),
            brackets2: "/**/<??><%%>".to_string(),
        }
    }
}

/// What is under the cursor (FAR's `BrOne`, `BrTwo`, `BrRight`, `BrOneMath`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    One,
    Two,
    Right,
    OneMath,
}

/// The bracket found at the cursor: its kind, the way to its pair (+1, -1; 0 for a quotation mark,
/// asked), the pair's characters (FAR's `B21` - `B24`), the character at the cursor and those beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bracket {
    pub kind: Kind,
    pub direction: i64,
    pub pair: [char; 4],
    pub at: char,
    pub before: char,
    pub after: char,
}

const NONE: char = '\0';

fn char_at(line: &[char], pos: i64) -> char {
    usize::try_from(pos)
        .ok()
        .and_then(|pos| line.get(pos))
        .copied()
        .unwrap_or(NONE)
}

/// The quotation marks' pairs (an odd one left out); none: quotes are not looked at.
fn quote_pairs(quotes: &str) -> Vec<(char, char)> {
    let chars: Vec<char> = quotes.chars().collect();
    chars
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

/// Whether `before` and `after` are a pair of quotation marks (the character between them quoted).
fn quoted(quotes: &[(char, char)], before: char, after: char) -> bool {
    quotes
        .iter()
        .any(|&(open, close)| before == open && after == close)
}

/// FAR's analysis of the character under the cursor at `pos` of `line`: none when there is nothing to
/// match, or the cursor stands between quotation marks (unless quotes are ignored).
pub fn analyse(line: &[char], pos: usize, options: &Options) -> Option<Bracket> {
    if pos > line.len() {
        return None;
    }
    let pos = pos as i64;
    let (before, at, after) = (
        char_at(line, pos - 1),
        char_at(line, pos),
        char_at(line, pos + 1),
    );
    let quotes = quote_pairs(&options.quotes);
    if !options.ignore_quotes && !quotes.is_empty() && quoted(&quotes, before, after) {
        return None;
    }
    let singles: Vec<char> = options.brackets1.chars().collect();
    let doubles: Vec<char> = options.brackets2.chars().collect();
    let found = match options.bracket_prior {
        true => double(&doubles, (before, at, after))
            .or_else(|| single(&singles, options, (before, at))),
        false => single(&singles, options, (before, at))
            .or_else(|| double(&doubles, (before, at, after))),
    }?;
    let (kind, direction, pair) = found;
    let (kind, direction) = match pair[0] == pair[1] {
        true => (Kind::OneMath, 0),
        false => (kind, direction),
    };
    Some(Bracket {
        kind,
        direction,
        pair,
        at,
        before,
        after,
    })
}

/// The double brackets: the first four with the character at the cursor in it decides.
fn double(
    doubles: &[char],
    (before, at, after): (char, char, char),
) -> Option<(Kind, i64, [char; 4])> {
    let four = doubles
        .as_chunks::<4>()
        .0
        .iter()
        .find(|four| four.contains(&at))?;
    let pair = [four[0], four[1], four[2], four[3]];
    let [b21, b22, b23, b24] = pair;
    if (at == b21 && after == b22) || (at == b22 && before == b21) {
        return Some((Kind::Two, 1, pair));
    }
    if (before == b23 && at == b24) || (at == b23 && after == b24) {
        return Some((Kind::Two, -1, pair));
    }
    None
}

/// The single brackets: at the cursor, else (unless "Do not jump if after bracket") just before it.
fn single(
    singles: &[char],
    options: &Options,
    (before, at): (char, char),
) -> Option<(Kind, i64, [char; 4])> {
    let pairs: Vec<[char; 4]> = (singles.as_chunks::<2>().0.iter())
        .map(|pair| [pair[0], pair[1], NONE, NONE])
        .collect();
    let by = |ch: char, kind: Kind| {
        (pairs.iter()).find_map(|pair| match ch {
            _ if ch == pair[0] => Some((kind, 1, *pair)),
            _ if ch == pair[1] => Some((kind, -1, *pair)),
            _ => None,
        })
    };
    by(at, Kind::One).or_else(|| {
        (!options.ignore_after)
            .then(|| by(before, Kind::Right))
            .flatten()
    })
}

/// FAR's search for the pair from `(line, pos)` in `direction`, over `lines` lines read by `read`: the
/// line and the position the cursor goes to (a double bracket's: its first character), none when
/// there is no pair.
pub fn search(
    read: &mut dyn FnMut(usize) -> Option<Vec<char>>,
    (line, pos): (usize, usize),
    lines: usize,
    (bracket, direction): (&Bracket, i64),
    options: &Options,
) -> Option<(usize, usize)> {
    let quotes = quote_pairs(&options.quotes);
    let mut text = read(line)?;
    let (mut line, mut pos) = (line as i64, pos as i64);
    let mut count = 1_i64;
    loop {
        pos += direction;
        let past = pos >= text.len() as i64;
        if past || pos < 0 {
            line += if past { 1 } else { -1 };
            if line < 0 || line >= lines as i64 {
                return None;
            }
            text = read(line as usize)?;
            pos = if past { 0 } else { text.len() as i64 - 1 };
        }
        if pos > text.len() as i64 || pos < 0 {
            continue;
        }
        let (before, at, after) = (
            char_at(&text, pos - 1),
            char_at(&text, pos),
            char_at(&text, pos + 1),
        );
        if options.ignore_quotes && quoted(&quotes, before, after) {
            continue;
        }
        if matched(bracket, direction, (before, at, after), &mut count) {
            return Some((line as usize, adjusted(bracket, direction, pos) as usize));
        }
    }
}

/// Whether the characters at a place close the pair (nested ones counted in `count`).
fn matched(
    bracket: &Bracket,
    direction: i64,
    (before, at, after): (char, char, char),
    count: &mut i64,
) -> bool {
    let [b21, b22, b23, b24] = bracket.pair;
    let start = bracket.at;
    match bracket.kind {
        Kind::OneMath => at == start && ((bracket.before == '\\') == (before == '\\')),
        Kind::Right => {
            if at == bracket.before {
                *count += 1;
                return false;
            }
            if (at == b21 && bracket.before == b22) || (at == b22 && bracket.before == b21) {
                *count -= 1;
                return (direction == 1 && *count == 0) || (direction == -1 && *count == 1);
            }
            false
        }
        Kind::One => {
            if at == start {
                *count += 1;
                return false;
            }
            if (at == b21 && start == b22) || (at == b22 && start == b21) {
                *count -= 1;
                return *count == 0;
            }
            false
        }
        Kind::Two => {
            let nested = (direction == 1
                && ((start == b21 && at == b21 && after == b22)
                    || (start == b22 && at == b22 && before == b21)))
                || (direction == -1
                    && ((start == b23 && at == b23 && after == b24)
                        || (start == b24 && at == b24 && before == b23)));
            if nested {
                *count += 1;
                return false;
            }
            let closes = (start == b21 && at == b23 && bracket.after == b22 && after == b24)
                || (start == b22 && at == b24 && bracket.before == b21 && before == b23)
                || (start == b23 && at == b21 && bracket.after == b24 && after == b22)
                || (start == b24 && at == b22 && bracket.before == b23 && before == b21);
            if closes {
                *count -= 1;
                return *count == 0;
            }
            false
        }
    }
}

/// FAR's place on a double bracket's pair: the cursor on the same character of it as it was on.
fn adjusted(bracket: &Bracket, direction: i64, pos: i64) -> i64 {
    let [b21, b22, b23, b24] = bracket.pair;
    match bracket.kind {
        Kind::Two if bracket.at == b21 || bracket.at == b24 => pos + direction,
        Kind::Two if bracket.at == b22 || bracket.at == b23 => pos - direction,
        _ => pos,
    }
}

/// FAR's block of "Select" from the cursor's place `from` to the pair's `to`: its first line, the
/// position it starts at, its width (as `ECTL_SELECT` takes it) and its lines.
pub fn selection(
    from: (usize, usize),
    to: (usize, usize),
    (kind, direction): (Kind, i64),
) -> (usize, usize, i64, usize) {
    let (first, height) = (from.0.min(to.0), from.0.max(to.0) - from.0.min(to.0) + 1);
    let (from_pos, to_pos) = (from.1 as i64, to.1 as i64);
    let (mut start, mut width) = match direction > 0 {
        true => (from_pos, to_pos - from_pos + 1),
        false => (to_pos, from_pos - to_pos + 1),
    };
    if kind == Kind::Right {
        if direction > 0 {
            start -= 1;
            width += 1;
        } else {
            width -= 1;
        }
    }
    (first, start.max(0) as usize, width, height)
}

#[cfg(test)]
#[path = "matching_tests.rs"]
mod tests;
