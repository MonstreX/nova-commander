//! The editor's view: FAR's moves (past a line's end too), typing, Enter, BS, Del, Ctrl+Y, the stream
//! block and its text, undo and redo, the screen kept on the cursor, the status line's counts.

use super::*;
use crate::editor::view::BLOCK;

pub(super) fn view_of(text: &str) -> EditView {
    let bytes = text.as_bytes();
    let mut doc = PieceTable::new(bytes.len() as u64);
    for (index, chunk) in bytes.chunks(BLOCK as usize).enumerate() {
        doc.insert_block(index as u64, chunk.to_vec());
    }
    EditView::new(doc, (Encoding::Utf8, 0), "\r\n")
}

pub(super) fn text_of(view: &EditView) -> String {
    let mut out = Vec::new();
    while (out.len() as u64) < view.doc.len() {
        out.extend_from_slice(view.doc.at(out.len() as u64).unwrap());
    }
    String::from_utf8(out).unwrap()
}

pub(super) fn run(view: &mut EditView, steps: &[Step]) {
    for step in steps {
        view.apply(step, 5).unwrap();
        view.scroll(20, 5).unwrap();
    }
}

#[test]
fn the_cursor_moves_as_far_s_past_the_line_s_end_too() {
    let mut view = view_of("one\r\ntwo three\r\n\r\nfour");
    run(&mut view, &[Step::End]);
    assert_eq!(view.cursor(), (0, 3));
    run(&mut view, &[Step::Right, Step::Right]);
    assert_eq!(view.cursor(), (0, 5), "beyond the end");
    run(&mut view, &[Step::Down]);
    assert_eq!(view.cursor(), (5, 5), "the column kept");
    run(&mut view, &[Step::Home, Step::WordRight]);
    assert_eq!(view.cursor(), (5, 4), "the next word");
    run(&mut view, &[Step::Finish]);
    assert_eq!(view.cursor(), (18, 4));
    run(&mut view, &[Step::Up, Step::Up]);
    assert_eq!(view.cursor().0, 5);
    run(&mut view, &[Step::Start]);
    assert_eq!(view.cursor(), (0, 0));
    run(&mut view, &[Step::Left]);
    assert_eq!(view.cursor(), (0, 0), "nothing before the start");
    run(&mut view, &[Step::Down, Step::Home, Step::Left]);
    assert_eq!(view.cursor(), (0, 3), "to the line before's end");
}

#[test]
fn typing_enter_bs_del_and_ctrl_y_edit_the_document() {
    let mut view = view_of("ab\r\ncd");
    run(&mut view, &[Step::Right, Step::Type('X')]);
    assert_eq!(text_of(&view), "aXb\r\ncd");
    run(&mut view, &[Step::Enter]);
    assert_eq!(text_of(&view), "aX\r\nb\r\ncd");
    assert_eq!(view.cursor(), (4, 0));
    run(&mut view, &[Step::Backspace]);
    assert_eq!(text_of(&view), "aXb\r\ncd", "the lines joined");
    assert_eq!(view.cursor(), (0, 2));
    run(
        &mut view,
        &[Step::End, Step::Right, Step::Right, Step::Delete],
    );
    assert_eq!(
        text_of(&view),
        "aXb  cd",
        "joined after spaces to the cursor"
    );
    run(&mut view, &[Step::Home, Step::Delete]);
    assert_eq!(text_of(&view), "Xb  cd");
    run(
        &mut view,
        &[Step::End, Step::Right, Step::Right, Step::Type('!')],
    );
    assert_eq!(
        text_of(&view),
        "Xb  cd  !",
        "typed past the end after spaces"
    );
    run(&mut view, &[Step::DeleteLine]);
    assert_eq!(text_of(&view), "");
}

#[test]
fn undo_and_redo_put_the_cursor_where_the_edit_was() {
    let mut view = view_of("abc");
    run(&mut view, &[Step::End, Step::Type('d'), Step::Home]);
    run(&mut view, &[Step::Undo]);
    assert_eq!(text_of(&view), "abc");
    assert_eq!(view.cursor(), (0, 3));
    run(&mut view, &[Step::Home, Step::Redo]);
    assert_eq!(text_of(&view), "abcd");
}

