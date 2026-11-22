//! What an edit moves besides the text: the bookmarks past it, the line counts past it (those it took
//! counted again), a plugin's line set with the break a last line gets, a block typed over as one step.

use super::*;
use crate::editor::view::blocks::Blocks;
use crate::editor::view::line_count::breaks_between;

/// S2d.6f: a bookmark at the byte text goes in at stays (its line starts there still), one past it
/// moves on; one in the bytes taken out goes to where they were, one past them moves back.
#[test]
fn bookmarks_move_with_the_edits_before_them() {
    let mut view = view_of("ab\r\ncd\r\nef\r\ngh");
    run(&mut view, &[Step::GoTo(1, 0), Step::SetMark(1)]);
    run(&mut view, &[Step::GoTo(2, 0), Step::SetMark(2)]);
    run(&mut view, &[Step::GoTo(3, 0), Step::SetMark(3)]);
    run(&mut view, &[Step::GoTo(1, 0), Step::Type('x')]);
    let lines = |view: &EditView| view.marks.map(|mark| mark.map(|mark| mark.line));
    assert_eq!(lines(&view)[1..4], [Some(4), Some(9), Some(13)]);
    let select = |step: Step| Step::Select(Box::new(step));
    run(
        &mut view,
        &[
            Step::GoTo(0, 1),
            select(Step::Down),
            select(Step::Down),
            select(Step::Right),
            Step::Delete,
        ],
    );
    assert_eq!(text_of(&view), "a\r\ngh");
    assert_eq!(
        lines(&view)[1..4],
        [Some(1), Some(1), Some(3)],
        "in the bytes taken out: at their start"
    );
}

fn loaded(bytes: &[u8]) -> Blocks {
    let mut blocks = Blocks::new(bytes.len() as u64);
    for (index, chunk) in bytes.chunks(BLOCK as usize).enumerate() {
        blocks.insert(index as u64, chunk.to_vec());
    }
    blocks
}

/// Review A7: the counts kept past an edit moved with it - its added breaks counted, the counts in the
/// bytes it took (a `\r\n` across a checkpoint's byte too) counted again - match a count from the start.
#[test]
fn shifted_line_counts_match_a_count_from_the_start() {
    let every = 64 * 1024;
    let mut text = vec![b'a'; 3 * every + 100];
    for at in (50..text.len()).step_by(997) {
        text[at] = b'\n';
    }
    (text[every - 1], text[every]) = (b'\r', b'\n');
    (text[2 * every - 1], text[2 * every]) = (b'\r', b'\n');
    // (where, bytes taken out, bytes put in)
    let edits: [(usize, usize, &[u8]); 5] = [
        (every - 6, 6, b""),
        (every - 1, 1, b"x\n\n"),
        (every, 0, b"\r"),
        (every - 3, 3, b"\r\r\r\r"),
        (2 * every - 1, 2, b"y"),
    ];
    let units = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|&byte| u32::from(byte))
            .collect::<Vec<_>>()
    };
    for (pos, removed, added) in edits {
        let mut counter = LineCounter::default();
        let end = text.len() as u64;
        counter
            .line_of(&loaded(&text), &Encoding::Utf8, (0, end))
            .unwrap();
        let prev = pos.checked_sub(1).map(|at| u32::from(text[at]));
        let next = text.get(pos + removed).map(|&byte| u32::from(byte));
        let lines = breaks_between(prev, &units(added), next)
            - breaks_between(prev, &units(&text[pos..pos + removed]), next);
        let mut edited = text.clone();
        edited.splice(pos..pos + removed, added.iter().copied());
        let shift = (removed as u64, added.len() as u64);
        counter.shift(pos as u64, shift, lines);
        let blocks = loaded(&edited);
        let len = edited.len() as u64;
        let near = |at: usize| (at.saturating_sub(3)..at + 4).map(|at| at as u64);
        let places = (near(pos).chain(near(every)).chain(near(2 * every)))
            .chain(near(2 * every + added.len() - removed))
            .chain([len]);
        for at in places.filter(|at| *at <= len) {
            let fresh = LineCounter::default().line_of(&blocks, &Encoding::Utf8, (0, at));
            let moved = counter.line_of(&blocks, &Encoding::Utf8, (0, at));
            assert_eq!(moved, fresh, "byte {at} after {pos} -{removed} +{added:?}");
        }
    }
}

/// Review D4 (FAR's `ECTL_SETSTRING`): a line set keeps its break; the last line, without one, gets
/// the break given, else the file's; Expand tabs gives it none.
#[test]
fn a_plugins_line_gets_the_break_a_last_line_has_not() {
    let set = |text: &str, start: u64, eol: Option<&str>| {
        let mut view = view_of(text);
        let line = "NEW".to_string();
        run(
            &mut view,
            &[Step::SetLineText(start, line, eol.map(str::to_string))],
        );
        text_of(&view)
    };
    assert_eq!(set("one\r\ntwo", 5, Some("\n")), "one\r\nNEW\n");
    assert_eq!(set("one\r\ntwo", 5, None), "one\r\nNEW\r\n", "the file's");
    assert_eq!(
        set("one\ntwo\n", 0, Some("\r")),
        "NEW\ntwo\n",
        "its own kept"
    );
    let mut view = view_of("x\r\na\tb");
    run(&mut view, &[Step::ExpandTabs(3)]);
    assert_eq!(text_of(&view), "x\r\na       b");
}

