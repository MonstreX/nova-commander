//! Align's reformatting and justification as FAR's.

use super::*;

fn lines(text: &[&str]) -> Vec<String> {
    text.iter().map(|line| (*line).to_string()).collect()
}

#[test]
fn the_block_is_broken_again_at_the_margin_under_its_least_indent() {
    let block = lines(&["  one two three", "    four five six seven", "", "  eight"]);
    assert_eq!(
        reformat(&block, 14, false, false),
        ["  one two", "  three four", "  five six", "  seven eight"]
    );
    assert_eq!(
        reformat(&lines(&["averyveryverylongword x"]), 5, false, false),
        ["averyveryverylongword", "x"],
        "a word longer than the margin on a line of its own"
    );
    // Review D1: a long word after the first (FAR's `LastLength <= 0`) - no underflow, and the line
    // after it broken again.
    assert_eq!(
        reformat(&lines(&["aaaa bbbbbbbbbbbbbbb ccc dd"]), 10, false, false),
        ["aaaa", "bbbbbbbbbbbbbbb", "ccc dd"]
    );
    assert_eq!(
        reformat(
            &lines(&["aaaa bbbbbbbbbbbbbbb ccc dd ee ff gg"]),
            10,
            false,
            false
        ),
        ["aaaa", "bbbbbbbbbbbbbbb", "ccc dd ee", "ff gg"]
    );
}

#[test]
fn justified_lines_reach_the_margin_by_their_spaces() {
    let line: Vec<char> = "a b c".chars().collect();
    assert_eq!(
        justified(&line, 9)
            .map(|chars| chars.iter().collect::<String>())
            .as_deref(),
        Some("a   b   c")
    );
    assert_eq!(
        justified(&line, 8)
            .map(|chars| chars.iter().collect::<String>())
            .as_deref(),
        Some("a   b  c"),
        "the first spaces one more"
    );
    assert_eq!(justified(&"word".chars().collect::<Vec<_>>(), 9), None);
    assert_eq!(
        reformat(&lines(&["one two three four"]), 10, false, true),
        ["one    two", "three four"],
        "the last line not justified"
    );
}

#[test]
fn smart_mode_takes_a_short_word_to_the_next_line() {
    let text = "aaaa bbbb cccc dddd eeee ff gggg hhhh";
    assert_eq!(
        reformat(&lines(&[text]), 29, false, false),
        ["aaaa bbbb cccc dddd eeee ff", "gggg hhhh"]
    );
    assert_eq!(
        reformat(&lines(&[text]), 29, true, false),
        ["aaaa bbbb cccc dddd eeee", "ff gggg hhhh"],
        "\"ff\" not left at the line's end"
    );
}
