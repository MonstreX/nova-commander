use super::*;

/// Items from column 5 of row 2 on: a label its text's width, a check box four more, an edit line to the
/// row's end (one less with a history's arrow); the size the widest row with the margins.
#[test]
fn items_go_along_their_rows() {
    let mut layout = Layout::new("Title", 20);
    let label = layout.label("&Name:");
    let edit = layout.edit(DialogItem::edit("x"), 0);
    layout.new_line();
    let check = layout.boxed(DialogItem::check("&Check", false));
    layout.spacer(2);
    layout.pad(30);
    let wide = layout.label("end");
    layout.new_line();
    layout.new_line();
    let history = layout.edit(DialogItem::edit("").history("h", false), 10);
    layout.new_line();
    let dialog = layout.dialog();
    assert_eq!(dialog.items[label].place, (5, 2, 9, 2));
    assert_eq!(dialog.items[edit].place, (10, 2, 24, 2), "to the row's end");
    assert_eq!(dialog.items[check].place, (5, 3, 13, 3));
    assert_eq!(dialog.items[wide].place, (35, 3, 37, 3), "padded to 30");
    assert_eq!(dialog.items[history].place, (5, 5, 13, 5), "its arrow");
    assert_eq!(dialog.size, Some((43, 8)), "33 wide, 4 rows");
}

/// FAR's `DIF_CENTERGROUP`: the row's buttons centred with one column between them.
#[test]
fn buttons_are_centred() {
    let mut layout = Layout::new("T", 20);
    let group = layout.buttons(vec![DialogItem::button("OK"), DialogItem::button("Cancel")]);
    layout.new_line();
    let dialog = layout.dialog();
    // 6 + 1 + 10 = 17 of 20: from column 1.
    assert_eq!(dialog.items[group[0]].place, (6, 2, 11, 2));
    assert_eq!(dialog.items[group[1]].place, (13, 2, 22, 2));
}

/// arclite's `fit_str`: the middle cut out for `...`.
#[test]
fn a_long_text_is_cut_in_its_middle() {
    assert_eq!(fit("short", 10), "short");
    assert_eq!(fit("abcdefghij", 7), "ab...ij");
    assert_eq!(fit("abcdefghij", 8), "ab...hij");
    assert_eq!(fit("abcdef", 2), "ab");
    assert_eq!(label_len("Pass&word:"), 9);
    assert_eq!(label_len("a&&b"), 3, "&& one & shown");
}

/// `fit`'s edges: a text as long as the size as it is, one longer cut; three or less no `...`.
#[test]
fn fit_at_its_edges() {
    assert_eq!(fit("abcde", 5), "abcde", "exactly the size");
    assert_eq!(fit("abcdef", 5), "a...f", "one more");
    assert_eq!(fit("abcdef", 4), "...f", "the tail takes the odd one");
    assert_eq!(fit("abcdef", 3), "abc", "no room for ...");
    assert_eq!(fit("abcdef", 0), "");
    assert_eq!(
        fit("\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}", 5),
        "\u{e9}...\u{e9}"
    );
}

/// `HiStrlen`: a lone `&` not shown wherever it is; `&&` one `&`, three of them one shown.
#[test]
fn a_labels_width_counts_what_is_shown() {
    assert_eq!(label_len(""), 0);
    assert_eq!(label_len("&"), 0);
    assert_eq!(label_len("ab&"), 2);
    assert_eq!(label_len("&&"), 1);
    assert_eq!(label_len("&&&a"), 2, "&& then the hotkey a");
    assert_eq!(label_len("&a&b"), 2, "every lone & hidden");
    assert_eq!(label_len("plain"), 5);
    assert_eq!(label_len("\u{e9}t\u{e9}"), 3, "characters, not bytes");
}

/// A title wider than asked widens the dialog; `pad` to a column passed already stays; a separator spans
/// the row; the rows count once each.
#[test]
fn the_dialog_takes_its_titles_width_and_pad_never_goes_back() {
    let mut layout = Layout::new("A rather long title here", 10);
    let first = layout.label("abcdef");
    layout.pad(3);
    let second = layout.label("x");
    layout.new_line();
    let line = layout.separator();
    layout.new_line();
    let button = layout.boxed(DialogItem::button("&OK"));
    layout.new_line();
    let dialog = layout.dialog();
    assert_eq!(dialog.items[first].place, (5, 2, 10, 2));
    assert_eq!(dialog.items[second].place, (11, 2, 11, 2), "right after");
    assert_eq!(dialog.items[line].place, (-1, 3, -1, 3));
    assert_eq!(
        dialog.items[button].place,
        (5, 4, 10, 4),
        "OK and four more"
    );
    assert_eq!(dialog.size, Some((24 + 10, 3 + 4)), "the title's 24");
}

/// A group of one centred, no gap counted after it.
#[test]
fn a_single_button_is_centred_too() {
    let mut layout = Layout::new("", 12);
    let group = layout.buttons(vec![DialogItem::button("Go")]);
    layout.new_line();
    let dialog = layout.dialog();
    // 6 of 12: 3 before it (7 with a gap counted: 2).
    assert_eq!(dialog.items[group[0]].place, (8, 2, 13, 2));
    assert_eq!(dialog.size, Some((22, 5)));
}

/// An edit line of a width moves along by it; one to the row's end after another row widened it.
#[test]
fn an_edit_line_ends_where_it_says() {
    let mut layout = Layout::new("", 5);
    let narrow = layout.edit(DialogItem::edit(""), 4);
    let after = layout.label("z");
    layout.new_line();
    layout.label("0123456789");
    layout.new_line();
    let whole = layout.edit(DialogItem::edit(""), 0);
    layout.new_line();
    let dialog = layout.dialog();
    assert_eq!(dialog.items[narrow].place, (5, 2, 8, 2));
    assert_eq!(dialog.items[after].place, (9, 2, 9, 2));
    assert_eq!(
        dialog.items[whole].place,
        (5, 4, 14, 4),
        "the row's end: 10 wide"
    );
}
