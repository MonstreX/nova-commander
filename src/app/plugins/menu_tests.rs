//! A plugin's menu: the cursor over separators and grey lines, Enter and hotkeys, break keys by name.

use super::*;

fn line(text: &str) -> MenuLine {
    MenuLine {
        text: text.to_string(),
        ..MenuLine::default()
    }
}

fn separator() -> MenuLine {
    MenuLine {
        separator: true,
        ..MenuLine::default()
    }
}

fn grey(text: &str) -> MenuLine {
    MenuLine {
        disabled: true,
        ..line(text)
    }
}

fn menu(lines: Vec<MenuLine>, selected: usize, keys: &[&str]) -> PluginMenu {
    PluginMenu::new(MenuRequest {
        title: "T".to_string(),
        lines,
        selected,
        break_keys: keys.iter().map(ToString::to_string).collect(),
        ..MenuRequest::default()
    })
}

fn press(menu: &mut PluginMenu, code: KeyCode, modifiers: KeyModifiers) -> MenuOutcome {
    menu.handle_key(KeyEvent::new(code, modifiers))
}

fn plain(menu: &mut PluginMenu, code: KeyCode) -> MenuOutcome {
    press(menu, code, KeyModifiers::NONE)
}

fn chosen(item: usize) -> MenuOutcome {
    MenuOutcome::Chosen {
        item: Some(item),
        key: None,
    }
}

#[test]
fn the_cursor_starts_where_asked_or_on_the_first_line_it_can_take() {
    // FAR's `UpdateItemFlags` and `SetSelectPos(0, 1)`: an item that cannot take the cursor is not
    // selected, the first from the top that can is.
    let lines = || vec![separator(), line("&One"), grey("Two"), line("Three")];
    assert_eq!(menu(lines(), 3, &[]).selected, 3);
    assert_eq!(menu(lines(), 2, &[]).selected, 1, "a grey line");
    assert_eq!(menu(lines(), 9, &[]).selected, 1, "none");
    let hidden = MenuLine {
        hidden: true,
        ..line("Hidden")
    };
    assert_eq!(
        menu(vec![line("A"), line("B"), hidden], 2, &[]).selected,
        0,
        "a hidden one"
    );
}

#[test]
fn the_cursor_passes_over_separators_and_grey_lines_round_the_ends_only_when_asked() {
    let lines = || vec![line("One"), separator(), grey("Two"), line("Three")];
    let mut wrapping = PluginMenu::new(MenuRequest {
        lines: lines(),
        wrap: true,
        ..MenuRequest::default()
    });
    assert_eq!(plain(&mut wrapping, KeyCode::Down), MenuOutcome::Stay);
    assert_eq!(wrapping.selected, 3);
    plain(&mut wrapping, KeyCode::Down);
    assert_eq!(wrapping.selected, 0, "round the end (FMENU_WRAPMODE)");
    plain(&mut wrapping, KeyCode::Up);
    assert_eq!(wrapping.selected, 3);
    assert_eq!(plain(&mut wrapping, KeyCode::Enter), chosen(3));
    let mut plain_menu = menu(lines(), 3, &[]);
    plain(&mut plain_menu, KeyCode::Down);
    assert_eq!(
        plain_menu.selected, 3,
        "FAR's menu without FMENU_WRAPMODE stops"
    );
    plain(&mut plain_menu, KeyCode::Up);
    assert_eq!(plain_menu.selected, 0);
    plain(&mut plain_menu, KeyCode::Up);
    assert_eq!(plain_menu.selected, 0);
}

