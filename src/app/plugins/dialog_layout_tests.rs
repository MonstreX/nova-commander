//! The layout of a plugin's dialog: rows, widths, the buttons' row under a separator, the screen's limit.

use super::*;

fn item(kind: ItemKind, text: &str) -> DialogItem {
    DialogItem {
        kind,
        text: text.to_string(),
        ..DialogItem::default()
    }
}

fn request(items: Vec<DialogItem>) -> DialogRequest {
    DialogRequest {
        title: "Title".to_string(),
        items,
        ..DialogRequest::default()
    }
}

/// Each form item's column, row and width.
fn places(laid: &Laid) -> Vec<(u16, u16, u16)> {
    (laid.form.items.iter())
        .map(|item| (item.x, item.y, item.width))
        .collect()
}

#[test]
fn a_label_stands_above_its_line_and_the_buttons_end_under_a_separator() {
    let laid = lay_out(
        &request(vec![
            item(ItemKind::Text, "&Name:"),
            item(ItemKind::Edit, "start"),
            item(ItemKind::Button, "OK"),
            item(ItemKind::Button, "Cancel"),
        ]),
        100,
    );
    assert_eq!(laid.form.width, 40, "at least 40 columns");
    assert_eq!(
        laid.form.height, 8,
        "rows 2..5 and the box and margin under them"
    );
    assert_eq!(
        places(&laid),
        [(5, 2, 0), (5, 3, 30), (0, 4, 0), (5, 5, 0), (12, 5, 0)],
        "the line as wide as the dialog lets it, a separator added before the buttons"
    );
    assert!(matches!(laid.form.items[2].control, Control::Separator));
    assert_eq!(
        laid.items,
        [0, 1, 3, 4],
        "the plugin's items in the form, the separator not one"
    );
    assert!(
        laid.form.items[3..]
            .iter()
            .all(|item| item.place == Place::Centre),
        "a row of buttons is centred together"
    );
}

#[test]
fn the_plugins_own_separator_before_the_buttons_is_the_only_one() {
    let laid = lay_out(
        &request(vec![
            item(ItemKind::Text, "Text"),
            item(ItemKind::Separator, ""),
            item(ItemKind::Button, "OK"),
        ]),
        100,
    );
    let separators = (laid.form.items.iter())
        .filter(|item| matches!(item.control, Control::Separator))
        .count();
    assert_eq!(separators, 1);
    assert_eq!(laid.items, [0, 1, 2]);
    let only_buttons = lay_out(&request(vec![item(ItemKind::Button, "OK")]), 100);
    assert_eq!(
        only_buttons.form.items.len(),
        1,
        "no separator above the first row"
    );
}

#[test]
fn an_item_on_the_same_row_follows_the_one_before_and_a_line_fills_what_is_left() {
    let mut edit = item(ItemKind::Edit, "x");
    edit.same_row = true;
    let mut two = item(ItemKind::Radio, "&Two");
    two.same_row = true;
    let laid = lay_out(
        &request(vec![
            item(ItemKind::Text, "&Name:"),
            edit,
            item(ItemKind::Radio, "&One"),
            two,
        ]),
        100,
    );
    assert_eq!(
        places(&laid),
        [(5, 2, 0), (11, 2, 24), (5, 3, 0), (13, 3, 0)],
        "the line from after the label to the right margin; the radio buttons one column apart"
    );
}

#[test]
fn a_width_given_is_kept_and_a_wide_row_widens_the_dialog_up_to_the_screen() {
    let mut edit = item(ItemKind::Edit, "");
    edit.width = 50;
    let laid = lay_out(&request(vec![edit.clone()]), 100);
    assert_eq!((laid.form.width, laid.form.items[0].width), (60, 50));
    let narrow = lay_out(&request(vec![edit]), 45);
    assert_eq!(narrow.form.width, 45, "no wider than the screen");
    let long_title = DialogRequest {
        title: "A".repeat(40),
        ..request(vec![item(ItemKind::Text, "x")])
    };
    assert_eq!(
        lay_out(&long_title, 100).form.width,
        54,
        "the title and its frame fit"
    );
}

#[test]
fn a_list_without_a_width_is_as_wide_as_its_longest_line_needs_in_a_row() {
    let mut list = item(ItemKind::List, "");
    list.lines = vec![
        "Red".to_string(),
        "A rather long colour name here".to_string(),
    ];
    list.state = 7;
    let laid = lay_out(&request(vec![list]), 100);
    assert_eq!(laid.form.width, 43, "the line and its arrow");
    assert!(
        matches!(
            &laid.form.items[0].control,
            Control::DropDown { chosen: 1, lines, .. } if lines.len() == 2
        ),
        "a choice past the end is the last line"
    );
}