#[test]
fn a_stream_block_is_selected_copied_replaced_and_let_go() {
    let mut view = view_of("hello world\r\nnext");
    let select = |step: Step| Step::Select(Box::new(step));
    run(&mut view, &[select(Step::WordRight)]);
    assert_eq!(view.block_text().unwrap().as_deref(), Some("hello "));
    let screen = view.screen(20, 5);
    assert_eq!(screen.marks[0], Some((0, 6)));
    run(&mut view, &[Step::Type('X')]);
    assert_eq!(
        text_of(&view),
        "Xworld\r\nnext",
        "typing takes the block's place"
    );
    run(&mut view, &[select(Step::Right), Step::Right]);
    assert_eq!(view.block().unwrap(), None, "a move lets it go");
    run(&mut view, &[Step::SelectAll]);
    assert_eq!(
        view.block_text().unwrap().as_deref(),
        Some("Xworld\r\nnext")
    );
    run(&mut view, &[Step::Cut]);
    assert_eq!(text_of(&view), "");
    run(&mut view, &[Step::Paste("a\nb".to_string())]);
    assert_eq!(text_of(&view), "a\r\nb", "the file's breaks");
}

#[test]
fn the_screen_follows_the_cursor_and_tells_its_place() {
    let text: String = (1..=20).map(|n| format!("line {n}\r\n")).collect();
    let mut view = view_of(&text);
    for _ in 0..7 {
        run(&mut view, &[Step::Down]);
    }
    let screen = view.screen(20, 5);
    assert_eq!(
        screen.rows[0], "line 4",
        "the cursor's line on the last row"
    );
    assert_eq!(screen.cursor, Some((4, 0)));
    assert_eq!(view.place().unwrap(), (8, 21, 1, 1));
    run(&mut view, &[Step::PageDown]);
    assert_eq!(view.place().unwrap().0, 12);
    run(&mut view, &[Step::GoTo(1, 3)]);
    assert_eq!(view.cursor(), (8, 3));
    assert_eq!(view.screen(20, 5).rows[0], "line 2");
}

/// S2d.6d: FAR's "Cursor beyond end of line" off - Right at the end goes on to the next line, a
/// move down stops at the line's end.
#[test]
fn without_cursor_beyond_the_end_the_cursor_keeps_to_the_text() {
    let mut view = view_of("one\r\nfourteen\r\nab");
    view.options.cursor_beyond_eol = false;
    run(&mut view, &[Step::End, Step::Right]);
    assert_eq!(view.cursor(), (5, 0), "the next line's start");
    run(&mut view, &[Step::End, Step::Down]);
    assert_eq!(view.cursor(), (15, 2), "at the shorter line's end");
}

/// S2d.6d: persistent blocks, "Del removes blocks" off, auto indent, expanded tabs.
#[test]
fn far_s_editing_options_change_blocks_enter_and_tab() {
    let mut view = view_of("  one\r\n");
    view.options.persistent_blocks = true;
    view.options.del_removes_blocks = false;
    run(
        &mut view,
        &[Step::Select(Box::new(Step::Right)), Step::Right],
    );
    assert!(
        view.block().unwrap().is_some(),
        "a move keeps a persistent block"
    );
    run(&mut view, &[Step::Type('x')]);
    assert_eq!(
        text_of(&view),
        "  xone\r\n",
        "typing beside the block, not over it"
    );
    run(&mut view, &[Step::Delete]);
    assert_eq!(
        text_of(&view),
        "  xne\r\n",
        "Del: the character, not the block"
    );
    let mut view = view_of("  one\r\n");
    view.options.auto_indent = true;
    run(&mut view, &[Step::End, Step::Enter, Step::Type('t')]);
    assert_eq!(
        text_of(&view),
        "  one\r\n  t\r\n",
        "under the line's first character"
    );
    let mut view = view_of("ab\r\n");
    view.options.expand_tabs = 1;
    run(&mut view, &[Step::Right, Step::Type('\t')]);
    assert_eq!(text_of(&view), "a       b\r\n", "spaces to the next stop");
    let mut view = view_of("\tb\r\n");
    view.options.expand_tabs = 2;
    run(&mut view, &[Step::End, Step::Type('c')]);
    assert_eq!(
        text_of(&view),
        "        bc\r\n",
        "the edited line's tabs as spaces"
    );
}

/// S2d.6d: FAR's "Show white space": a space `·`, a tab `→` and its cells, CR `♪`, LF `◙`.
#[test]
fn white_space_is_shown_as_far_s() {
    let mut view = view_of("a b\tc\r\nd\n");
    view.set_white(true);
    let rows = view.screen(20, 5).rows;
    assert_eq!(rows[0], "a·b→    c♪◙", "the tab's five cells");
    assert_eq!(rows[1], "d◙");
    assert_eq!(view.top_number().unwrap(), 1);
}