#[test]
fn a_grayed_line_takes_the_cursor_but_not_enter_and_a_hidden_one_keeps_the_indexes() {
    let grayed = MenuLine {
        grayed: true,
        ..line("&Grayed")
    };
    let hidden = MenuLine {
        hidden: true,
        ..line("&Hidden")
    };
    let mut menu = menu(
        vec![line("&One"), hidden, grayed, line("&Three")],
        2,
        &["F4"],
    );
    assert_eq!(menu.lines.len(), 3, "the hidden line left out");
    assert_eq!(
        menu.selected, 1,
        "the plugin's item 2 is the second line shown"
    );
    assert_eq!(
        plain(&mut menu, KeyCode::Enter),
        MenuOutcome::Stay,
        "MIF_GRAYED"
    );
    assert_eq!(
        plain(&mut menu, KeyCode::F(4)),
        MenuOutcome::Chosen {
            item: Some(2),
            key: Some(0)
        },
        "a break key on it: its item"
    );
    plain(&mut menu, KeyCode::Down);
    assert_eq!(
        plain(&mut menu, KeyCode::Enter),
        chosen(3),
        "the plugin's index"
    );
    plain(&mut menu, KeyCode::Home);
    assert_eq!(plain(&mut menu, KeyCode::Char('g')), MenuOutcome::Stay);
    assert_eq!(menu.selected, 1, "its hotkey only moves the cursor");
    assert_eq!(
        plain(&mut menu, KeyCode::Char('h')),
        MenuOutcome::Stay,
        "a hidden line's hotkey"
    );
}

#[test]
fn ampersands_shown_as_they_are_and_automatic_hotkeys_from_the_end() {
    let shown = PluginMenu::new(MenuRequest {
        lines: vec![line("R&D"), line("&Sales")],
        show_ampersand: true,
        auto_hotkeys: true,
        ..MenuRequest::default()
    });
    let texts: Vec<&str> = shown.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(
        texts,
        ["&R&&D", "&&S&ales"],
        "the texts' & shown, automatic hotkeys only, none right after &&"
    );
    // Hidden items keep their hotkeys from the others (FAR's `AssignHighlights` goes over them all).
    let with_hidden = PluginMenu::new(MenuRequest {
        lines: vec![
            MenuLine {
                hidden: true,
                ..line("&Alpha")
            },
            line("&Apple"),
        ],
        auto_hotkeys: true,
        ..MenuRequest::default()
    });
    assert_eq!(
        with_hidden.lines[0].text, "A&pple",
        "its `&A` taken by the hidden one"
    );
    // `FMENU_REVERSEAUTOHIGHLIGHT` alone turns them on (FAR's `apiMenuFn`).
    let reverse_only = PluginMenu::new(MenuRequest {
        lines: vec![line("Apple"), line("Avocado")],
        reverse_auto_hotkeys: true,
        ..MenuRequest::default()
    });
    let texts: Vec<&str> = reverse_only
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(texts, ["A&pple", "&Avocado"]);
    let reversed = PluginMenu::new(MenuRequest {
        lines: vec![line("Apple"), line("Avocado")],
        auto_hotkeys: true,
        reverse_auto_hotkeys: true,
        ..MenuRequest::default()
    });
    let texts: Vec<&str> = reversed
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(
        texts,
        ["A&pple", "&Avocado"],
        "the last item first (FAR's VMENU_REVERSEHIGHLIGHT)"
    );
}

#[test]
fn esc_and_f10_close_it_unchosen() {
    let mut menu = menu(vec![line("One")], 0, &[]);
    assert_eq!(plain(&mut menu, KeyCode::Esc), MenuOutcome::Esc);
    assert_eq!(plain(&mut menu, KeyCode::F(10)), MenuOutcome::Esc);
}