#[test]
fn each_kind_is_its_control_with_its_state() {
    let mut three = item(ItemKind::Check, "&Three");
    three.three_state = true;
    three.state = 2;
    let mut checked = item(ItemKind::Check, "&On");
    checked.state = 1;
    let mut radio = item(ItemKind::Radio, "&R");
    radio.group = true;
    radio.state = 1;
    let mut default = item(ItemKind::Button, "OK");
    default.default = true;
    let mut password = item(ItemKind::Password, "secret");
    password.disabled = true;
    password.hidden = true;
    let laid = lay_out(
        &request(vec![three, checked, radio, password, default]),
        100,
    );
    let controls: Vec<&Control> = laid.form.items.iter().map(|item| &item.control).collect();
    assert!(matches!(
        controls[0],
        Control::Tristate {
            state: None,
            three: true,
            ..
        }
    ));
    assert!(matches!(controls[1], Control::Check { checked: true, .. }));
    assert!(matches!(
        controls[2],
        Control::Radio {
            checked: true,
            group: true,
            ..
        }
    ));
    assert!(matches!(controls[3], Control::Edit(input) if input.text() == "secret"));
    let password = &laid.form.items[3];
    assert!(password.password && password.disabled && password.hidden);
    assert!(matches!(controls[5], Control::Button { default: true, .. }));
}

#[test]
fn a_button_among_other_items_stands_at_its_column() {
    let mut button = item(ItemKind::Button, "&Browse");
    button.same_row = true;
    let laid = lay_out(&request(vec![item(ItemKind::Edit, ""), button]), 100);
    assert_eq!(laid.form.items[1].place, Place::At);
    assert_eq!(places(&laid), [(5, 2, 19), (25, 2, 0)]);
}

#[test]
fn the_focus_is_where_the_plugin_asks() {
    let mut second = item(ItemKind::Edit, "b");
    second.focus = true;
    let laid = lay_out(&request(vec![item(ItemKind::Edit, "a"), second]), 100);
    assert_eq!(laid.form.focus, 1);
    let first = lay_out(
        &request(vec![item(ItemKind::Edit, "a"), item(ItemKind::Edit, "b")]),
        100,
    );
    assert_eq!(first.form.focus, 0, "else the first that takes it");
}

#[test]
fn a_warning_dialog_has_the_warning_colours() {
    let warning = DialogRequest {
        warning: true,
        ..request(vec![item(ItemKind::Text, "x")])
    };
    assert!(lay_out(&warning, 100).form.warning);
    assert!(!lay_out(&request(vec![]), 100).form.warning);
}

#[test]
fn hotkey_marks_take_no_column() {
    assert_eq!(shown_width("&Name"), 4);
    assert_eq!(shown_width("A&&B"), 3);
    assert_eq!(shown_width(""), 0);
}

#[test]
fn widths_and_labels_past_any_screen_never_overflow_and_stay_inside() {
    let mut edit = item(ItemKind::Edit, "");
    edit.width = u16::MAX;
    edit.same_row = true;
    let mut list = item(ItemKind::List, "");
    list.lines = vec!["x".repeat(70_000)];
    let laid = lay_out(
        &request(vec![
            item(ItemKind::Text, &"y".repeat(70_000)),
            edit,
            list,
            item(ItemKind::Check, &"z".repeat(70_000)),
        ]),
        80,
    );
    assert_eq!(laid.form.width, 80, "no wider than the screen");
    for placed in &laid.form.items {
        // What starts past the box is not drawn (`ui::form`); what starts in it ends in it.
        assert!(
            placed.x >= 75 || placed.x.saturating_add(placed.width) <= 75,
            "inside the box: {} + {}",
            placed.x,
            placed.width
        );
    }
}

#[test]
fn a_width_given_is_cut_to_the_dialog() {
    let mut edit = item(ItemKind::Edit, "");
    edit.width = 50;
    let narrow = lay_out(&request(vec![edit]), 45);
    assert_eq!((narrow.form.width, narrow.form.items[0].width), (45, 35));
}

#[test]
fn consecutive_radio_buttons_are_one_group_and_another_item_ends_it() {
    let radio = |label: &str| item(ItemKind::Radio, label);
    let laid = lay_out(
        &request(vec![
            item(ItemKind::Text, "First"),
            radio("&A"),
            radio("&B"),
            item(ItemKind::Text, "Second"),
            radio("&C"),
            radio("&D"),
        ]),
        100,
    );
    let groups: Vec<bool> = (laid.form.items.iter())
        .filter_map(|item| match item.control {
            Control::Radio { group, .. } => Some(group),
            _ => None,
        })
        .collect();
    assert_eq!(groups, [true, false, true, false]);
    let mut form = laid.form;
    form.select_radio(5);
    assert!(
        form.is_checked(5) && !form.is_checked(4),
        "D in its own group"
    );
    form.select_radio(2);
    assert!(
        form.is_checked(2) && form.is_checked(5),
        "B does not touch D's group"
    );
}

