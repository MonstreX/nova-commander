//! EditCase's conversions (FAR's `EditCase.cpp`): lower case, Title Case, UPPER CASE, tOGGLE cASE and the
//! cyclic change (lower, Title, UPPER as Microsoft Word's Shift+F3), the word under the cursor found as
//! FAR's `FindBounds` finds it. A letter is any character that is not a word divider (FAR's `MyIsAlpha`:
//! the editor's `WordDiv`, the plugin's `AddWordDiv`, the blanks and the breaks).

/// FAR's editor `WordDiv` default (`WordDiv0` in `far/config.cpp`).
pub const WORD_DIV: &str = "~!%^&*()+|{}:\"<>?`-=\\[];',./";
/// EditCase's own extra dividers (its setting `AddWordDiv`, `#` by default).
pub const ADD_WORD_DIV: &str = "#";

/// A conversion (FAR's `ENUMCCTYPES`, the menu's order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Lower,
    Title,
    Upper,
    Toggle,
    Cyclic,
}

impl Case {
    pub const ALL: [Self; 5] = [
        Self::Lower,
        Self::Title,
        Self::Upper,
        Self::Toggle,
        Self::Cyclic,
    ];
}

/// What is not a letter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dividers(Vec<char>);

impl Dividers {
    /// FAR's `WordDiv`: the editor's dividers, the plugin's own, the blanks and the breaks.
    pub fn new(add: &str) -> Self {
        Self(
            WORD_DIV
                .chars()
                .chain(add.chars())
                .chain(" \n\r\t".chars())
                .collect(),
        )
    }

    /// FAR's `MyIsAlpha`.
    pub fn is_letter(&self, ch: char) -> bool {
        !self.0.contains(&ch)
    }
}

fn lower(ch: char) -> char {
    let mut lower = ch.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(one), None) => one,
        _ => ch,
    }
}

fn upper(ch: char) -> char {
    let mut upper = ch.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(one), None) => one,
        _ => ch,
    }
}

/// FAR's `FindBounds`: the word to convert at or nearest to `pos` within `start..end` (on a non-letter
/// the nearer word, the left one on a tie); none without a letter there.
pub fn find_bounds(
    line: &[char],
    pos: usize,
    (start, end): (usize, usize),
    dividers: &Dividers,
) -> Option<(usize, usize)> {
    if line.len() <= start {
        return None;
    }
    let end = end.min(line.len());
    let pos = pos.max(start).min(end);
    let letter = |at: usize| line.get(at).is_some_and(|ch| dividers.is_letter(*ch));
    if letter(pos) {
        return Some((
            find_start(line, start, pos, dividers),
            find_end(line, pos, end, dividers),
        ));
    }
    let left = (1..=pos - start).find(|&radius| letter(pos - radius));
    let right = (1..)
        .take_while(|radius| pos + radius <= end)
        .find(|&radius| letter(pos + radius));
    match (left, right) {
        (Some(left), right) if right.is_none_or(|right| left <= right) => {
            let end = pos - left + 1;
            Some((find_start(line, start, end, dividers), end))
        }
        (_, Some(right)) => {
            let start = pos + right;
            Some((start, find_end(line, start, end, dividers)))
        }
        _ => None,
    }
}

/// FAR's `FindStart`: back over the letters before `end`.
fn find_start(line: &[char], start: usize, end: usize, dividers: &Dividers) -> usize {
    let mut at = end;
    while at > start && dividers.is_letter(line[at - 1]) {
        at -= 1;
    }
    at
}

/// FAR's `FindEnd`: on over the letters from `start`.
fn find_end(line: &[char], start: usize, end: usize, dividers: &Dividers) -> usize {
    let mut at = start;
    while at < end && dividers.is_letter(line[at]) {
        at += 1;
    }
    at
}

/// FAR's `ChangeCase`: the letters of `start..end` converted (not `Cyclic`).
pub fn change_case(
    line: &mut [char],
    (start, end): (usize, usize),
    case: Case,
    dividers: &Dividers,
) {
    let mut after_letter = false;
    for ch in line.iter_mut().take(end).skip(start) {
        if !dividers.is_letter(*ch) {
            after_letter = false;
            continue;
        }
        *ch = match case {
            Case::Lower => lower(*ch),
            Case::Title if after_letter => lower(*ch),
            Case::Title => upper(*ch),
            Case::Upper => upper(*ch),
            Case::Toggle if ch.is_lowercase() => upper(*ch),
            Case::Toggle => lower(*ch),
            Case::Cyclic => *ch,
        };
        after_letter = true;
    }
}

/// FAR's `GetNextCCType`: the cyclic change's next case by the first word of `start..end` - lower after
/// a single capital or Title, Title after UPPER, UPPER after lower or a single small letter, tOGGLE when
/// most of it is capitals in no other pattern, else lower; `Cyclic` when there is no word.
pub fn next_case(line: &[char], (start, end): (usize, usize), dividers: &Dividers) -> Case {
    let (start, end) = (start.min(end), start.max(end));
    if line.len() < start {
        return Case::Cyclic;
    }
    let Some((first, past)) = find_bounds(line, start, (start, end), dividers) else {
        return Case::Cyclic;
    };
    let word = &line[first..past];
    let uppered: Vec<char> = word.iter().map(|ch| upper(*ch)).collect();
    if word.len() == 1 {
        return if word == uppered.as_slice() {
            Case::Lower
        } else {
            Case::Upper
        };
    }
    if word == uppered.as_slice() {
        return Case::Title;
    }
    let lowered: Vec<char> = word.iter().map(|ch| lower(*ch)).collect();
    if word == lowered.as_slice() {
        return Case::Upper;
    }
    let mut titled = lowered.clone();
    titled[0] = upper(titled[0]);
    if word == titled.as_slice() {
        return Case::Lower;
    }
    let capitals = word
        .iter()
        .zip(&uppered)
        .filter(|(ch, up)| ch == up)
        .count();
    match capitals > word.len() / 2 {
        true => Case::Toggle,
        false => Case::Lower,
    }
}

#[cfg(test)]
#[path = "case_tests.rs"]
mod tests;
