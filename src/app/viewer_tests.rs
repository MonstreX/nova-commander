//! The viewer on the view core: FAR's status line, its keys of text mode, F2, F8, the wheel, the list.

use std::time::{Duration, Instant};

use super::*;
use crate::app::App;
use crate::editor::view::Screen;

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn press(viewer: &mut Viewer, code: KeyCode) -> ViewerAction {
    viewer.handle_key(key(code, KeyModifiers::NONE))
}

/// Pumps until the rows of a 20 by 5 room are what `done` waits for.
fn settle(viewer: &mut Viewer, done: impl Fn(&Screen) -> bool) -> Screen {
    viewer.text.room.set((20, 5));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        viewer.pump();
        if let Some(screen) = viewer.text.screen(20, 5)
            && done(&screen)
        {
            return screen;
        }
        assert!(Instant::now() < deadline, "not shown");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

fn viewer_of(temp: &tempfile::TempDir, text: &str) -> Viewer {
    let path = temp.path().join("v.txt");
    std::fs::write(&path, text).unwrap();
    let mut viewer = Viewer::open(&path);
    settle(&mut viewer, |screen| !screen.rows.is_empty());
    viewer
}

fn first_row(viewer: &mut Viewer, row: &str) {
    settle(viewer, |screen| {
        screen.rows.first().is_some_and(|it| it == row)
    });
}

const AT_START: Facts = Facts {
    len: 1234,
    position: 0,
    left: 0,
    at_end: false,
    mode: Mode::Text,
};

#[test]
fn the_status_line_is_far_s_with_the_name_in_the_room_left() {
    let line = status_line(r"C:\work\file.txt", ("UTF-8", AT_START), 60, 0);
    assert_eq!(
        line,
        r"C:\work\file.txt            │t│UTF-8│1234      │Col 0  │0%  "
    );
    assert_eq!(line.chars().count(), 60);
    let later = Facts {
        position: 617,
        left: 12,
        ..AT_START
    };
    let line = status_line("f", ("ANSI", later), 40, 0);
    assert!(line.ends_with("│t│ANSI │1234      │Col 12 │50% "), "{line}");
    let end = Facts {
        at_end: true,
        ..AT_START
    };
    assert!(status_line("f", ("OEM", end), 40, 0).ends_with("│100%"));
    assert!(status_line("f", ("1251", AT_START), 40, 0).contains("│1251 │"));
}

#[test]
fn a_long_name_is_cut_as_a_path_and_the_clock_keeps_its_room() {
    let name = r"C:\a very long folder name\and another one\file.txt";
    let line = status_line(name, ("UTF-8", AT_START), 50, 5);
    assert_eq!(
        line.chars().count(),
        45,
        "the clock's five and a separator kept"
    );
    assert!(line.ends_with("│0%  │"), "{line}");
    assert!(line.contains("file.txt"), "the name's end kept: {line}");
    assert!(line.starts_with("C:"), "and its start: {line}");
    let narrow = status_line(name, ("UTF-8", AT_START), 10, 0);
    assert_eq!(narrow, "│t│UTF-8│1", "no room for the name: the line cut");
}

#[test]
fn percents_are_far_s() {
    assert_eq!(to_percent(0, 10), 0);
    assert_eq!(to_percent(5, 0), 0);
    assert_eq!(to_percent(1, 3), 33);
    assert_eq!(to_percent(10, 10), 100);
    assert_eq!(to_percent(u64::MAX - 1, u64::MAX), 99);
}

#[test]
fn the_keys_of_text_mode_are_far_s() {
    let none = KeyModifiers::NONE;
    let ctrl = KeyModifiers::CONTROL;
    for (code, modifiers, step) in [
        (KeyCode::Up, none, Move::Up),
        (KeyCode::Down, none, Move::Down),
        (KeyCode::PageUp, none, Move::PageUp),
        (KeyCode::Up, ctrl, Move::PageUp),
        (KeyCode::PageDown, none, Move::PageDown),
        (KeyCode::Down, ctrl, Move::PageDown),
        (KeyCode::Home, none, Move::Home),
        (KeyCode::Home, ctrl, Move::Home),
        (KeyCode::End, ctrl, Move::End),
        (KeyCode::PageUp, ctrl, Move::Start),
        (KeyCode::PageDown, ctrl, Move::Finish),
        (KeyCode::Left, none, Move::Left(1)),
        (KeyCode::Right, ctrl, Move::Right(20)),
        (KeyCode::Left, ctrl | KeyModifiers::SHIFT, Move::LineStarts),
    ] {
        assert_eq!(text_move(key(code, modifiers)), Some(step), "{code:?}");
    }
    assert_eq!(text_move(key(KeyCode::Up, KeyModifiers::ALT)), None);
}

#[test]
fn a_file_is_moved_through_and_its_status_follows() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = viewer_of(&temp, &numbered(100));
    first_row(&mut viewer, "line 1");
    assert!(viewer.status(60, 0).ends_with("│0%  "));
    press(&mut viewer, KeyCode::PageDown);
    first_row(&mut viewer, "line 5");
    assert!(viewer.status(60, 0).contains(&format!("│{:<10}│", 792)));
    let facts = viewer.text.facts().unwrap();
    assert_eq!(facts.position, 4 * 7, "the first row's byte");
    press(&mut viewer, KeyCode::End);
    settle(&mut viewer, |screen| screen.at_end);
    assert!(viewer.status(60, 0).ends_with("│100%"));
    // FAR's End with the last line broken: its empty row last, the top at line 97.
    first_row(&mut viewer, "line 97");
    viewer.wheel(1, 3, false);
    first_row(&mut viewer, "line 94");
    viewer.wheel(-2, 1, false);
    first_row(&mut viewer, "line 96");
}

