//! The code editing mode (owner 2026-11-22): Tab / Shift+Tab over a block's lines, Enter's indent, a
//! closing bracket, Home, BS, Ctrl+/ and Alt+Up / Alt+Down; with the mode off FAR's keys.

use super::super::tests::{run, text_of, view_of};
use super::*;

/// A view of `text` in the code mode, the tab 4 wide, Tab as spaces when `spaces`.
fn code_view(text: &str, spaces: bool) -> EditView {
    let mut view = view_of(text);
    view.options.code_editing = true;
    view.options.expand_tabs = u8::from(spaces);
    view.set_tab(4);
    view
}

fn select(step: Step) -> Step {
    Step::Select(Box::new(step))
}

/// Right `count` times.
fn rights(count: usize) -> Vec<Step> {
    std::iter::repeat_n(Step::Right, count).collect()
}

#[test]
fn tab_and_shift_tab_indent_and_outdent_a_block_s_lines() {
    let mut view = code_view("a\r\n  b\r\n\r\nc\r\n", true);
    run(&mut view, &[select(Step::Down), select(Step::Down)]);
    run(&mut view, &[Step::Type('\t')]);
    assert_eq!(
        text_of(&view),
        "    a\r\n      b\r\n\r\nc\r\n",
        "the lines of the block, not the one it ends at the start of"
    );
    assert!(view.block().unwrap().is_some(), "the block kept");
    run(&mut view, &[Step::Outdent]);
    assert_eq!(text_of(&view), "a\r\n  b\r\n\r\nc\r\n");
    run(&mut view, &[Step::Undo]);
    assert_eq!(
        text_of(&view),
        "    a\r\n      b\r\n\r\nc\r\n",
        "one step each"
    );
    let mut tabs = code_view("a\r\nb\r\n", false);
    run(&mut tabs, &[select(Step::Down), select(Step::End)]);
    run(&mut tabs, &[Step::Type('\t')]);
    assert_eq!(
        text_of(&tabs),
        "\ta\r\n\tb\r\n",
        "a tab without \"Expand tabs\""
    );
    assert_eq!(tabs.cursor(), (4, 5), "the cursor on its text");
}

#[test]
fn shift_tab_alone_outdents_the_cursor_s_line_and_tab_on_one_line_types() {
    let mut view = code_view("      x\r\n", true);
    run(&mut view, &[Step::End, Step::Outdent]);
    assert_eq!(text_of(&view), "  x\r\n");
    assert_eq!(view.cursor(), (0, 3));
    run(&mut view, &[Step::Outdent, Step::Outdent]);
    assert_eq!(text_of(&view), "x\r\n", "nothing left to take");
    run(
        &mut view,
        &[Step::Home, select(Step::Right), Step::Type('\t')],
    );
    assert_eq!(
        text_of(&view),
        "    \r\n",
        "a block on one line: typed over"
    );
}

#[test]
fn enter_keeps_the_indent_and_opens_a_bracket() {
    let mut view = code_view("    foo\r\n", true);
    run(&mut view, &[Step::End, Step::Enter]);
    assert_eq!(text_of(&view), "    foo\r\n    \r\n");
    assert_eq!(view.cursor(), (9, 4));
    let mut braces = code_view("  if x {}\r\n", true);
    run(&mut braces, &[Step::End, Step::Left, Step::Enter]);
    assert_eq!(
        text_of(&braces),
        "  if x {\r\n      \r\n  }\r\n",
        "the closing bracket on a line of its own"
    );
    assert_eq!(braces.cursor(), (10, 6));
    let mut call = code_view("f(  x)\r\n", true);
    run(&mut call, &[Step::Right, Step::Right, Step::Enter]);
    assert_eq!(
        text_of(&call),
        "f(\r\n    x)\r\n",
        "the blanks after the cursor go"
    );
}

#[test]
fn a_closing_bracket_first_on_a_line_takes_an_indent_back() {
    let mut view = code_view("{\r\n        \r\n", true);
    run(&mut view, &[Step::Down, Step::End, Step::Type('}')]);
    assert_eq!(text_of(&view), "{\r\n    }\r\n");
    run(&mut view, &[Step::Type(')')]);
    assert_eq!(text_of(&view), "{\r\n    })\r\n", "not first: as it is");
}