#[test]
fn a_password_line_without_a_width_fills_its_row_as_an_edit_line_does() {
    let laid = lay_out(&request(vec![item(ItemKind::Password, "pw")]), 100);
    assert_eq!(places(&laid), [(5, 2, 30)]);
}

#[test]
fn a_separator_is_a_row_of_its_own_even_on_the_same_row() {
    let mut separator = item(ItemKind::Separator, "");
    separator.same_row = true;
    let mut after = item(ItemKind::Text, "After");
    after.same_row = true;
    let laid = lay_out(
        &request(vec![item(ItemKind::Text, "Before"), separator, after]),
        100,
    );
    let rows: Vec<u16> = laid.form.items.iter().map(|item| item.y).collect();
    assert_eq!(rows, [2, 3, 4]);
}

#[test]
fn buttons_before_other_rows_get_no_separator() {
    let laid = lay_out(
        &request(vec![
            item(ItemKind::Text, "Text"),
            item(ItemKind::Button, "OK"),
            item(ItemKind::Text, "More"),
        ]),
        100,
    );
    assert_eq!(laid.items, [0, 1, 2], "no separator added");
    assert_eq!(laid.form.items.len(), 3);
}

#[test]
fn a_radio_button_first_or_marked_starts_a_group() {
    let mut second = item(ItemKind::Radio, "&B");
    second.group = true;
    let laid = lay_out(
        &request(vec![
            item(ItemKind::Radio, "&A"),
            second,
            item(ItemKind::Radio, "&C"),
        ]),
        100,
    );
    let groups: Vec<bool> = (laid.form.items.iter())
        .filter_map(|item| match item.control {
            Control::Radio { group, .. } => Some(group),
            _ => None,
        })
        .collect();
    assert_eq!(groups, [true, true, false]);
}

#[test]
fn the_focus_asked_is_the_form_item_showing_it() {
    let mut cancel = item(ItemKind::Button, "Cancel");
    cancel.focus = true;
    let laid = lay_out(
        &request(vec![
            item(ItemKind::Text, "Text"),
            item(ItemKind::Button, "OK"),
            cancel,
        ]),
        100,
    );
    assert_eq!(laid.items, [0, 2, 3], "a separator before the buttons");
    assert_eq!(laid.form.focus, 3);
}

#[test]
fn every_state_of_a_check_box_and_a_list_is_shown() {
    let state = |kind, three_state, state| DialogItem {
        three_state,
        state,
        ..item(kind, "x")
    };
    let mut list = state(ItemKind::List, false, -1);
    list.lines = vec!["Red".to_string(), "Green".to_string()];
    let mut empty = state(ItemKind::List, false, 3);
    empty.lines.clear();
    let laid = lay_out(
        &request(vec![
            state(ItemKind::Check, true, 0),
            state(ItemKind::Check, true, 1),
            state(ItemKind::Check, false, 0),
            list,
            empty,
        ]),
        100,
    );
    let form = &laid.form;
    assert_eq!(
        (form.tristate(0), form.tristate(1)),
        (Some(false), Some(true))
    );
    assert!(matches!(
        form.items[0].control,
        Control::Tristate {
            state: Some(false),
            ..
        }
    ));
    assert!(!form.is_checked(2));
    assert_eq!(form.chosen(3), 0, "a choice before the first: the first");
    assert_eq!(form.chosen(4), 0, "no lines");
}

#[test]
fn a_check_box_takes_its_mark_and_label() {
    let mut text = item(ItemKind::Text, "After");
    text.same_row = true;
    let laid = lay_out(&request(vec![item(ItemKind::Check, "&On"), text]), 100);
    assert_eq!(places(&laid), [(5, 2, 0), (12, 2, 0)], "[ ] On and a space");
}

#[test]
fn a_masked_line_is_its_masks_width_and_a_list_box_takes_its_rows_and_closes() {
    let fixed = DialogItem {
        mask: "99:99".to_string(),
        ..item(ItemKind::Fixed, "12:30")
    };
    let mut list = item(ItemKind::ListBox, "Pick");
    list.lines = vec!["One".to_string(), "Two".to_string(), "Three".to_string()];
    list.state = 2;
    let mut kept = list.clone();
    kept.no_close = true;
    kept.height = 1;
    let laid = lay_out(&request(vec![fixed, list, kept]), 100);
    let form = &laid.form;
    match form.control(laid.items[0]) {
        Some(Control::Masked(input)) => assert_eq!(input.text(), "12:30"),
        other => panic!("a masked line: {other:?}"),
    }
    let Some(Control::ListBox(shown)) = form.control(laid.items[1]) else {
        panic!("a list box");
    };
    assert_eq!((shown.title.as_str(), shown.rows), ("Pick", 3));
    assert_eq!((shown.list.cursor, shown.current()), (2, "Three"));
    let rows: Vec<u16> = laid.items.iter().map(|&at| form.items[at].y).collect();
    assert_eq!(rows, [2, 3, 8], "the box: its lines and two rows");
    assert!(form.items[laid.items[1]].closes, "Enter closes with it");
    assert!(!form.items[laid.items[2]].closes, "FAR's DIF_LISTNOCLOSE");
    assert_eq!(form.height, 8 + 3 + 2, "one row shown in the last box");
}