#[test]
fn f2_unwraps_and_wraps_and_its_label_says_what_it_would_do() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = viewer_of(&temp, &format!("{}\nnext\n", "x".repeat(30)));
    assert_eq!(
        viewer.labels()[0],
        "Unwrap",
        "wrapped at first (FAR's default)"
    );
    settle(&mut viewer, |screen| screen.rows.len() == 3);
    press(&mut viewer, KeyCode::F(2));
    let screen = settle(&mut viewer, |screen| screen.rows.len() == 2);
    assert_eq!(screen.rows[1], "next");
    assert_eq!(viewer.labels()[0], "Wrap");
    viewer.handle_key(key(KeyCode::Right, KeyModifiers::CONTROL));
    settle(&mut viewer, |screen| screen.rows[0] == "x".repeat(10));
    assert!(viewer.status(60, 0).contains("│Col 20 │"));
    press(&mut viewer, KeyCode::F(2));
    settle(&mut viewer, |screen| screen.rows.len() == 3);
    assert!(
        viewer.status(60, 0).contains("│Col 0  │"),
        "wrapped: column 0"
    );
}

/// S2d.6e: Shift+F2 - FAR's word wrap, wrapping a line not wrapped, the labels of F2 and Shift+F2
/// saying what each would do, the settings following it (FAR's `ViewerIsWrap`, `ViewerWrap`).
#[test]
fn shift_f2_wraps_at_the_words_and_its_label_says_what_it_would_do() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = viewer_of(&temp, "one two three four five six\n");
    settle(&mut viewer, |screen| screen.rows.len() == 2);
    assert_eq!(viewer.shift_f2_label(), "WWrap");
    press(&mut viewer, KeyCode::F(2));
    assert_eq!(viewer.labels()[0], "Wrap");
    let action = viewer.handle_key(key(KeyCode::F(2), KeyModifiers::SHIFT));
    assert!(matches!(
        action,
        ViewerAction::Wrapped {
            wrap: true,
            words: true
        }
    ));
    let screen = settle(&mut viewer, |screen| screen.rows.len() == 2);
    assert_eq!(screen.rows, ["one two three four", "five six"]);
    assert_eq!(viewer.shift_f2_label(), "Wrap");
    assert_eq!(viewer.labels()[0], "Unwrap");
    press(&mut viewer, KeyCode::F(2));
    assert_eq!(
        viewer.labels()[0],
        "WWrap",
        "unwrapped: F2 wraps at the words"
    );
    assert!(viewer.settings.word_wrap && !viewer.settings.wrap);
}

