//! Alt+F8: FAR's parsing of an offset, a percentage, hex, relative places and the column; ours of a
//! line; the dialog; the viewer going there.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::{App, Overlay, Screen};

fn coord(value: u64) -> Coord {
    Coord {
        value,
        exist: true,
        ..Coord::default()
    }
}

#[test]
fn far_s_forms_are_read() {
    assert_eq!(parse("1234", false), Some((coord(1234), Coord::default())));
    assert_eq!(
        parse("1234", true).unwrap().0.value,
        0x1234,
        "Hex value checked"
    );
    assert_eq!(parse("0x10", false).unwrap().0.value, 16);
    assert_eq!(parse("$10", false).unwrap().0.value, 16);
    assert_eq!(parse("10h", false).unwrap().0.value, 16);
    assert_eq!(parse("10m", true).unwrap().0.value, 10, "m: decimal");
    let percent = parse("50%", true).unwrap().0;
    assert!(
        percent.percent && percent.value == 50,
        "a percentage is decimal"
    );
    let back = parse("-100", false).unwrap().0;
    assert_eq!((back.relative, back.value), (-1, 100));
    assert_eq!(parse("+5", false).unwrap().0.relative, 1);
    let (row, col) = parse("200 30", false).unwrap();
    assert_eq!((row.value, col.value), (200, 30));
    assert_eq!(parse("200:+3", false).unwrap().1.relative, 1);
    assert_eq!(parse(",7", false).unwrap(), (Coord::default(), coord(7)));
    assert_eq!(parse("zz", false), None);
}

#[test]
fn a_trailing_l_names_a_line() {
    let line = parse("1500l", true).unwrap().0;
    assert!(
        line.line && line.value == 1500,
        "decimal even with Hex value"
    );
    assert!(parse("0x10L", false).unwrap().0.line);
}

#[test]
fn offsets_and_percentages_land_within_the_file() {
    assert_eq!(offset_of(coord(50), 0, 100), 50);
    assert_eq!(
        offset_of(coord(500), 0, 100),
        100,
        "no further than the end"
    );
    let half = Coord {
        percent: true,
        ..coord(50)
    };
    assert_eq!(offset_of(half, 0, 1000), 500);
    let back = Coord {
        relative: -1,
        ..coord(30)
    };
    assert_eq!(offset_of(back, 20, 1000), 0);
    let on = Coord {
        relative: 1,
        ..coord(30)
    };
    assert_eq!(offset_of(on, 20, 1000), 50);
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn first_row(app: &mut App) -> String {
    let Screen::Viewer(viewer) = &app.screen else {
        panic!("no viewer");
    };
    viewer.text.room.set((40, 5));
    viewer
        .text
        .screen(40, 5)
        .and_then(|screen| screen.rows.first().cloned())
        .unwrap_or_default()
}

fn shows(app: &mut App, row: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while first_row(app) != row {
        assert!(Instant::now() < deadline, "not {row}: {}", first_row(app));
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn go_to(app: &mut App, text: &str) {
    press(app, KeyCode::F(8), KeyModifiers::ALT);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(_))),
        "the dialog"
    );
    // The last line given comes back in it (FAR's `DIF_USELASTHISTORY`): Ctrl+Y empties it.
    press(app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    for ch in text.chars() {
        press(app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

#[test]
fn alt_f8_goes_to_an_offset_a_percentage_and_a_line() {
    let temp = tempfile::tempdir().unwrap();
    let text: String = (1..=3000).map(|n| format!("line {n}\n")).collect();
    std::fs::write(temp.path().join("v.txt"), &text).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.view_file(&temp.path().join("v.txt"), false);
    shows(&mut app, "line 1");
    go_to(&mut app, "2500l");
    shows(&mut app, "line 2500");
    // Byte 20 is in "line 3" (7 bytes a line for 1..9).
    go_to(&mut app, "20m");
    shows(&mut app, "line 3");
    go_to(&mut app, "100%");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !first_row(&mut app).is_empty() && first_row(&mut app) != "line 3000" {
        assert!(Instant::now() < deadline, "the end");
        app.tick();
    }
    go_to(&mut app, "99999l");
    shows(&mut app, "line 2997");
}