/// S2d.6d: FAR's lock mode - the steps that edit are not done, the moves are.
#[test]
fn a_locked_view_moves_but_is_not_edited() {
    let mut view = view_of("ab\r\ncd");
    view.locked = true;
    let select = Step::Select(Box::new(Step::Right));
    let steps = [
        Step::Type('x'),
        Step::Enter,
        Step::Delete,
        Step::DeleteLine,
        Step::Paste("p".to_string()),
        select,
        Step::Cut,
        Step::Down,
        Step::Backspace,
    ];
    run(&mut view, &steps);
    assert_eq!(text_of(&view), "ab\r\ncd");
    assert_eq!(view.cursor(), (4, 1));
    view.locked = false;
    run(&mut view, &[Step::Type('x')]);
    assert_eq!(text_of(&view), "ab\r\ncxd");
}

/// S2d.6d: a kept place put back, its bytes snapped to the lines' starts.
#[test]
fn a_kept_place_is_put_back_on_the_lines() {
    let text: String = (1..=20).map(|n| format!("line {n}\r\n")).collect();
    let mut view = view_of(&text);
    run(&mut view, &[Step::Restore(43, 2, 18, 0)]);
    assert_eq!(view.cursor(), (40, 2), "line 6's start");
    assert_eq!(view.window().0, 16, "line 3 on the first row");
}

/// S2d.6d: FAR's "Select found" and "Cursor at the end" for what a search found.
#[test]
fn a_search_s_find_selected_with_the_cursor_at_its_end() {
    let mut view = view_of("one two\r\n");
    view.options.select_found = true;
    view.options.cursor_at_end = true;
    run(&mut view, &[Step::Show(4, 3)]);
    assert_eq!(view.cursor(), (0, 7));
    assert_eq!(view.block().unwrap(), Some((4, 7)));
}

/// S2d.6f: FAR's bookmarks - the cursor's place kept with its row on the screen, moved by the lines
/// put in above it, gone back to; one not set does nothing.
#[test]
fn a_bookmark_follows_its_line_and_is_gone_back_to() {
    let text: String = (1..=12).map(|n| format!("line {n}\r\n")).collect();
    let mut view = view_of(&text);
    run(&mut view, &[Step::Down, Step::Down, Step::Right]);
    run(&mut view, &[Step::SetMark(3)]);
    assert_eq!(
        view.marks[3].map(|mark| (mark.line, mark.col, mark.row)),
        Some((16, 1, 2))
    );
    run(&mut view, &[Step::Start, Step::Enter, Step::Finish]);
    run(&mut view, &[Step::GoToMark(3)]);
    let place = view.place().unwrap();
    assert_eq!((place.0, place.2), (4, 2), "line 3 is line 4 now");
    assert_eq!(view.screen(20, 5).rows[2], "line 3", "on its row");
    run(&mut view, &[Step::Start, Step::GoToMark(7)]);
    assert_eq!(view.cursor(), (0, 0), "not set");
}

/// S2d.6f: FAR's Ctrl+Up / Ctrl+Down (the screen with the cursor), Ctrl+N / Ctrl+E (the screen's
/// first / last row), Ctrl+PgUp / Ctrl+PgDn (the first / last line, the column kept).
#[test]
fn far_s_screen_moves_scroll_with_the_cursor() {
    let text: String = (1..=12).map(|n| format!("line {n}\r\n")).collect();
    let mut view = view_of(&text);
    let place = |view: &mut EditView| {
        let place = view.place().unwrap();
        (place.0, place.2, view.screen(20, 5).rows[0].clone())
    };
    run(&mut view, &[Step::Down, Step::Right, Step::ScrollDown]);
    assert_eq!(
        place(&mut view),
        (3, 2, "line 2".to_string()),
        "both a line down"
    );
    run(&mut view, &[Step::ScrollUp]);
    assert_eq!(place(&mut view), (2, 2, "line 1".to_string()));
    run(&mut view, &[Step::ScrollUp]);
    assert_eq!(
        place(&mut view),
        (1, 2, "line 1".to_string()),
        "the cursor alone"
    );
    run(&mut view, &[Step::ScreenBottom]);
    assert_eq!(place(&mut view).0, 5);
    run(&mut view, &[Step::ScreenTop]);
    assert_eq!(place(&mut view).0, 1);
    run(&mut view, &[Step::LastLine]);
    assert_eq!(
        place(&mut view),
        (13, 2, "line 9".to_string()),
        "the column kept"
    );
    for _ in 0..3 {
        run(&mut view, &[Step::ScrollDown]);
    }
    assert_eq!(place(&mut view).0, 13, "nothing past the last line");
    run(&mut view, &[Step::FirstLine]);
    assert_eq!(place(&mut view), (1, 2, "line 1".to_string()));
}