#[test]
fn f8_goes_between_ansi_and_oem() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = viewer_of(&temp, "plain\n");
    let page = |viewer: &Viewer| viewer.status(60, 0);
    assert!(page(&viewer).contains("│ANSI │"), "{}", page(&viewer));
    assert_eq!(viewer.labels()[2], "OEM");
    press(&mut viewer, KeyCode::F(8));
    let oem = crate::platform::locale::oem_code_page();
    let ansi = crate::platform::locale::ansi_code_page();
    if oem != ansi {
        assert!(page(&viewer).contains("│OEM  │"), "{}", page(&viewer));
        assert_eq!(viewer.labels()[2], "ANSI");
    }
    press(&mut viewer, KeyCode::F(8));
    assert!(page(&viewer).contains("│ANSI │"));
    let mut utf8 = Viewer::open(&{
        let path = temp.path().join("u.txt");
        std::fs::write(&path, "текст\n").unwrap();
        path
    });
    settle(&mut utf8, |screen| !screen.rows.is_empty());
    assert!(page(&utf8).contains("│UTF-8│"));
    press(&mut utf8, KeyCode::F(8));
    assert!(page(&utf8).contains("│ANSI │"), "ANSI after any other");
}

/// The wheel reaches the viewer through the app's mouse: Alt turns a notch into one row; a click
/// does nothing there.
#[test]
fn the_app_s_wheel_moves_the_viewer() {
    use crate::app::console_input::{Mouse, MouseAction};
    use crate::platform::input::MouseButtons;
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("v.txt"), numbered(50)).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.mouse.enabled = true;
    app.view_file(&temp.path().join("v.txt"), false);
    fn shown(app: &mut App, row: &str) {
        match &mut app.screen {
            super::super::Screen::Viewer(viewer) => first_row(viewer, row),
            _ => panic!("no viewer"),
        }
    }
    shown(&mut app, "line 1");
    let none = MouseButtons {
        left: false,
        right: false,
        middle: false,
    };
    let record = |action, modifiers| Mouse {
        column: 5,
        row: 5,
        buttons: none,
        pressed: none,
        action,
        modifiers,
    };
    assert!(app.handle_mouse(record(MouseAction::Wheel(-2), KeyModifiers::ALT)));
    shown(&mut app, "line 3");
    assert!(!app.handle_mouse(record(MouseAction::Button, KeyModifiers::NONE)));
    assert!(!app.handle_mouse(record(MouseAction::Wheel(-1), KeyModifiers::CONTROL)));
    shown(&mut app, "line 3");
}

#[test]
fn quit_keys_edit_and_the_list() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = viewer_of(&temp, "a\n");
    for code in [KeyCode::Esc, KeyCode::F(3), KeyCode::F(10)] {
        assert!(matches!(press(&mut viewer, code), ViewerAction::Close));
    }
    assert!(
        matches!(press(&mut viewer, KeyCode::F(6)), ViewerAction::Edit(path) if path == viewer.path)
    );
    let other = temp.path().join("w.txt");
    viewer.list = vec![viewer.path.clone(), other.clone()];
    let gray = |ch| {
        KeyEvent::new_with_kind_and_state(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
            ratatui::crossterm::event::KeyEventKind::Press,
            KeyEventState::KEYPAD,
        )
    };
    assert!(matches!(viewer.handle_key(gray('+')), ViewerAction::Open(path) if path == other));
    assert!(
        matches!(viewer.handle_key(gray('-')), ViewerAction::None),
        "none before the first"
    );
}

