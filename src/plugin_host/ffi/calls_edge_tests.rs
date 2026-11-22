//! The host's functions at their edges: each flag by itself, the caps on what a plugin hands, a newer
//! plugin's larger menu, an index past what C counts.

use super::*;

#[test]
fn each_message_flag_is_read_by_itself() {
    for (flags, warning, left_align) in [
        (nc_plugin_api::MESSAGE_WARNING, true, false),
        (nc_plugin_api::MESSAGE_LEFT_ALIGN, false, true),
    ] {
        let (context, events) = host_context(true, &no_logs());
        let asked = program(events, Some(Answer::Button(0)));
        message(&context, flags, "a", "OK");
        let Request::Message(shown) = asked.join().unwrap() else {
            panic!("a message");
        };
        assert_eq!((shown.warning, shown.left_align), (warning, left_align));
    }
}

#[test]
fn a_message_hands_at_most_its_cap_of_lines_and_buttons() {
    let body = vec!["l"; MOST_MESSAGE_LINES + 5].join("\n");
    let buttons = vec!["b"; MOST_BUTTONS + 5].join("\n");
    let (context, events) = host_context(true, &no_logs());
    let asked = program(events, Some(Answer::Button(0)));
    message(&context, 0, &body, &buttons);
    let Request::Message(shown) = asked.join().unwrap() else {
        panic!("a message");
    };
    assert_eq!(shown.lines.len(), MOST_MESSAGE_LINES);
    assert_eq!(shown.buttons.len(), MOST_BUTTONS);
    let (context, events) = host_context(true, &no_logs());
    message(&context, 0, &body, "");
    let Ok(Event::Busy { message: shown, .. }) = events.try_recv() else {
        panic!("a window without buttons");
    };
    assert_eq!(shown.lines.len(), MOST_MESSAGE_LINES, "without buttons too");
}

#[test]
fn a_button_past_what_c_counts_is_esc() {
    assert_eq!(to_answer(Some(7)), 7);
    assert_eq!(to_answer(None), -1);
    let past = usize::try_from(i32::MAX).unwrap() + 1;
    assert_eq!(to_answer(Some(past)), -1);
    assert_eq!(to_answer(Some(past * 2 + 5)), -1);
}

#[test]
fn each_menu_item_flag_and_the_menus_own_are_read_by_themselves() {
    let raw_menu = RawMenu::new(
        &[
            ("c", MENU_ITEM_CHECKED),
            ("d", MENU_ITEM_DISABLED),
            ("s", MENU_ITEM_SEPARATOR),
            ("g", MENU_ITEM_GRAYED),
            ("h", MENU_ITEM_HIDDEN),
        ],
        &[],
    );
    let shown_with = |flags: u32| {
        let (context, events) = host_context(true, &no_logs());
        let asked = program(events, Some(Answer::Esc));
        let table = NcMenu {
            flags,
            ..raw_menu.table()
        };
        call_menu(&context, &raw const table, std::ptr::null_mut());
        let Request::Menu(shown) = asked.join().unwrap() else {
            panic!("a menu");
        };
        shown
    };
    let shown = shown_with(0);
    let flags: Vec<[bool; 5]> = (shown.lines.iter())
        .map(|line| {
            [
                line.separator,
                line.checked,
                line.disabled,
                line.grayed,
                line.hidden,
            ]
        })
        .collect();
    assert_eq!(
        flags,
        [
            [false, true, false, false, false],
            [false, false, true, false, false],
            [true, false, false, false, false],
            [false, false, false, true, false],
            [false, false, false, false, true],
        ]
    );
    let menu_flags = |shown: &MenuRequest| {
        [
            shown.auto_hotkeys,
            shown.wrap,
            shown.show_ampersand,
            shown.reverse_auto_hotkeys,
        ]
    };
    assert_eq!(menu_flags(&shown), [false; 4], "none asked");
    for (flag, at) in [
        (MENU_AUTO_HOTKEYS, 0),
        (MENU_WRAP, 1),
        (MENU_SHOW_AMPERSAND, 2),
        (MENU_REVERSE_AUTO_HOTKEYS, 3),
    ] {
        let mut wanted = [false; 4];
        wanted[at] = true;
        assert_eq!(menu_flags(&shown_with(flag)), wanted, "{flag:#x}");
    }
}