/// S2d.6f: FAR's Ctrl+BS, Ctrl+Del, Ctrl+Shift+BS, Ctrl+K, Ins's overtype and Ctrl+Q's character.
#[test]
fn far_s_line_keys_take_words_and_parts_and_type_over() {
    let mut view = view_of("one two,three\r\nx\r\n");
    run(&mut view, &[Step::End, Step::DeleteWordLeft]);
    assert_eq!(text_of(&view), "one two,\r\nx\r\n", "up to the divider");
    run(&mut view, &[Step::DeleteWordLeft]);
    assert_eq!(
        text_of(&view),
        "one \r\nx\r\n",
        "the divider and the word, up to the blank"
    );
    run(
        &mut view,
        &[Step::Paste("two three".to_string()), Step::Home],
    );
    run(&mut view, &[Step::DeleteWordRight]);
    assert_eq!(
        text_of(&view),
        "two three\r\nx\r\n",
        "the word and its blanks"
    );
    run(&mut view, &[Step::Right, Step::DeleteToEnd]);
    assert_eq!(text_of(&view), "t\r\nx\r\n");
    assert_eq!(view.cursor(), (0, 1));
    run(&mut view, &[Step::Down, Step::Home, Step::DeleteWordLeft]);
    assert_eq!(text_of(&view), "tx\r\n", "at a line's start: BS");
    run(&mut view, &[Step::DeleteToStart]);
    assert_eq!(text_of(&view), "x\r\n");
    assert_eq!(view.cursor(), (0, 0));
    run(
        &mut view,
        &[Step::Overtype, Step::Type('a'), Step::Type('b')],
    );
    assert_eq!(text_of(&view), "ab\r\n", "x typed over, b past the end");
    run(&mut view, &[Step::Overtype, Step::Quoted('\r')]);
    assert_eq!(text_of(&view), "ab\r\r\n", "a CR as it is");
}

/// S2d.6f: FAR's Ctrl+P (a copy at the cursor), Ctrl+M (moved there), Ctrl+D (taken out).
#[test]
fn far_s_block_keys_copy_move_and_take_out() {
    let select = |step: Step| Step::Select(Box::new(step));
    let mut view = view_of("abc def\r\n");
    run(&mut view, &[select(Step::Right), select(Step::Right)]);
    run(&mut view, &[Step::CopyBlock(false)]);
    assert_eq!(text_of(&view), "ababc def\r\n", "a copy at its end");
    assert_eq!(view.block().unwrap(), None, "no persistent blocks: let go");
    // A move lets a block go without FAR's persistent blocks: they carry it to another place.
    let mut view = view_of("abc def\r\n");
    view.options.persistent_blocks = true;
    run(&mut view, &[select(Step::Right), select(Step::Right)]);
    run(&mut view, &[Step::End, Step::CopyBlock(true)]);
    assert_eq!(text_of(&view), "c defab\r\n", "moved");
    assert_eq!(view.cursor(), (0, 7));
    assert_eq!(view.block_text().unwrap().as_deref(), Some("ab"));
    let mut view = view_of("abc def\r\n");
    view.options.persistent_blocks = true;
    run(
        &mut view,
        &[select(Step::Right), Step::End, Step::CopyBlock(false)],
    );
    assert_eq!(view.block_text().unwrap().as_deref(), Some("a"), "the copy");
    run(&mut view, &[Step::DeleteBlock]);
    assert_eq!(text_of(&view), "abc def\r\n");
}