/// FAR's mode letter in the status line: t, h, d.
#[test]
fn the_status_line_says_the_mode() {
    let hex = Facts {
        mode: Mode::Hex,
        ..AT_START
    };
    assert!(status_line("f", ("UTF-8", hex), 40, 0).contains("│h│UTF-8│"));
    let dump = Facts {
        mode: Mode::Dump,
        ..AT_START
    };
    assert!(status_line("f", ("UTF-8", dump), 40, 0).contains("│d│UTF-8│"));
}

/// F4 to hex and back, F2 from dump to text (in hex it edits the bytes: ADR-0027); Shift+F4's choice
/// of dump makes F4 go from hex to dump (FAR's `m_DumpTextMode`); the labels say what each would do.
#[test]
fn f4_f2_and_the_mode_menu_go_between_text_hex_and_dump() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = viewer_of(&temp, "0123456789abcdef0123\n");
    let mode = |viewer: &mut Viewer, mode: Mode| {
        settle(viewer, |_| true);
        let deadline = Instant::now() + Duration::from_secs(10);
        while viewer.text.mode() != mode {
            assert!(Instant::now() < deadline, "not {mode:?}");
            viewer.pump();
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    assert_eq!(viewer.labels()[..2], ["Unwrap", "Hex"]);
    press(&mut viewer, KeyCode::F(4));
    mode(&mut viewer, Mode::Hex);
    assert_eq!(viewer.labels()[..2], ["Edit", "Text"]);
    let screen = settle(&mut viewer, |screen| !screen.rows.is_empty());
    assert!(
        screen.rows[0].starts_with("0000000000: 30 31 32"),
        "{:?}",
        screen.rows
    );
    assert!(viewer.status(60, 0).contains("│h│"));
    press(&mut viewer, KeyCode::F(2));
    assert_eq!(viewer.labels()[0], "Save", "the bytes edited");
    press(&mut viewer, KeyCode::Esc);
    assert_eq!(viewer.text.mode(), Mode::Hex);
    viewer.choose_mode(Mode::Dump);
    mode(&mut viewer, Mode::Dump);
    assert_eq!(viewer.labels()[..2], ["Text", "Hex"]);
    press(&mut viewer, KeyCode::F(2));
    mode(&mut viewer, Mode::Text);
    let menu = viewer.handle_key(key(KeyCode::F(4), KeyModifiers::SHIFT));
    assert!(matches!(menu, ViewerAction::ModeMenu(Mode::Text)));
    viewer.choose_mode(Mode::Dump);
    mode(&mut viewer, Mode::Dump);
    press(&mut viewer, KeyCode::F(4));
    mode(&mut viewer, Mode::Hex);
    assert_eq!(
        viewer.labels()[..2],
        ["Edit", "Dump"],
        "dump chosen: F4 back to dump"
    );
    press(&mut viewer, KeyCode::F(4));
    mode(&mut viewer, Mode::Dump);
}

#[test]
fn the_keys_of_hex_and_dump_are_far_s() {
    let none = KeyModifiers::NONE;
    let ctrl = KeyModifiers::CONTROL;
    let alt = KeyModifiers::ALT;
    for (code, modifiers, step) in [
        (KeyCode::Down, none, Move::Down),
        (KeyCode::Left, none, Move::Left(1)),
        (KeyCode::Left, ctrl, Move::Roll(-1)),
        (KeyCode::Right, ctrl, Move::Roll(1)),
        (KeyCode::Left, alt, Move::Bytes(-1)),
        (KeyCode::Right, alt, Move::Bytes(1)),
        (KeyCode::Left, ctrl | alt, Move::Bytes(-16)),
        (KeyCode::Right, ctrl | alt, Move::Bytes(16)),
        (KeyCode::PageDown, ctrl, Move::Finish),
    ] {
        assert_eq!(bytes_move(key(code, modifiers)), Some(step), "{code:?}");
    }
}

/// S2d.5e: the text mode highlighted by Colorer's schemes (over FAR: its viewer has none), the rows of
/// a wrapped line each painted from its own cells; hex shows none.
#[test]
fn the_text_mode_is_highlighted_and_hex_is_not() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("main.rs");
    std::fs::write(&path, "fn main() {}\n// a comment longer than a row\n").unwrap();
    let mut viewer = Viewer::open(&path);
    viewer.text.room.set((20, 5));
    let deadline = Instant::now() + Duration::from_secs(10);
    while viewer.paints(20, 5).iter().all(Vec::is_empty) {
        viewer.pump();
        assert!(Instant::now() < deadline, "never highlighted");
        std::thread::sleep(Duration::from_millis(2));
    }
    let paints = viewer.paints(20, 5);
    let classes = |row: usize| -> Vec<(usize, usize, &str)> {
        paints[row]
            .iter()
            .map(|p| (p.from, p.to, p.class))
            .collect()
    };
    assert!(classes(0).contains(&(0, 2, "Keyword")), "{:?}", classes(0));
    assert!(
        classes(1)
            .iter()
            .any(|p| p.0 == 0 && p.2.contains("Comment")),
        "{:?}",
        classes(1)
    );
    assert!(
        classes(2)
            .iter()
            .any(|p| p.0 == 0 && p.2.contains("Comment")),
        "the comment's second row: {:?}",
        classes(2)
    );
    press(&mut viewer, KeyCode::F(4));
    assert!(viewer.paints(20, 5).is_empty(), "hex: no colours");
}

