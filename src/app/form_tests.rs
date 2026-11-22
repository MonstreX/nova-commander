//! Forms: focus, buttons, hotkeys, check boxes, radio buttons, drop-down lists.

use super::*;

fn form() -> Form {
    let radio = |label: &str, checked, group| Control::Radio {
        label: label.to_string(),
        checked,
        group,
    };
    Form::new(
        "Copy",
        76,
        12,
        vec![
            Item::new(5, 2, Control::Text("Copy \"a\" t&o:".to_string())),
            Item::new(5, 3, Control::Edit(LineInput::with_text("D:\\"))).wide(66),
            Item::new(5, 4, radio("Defau&lt", true, true)),
            Item::new(20, 4, radio("Co&py", false, false)),
            Item::new(30, 4, radio("&Inherit", false, false)),
            Item::new(5, 5, Control::Text("Already e&xisting files:".to_string())),
            Item::new(
                29,
                5,
                Control::DropDown {
                    lines: vec![
                        ListLine::Choice("&Ask".to_string()),
                        ListLine::Choice("&Overwrite".to_string()),
                        ListLine::Separator,
                        ListLine::Toggle {
                            label: "Also ask on &R/O files".to_string(),
                            checked: false,
                        },
                    ],
                    chosen: 0,
                    open: None,
                    caption: None,
                    bottom_title: None,
                },
            )
            .wide(42),
            Item::new(
                5,
                6,
                Control::Check {
                    label: "Cop&y contents of symbolic links".to_string(),
                    checked: false,
                },
            ),
            Item::new(
                5,
                7,
                Control::Check {
                    label: "&Use filter".to_string(),
                    checked: false,
                },
            )
            .disabled(true),
            Item::new(
                0,
                9,
                Control::Button {
                    label: "&Copy".to_string(),
                    default: true,
                },
            ),
            Item::new(
                0,
                9,
                Control::Button {
                    label: "Ca&ncel".to_string(),
                    default: false,
                },
            ),
        ],
    )
}

fn press(form: &mut Form, code: KeyCode, modifiers: KeyModifiers) -> FormOutcome {
    form.handle_key(
        KeyEvent::new(code, modifiers),
        super::super::clipboard::for_app().as_mut(),
    )
}

const NONE: KeyModifiers = KeyModifiers::NONE;
const ALT: KeyModifiers = KeyModifiers::ALT;

#[test]
fn tab_and_arrows_walk_the_items_that_take_the_focus_round_the_end() {
    let mut form = form();
    assert_eq!(form.focus, 1, "the edit line first");
    let walk: Vec<usize> = (0..7)
        .map(|_| {
            press(&mut form, KeyCode::Tab, NONE);
            form.focus
        })
        .collect();
    assert_eq!(walk, [2, 3, 4, 6, 7, 9, 10], "not text, not disabled");
    press(&mut form, KeyCode::Down, NONE);
    assert_eq!(form.focus, 1, "round the end");
    press(&mut form, KeyCode::BackTab, NONE);
    assert_eq!(form.focus, 10);
    press(&mut form, KeyCode::Left, NONE);
    assert_eq!(form.focus, 9, "between buttons");
}

#[test]
fn enter_presses_the_default_button_unless_another_has_the_focus() {
    let mut form = form();
    assert_eq!(
        press(&mut form, KeyCode::Enter, NONE),
        FormOutcome::Button(9)
    );
    form.focus = 10;
    assert_eq!(
        press(&mut form, KeyCode::Enter, NONE),
        FormOutcome::Button(10)
    );
    assert_eq!(press(&mut form, KeyCode::Esc, NONE), FormOutcome::Cancel);
}

#[test]
fn typing_goes_to_the_edit_line_and_hotkeys_act_on_their_items() {
    let mut form = form();
    press(&mut form, KeyCode::Char('x'), NONE);
    assert_eq!(form.edit_text(1), "D:\\x");
    press(&mut form, KeyCode::Char('y'), ALT);
    assert!(form.is_checked(7));
    press(&mut form, KeyCode::Char(' '), NONE);
    assert!(!form.is_checked(7), "Space toggles");
    press(&mut form, KeyCode::Char('o'), ALT);
    assert_eq!(form.focus, 1, "a text's hotkey: the item after it");
    press(&mut form, KeyCode::Char('u'), ALT);
    assert!(!form.is_checked(8), "disabled");
    assert_eq!(
        press(&mut form, KeyCode::Char('n'), ALT),
        FormOutcome::Button(10)
    );
}