#[test]
fn a_hotkey_chooses_its_line_at_once_but_never_a_grey_one() {
    let mut menu = menu(vec![line("&One"), grey("&Two"), line("T&hree")], 0, &[]);
    assert_eq!(plain(&mut menu, KeyCode::Char('h')), chosen(2));
    assert_eq!(menu.selected, 2);
    assert_eq!(
        plain(&mut menu, KeyCode::Char('t')),
        MenuOutcome::Stay,
        "grey"
    );
    assert_eq!(plain(&mut menu, KeyCode::Char('x')), MenuOutcome::Stay);
    assert_eq!(
        press(&mut menu, KeyCode::Char('O'), KeyModifiers::SHIFT),
        chosen(0),
        "either case"
    );
    assert_eq!(
        press(&mut menu, KeyCode::Char('o'), KeyModifiers::ALT),
        chosen(0)
    );
    assert_eq!(
        press(&mut menu, KeyCode::Char('o'), KeyModifiers::CONTROL),
        MenuOutcome::Stay
    );
}

#[test]
fn automatic_hotkeys_mark_the_lines_without_one() {
    let menu = PluginMenu::new(MenuRequest {
        lines: vec![line("Open"), line("&Close"), line("Copy")],
        auto_hotkeys: true,
        ..MenuRequest::default()
    });
    let texts: Vec<&str> = menu.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(texts, ["&Open", "&Close", "Co&py"], "the first letter free");
    let plain = PluginMenu::new(MenuRequest {
        lines: vec![line("Open")],
        ..MenuRequest::default()
    });
    assert_eq!(plain.lines[0].text, "Open", "not asked: as given");
}

#[test]
fn a_break_key_closes_it_on_the_cursors_line() {
    let mut menu = menu(
        vec![line("One"), line("Two")],
        1,
        &["F4", "Shift+Del", "nonsense", "Ctrl+Up", "A"],
    );
    let key = |item: Option<usize>, key: usize| MenuOutcome::Chosen {
        item,
        key: Some(key),
    };
    assert_eq!(plain(&mut menu, KeyCode::F(4)), key(Some(1), 0));
    assert_eq!(
        press(&mut menu, KeyCode::Delete, KeyModifiers::SHIFT),
        key(Some(1), 1)
    );
    assert_eq!(
        plain(&mut menu, KeyCode::Delete),
        MenuOutcome::Stay,
        "without Shift"
    );
    assert_eq!(
        press(&mut menu, KeyCode::Up, KeyModifiers::CONTROL),
        key(Some(1), 3),
        "the index among all named, the one not read too"
    );
    assert_eq!(
        press(&mut menu, KeyCode::Char('A'), KeyModifiers::SHIFT),
        key(Some(1), 4)
    );
    let mut empty = super::tests::menu(vec![separator()], 0, &["F4"]);
    assert_eq!(plain(&mut empty, KeyCode::F(4)), key(None, 0), "no line");
    assert_eq!(plain(&mut empty, KeyCode::Enter), MenuOutcome::Stay);
}

