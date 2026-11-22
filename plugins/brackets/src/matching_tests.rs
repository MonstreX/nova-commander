//! Brackets matching as FAR's: the analysis under the cursor, the pair over the lines, the block.

use super::*;

fn chars(text: &str) -> Vec<char> {
    text.chars().collect()
}

/// The pair of the bracket at `(line, pos)` in `text`'s lines, in the found direction.
fn pair(text: &[&str], (line, pos): (usize, usize), options: &Options) -> Option<(usize, usize)> {
    let lines: Vec<Vec<char>> = text.iter().map(|line| chars(line)).collect();
    let bracket = analyse(&lines[line], pos, options)?;
    let mut read = |at: usize| lines.get(at).cloned();
    search(
        &mut read,
        (line, pos),
        lines.len(),
        (&bracket, bracket.direction),
        options,
    )
}

#[test]
fn a_single_bracket_s_pair_is_found_over_nested_ones_and_lines() {
    let options = Options::default();
    let text = ["f(a, (b)) {", "  x[1]", "}"];
    assert_eq!(
        pair(&text, (0, 1), &options),
        Some((0, 8)),
        "the outer pair"
    );
    assert_eq!(
        pair(&text, (0, 8), &options),
        Some((0, 1)),
        "back from the closing one"
    );
    assert_eq!(
        pair(&text, (0, 10), &options),
        Some((2, 0)),
        "over the lines"
    );
    assert_eq!(
        pair(&text, (0, 2), &options),
        Some((0, 8)),
        "just after a bracket: that one"
    );
    assert_eq!(pair(&text, (0, 3), &options), None, "no bracket: nothing");
    let bracket = analyse(&chars("(a)"), 3, &options).unwrap();
    assert_eq!(
        (bracket.kind, bracket.direction),
        (Kind::Right, -1),
        "just after it"
    );
    let after = Options {
        ignore_after: true,
        ..Options::default()
    };
    assert_eq!(analyse(&chars("(a)"), 3, &after), None, "not after it");
}

#[test]
fn a_double_bracket_s_pair_and_quotes_as_far_s() {
    let options = Options::default();
    let text = ["/* a /* b */ c */"];
    assert_eq!(
        pair(&text, (0, 0), &options),
        Some((0, 16)),
        "the outer one, on its slash as the cursor was"
    );
    assert_eq!(pair(&text, (0, 16), &options), Some((0, 0)));
    let quote = analyse(&chars("say \"hi\" now"), 4, &options).unwrap();
    assert_eq!(
        (quote.kind, quote.direction),
        (Kind::OneMath, 0),
        "its own pair: the way asked"
    );
    assert_eq!(
        analyse(&chars("'(' x"), 1, &options),
        None,
        "between quotation marks"
    );
    let lines = [chars("\"a\\\"b\" c")];
    let mut read = |at: usize| lines.get(at).cloned();
    let found = search(
        &mut read,
        (0, 0),
        1,
        (&analyse(&lines[0], 0, &options).unwrap(), 1),
        &options,
    );
    assert_eq!(found, Some((0, 5)), "an escaped quotation mark skipped");
}

#[test]
fn select_marks_from_the_bracket_through_its_pair() {
    assert_eq!(selection((0, 1), (0, 8), (Kind::One, 1)), (0, 1, 8, 1));
    assert_eq!(selection((2, 0), (0, 10), (Kind::One, -1)), (0, 10, -9, 3));
    assert_eq!(
        selection((0, 3), (0, 0), (Kind::Right, -1)),
        (0, 0, 3, 1),
        "after the bracket: the cursor's place left out"
    );
}