#[test]
fn away_from_the_edit_line_a_plain_letter_is_a_hotkey() {
    let mut form = form();
    form.focus = 7;
    press(&mut form, KeyCode::Char('y'), NONE);
    assert!(form.is_checked(7), "the check box's own letter, no Alt");
    press(&mut form, KeyCode::Char(' '), NONE);
    assert!(!form.is_checked(7), "Space still toggles the focused item");
    press(&mut form, KeyCode::Char('q'), NONE);
    assert_eq!(form.focus, 7, "a letter no item has does nothing");
    assert_eq!(
        press(&mut form, KeyCode::Char('n'), KeyModifiers::CONTROL),
        FormOutcome::Stay,
        "Ctrl+letter is no hotkey"
    );
    assert_eq!(
        press(&mut form, KeyCode::Char('n'), NONE),
        FormOutcome::Button(10)
    );
    form.focus = 1;
    press(&mut form, KeyCode::Char('n'), NONE);
    assert_eq!(form.edit_text(1), r"D:\n", "in the edit line it is typed");
}

#[test]
fn a_radio_button_unchecks_the_others_of_its_group() {
    let mut form = form();
    press(&mut form, KeyCode::Char('i'), ALT);
    assert_eq!(
        [form.is_checked(2), form.is_checked(3), form.is_checked(4)],
        [false, false, true]
    );
    form.focus = 3;
    press(&mut form, KeyCode::Char(' '), NONE);
    assert_eq!(
        [form.is_checked(2), form.is_checked(3), form.is_checked(4)],
        [false, true, false]
    );
}

#[test]
fn a_drop_down_list_opens_chooses_and_toggles_its_marked_line() {
    let mut form = form();
    press(&mut form, KeyCode::Char('x'), ALT);
    assert_eq!(form.focus, 6);
    press(&mut form, KeyCode::Down, ALT);
    press(&mut form, KeyCode::Down, NONE);
    press(&mut form, KeyCode::Enter, NONE);
    assert_eq!(form.chosen(6), 1, "Overwrite");
    assert!(matches!(
        form.control(6),
        Some(Control::DropDown { open: None, .. })
    ));

    press(&mut form, KeyCode::Down, ALT);
    press(&mut form, KeyCode::Down, NONE);
    assert!(
        matches!(
            form.control(6),
            Some(Control::DropDown { open: Some(list), .. }) if list.cursor == 3
        ),
        "the separator is skipped"
    );
    press(&mut form, KeyCode::Enter, NONE);
    assert!(form.line_checked(6, 3));
    assert_eq!(form.chosen(6), 1, "the choice stays");

    press(&mut form, KeyCode::Down, ALT);
    press(&mut form, KeyCode::Char('a'), NONE);
    assert_eq!(form.chosen(6), 0, "a line's hotkey chooses it");
    press(&mut form, KeyCode::Down, ALT);
    assert_eq!(
        press(&mut form, KeyCode::Esc, NONE),
        FormOutcome::Stay,
        "Esc closes the list"
    );
    assert_eq!(press(&mut form, KeyCode::Esc, NONE), FormOutcome::Cancel);
}

/// A list of a line to choose, a section and twenty lines that take marks.
fn long_list() -> Form {
    let mut lines = vec![
        ListLine::Choice("first".to_string()),
        ListLine::Section("Part".to_string()),
    ];
    lines.extend((0..20).map(|n| ListLine::Marked {
        label: format!("page {n}"),
        checked: false,
    }));
    let list = Control::DropDown {
        lines,
        chosen: 0,
        open: None,
        caption: None,
        bottom_title: None,
    };
    Form::new("List", 40, 8, vec![Item::new(5, 2, list).wide(20)])
}

fn cursor(form: &Form) -> usize {
    match form.control(0) {
        Some(Control::DropDown {
            open: Some(list), ..
        }) => list.cursor,
        _ => panic!("the list is not open"),
    }
}

#[test]
fn an_open_list_pages_by_the_lines_it_shows_over_the_sections() {
    let mut form = long_list();
    press(&mut form, KeyCode::Down, ALT);
    press(&mut form, KeyCode::PageDown, NONE);
    assert_eq!(cursor(&form), LIST_ROWS);
    press(&mut form, KeyCode::PageUp, NONE);
    assert_eq!(cursor(&form), 0);
    press(&mut form, KeyCode::PageUp, NONE);
    assert_eq!(cursor(&form), 0, "not past the top");
    press(&mut form, KeyCode::Down, NONE);
    assert_eq!(cursor(&form), 2, "the section is passed over");
    press(&mut form, KeyCode::Up, NONE);
    press(&mut form, KeyCode::PageDown, NONE);
    press(&mut form, KeyCode::PageDown, NONE);
    press(&mut form, KeyCode::PageDown, NONE);
    assert_eq!(cursor(&form), 21, "not past the end");
    press(&mut form, KeyCode::PageUp, NONE);
    assert_eq!(cursor(&form), 13);
    // Up a page onto the section: off it, down to the line after it.
    form.set_open_line(9);
    press(&mut form, KeyCode::PageUp, NONE);
    assert_eq!(cursor(&form), 2);
}