#[test]
fn key_names_are_read_as_plugins_write_them() {
    let key = |code, modifiers| Some(BreakKey { code, modifiers });
    assert_eq!(parse("F4"), key(KeyCode::F(4), KeyModifiers::NONE));
    assert_eq!(parse("f24"), key(KeyCode::F(24), KeyModifiers::NONE));
    assert_eq!(parse("F25"), None);
    assert_eq!(
        parse("Ctrl+Alt+PgDn"),
        key(KeyCode::PageDown, KeyModifiers::CONTROL | KeyModifiers::ALT)
    );
    assert_eq!(
        parse("shift+ins"),
        key(KeyCode::Insert, KeyModifiers::SHIFT)
    );
    assert_eq!(
        parse("Ctrl++"),
        key(KeyCode::Char('+'), KeyModifiers::CONTROL)
    );
    assert_eq!(parse("+"), key(KeyCode::Char('+'), KeyModifiers::NONE));
    assert_eq!(parse("Space"), key(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(parse("BS"), key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(parse("Apps"), key(KeyCode::Menu, KeyModifiers::NONE));
    assert_eq!(parse("Win+F4"), None, "a modifier not known");
    assert_eq!(parse("Ctrl+"), None);
    assert_eq!(parse(""), None);
}

#[test]
fn characters_compare_as_typed() {
    let matches = |name: &str, code, modifiers| {
        parse(name).is_some_and(|wanted| wanted.matches(KeyEvent::new(code, modifiers)))
    };
    assert!(matches("a", KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(
        !matches("a", KeyCode::Char('A'), KeyModifiers::SHIFT),
        "a capital is Shift"
    );
    assert!(matches("Shift+a", KeyCode::Char('A'), KeyModifiers::SHIFT));
    assert!(matches("A", KeyCode::Char('A'), KeyModifiers::SHIFT));
    assert!(
        matches("!", KeyCode::Char('!'), KeyModifiers::SHIFT),
        "Shift is in the character"
    );
    assert!(matches("Ctrl+a", KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert!(!matches("Ctrl+a", KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(
        matches("Ctrl+A", KeyCode::Char('a'), KeyModifiers::CONTROL),
        "FAR writes the letter in capitals"
    );
    assert!(matches("Alt+X", KeyCode::Char('x'), KeyModifiers::ALT));
    assert!(!matches(
        "Ctrl+A",
        KeyCode::Char('A'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT
    ));
    assert!(matches(
        "Ctrl+Shift+A",
        KeyCode::Char('A'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT
    ));
    assert!(matches("Shift+Tab", KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(matches("Tab", KeyCode::Tab, KeyModifiers::NONE));
}

#[test]
fn every_key_name_is_its_key() {
    let named = [
        ("Enter", KeyCode::Enter),
        ("Tab", KeyCode::Tab),
        ("BS", KeyCode::Backspace),
        ("BackSpace", KeyCode::Backspace),
        ("Ins", KeyCode::Insert),
        ("Insert", KeyCode::Insert),
        ("Del", KeyCode::Delete),
        ("Delete", KeyCode::Delete),
        ("Home", KeyCode::Home),
        ("End", KeyCode::End),
        ("PgUp", KeyCode::PageUp),
        ("PgDn", KeyCode::PageDown),
        ("Up", KeyCode::Up),
        ("Down", KeyCode::Down),
        ("Left", KeyCode::Left),
        ("Right", KeyCode::Right),
        ("F1", KeyCode::F(1)),
        ("F12", KeyCode::F(12)),
    ];
    for (name, code) in named {
        let plain = Some(BreakKey {
            code,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(parse(name), plain, "{name}");
        assert_eq!(parse(&name.to_uppercase()), plain, "{name} in capitals");
    }
    assert_eq!(parse("F0"), None, "no F0");
    assert_eq!(parse("Esc"), None, "Esc is the menu's own");
    assert_eq!(parse("PageUp"), None, "not a name of FAR's");
    assert_eq!(
        parse("  Ctrl+F4 "),
        Some(BreakKey {
            code: KeyCode::F(4),
            modifiers: KeyModifiers::CONTROL
        }),
        "spaces around"
    );
}

#[test]
fn only_ctrl_alt_and_shift_count_in_a_key_pressed() {
    let f4 = parse("Ctrl+F4").unwrap();
    assert!(f4.matches(KeyEvent::new(
        KeyCode::F(4),
        KeyModifiers::CONTROL | KeyModifiers::SUPER | KeyModifiers::META
    )));
    assert!(!f4.matches(KeyEvent::new(
        KeyCode::F(4),
        KeyModifiers::CONTROL | KeyModifiers::ALT
    )));
}

#[test]
fn a_break_key_comes_before_the_menus_own_keys() {
    let mut menu = menu(vec![line("&Add"), line("Two")], 1, &["a", "Enter", "Down"]);
    let key = |item, key| MenuOutcome::Chosen {
        item: Some(item),
        key: Some(key),
    };
    assert_eq!(
        plain(&mut menu, KeyCode::Char('a')),
        key(1, 0),
        "not the hotkey"
    );
    assert_eq!(menu.selected, 1);
    assert_eq!(plain(&mut menu, KeyCode::Enter), key(1, 1));
    assert_eq!(plain(&mut menu, KeyCode::Down), key(1, 2));
    assert_eq!(menu.selected, 1, "the cursor stays");
}

#[test]
fn a_page_is_the_rows_shown() {
    let lines = (0..10)
        .map(|index| line(&format!("Line {index}")))
        .collect();
    let mut menu = menu(lines, 0, &[]);
    menu.rows.set(4);
    assert_eq!(plain(&mut menu, KeyCode::PageDown), MenuOutcome::Stay);
    assert_eq!(menu.selected, 3, "a page less a line");
    plain(&mut menu, KeyCode::End);
    assert_eq!(menu.selected, 9);
    plain(&mut menu, KeyCode::PageUp);
    assert_eq!(menu.selected, 6);
    plain(&mut menu, KeyCode::Home);
    assert_eq!(menu.selected, 0);
}

#[test]
fn a_hotkey_before_a_separator_or_past_the_lines_is_none() {
    let mut menu = menu(vec![separator(), line("&One")], 1, &[]);
    assert_eq!(plain(&mut menu, KeyCode::Char('o')), chosen(1));
    assert!(!menu.choosable(0), "a separator");
    assert!(!menu.choosable(5), "past the lines");
    assert!(menu.choosable(1));
}

#[test]
fn alt_shift_and_a_digit_is_named_by_its_key_whatever_shift_makes_of_it() {
    use ratatui::crossterm::event::{KeyEventKind, KeyEventState};
    let named = super::parse("Alt+Shift+1").unwrap();
    let pressed = |state: u8| {
        KeyEvent::new_with_kind_and_state(
            KeyCode::Char('!'),
            KeyModifiers::ALT | KeyModifiers::SHIFT,
            KeyEventKind::Press,
            KeyEventState::from_bits_retain(state),
        )
    };
    // The console input's encoding of the digit 1: two, three bits up.
    assert!(named.matches(pressed(2 << 3)));
    assert!(!named.matches(pressed(3 << 3)), "another digit");
    assert!(!named.matches(pressed(0)), "no digit key");
    assert!(
        !super::parse("Alt+1").unwrap().matches(pressed(2 << 3)),
        "without Shift"
    );
}

#[test]
fn a_keys_name_is_read_back_as_the_key() {
    let named =
        |code: KeyCode, modifiers: KeyModifiers| super::key_name(KeyEvent::new(code, modifiers));
    let cases = [
        (KeyCode::F(4), KeyModifiers::NONE, "F4"),
        (KeyCode::Enter, KeyModifiers::CONTROL, "Ctrl+Enter"),
        (
            KeyCode::F(2),
            KeyModifiers::ALT | KeyModifiers::SHIFT,
            "Alt+Shift+F2",
        ),
        (KeyCode::Char('a'), KeyModifiers::CONTROL, "Ctrl+A"),
        (KeyCode::Char('A'), KeyModifiers::SHIFT, "A"),
        (KeyCode::Char(' '), KeyModifiers::NONE, "Space"),
        (KeyCode::Menu, KeyModifiers::SHIFT, "Shift+Apps"),
        (KeyCode::BackTab, KeyModifiers::SHIFT, "Shift+Tab"),
        (KeyCode::PageDown, KeyModifiers::NONE, "PgDn"),
    ];
    for (code, modifiers, name) in cases {
        assert_eq!(named(code, modifiers).as_deref(), Some(name));
        let key = KeyEvent::new(code, modifiers);
        assert!(super::parse(name).unwrap().matches(key), "{name}");
    }
    // A dialog's handler hears Esc by its name; a menu's break keys never take it (Esc is the menu's).
    assert_eq!(
        named(KeyCode::Esc, KeyModifiers::NONE).as_deref(),
        Some("Esc")
    );
    assert_eq!(named(KeyCode::Null, KeyModifiers::NONE), None);
}