#[test]
fn a_newer_plugins_larger_menu_is_read() {
    let raw_menu = RawMenu::new(&[("One", 0)], &["F4"]);
    let (context, events) = host_context(true, &no_logs());
    let asked = program(events, Some(Answer::Esc));
    let newer = NcMenu {
        size: size_of::<NcMenu>() + 8,
        ..raw_menu.table()
    };
    // The host reads only the fields it knows; `size` is the plugin's word for the rest.
    call_menu(&context, &raw const newer, std::ptr::null_mut());
    let Request::Menu(shown) = asked.join().unwrap() else {
        panic!("a menu");
    };
    assert_eq!(shown.lines.len(), 1);
    assert_eq!(shown.break_keys, ["F4"]);
}

#[test]
fn a_menu_hands_at_most_its_cap_of_lines_and_keys() {
    let item = NcMenuItem {
        text: NcStr::new("x"),
        flags: 0,
    };
    let items = vec![item; MOST_LINES + 1];
    let keys = vec![NcStr::new("F4"); MOST_KEYS + 1];
    let table = NcMenu {
        size: size_of::<NcMenu>(),
        flags: 0,
        title: NcStr::new("Menu"),
        bottom: NcStr::EMPTY,
        items: items.as_ptr(),
        count: items.len(),
        selected: 0,
        break_keys: keys.as_ptr(),
        break_count: keys.len(),
        x: -1,
        y: -1,
        max_height: 0,
    };
    let (context, events) = host_context(true, &no_logs());
    let asked = program(events, Some(Answer::Esc));
    call_menu(&context, &raw const table, std::ptr::null_mut());
    let Request::Menu(shown) = asked.join().unwrap() else {
        panic!("a menu");
    };
    assert_eq!(shown.lines.len(), MOST_LINES);
    assert_eq!(shown.break_keys.len(), MOST_KEYS);
}

#[test]
fn a_message_with_buttons_while_starting_is_esc_at_once() {
    let (starting, events) = host_context(false, &no_logs());
    let heard = crate::plugin_host::ffi::tests::answering_program(events);
    assert_eq!(message(&starting, 0, "a", "OK"), -1);
    drop(starting);
    assert!(!heard.join().unwrap(), "nothing shown");
}

#[test]
fn a_second_answer_replaces_the_first_one_kept() {
    let (mut context, events) = host_context(true, &no_logs());
    let asked = program(events, Some(Answer::Text("first".to_string())));
    call_input(&context, std::ptr::null_mut());
    asked.join().unwrap();
    assert_eq!(*context.answer.borrow(), "first");
    let (sender, events) = channel();
    context.events = sender;
    let asked = program(events, Some(Answer::Text("second".to_string())));
    let mut out = NcStr::EMPTY;
    assert_eq!(call_input(&context, &raw mut out), 0);
    asked.join().unwrap();
    // SAFETY: the host's answer lives in the context until its next answer.
    assert_eq!(unsafe { out.as_str() }, "second");
}