/// Review A2: text typed over a block - the block's going and the text - is one undo step.
#[test]
fn a_block_typed_over_is_one_undo_step() {
    let mut view = view_of("hello world\r\n");
    let select = |step: Step| Step::Select(Box::new(step));
    run(&mut view, &[select(Step::WordRight), Step::Type('X')]);
    assert_eq!(text_of(&view), "Xworld\r\n");
    assert_eq!(view.doc.counts().1, 1);
    run(&mut view, &[Step::Undo]);
    assert_eq!(text_of(&view), "hello world\r\n");
}

/// Review A4: a plugin's line set above them moves the cursor's line, the screen's first one and the
/// block's ends with its bytes.
#[test]
fn an_edit_above_moves_the_cursor_the_screen_and_the_block() {
    let mut view = view_of("ab\r\ncd\r\nef\r\n");
    run(&mut view, &[Step::Restore(8, 1, 8, 0), Step::MarkLine]);
    let line = "LONGER".to_string();
    run(&mut view, &[Step::SetLineText(0, line, None)]);
    assert_eq!(text_of(&view), "LONGER\r\ncd\r\nef\r\n");
    assert_eq!((view.cursor(), view.window().0), ((12, 1), 12));
    assert_eq!(view.block_text().unwrap().as_deref(), Some("ef\r\n"));
}

/// The document's lines counted again from its start (the totals' check).
fn fresh_total(view: &EditView) -> u64 {
    let len = view.doc.len();
    let counted = LineCounter::default().line_of(&view.doc, &view.encoding, (0, len));
    counted.unwrap() + 1
}

/// Review A7: an edit keeps the counts past it, moved, not forgotten; one unit of a `\r\n` taken out
/// leaves the other one a break of its own.
#[test]
fn an_edit_keeps_the_counts_past_it_and_half_a_break_is_one() {
    let text = format!("ab\r\ncd\r\n{}", "x\n".repeat(70_000));
    let mut view = view_of(&text);
    assert_eq!(view.place().unwrap().1, fresh_total(&view));
    let kept = view.counter.checkpoints_kept();
    assert!(kept > 1);
    let edits: [&[Step]; 3] = [
        &[Step::GoTo(0, 1), Step::Type('y')],
        &[Step::GoTo(1, 0), Step::Enter],
        &[Step::GoTo(0, 0), Step::Delete],
    ];
    for steps in edits {
        run(&mut view, steps);
        assert_eq!(view.counter.checkpoints_kept(), kept, "after {steps:?}");
    }
    let cr = text_of(&view).find('\r').unwrap() as u64;
    run(&mut view, &[Step::Replace(cr, 1, String::new(), false)]);
    assert_eq!(view.place().unwrap().1, fresh_total(&view), "a lone \\n");
    let lf = text_of(&view).find("\r\n").unwrap() as u64 + 1;
    run(&mut view, &[Step::Replace(lf, 1, String::new(), false)]);
    assert_eq!(view.place().unwrap().1, fresh_total(&view), "a lone \\r");
}

/// Review A1: a step stopped for a block not read gives back the screen's first line and the bookmarks
/// its edits moved (the cursor's line read, so that it stops after its edits: the line holding them
/// starts in block 1).
#[test]
fn a_stopped_step_gives_back_the_places_it_moved() {
    let len = 3 * BLOCK;
    let text = vec![b'a'; len as usize];
    let mut doc = PieceTable::new(len);
    doc.insert_block(0, text[..BLOCK as usize].to_vec());
    doc.insert_block(2, text[2 * BLOCK as usize..].to_vec());
    let mut view = EditView::new(doc, (Encoding::Utf8, 0), "\r\n");
    let past = 3 * BLOCK - 10;
    view.top = past;
    view.marks[0] = Some(Mark {
        line: past,
        ..Mark::default()
    });
    let step = Step::Replace(2 * BLOCK + 10, 1, "bb".to_string(), false);
    assert!(
        view.apply(&step, 5).is_err(),
        "its line's start is in block 1"
    );
    assert_eq!((view.line, view.top), (0, past));
    assert_eq!(view.marks[0].map(|mark| mark.line), Some(past));
}

/// R7 review F4: Alt+F8 far into a document too big to keep whole, its blocks brought as a reader
/// brings them and the first gone before the far ones come, gets there with each block brought once -
/// it walked from the start at each retry, for ever; a plugin's far line the same.
#[test]
fn a_far_line_of_a_big_document_is_gone_to_as_its_blocks_come() {
    let len = crate::editor::view::KEPT_WHOLE + BLOCK;
    // Lines of 8 bytes: 8192 in a block.
    let block = |index: u64| -> Vec<u8> {
        let size = usize::try_from(BLOCK.min(len - index * BLOCK)).unwrap();
        b"1234567\n".iter().copied().cycle().take(size).collect()
    };
    let mut view = EditView::new(PieceTable::new(len), (Encoding::Utf8, 0), "\r\n");
    let target = 100 * BLOCK / 8 + 5;
    let mut brought = 0;
    while let Err(Missing(index)) = view.apply(&Step::GoTo(target, 0), 5) {
        view.doc.insert_block(index, block(index));
        brought += 1;
        assert!(brought < 110, "a block brought again: {index}");
    }
    assert_eq!(view.cursor(), (target * 8, 0));
    let plugin = target + 3000;
    let found = loop {
        match view.line_start_of(Some(plugin)) {
            Ok(found) => break found,
            Err(Missing(index)) => view.doc.insert_block(index, block(index)),
        }
        brought += 1;
        assert!(brought < 120, "blocks brought again: {brought}");
    };
    assert_eq!(found, Some(plugin * 8));
}