#[test]
fn home_goes_to_the_first_character_then_the_line_s_start() {
    let mut view = code_view("    abc\r\n", true);
    run(&mut view, &[Step::End, Step::Home]);
    assert_eq!(view.cursor(), (0, 4));
    run(&mut view, &[Step::Home]);
    assert_eq!(view.cursor(), (0, 0));
    run(&mut view, &[Step::Home]);
    assert_eq!(view.cursor(), (0, 4));
    view.options.code_editing = false;
    run(&mut view, &[Step::Home]);
    assert_eq!(view.cursor(), (0, 0), "FAR's without the mode");
}

#[test]
fn backspace_in_the_leading_spaces_goes_back_to_a_tab_stop() {
    let mut view = code_view("      x\r\n", true);
    run(&mut view, &rights(6));
    run(&mut view, &[Step::Backspace]);
    assert_eq!(text_of(&view), "    x\r\n");
    run(&mut view, &[Step::Backspace]);
    assert_eq!(text_of(&view), "x\r\n");
    let mut text = code_view("ab  c\r\n", true);
    run(&mut text, &rights(4));
    run(&mut text, &[Step::Backspace]);
    assert_eq!(text_of(&text), "ab c\r\n", "after text: one character");
}

#[test]
fn ctrl_slash_comments_the_lines_out_and_in() {
    let mut view = code_view("  a\r\n\r\n    b\r\n", true);
    run(
        &mut view,
        &[select(Step::Down), select(Step::Down), select(Step::End)],
    );
    run(&mut view, &[Step::ToggleComment("//".to_string())]);
    assert_eq!(
        text_of(&view),
        "  // a\r\n\r\n  //   b\r\n",
        "at the least indent, the blank line left"
    );
    run(&mut view, &[Step::ToggleComment("//".to_string())]);
    assert_eq!(text_of(&view), "  a\r\n\r\n    b\r\n");
    let mut line = code_view("x = 1\r\n", true);
    run(
        &mut line,
        &[Step::End, Step::ToggleComment("#".to_string())],
    );
    assert_eq!(text_of(&line), "# x = 1\r\n", "the cursor's line");
    assert_eq!(line.cursor(), (0, 7), "the cursor on its text");
}

#[test]
fn alt_up_and_down_move_the_lines() {
    let mut view = code_view("one\r\ntwo\r\nthree", true);
    run(&mut view, &[Step::Right, Step::MoveLines(false)]);
    assert_eq!(text_of(&view), "two\r\none\r\nthree");
    assert_eq!(view.cursor(), (5, 1), "the cursor with its line");
    run(&mut view, &[Step::MoveLines(false)]);
    assert_eq!(
        text_of(&view),
        "two\r\nthree\r\none",
        "past the last line: the breaks where they belong"
    );
    run(&mut view, &[Step::MoveLines(false)]);
    assert_eq!(text_of(&view), "two\r\nthree\r\none", "nothing below");
    run(&mut view, &[Step::MoveLines(true), Step::MoveLines(true)]);
    assert_eq!(text_of(&view), "one\r\ntwo\r\nthree");
    run(&mut view, &[Step::Undo]);
    assert_eq!(text_of(&view), "two\r\none\r\nthree", "one step each");
    let mut block = code_view("a\r\nb\r\nc\r\n", true);
    run(
        &mut block,
        &[
            select(Step::Down),
            select(Step::End),
            Step::MoveLines(false),
        ],
    );
    assert_eq!(
        text_of(&block),
        "c\r\na\r\nb\r\n",
        "the block's lines together"
    );
    assert!(block.block().unwrap().is_some(), "the block with them");
}

