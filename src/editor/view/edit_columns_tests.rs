//! FAR's vertical blocks: marked with Alt+Shift and the moves (turning over, past a line's end, by
//! words, to the file's end), their text, taken out, put in, shifted, copied and moved, their lines
//! kept through Enter and Ctrl+Y; the stream block and the vertical one let each other go.

use super::tests::{run, text_of, view_of};
use super::*;

fn column(step: Step) -> Step {
    Step::Column(Box::new(step))
}

/// The block's lines (their first bytes), first column and columns.
fn span(view: &EditView) -> Option<(u64, u64, usize, usize)> {
    let column = view.column_span().unwrap()?;
    Some((column.first, column.last, column.x, column.width))
}

#[test]
fn alt_shift_moves_mark_far_s_vertical_block_and_its_text() {
    let mut view = view_of("abcd\r\nefgh\r\nijkl\r\n");
    run(
        &mut view,
        &[Step::Right, column(Step::Right), column(Step::Right)],
    );
    run(&mut view, &[column(Step::Down)]);
    assert_eq!(span(&view), Some((0, 6, 1, 2)));
    assert_eq!(view.cursor(), (6, 3));
    assert_eq!(view.block_text().unwrap().as_deref(), Some("bc\r\nfg\r\n"));
    let screen = view.screen(20, 5);
    assert_eq!(&screen.marks[..3], [Some((1, 3)), Some((1, 3)), None]);
    run(&mut view, &[column(Step::Left), column(Step::Left)]);
    assert_eq!(span(&view), Some((0, 6, 1, 0)), "narrower to nothing");
    run(&mut view, &[column(Step::Left)]);
    assert_eq!(span(&view), Some((0, 6, 0, 1)), "over to the left side");
    run(&mut view, &[column(Step::Up)]);
    assert_eq!(span(&view), Some((0, 0, 0, 1)), "a line less at the bottom");
    run(&mut view, &[column(Step::Up)]);
    assert_eq!(
        span(&view),
        Some((0, 0, 0, 1)),
        "nothing above the first line"
    );
}

#[test]
fn a_vertical_block_past_a_line_s_end_is_padded_with_spaces() {
    let mut view = view_of("ab\r\nabcdef");
    run(&mut view, &[Step::Right, column(Step::Down)]);
    for _ in 0..4 {
        run(&mut view, &[column(Step::Right)]);
    }
    assert_eq!(
        view.block_text().unwrap().as_deref(),
        Some("b   \r\nbcde\r\n")
    );
    let screen = view.screen(20, 5);
    assert_eq!(screen.marks[0], Some((1, 5)), "past the end marked");
    run(&mut view, &[column(Step::End)]);
    assert_eq!(view.cursor(), (4, 6));
    assert_eq!(span(&view), Some((0, 4, 1, 5)));
}

#[test]
fn ctrl_alt_moves_mark_by_words_and_to_the_file_s_end() {
    let mut view = view_of("foo bar\r\nbaz qux\r\nend\r\n");
    run(&mut view, &[column(Step::WordRight)]);
    assert_eq!(span(&view), Some((0, 0, 0, 3)), "the word");
    run(&mut view, &[column(Step::WordRight)]);
    assert_eq!(span(&view), Some((0, 0, 0, 7)), "the blank, then the word");
    run(&mut view, &[column(Step::WordLeft)]);
    assert_eq!(span(&view), Some((0, 0, 0, 4)));
    run(&mut view, &[column(Step::Finish)]);
    assert_eq!(view.cursor().0, 23, "the last line");
    assert_eq!(span(&view).map(|span| span.1), Some(23));
}

#[test]
fn ctrl_d_takes_the_columns_out_and_undo_puts_them_back_at_once() {
    let mut view = view_of("abcd\r\nefgh\r\nij\r\n");
    run(
        &mut view,
        &[Step::Right, column(Step::Down), column(Step::Down)],
    );
    run(&mut view, &[column(Step::Right), column(Step::Right)]);
    run(&mut view, &[Step::DeleteBlock]);
    assert_eq!(text_of(&view), "ad\r\neh\r\ni\r\n");
    assert_eq!(view.cursor(), (0, 1), "at the block's top left");
    assert!(!view.is_column(), "let go");
    run(&mut view, &[Step::Undo]);
    assert_eq!(text_of(&view), "abcd\r\nefgh\r\nij\r\n", "one undo step");
}