/// S2d.6f: FAR's Alt+I / Alt+U - the block's lines a column right / left (a tab one column less of
/// spaces), the cursor's line without a block, the block kept, one undo step.
#[test]
fn alt_i_and_alt_u_shift_the_block_s_lines() {
    let mut view = view_of("one\r\n\ttwo\r\nthree\r\n");
    run(&mut view, &[Step::ShiftBlock(true)]);
    assert_eq!(
        text_of(&view),
        " one\r\n\ttwo\r\nthree\r\n",
        "the cursor's line"
    );
    assert_eq!(view.cursor(), (0, 1));
    let select = |step: Step| Step::Select(Box::new(step));
    run(
        &mut view,
        &[Step::Home, select(Step::Down), select(Step::Down)],
    );
    run(&mut view, &[Step::ShiftBlock(false)]);
    assert_eq!(
        text_of(&view),
        "one\r\n       two\r\nthree\r\n",
        "not the line the block ends at the start of"
    );
    assert!(view.block().unwrap().is_some(), "the block kept");
    run(&mut view, &[Step::ShiftBlock(true)]);
    assert_eq!(text_of(&view), " one\r\n        two\r\nthree\r\n");
    run(&mut view, &[Step::Undo]);
    assert_eq!(text_of(&view), "one\r\n       two\r\nthree\r\n", "one step");
}

#[test]
fn the_cursor_and_the_edits_take_wide_and_combining_characters_whole() {
    let mut view = view_of("中e\u{301}x\r\nabcd\r\n");
    run(&mut view, &[Step::Right]);
    assert_eq!(view.cursor(), (0, 2), "over the wide character's two cells");
    run(&mut view, &[Step::Right]);
    assert_eq!(view.cursor(), (0, 3), "over the letter and its accent");
    run(&mut view, &[Step::Left, Step::Delete]);
    assert_eq!(
        text_of(&view),
        "中x\r\nabcd\r\n",
        "the accent goes with its letter"
    );
    run(&mut view, &[Step::Down, Step::Right, Step::Up]);
    assert_eq!(view.cursor(), (0, 3));
    run(&mut view, &[Step::Down, Step::Home, Step::Right, Step::Up]);
    assert_eq!(view.cursor(), (0, 1), "the column kept: on the tail");
    let screen = view.screen(20, 5);
    assert_eq!(
        screen.cursor,
        Some((0, 0)),
        "shown on the character's first cell"
    );
    assert_eq!(screen.rows[0], "中x");
    run(&mut view, &[Step::Type('y')]);
    assert_eq!(
        text_of(&view),
        "y中x\r\nabcd\r\n",
        "typed before the character"
    );
}

/// A step that stops for a block not read yet after it edited leaves nothing done: its retry once the
/// block is read does its edits once (review A1: Replace "a" -> "bb" gave "bbb").
#[test]
fn a_step_stopped_by_a_block_not_read_is_done_once_on_its_retry() {
    let len = 3 * BLOCK;
    let text = "a".repeat(len as usize);
    let mut doc = PieceTable::new(len);
    doc.insert_block(0, text.as_bytes()[..BLOCK as usize].to_vec());
    doc.insert_block(2, text.as_bytes()[2 * BLOCK as usize..].to_vec());
    let mut view = EditView::new(doc, (Encoding::Utf8, 0), "\r\n");
    let at = 2 * BLOCK + 10;
    let step = Step::Replace(at, 1, "bb".to_string(), false);
    assert!(
        view.apply(&step, 5).is_err(),
        "its line's start is in block 1"
    );
    assert_eq!(view.doc.len(), len, "nothing done");
    assert!(!view.doc.is_changed());
    view.doc.insert_block(
        1,
        text.as_bytes()[BLOCK as usize..2 * BLOCK as usize].to_vec(),
    );
    view.apply(&step, 5).unwrap();
    assert_eq!(view.doc.len(), len + 1, "one replacement");
    let whole = text_of(&view);
    assert_eq!(&whole[(at - 1) as usize..][..4], "abba");
    view.apply(&Step::Undo, 5).unwrap();
    view.apply(&Step::Undo, 5).unwrap();
    assert_eq!(view.doc.len(), len, "undone");
    assert!(!view.doc.is_changed());
}