#[test]
fn a_button_set_takes_the_place_of_the_plugins_buttons() {
    for (set, labels) in [
        (MESSAGE_BUTTONS_OK, vec!["OK"]),
        (MESSAGE_BUTTONS_OK_CANCEL, vec!["OK", "Cancel"]),
        (
            MESSAGE_BUTTONS_ABORT_RETRY_IGNORE,
            vec!["Abort", "Retry", "Ignore"],
        ),
        (MESSAGE_BUTTONS_YES_NO, vec!["Yes", "No"]),
        (MESSAGE_BUTTONS_YES_NO_CANCEL, vec!["Yes", "No", "Cancel"]),
        (MESSAGE_BUTTONS_RETRY_CANCEL, vec!["Retry", "Cancel"]),
        // A set it does not know: the plugin's own (FAR's `switch` without it).
        (0x70, vec!["Mine"]),
    ] {
        let (context, events) = host_context(true, &no_logs());
        let asked = program(events, Some(Answer::Button(1)));
        assert_eq!(message(&context, set | MESSAGE_WARNING, "a", "Mine"), 1);
        let Request::Message(shown) = asked.join().unwrap() else {
            panic!("a message");
        };
        assert_eq!(shown.buttons, labels, "{set:#x}");
        assert!(shown.warning, "the other flags still read");
    }
}

#[cfg(windows)]
#[test]
fn the_error_flag_adds_the_threads_last_system_error_as_the_call_comes() {
    let place = tempfile::tempdir().unwrap();
    for (flags, wanted) in [(MESSAGE_ERROR, true), (0, false)] {
        let (context, events) = host_context(true, &no_logs());
        let asked = program(events, Some(Answer::Button(0)));
        // The system's "file not found" (2) left on this thread, as a plugin's failed call leaves it.
        let missing = std::fs::File::open(place.path().join("missing"));
        assert!(missing.is_err());
        message(&context, flags, "a", "OK");
        let Request::Message(shown) = asked.join().unwrap() else {
            panic!("a message");
        };
        let error = shown.error.unwrap_or_default();
        assert_eq!(error.starts_with("0x00000002 - "), wanted, "{error}");
    }
}

#[test]
fn the_system_text_is_fars_code_and_text_and_none_for_no_error() {
    let text = system_error(&std::io::Error::from_raw_os_error(5)).unwrap();
    assert!(text.starts_with("0x00000005 - "), "{text}");
    assert!(!text.contains("os error") && text.len() > 13, "{text}");
    assert_eq!(system_error(&std::io::Error::from_raw_os_error(0)), None);
    // An application's own code (bit 29): the system has no text for it.
    #[cfg(windows)]
    assert_eq!(
        system_error(&std::io::Error::from_raw_os_error(0x2000_1234)).as_deref(),
        Some("0x20001234 - Unknown error")
    );
    assert_eq!(system_error(&std::io::Error::other("no code")), None);
}

#[test]
fn each_input_flag_is_read_by_itself() {
    let read = |request: &InputRequest| {
        (
            request.enable_empty,
            request.password,
            request.expand_env,
            request.buttons,
            request.no_ampersand,
            request.no_last_history,
            request.edit_path,
        )
    };
    let only = |at: usize| std::array::from_fn::<bool, 7, _>(|flag| flag == at);
    let tuple = |on: [bool; 7]| (on[0], on[1], on[2], on[3], on[4], on[5], on[6]);
    for (flags, wanted) in [
        (INPUT_ENABLE_EMPTY, tuple(only(0))),
        (INPUT_PASSWORD, tuple(only(1))),
        (INPUT_EXPAND_ENV, tuple(only(2))),
        (INPUT_BUTTONS, tuple(only(3))),
        (INPUT_NO_AMPERSAND, tuple(only(4))),
        (INPUT_NO_LAST_HISTORY, tuple(only(5))),
        (INPUT_EDIT_PATH, tuple(only(6))),
        // FAR's `FIB_EDITPATHEXEC`: not the program's.
        (0x80, tuple(only(9))),
    ] {
        let (context, events) = host_context(true, &no_logs());
        let asked = program(events, Some(Answer::Esc));
        // SAFETY: the context and the strings live through the call; no place for the answer.
        let code = unsafe {
            super::input(
                raw(&context),
                NcStr::new("T"),
                NcStr::new("P"),
                NcStr::new(""),
                NcStr::EMPTY,
                flags,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(code, -1);
        let Request::Input(shown) = asked.join().unwrap() else {
            panic!("an input line");
        };
        assert_eq!(read(&shown), wanted, "{flags:#x}");
    }
}