#[test]
fn a_vertical_block_is_put_in_at_the_cursor_s_column_line_by_line() {
    let mut view = view_of("abc\r\ndef");
    run(
        &mut view,
        &[Step::Right, Step::PasteColumn("XY\r\nZW\r\n".into())],
    );
    assert_eq!(text_of(&view), "aXYbc\r\ndZWef");
    assert_eq!(view.cursor(), (0, 1), "back at its top left");
    assert!(!view.is_column(), "FAR's Shift+Ins lets it go");
    let mut view = view_of("ab");
    run(
        &mut view,
        &[Step::Right, Step::PasteColumn("1\r\n2\r\n3\r\n".into())],
    );
    assert_eq!(text_of(&view), "a1b\r\n 2\r\n 3", "lines added at the end");
    let mut view = view_of("abc\r\ndef\r\n");
    view.options.persistent_blocks = true;
    run(
        &mut view,
        &[Step::End, Step::PasteColumn("12\r\n3\r\n".into())],
    );
    assert_eq!(text_of(&view), "abc12\r\ndef3\r\n");
    assert_eq!(span(&view), Some((0, 7, 3, 2)), "persistent: marked");
}

#[test]
fn ctrl_p_and_ctrl_m_copy_and_move_a_persistent_vertical_block() {
    let mut view = view_of("abcd\r\nefgh\r\n");
    view.options.persistent_blocks = true;
    run(&mut view, &[column(Step::Right), column(Step::Down)]);
    run(&mut view, &[Step::Up, Step::End, Step::CopyBlock(false)]);
    assert_eq!(text_of(&view), "abcda\r\nefghe\r\n");
    assert_eq!(span(&view), Some((0, 7, 4, 1)), "the copy marked");
    run(&mut view, &[Step::Home, Step::CopyBlock(true)]);
    assert_eq!(text_of(&view), "aabcd\r\neefgh\r\n", "moved to the start");
    run(&mut view, &[Step::Undo]);
    assert_eq!(text_of(&view), "abcda\r\nefghe\r\n", "the move is one step");
}

#[test]
fn alt_i_and_alt_u_shift_a_vertical_block_a_column() {
    let mut view = view_of("abcd\r\nefgh\r\n");
    view.options.persistent_blocks = true;
    run(
        &mut view,
        &[Step::Right, column(Step::Right), column(Step::Right)],
    );
    run(&mut view, &[column(Step::Down), Step::ShiftBlock(true)]);
    assert_eq!(text_of(&view), "adbc\r\nehfg\r\n");
    assert_eq!(span(&view), Some((0, 6, 2, 2)));
    assert_eq!(view.cursor().1, 4, "at the block's right");
    run(
        &mut view,
        &[Step::ShiftBlock(false), Step::ShiftBlock(false)],
    );
    assert_eq!(text_of(&view), "bcad\r\nfgeh\r\n");
    run(&mut view, &[Step::ShiftBlock(false)]);
    assert_eq!(
        text_of(&view),
        "bcad\r\nfgeh\r\n",
        "not left of the first column"
    );
    let mut view = view_of("a\tb\r\n");
    view.options.persistent_blocks = true;
    run(&mut view, &[column(Step::Right), Step::ShiftBlock(true)]);
    assert_eq!(
        text_of(&view),
        " a      b\r\n",
        "a tab beside: the line's tabs as spaces"
    );
}

#[test]
fn enter_and_ctrl_y_change_the_vertical_block_s_lines() {
    let mut view = view_of("ab\r\ncd\r\nef\r\n");
    view.options.persistent_blocks = true;
    run(&mut view, &[column(Step::Right), column(Step::Down)]);
    run(&mut view, &[Step::End, Step::Enter]);
    assert_eq!(span(&view), Some((0, 8, 0, 1)), "the new line taken in");
    run(&mut view, &[Step::DeleteLine]);
    assert_eq!(span(&view), Some((0, 4, 0, 1)), "its last line taken off");
    run(&mut view, &[Step::Up, Step::DeleteLine]);
    assert_eq!(span(&view), Some((0, 0, 0, 1)), "one line left");
    run(&mut view, &[Step::Up, Step::DeleteLine]);
    assert_eq!(span(&view), None, "its only line gone: the block too");
}

#[test]
fn the_stream_block_and_the_vertical_one_let_each_other_go() {
    let mut view = view_of("abcd\r\nefgh\r\n");
    run(&mut view, &[column(Step::Right)]);
    run(&mut view, &[Step::Select(Box::new(Step::Right))]);
    assert!(!view.is_column());
    assert_eq!(view.block().unwrap(), Some((1, 2)));
    run(&mut view, &[column(Step::Right)]);
    assert_eq!(view.block().unwrap(), None);
    assert_eq!(span(&view), Some((0, 0, 2, 1)));
    run(&mut view, &[Step::Down]);
    assert_eq!(span(&view), None, "not persistent: a move lets it go");
    view.options.persistent_blocks = true;
    run(
        &mut view,
        &[column(Step::Right), Step::Right, Step::Type('x')],
    );
    // Down kept the column (3); after the block, Right and "x" past the line's end the cursor is at 6.
    assert_eq!(span(&view), Some((6, 6, 3, 1)), "persistent: kept");
    run(&mut view, &[column(Step::Right)]);
    assert_eq!(
        span(&view),
        Some((6, 6, 6, 1)),
        "a new one begun after another key"
    );
}
