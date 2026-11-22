//! EditCase's conversions, its word bounds and its cyclic change as FAR's.

use super::*;

fn chars(text: &str) -> Vec<char> {
    text.chars().collect()
}

fn dividers() -> Dividers {
    Dividers::new(ADD_WORD_DIV)
}

#[test]
fn the_word_is_the_one_under_the_cursor_or_the_nearer_one() {
    let line = chars("one  two#three");
    let all = (0, line.len());
    assert_eq!(find_bounds(&line, 1, all, &dividers()), Some((0, 3)));
    assert_eq!(
        find_bounds(&line, 3, all, &dividers()),
        Some((0, 3)),
        "just past it: the left one"
    );
    assert_eq!(
        find_bounds(&line, 4, all, &dividers()),
        Some((5, 8)),
        "nearer on the right"
    );
    assert_eq!(
        find_bounds(&line, 9, all, &dividers()),
        Some((9, 14)),
        "`#` is EditCase's own divider"
    );
    assert_eq!(find_bounds(&chars("  "), 1, (0, 2), &dividers()), None);
    assert_eq!(
        find_bounds(&line, 1, (20, 30), &dividers()),
        None,
        "past the line"
    );
}

#[test]
fn each_case_converts_the_letters_only() {
    let convert = |case| {
        let mut line = chars("hello WORLD-mIxEd");
        let len = line.len();
        change_case(&mut line, (0, len), case, &dividers());
        line.iter().collect::<String>()
    };
    assert_eq!(convert(Case::Lower), "hello world-mixed");
    assert_eq!(convert(Case::Upper), "HELLO WORLD-MIXED");
    assert_eq!(convert(Case::Title), "Hello World-Mixed");
    assert_eq!(convert(Case::Toggle), "HELLO world-MiXeD");
    let mut part = chars("abc def");
    change_case(&mut part, (4, 7), Case::Upper, &dividers());
    assert_eq!(
        part.iter().collect::<String>(),
        "abc DEF",
        "within the bounds"
    );
}

#[test]
fn the_cyclic_change_goes_lower_title_upper_as_far_s() {
    let next = |text: &str| next_case(&chars(text), (0, text.chars().count()), &dividers());
    assert_eq!(next("word"), Case::Upper, "lower: UPPER");
    assert_eq!(next("WORD"), Case::Title, "UPPER: Title");
    assert_eq!(next("Word"), Case::Lower, "Title: lower");
    assert_eq!(next("WoRD"), Case::Toggle, "mostly capitals: tOGGLE");
    assert_eq!(next("woRd"), Case::Lower, "else lower");
    assert_eq!(next("A"), Case::Lower, "a single capital");
    assert_eq!(next("a"), Case::Upper, "a single small letter");
    assert_eq!(next("  "), Case::Cyclic, "no word");
}
