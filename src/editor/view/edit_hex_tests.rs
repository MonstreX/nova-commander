//! The editor's hex mode (ADR-0027): VC's rows, typing over bytes, undo, back to the text.

use super::super::tests::{run, text_of, view_of};
use super::super::{EditView, Step};

fn screen(view: &EditView) -> super::super::EditScreen {
    view.screen(80, 5)
}

#[test]
fn alt_f4_shows_vc_s_rows_with_the_cursor_on_the_byte() {
    let mut view = view_of("one\r\ntwo three four five six\r\n");
    run(&mut view, &[Step::Down, Step::Right, Step::Hex]);
    let shown = screen(&view);
    assert_eq!(
        shown.rows[0],
        "00000000: 6F 6E 65 0D  0A 74 77 6F  20 74 68 72  65 65 20 66  one♪◙two three f"
    );
    assert_eq!(&shown.rows[1][..12], "00000010: 6F");
    assert_eq!(shown.rows.len(), 2);
    // The cursor on byte 6 ('w'): its high half in the hex column.
    assert_eq!(shown.cursor, Some((0, 10 + 3 * 6 + 1)));
    assert_eq!(view.cursor_pos().unwrap(), 6);
    run(&mut view, &[Step::Right]);
    assert_eq!(
        screen(&view).cursor,
        Some((0, 10 + 3 * 6 + 2)),
        "its low half"
    );
    run(&mut view, &[Step::Type('\t')]);
    assert_eq!(
        screen(&view).cursor,
        Some((0, 10 + 52 + 6)),
        "the text column"
    );
    run(&mut view, &[Step::Down]);
    assert_eq!(view.cursor_pos().unwrap(), 22);
    run(&mut view, &[Step::Hex]);
    assert_eq!(view.hex_cursor(), None);
    assert_eq!(view.cursor(), (5, 17), "the text at the same byte");
}

#[test]
fn typing_overwrites_bytes_and_undo_puts_them_back() {
    let mut view = view_of("abcd");
    run(&mut view, &[Step::Hex, Step::Type('4'), Step::Type('1')]);
    assert_eq!(text_of(&view), "Abcd");
    assert_eq!(view.cursor_pos().unwrap(), 1);
    run(
        &mut view,
        &[Step::Type('x'), Step::Type('\t'), Step::Type('Z')],
    );
    assert_eq!(text_of(&view), "AZcd", "a character in the text column");
    assert_eq!(view.doc().size(), 4, "nothing put in");
    run(&mut view, &[Step::Delete, Step::Backspace, Step::Enter]);
    assert_eq!(text_of(&view), "AZcd", "Del, BS and Enter edit nothing");
    let shown = screen(&view);
    assert_eq!(
        shown.spans[0],
        vec![(10, 15), (62, 64)],
        "the bytes changed"
    );
    run(&mut view, &[Step::Undo]);
    assert_eq!(text_of(&view), "Abcd");
    assert_eq!(view.cursor_pos().unwrap(), 1, "on the byte undone");
    run(&mut view, &[Step::Undo, Step::Undo]);
    assert_eq!(text_of(&view), "abcd", "a byte's two halves are two steps");
    assert!(!view.doc().is_changed());
    run(&mut view, &[Step::Redo]);
    assert_eq!(text_of(&view), "Abcd", "the high half again");
}

#[test]
fn nothing_is_typed_past_the_end_or_in_the_lock_mode() {
    let mut view = view_of("");
    run(&mut view, &[Step::Hex, Step::Type('1')]);
    assert_eq!(text_of(&view), "");
    assert_eq!(
        screen(&view).rows,
        vec!["00000000: ".to_string() + &" ".repeat(52)]
    );
    let mut view = view_of("ab");
    view.locked = true;
    run(&mut view, &[Step::Hex, Step::Type('1')]);
    assert_eq!(text_of(&view), "ab");
}

#[test]
fn the_screen_follows_the_cursor_over_a_long_document() {
    let text = "x".repeat(16 * 100);
    let mut view = view_of(&text);
    run(&mut view, &[Step::Hex, Step::Finish]);
    let shown = screen(&view);
    assert_eq!(&shown.rows[4][..10], "00000630: ");
    assert_eq!(shown.cursor, Some((4, 10 + 3 * 15 + 3)));
    run(&mut view, &[Step::PageUp]);
    assert_eq!(view.cursor_pos().unwrap(), 1599 - 16 * 4);
    run(&mut view, &[Step::Start]);
    assert_eq!(&screen(&view).rows[0][..10], "00000000: ");
    run(&mut view, &[Step::Show(800, 1)]);
    assert_eq!(view.cursor_pos().unwrap(), 800, "what a search found");
}

#[test]
fn a_save_s_reopening_puts_the_hex_mode_back_on_its_byte() {
    let mut view = view_of("abcdef");
    run(&mut view, &[Step::HexAt(3, true)]);
    let cursor = view.hex_cursor().unwrap();
    assert_eq!((cursor.pos, cursor.text), (3, true));
}