/// S2d.6a: "Viewer settings" - the tab size laid out, the highlighting switched off.
#[test]
fn the_viewer_s_settings_set_its_tabs_and_its_highlighting() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("t.rs");
    std::fs::write(&path, "\tfn x() {}\n").unwrap();
    let mut viewer = Viewer::open(&path);
    let settings = crate::settings::ViewerSettings {
        tab_size: 4,
        highlighting: false,
        ..crate::settings::ViewerSettings::default()
    };
    viewer.apply_settings(&settings);
    let screen = settle(&mut viewer, |screen| !screen.rows.is_empty());
    assert_eq!(screen.rows[0], "    fn x() {}", "a tab of four");
    for _ in 0..50 {
        viewer.pump();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        viewer.paints(20, 5).iter().all(Vec::is_empty),
        "no highlighting"
    );
}

/// S2d.6b: FAR's "Persistent selection" off - a move drops what a search found, Space keeps it.
#[test]
fn without_persistent_selection_a_move_drops_the_found_text() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = viewer_of(&temp, &numbered(20));
    let settings = crate::settings::ViewerSettings {
        persistent_selection: false,
        ..crate::settings::ViewerSettings::default()
    };
    viewer.apply_settings(&settings);
    let marked = |viewer: &mut Viewer| {
        let screen = settle(viewer, |_| true);
        screen.marks.iter().any(Option::is_some)
    };
    viewer.text.select(Some((0, 4)));
    assert!(marked(&mut viewer));
    press(&mut viewer, KeyCode::Char(' '));
    assert!(marked(&mut viewer), "Space goes on with the search: kept");
    press(&mut viewer, KeyCode::Down);
    assert!(!marked(&mut viewer), "a move: dropped");
}

/// S2d.6b: FAR's "Detect dump view mode": a NUL in the first 2 KB opens the file in dump mode.
#[test]
fn a_file_with_a_nul_opens_in_dump_mode() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = viewer_of(&temp, "bin\0ary");
    let deadline = Instant::now() + Duration::from_secs(5);
    while viewer.text().mode() != Mode::Dump {
        viewer.pump();
        assert!(Instant::now() < deadline, "not dump");
    }
    let off = crate::settings::ViewerSettings {
        detect_dump: false,
        ..crate::settings::ViewerSettings::default()
    };
    let path = temp.path().join("v.txt");
    let mut plain = Viewer::open(&path);
    plain.apply_settings(&off);
    settle(&mut plain, |screen| !screen.rows.is_empty());
    assert_eq!(plain.text().mode(), Mode::Text, "off: text");
}

