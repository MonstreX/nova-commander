//! Align's text work as FAR's `Align.cpp`: a block's lines joined and broken again at the right margin
//! under their least indent (`ReformatBlock`, with "Do not leave short words at line ends" its
//! `SmartMode`), a line spread to the margin by its spaces (`JustifyString`).

/// FAR's limit of the indent (`MaxIndent`).
const MAX_INDENT: usize = 1023;

/// FAR's `ReformatBlock` on the block's lines (their tabs as spaces): the lines that replace them.
pub fn reformat(lines: &[String], margin: usize, smart: bool, justify: bool) -> Vec<String> {
    let mut indent = usize::MAX;
    let mut total: Vec<char> = Vec::new();
    for line in lines {
        let chars: Vec<char> = line.chars().collect();
        let spaces = chars.iter().take_while(|ch| **ch == ' ').count();
        if spaces == chars.len() {
            continue;
        }
        indent = indent.min(spaces);
        if total.last().is_some_and(|last| *last != ' ') {
            total.push(' ');
        }
        total.extend(&chars[spaces..]);
    }
    if !total.is_empty() {
        total.push(' ');
    }
    let indent = indent.min(margin.saturating_sub(1)).min(MAX_INDENT);
    let width = margin.saturating_sub(indent);
    let pad = " ".repeat(indent);
    let mut out = Vec::new();
    let len = total.len();
    let mut split = total.iter().take_while(|ch| **ch == ' ').count();
    let mut space = split;
    let mut at = split;
    while at < len {
        let length = at - split;
        // FAR's `LastLength` is signed: after a long word's break the last space is before the line's
        // start (review D1: the unsigned difference underflowed).
        let last_signed = space as isize - split as isize;
        if total[at] == ' ' && length > width && last_signed <= width as isize {
            let mut last = last_signed.max(0) as usize;
            if last_signed <= 0 {
                (space, last) = (at, length);
            }
            if smart {
                (space, last) = smart_break(&total, split, space).unwrap_or((space, last));
            }
            at = space;
            let chunk = trimmed(&total[split..split + last]);
            let line = match justify && chunk.len() < width {
                true => justified(&chunk, width).unwrap_or(chunk),
                false => chunk,
            };
            out.push(format!("{pad}{}", line.iter().collect::<String>()));
            while at < len && total[at] == ' ' {
                space = at;
                at += 1;
            }
            split = at;
        }
        if at < len && total[at] == ' ' {
            space = at;
        }
        at += 1;
    }
    let rest = trimmed(&total[split.min(len)..]);
    out.push(format!("{pad}{}", rest.iter().collect::<String>()));
    out
}

/// `chars` without their trailing spaces.
fn trimmed(chars: &[char]) -> Vec<char> {
    let end = chars.len() - chars.iter().rev().take_while(|ch| **ch == ' ').count();
    chars[..end].to_vec()
}

/// FAR's `SmartMode`: a short word just before the break goes to the next line (the break and the
/// line's length moved back); none: the break stays.
fn smart_break(total: &[char], split: usize, space: usize) -> Option<(usize, usize)> {
    let (mut first, mut second) = (None, None);
    for at in (split + 21..space).rev() {
        if total[at] != ' ' {
            continue;
        }
        match (second, first) {
            (None, _) => second = Some(at),
            (Some(_), None) => first = Some(at),
            _ => break,
        }
    }
    let second = second.filter(|second| space - second < 4)?;
    let takes = first.is_none_or(|first| second - first > 4) || space - second == 2;
    if !takes {
        return None;
    }
    let mut space = second;
    while space > split + 1 && total[space - 1] == ' ' {
        space -= 1;
    }
    Some((space, space - split))
}

/// FAR's `JustifyString`: `line` spread to `width` by its spaces after the words (the first ones one
/// more); none when it has no space between words.
pub fn justified(line: &[char], width: usize) -> Option<Vec<char>> {
    let mut words = line
        .windows(2)
        .filter(|pair| pair[0] != ' ' && pair[1] == ' ')
        .count();
    if line.last() == Some(&' ') {
        words = words.saturating_sub(1);
    }
    if words == 0 {
        return None;
    }
    let text = trimmed(line);
    let total = width.saturating_sub(text.len());
    let (add, mut rest) = (total / words, total % words);
    let mut out = vec![' '; width.max(text.len())];
    out[..text.len()].copy_from_slice(&text);
    let mut at = 0;
    while at + 1 < width {
        if out[at] != ' ' && out[at + 1] == ' ' {
            let mut moved = add;
            if rest > 0 {
                moved += 1;
                rest -= 1;
            }
            if moved == 0 {
                break;
            }
            out.copy_within(at + 1..width - moved, at + 1 + moved);
            out[at + 1..at + 1 + moved].fill(' ');
        }
        at += 1;
    }
    out.truncate(width);
    Some(out)
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