#[test]
fn space_and_ins_mark_a_line_and_go_on_to_the_next() {
    let mut form = long_list();
    press(&mut form, KeyCode::Down, ALT);
    press(&mut form, KeyCode::Char(' '), NONE);
    assert_eq!(cursor(&form), 0, "the first line takes no mark");
    press(&mut form, KeyCode::Down, NONE);
    press(&mut form, KeyCode::Char(' '), NONE);
    assert_eq!(cursor(&form), 3);
    press(&mut form, KeyCode::Insert, NONE);
    assert_eq!(cursor(&form), 4);
    assert!(form.line_checked(0, 2) && form.line_checked(0, 3));
    press(&mut form, KeyCode::Up, NONE);
    press(&mut form, KeyCode::Insert, NONE);
    assert!(!form.line_checked(0, 3), "the mark taken away");
    press(&mut form, KeyCode::End, NONE);
    press(&mut form, KeyCode::Char(' '), NONE);
    assert_eq!(cursor(&form), 21, "the last line stays");
    assert!(form.line_checked(0, 21));
    press(&mut form, KeyCode::Enter, NONE);
    assert_eq!(form.chosen(0), 21, "a marked line is chosen as any");
    assert!(form.line_checked(0, 2), "the marks stay");
}

#[test]
fn a_mark_before_the_last_separator_keeps_the_cursor() {
    let lines = vec![
        ListLine::Choice("first".to_string()),
        ListLine::Marked {
            label: "page".to_string(),
            checked: false,
        },
        ListLine::Section("End".to_string()),
    ];
    let list = Control::DropDown {
        lines,
        chosen: 0,
        open: None,
        caption: None,
        bottom_title: None,
    };
    let mut form = Form::new("List", 40, 8, vec![Item::new(5, 2, list).wide(20)]);
    press(&mut form, KeyCode::Down, ALT);
    press(&mut form, KeyCode::Down, NONE);
    press(&mut form, KeyCode::Char(' '), NONE);
    assert!(form.line_checked(0, 1));
    assert_eq!(cursor(&form), 1, "not onto the separator");
    press(&mut form, KeyCode::PageDown, NONE);
    assert_eq!(
        cursor(&form),
        1,
        "a page down ends on the last line, not the separator"
    );
}

#[test]
fn a_password_line_types_but_never_gives_its_text_to_the_clipboard() {
    let mut form = Form::new(
        "Password",
        40,
        6,
        vec![
            Item::new(5, 2, Control::Edit(LineInput::with_text("secret")))
                .wide(20)
                .password(),
        ],
    );
    let clipboard = super::super::clipboard::Memory::default();
    let key = |form: &mut Form, code, modifiers| {
        let mut shared = clipboard.clone();
        form.handle_key(KeyEvent::new(code, modifiers), &mut shared);
    };
    key(&mut form, KeyCode::Home, KeyModifiers::SHIFT);
    for (code, modifiers) in [
        (KeyCode::Char('c'), KeyModifiers::CONTROL),
        (KeyCode::Insert, KeyModifiers::CONTROL),
        (KeyCode::Char('x'), KeyModifiers::CONTROL),
        (KeyCode::Delete, KeyModifiers::SHIFT),
    ] {
        key(&mut form, code, modifiers);
        assert_eq!(clipboard.contents().text, None, "{code:?}");
    }
    assert_eq!(form.edit_text(0), "secret", "nothing cut");
    key(&mut form, KeyCode::End, NONE);
    key(&mut form, KeyCode::Char('!'), NONE);
    assert_eq!(form.edit_text(0), "secret!", "typing goes on");
    let mut plain = Form::new(
        "Plain",
        40,
        6,
        vec![Item::new(5, 2, Control::Edit(LineInput::with_text("open"))).wide(20)],
    );
    let mut shared = clipboard.clone();
    plain.handle_key(
        KeyEvent::new(KeyCode::Home, KeyModifiers::SHIFT),
        &mut shared,
    );
    plain.handle_key(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        &mut shared,
    );
    assert_eq!(
        clipboard.contents().text.as_deref(),
        Some("open"),
        "a plain line copies"
    );
}