/// Review A7: an edit moves the counts past it instead of counting the document to its end again -
/// the totals as a count from the start would make them, `\r\n` pairs made or broken by an edit too.
#[test]
fn an_edit_moves_the_line_counts_past_it() {
    let text = "ab\r\ncd\ref\ngh\r\n".repeat(20_000);
    let mut view = view_of(&text);
    let total = |view: &mut EditView| view.place().unwrap().1;
    let fresh = |view: &EditView| {
        let len = view.doc.len();
        let counted = LineCounter::default().line_of(&view.doc, &view.encoding, (0, len));
        counted.unwrap() + 1
    };
    assert_eq!(total(&mut view), fresh(&view));
    view.options.expand_tabs = 2;
    let edits: [&[Step]; 10] = [
        // Review A8: the tabs' spaces put in as every edit, the counts past them moved.
        &[Step::GoTo(8, 1), Step::Insert("\t\t".to_string())],
        // A lone `\n` after a `\r`: one `\r\n` break, no new line.
        &[Step::GoTo(6, 0), Step::Insert("\n".to_string())],
        &[Step::GoTo(9, 0), Step::Insert("\r".to_string())],
        &[Step::GoTo(2, 0), Step::Type('x')],
        &[Step::GoTo(3, 1), Step::Enter],
        &[Step::GoTo(0, 2), Step::Delete],
        &[Step::GoTo(1, 0), Step::Backspace],
        &[Step::GoTo(5, 2), Step::Paste("\n".to_string())],
        &[Step::GoTo(7, 0), Step::Paste("1\r\n2\r3".to_string())],
        &[Step::GoTo(4, 0), Step::DeleteLine],
    ];
    for steps in edits {
        run(&mut view, steps);
        assert_eq!(total(&mut view), fresh(&view), "after {steps:?}");
        assert!(
            view.counter.checkpoints_kept() > 1,
            "kept, not counted again"
        );
    }
}

/// Review D5 (FAR's `m_it_LastGetLine`): lines asked by number one after another, down and up, are
/// found from the last one asked - each the right line.
#[test]
fn lines_by_number_go_on_from_the_last_one_found() {
    let text: String = (0..3000).map(|n| format!("{n}\r\n")).collect();
    let starts: Vec<u64> = std::iter::once(0)
        .chain(text.match_indices("\r\n").map(|(at, _)| at as u64 + 2))
        .collect();
    let mut view = view_of(&text);
    run(&mut view, &[Step::GoTo(1500, 0)]);
    for number in (0..3000).rev().step_by(7).chain((0..3000).step_by(11)) {
        let start = view.line_start_of(Some(number)).unwrap();
        assert_eq!(start, Some(starts[number as usize]), "line {number}");
        assert_eq!(
            view.counter.last_line,
            Some((number, starts[number as usize]))
        );
    }
    assert_eq!(
        view.line_start_of(Some(5000)).unwrap(),
        None,
        "past the last"
    );
    run(&mut view, &[Step::Type('x')]);
    assert_eq!(view.counter.last_line, None, "an edit forgets it");
}

/// Review A2 / A3 (FAR's `AddUndoData`, `undo_block`): characters typed one after another are one
/// undo step and one piece; a move starts a new step; a replacement, a character typed over and the
/// replacements of "All" are one step each.
#[test]
fn undo_steps_are_fars() {
    let mut view = view_of("one two\r\n");
    run(&mut view, &[Step::End]);
    for ch in "abcdefghij".repeat(100).chars() {
        view.apply(&Step::Type(ch), 5).unwrap();
    }
    assert_eq!(
        view.doc.counts(),
        (3, 1),
        "the original's two halves and one piece typed; one step"
    );
    run(&mut view, &[Step::Home, Step::Type('X'), Step::Type('Y')]);
    assert_eq!(view.doc.counts().1, 2, "a move: a new step");
    run(&mut view, &[Step::Undo]);
    assert_eq!(
        text_of(&view),
        format!("one two{}\r\n", "abcdefghij".repeat(100))
    );
    run(&mut view, &[Step::Undo]);
    assert_eq!(text_of(&view), "one two\r\n", "the typing at once");

    let mut view = view_of("a-a-a-a\r\n");
    let replace = |at: u64, all: bool| Step::Replace(at, 1, "bb".to_string(), all);
    run(&mut view, &[replace(0, false)]);
    assert_eq!(view.doc.counts().1, 1, "a replacement: one step");
    run(
        &mut view,
        &[replace(3, true), replace(6, true), replace(9, true)],
    );
    assert_eq!(text_of(&view), "bb-bb-bb-bb\r\n");
    assert_eq!(view.doc.counts().1, 2, "All's replacements: one step");
    run(&mut view, &[Step::Undo]);
    assert_eq!(text_of(&view), "bb-a-a-a\r\n");

    let mut view = view_of("abc\r\n");
    run(&mut view, &[Step::Overtype, Step::Type('X')]);
    assert_eq!(text_of(&view), "Xbc\r\n");
    assert_eq!(view.doc.counts().1, 1, "typed over: one step");
}

#[path = "edit_view_tests_places.rs"]
mod places;