#[test]
fn a_line_with_a_history_leaves_a_column_for_its_arrow() {
    let mut line = item(ItemKind::Edit, "");
    line.history = Some("Names".to_string());
    line.use_last_history = true;
    let plain = item(ItemKind::Edit, "");
    let laid = lay_out(&request(vec![line, plain]), 100);
    let form = &laid.form;
    let (with, without) = (&form.items[laid.items[0]], &form.items[laid.items[1]]);
    assert_eq!(with.width + 1, without.width);
    let history = with.history.as_deref().unwrap();
    assert_eq!((history.name.as_str(), history.use_last), ("Names", true));
    assert!(without.history.is_none());
}

#[test]
fn a_dialog_given_a_size_has_its_items_at_their_own_places() {
    let place = |kind, text: &str, place| DialogItem {
        place,
        ..item(kind, text)
    };
    let mut list = place(ItemKind::ListBox, "", (5, 4, 20, 9));
    list.lines = vec!["a".to_string()];
    let laid = lay_out(
        &DialogRequest {
            size: Some((50, 12)),
            ..request(vec![
                place(ItemKind::Text, "Centre", (-1, 2, 0, 0)),
                place(ItemKind::Edit, "e", (5, 3, 24, 3)),
                list,
                place(ItemKind::Button, "OK", (30, 10, 0, 0)),
            ])
        },
        100,
    );
    assert_eq!((laid.form.width, laid.form.height), (50, 12));
    assert_eq!(
        places(&laid),
        [(22, 2, 0), (5, 3, 20), (5, 4, 16), (30, 10, 0)],
        "a text centred, a line X1 to X2, the box down to Y2"
    );
    let Some(Control::ListBox(shown)) = laid.form.control(2) else {
        panic!("a list box");
    };
    assert_eq!(shown.rows, 4, "Y1 to Y2 less its frame");
    assert_eq!(laid.form.items[3].place, Place::At);
}

#[test]
fn a_separator_with_a_text_shows_it_at_its_column_or_in_its_middle() {
    let place = |text: &str, place| DialogItem {
        place,
        ..item(ItemKind::Separator, text)
    };
    let laid = lay_out(
        &DialogRequest {
            size: Some((50, 12)),
            ..request(vec![
                place("Box", (5, 2, 40, 2)),
                place("Middle", (-1, 4, -1, 4)),
                place("", (-1, 6, -1, 6)),
            ])
        },
        100,
    );
    let titled = |index: usize| match laid.form.control(index) {
        Some(Control::TitledSeparator { title, left, .. }) => Some((title.clone(), *left)),
        _ => None,
    };
    assert_eq!(titled(0), Some(("Box".to_string(), true)));
    assert_eq!(laid.form.items[0].x, 5);
    assert_eq!(titled(1), Some(("Middle".to_string(), false)));
    assert!(matches!(laid.form.control(2), Some(Control::Separator)));
}

/// FAR's `DI_SINGLEBOX` (API 8): as wide as `X1` to `X2`, as high as `Y1` to `Y2` (one row: a line), never
/// focused.
#[test]
fn a_box_stands_at_its_place_as_high_and_wide_as_it_is_given() {
    let place = |kind, text: &str, place| DialogItem {
        place,
        ..item(kind, text)
    };
    let laid = lay_out(
        &DialogRequest {
            size: Some((50, 12)),
            ..request(vec![
                place(ItemKind::Box, "Mode", (4, 2, 24, 2)),
                place(ItemKind::Box, "Frame", (26, 2, 45, 6)),
                place(ItemKind::Check, "&Check", (5, 3, 0, 3)),
            ])
        },
        100,
    );
    let boxed = |index: usize| match laid.form.control(index) {
        Some(Control::Box { title, rows }) => Some((title.clone(), *rows)),
        _ => None,
    };
    assert_eq!(boxed(0), Some(("Mode".to_string(), 1)));
    assert_eq!(boxed(1), Some(("Frame".to_string(), 5)));
    assert_eq!(places(&laid)[..2], [(4, 2, 21), (26, 2, 20)]);
    assert_eq!(laid.form.focus, 2, "a box takes no focus");
}