/// S2d.6b: "Autodetect code page" off - UTF-8 by its mark alone, else the default code page.
#[test]
fn without_autodetection_a_file_is_read_in_the_default_page_but_a_mark_tells() {
    let temp = tempfile::tempdir().unwrap();
    let plain = temp.path().join("plain.txt");
    std::fs::write(&plain, "caf\u{e9}\n").unwrap();
    let marked = temp.path().join("marked.txt");
    std::fs::write(&marked, "\u{feff}caf\u{e9}\n").unwrap();
    let settings = crate::settings::ViewerSettings {
        auto_detect: false,
        default_page: 1200,
        ..crate::settings::ViewerSettings::default()
    };
    for (path, page) in [(&plain, 1200), (&marked, 65001)] {
        let mut viewer = Viewer::open(path);
        viewer.apply_settings(&settings);
        settle(&mut viewer, |screen| !screen.rows.is_empty());
        assert_eq!(
            viewer.text().page_number(),
            Some(page),
            "{}",
            path.display()
        );
    }
    let mut viewer = Viewer::open(&plain);
    settle(&mut viewer, |screen| !screen.rows.is_empty());
    assert_eq!(
        viewer.text().page_number(),
        Some(65001),
        "told by its bytes"
    );
}

/// S2d.6b: FAR's arrows where a line not wrapped goes on, Ctrl+S's scroll bar, a NUL shown as `·`.
#[test]
fn arrows_the_scroll_bar_and_a_visible_nul() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("v.txt");
    std::fs::write(&path, format!("{}\na\0b\n", "x".repeat(30))).unwrap();
    let mut viewer = Viewer::open(&path);
    // Its NUL would open it in dump mode (FAR's `DetectDumpMode`).
    let settings = crate::settings::ViewerSettings {
        visible_nul: true,
        detect_dump: false,
        ..crate::settings::ViewerSettings::default()
    };
    viewer.apply_settings(&settings);
    settle(&mut viewer, |screen| !screen.rows.is_empty());
    press(&mut viewer, KeyCode::F(2));
    let screen = settle(&mut viewer, |screen| screen.rows.len() >= 2);
    assert_eq!(screen.rows[1], "a·b", "FAR's ZeroChar");
    assert_eq!(viewer.overflow(20, 5)[..2], [(false, true), (false, false)]);
    press(&mut viewer, KeyCode::Right);
    settle(&mut viewer, |screen| screen.rows[0].len() == 20);
    assert_eq!(viewer.overflow(20, 5)[0], (true, true), "« and »");
    assert_eq!(viewer.text_width(20), 20);
    let ctrl_s = key(KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert!(matches!(viewer.handle_key(ctrl_s), ViewerAction::Scrollbar));
    viewer.scrollbar = true;
    assert_eq!(viewer.text_width(20), 19, "the bar takes the last column");
    let (start, end, size) = viewer.extent(19, 5).unwrap();
    assert_eq!((start, size), (0, 35));
    assert!(end > 0);
}

/// S2d.6f: FAR's viewer bookmarks - Ctrl+Shift+0..9 (RCtrl+0..9) keeps the place shown, Ctrl+0..9 goes
/// back to it; kept in the place left with "Save bookmarks".
#[test]
fn ctrl_shift_digit_keeps_a_place_and_ctrl_digit_goes_back() {
    let temp = tempfile::tempdir().unwrap();
    let text: String = (1..=100).map(|n| format!("line {n}\n")).collect();
    let mut viewer = viewer_of(&temp, &text);
    for _ in 0..3 {
        press(&mut viewer, KeyCode::PageDown);
    }
    let kept = settle(&mut viewer, |screen| screen.rows[0] != "line 1").rows[0].clone();
    let set = key(
        KeyCode::Char('5'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert!(matches!(viewer.handle_key(set), ViewerAction::None));
    press(&mut viewer, KeyCode::Home);
    first_row(&mut viewer, "line 1");
    viewer.handle_key(key(KeyCode::Char('5'), KeyModifiers::CONTROL));
    first_row(&mut viewer, &kept);
    viewer.settings.save_bookmarks = true;
    let place = viewer.position().unwrap();
    assert!(place.bookmarks[5].is_some(), "{:?}", place.bookmarks);
}