#[test]
fn brackets_and_quotes_close_as_they_open_and_are_typed_over() {
    let mut view = code_view("\r\n", true);
    run(&mut view, &[Step::Type('f'), Step::Type('(')]);
    assert_eq!(text_of(&view), "f()\r\n");
    assert_eq!(view.cursor(), (0, 2), "between them");
    run(
        &mut view,
        &[Step::Type('"'), Step::Type('a'), Step::Type('"')],
    );
    assert_eq!(text_of(&view), "f(\"a\")\r\n", "the quote typed over");
    run(&mut view, &[Step::Type(')')]);
    assert_eq!(text_of(&view), "f(\"a\")\r\n", "the bracket typed over");
    assert_eq!(view.cursor(), (0, 6));
    let mut word = code_view("don\r\n", true);
    run(&mut word, &[Step::End, Step::Type('\'')]);
    assert_eq!(
        text_of(&word),
        "don'\r\n",
        "after a word's character: one quote"
    );
    let mut before = code_view("x\r\n", true);
    run(&mut before, &[Step::Type('(')]);
    assert_eq!(text_of(&before), "(x\r\n", "before a word: one bracket");
}

#[test]
fn a_block_is_wrapped_and_bs_takes_an_empty_pair() {
    let mut view = code_view("a word here\r\n", true);
    run(&mut view, &rights(2));
    run(&mut view, &[select(Step::WordRight)]);
    run(&mut view, &[Step::Type('[')]);
    assert!(text_of(&view).starts_with("a [word"), "{}", text_of(&view));
    assert_eq!(
        view.block_text().unwrap().as_deref(),
        Some("word "),
        "the block on its text"
    );
    let mut pair = code_view("\r\n", true);
    run(&mut pair, &[Step::Type('{'), Step::Backspace]);
    assert_eq!(text_of(&pair), "\r\n", "both taken");
}

#[test]
fn ctrl_shift_d_duplicates_the_lines() {
    let mut view = code_view("one\r\ntwo", true);
    run(&mut view, &[Step::Right, Step::Duplicate]);
    assert_eq!(text_of(&view), "one\r\none\r\ntwo");
    assert_eq!(view.cursor(), (5, 1), "on the copy");
    run(&mut view, &[Step::Finish, Step::Duplicate]);
    assert_eq!(
        text_of(&view),
        "one\r\none\r\ntwo\r\ntwo",
        "the last line given a break"
    );
}

#[test]
fn the_save_s_clean_up_takes_the_ending_blanks_and_breaks_the_last_line() {
    let mut view = code_view("a  \r\n\tb\t\r\n   \r\nc", true);
    run(&mut view, &[Step::CleanUp]);
    assert_eq!(text_of(&view), "a\r\n\tb\r\n\r\nc\r\n");
    run(&mut view, &[Step::Undo]);
    assert_eq!(text_of(&view), "a  \r\n\tb\t\r\n   \r\nc", "one step");
}

#[test]
fn ctrl_space_offers_the_words_that_start_as_the_typed_one() {
    let mut view = code_view("counter count Count cat\r\nco\r\n", true);
    run(&mut view, &[Step::Down, Step::End]);
    let (typed, words) = view.completions();
    assert_eq!(typed, 2);
    assert_eq!(
        words,
        ["Count", "count", "counter"],
        "the case aside, not shorter"
    );
    run(&mut view, &[Step::Complete(typed, "counter".to_string())]);
    assert_eq!(text_of(&view), "counter count Count cat\r\ncounter\r\n");
}

#[test]
fn the_screen_marks_the_pair_and_the_word_under_the_cursor() {
    let mut view = code_view("f(a, b)\r\nlet a = a2 + a;\r\n", true);
    run(&mut view, &[Step::Right]);
    let screen = view.screen(40, 5);
    assert_eq!(
        screen.pairs,
        [(0, 1), (0, 6)],
        "the bracket at the cursor and its pair"
    );
    run(&mut view, &[Step::Right]);
    let screen = view.screen(40, 5);
    assert_eq!(
        screen.words,
        [(0, 2, 3), (1, 4, 5), (1, 13, 14)],
        "the whole words only"
    );
    view.options.code_editing = false;
    assert!(view.screen(40, 5).words.is_empty(), "none without the mode");
}
