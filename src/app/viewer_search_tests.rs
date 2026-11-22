//! The viewer's search: FAR's dialog and its Text / Hex, the patterns, F7 / Shift+F7 / Alt+F7 in the
//! viewer with the selection, the question at the end and "Could not find".

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::{App, Dialog, Overlay, Screen};

#[test]
fn the_dialog_is_far_s_and_text_turns_into_hex() {
    let params = SearchParams {
        text: "AB".to_string(),
        ..SearchParams::default()
    };
    let mut form = form(&params);
    assert!(!form.items[TEXT].hidden && form.items[HEX].hidden);
    assert_eq!(form.edit_text(TEXT), "AB");
    if let Some(Control::Radio { checked, .. }) = form.control_mut(RADIO_HEX) {
        *checked = true;
    }
    if let Some(Control::Radio { checked, .. }) = form.control_mut(RADIO_TEXT) {
        *checked = false;
    }
    follow(&mut form, 65001);
    assert!(form.items[TEXT].hidden && !form.items[HEX].hidden);
    assert!(
        form.masked_text(HEX).starts_with("41 42"),
        "{}",
        form.masked_text(HEX)
    );
    assert!(form.items[CASE].disabled && form.items[REGEX].disabled);
    let read = read(&form);
    assert!(read.hex_mode && read.hex.starts_with("41 42"));
    assert_eq!(direction(Some(NEXT)), Some(false));
    assert_eq!(direction(Some(PREVIOUS)), Some(true));
    assert_eq!(direction(None), None);
}

#[test]
fn the_patterns_and_their_shown_forms() {
    let text = SearchParams {
        text: "two".to_string(),
        ..SearchParams::default()
    };
    let (_, shown) = pattern(&text).unwrap().unwrap();
    assert_eq!(shown, "\"two\"");
    let hex = SearchParams {
        hex: "0d 0A".to_string(),
        hex_mode: true,
        ..SearchParams::default()
    };
    let (pattern_of_hex, shown) = pattern(&hex).unwrap().unwrap();
    assert_eq!(shown, "0D 0A");
    assert!(matches!(pattern_of_hex, Pattern::Bytes(bytes) if bytes == [13, 10]));
    let broken = SearchParams {
        text: "(".to_string(),
        regex: true,
        ..SearchParams::default()
    };
    assert!(pattern(&broken).is_err());
    assert!(
        pattern(&SearchParams::default()).unwrap().is_none(),
        "nothing to look for"
    );
    // Review A6: `^` and `$` at each line's ends, as FAR's search a line at a time.
    let anchored = SearchParams {
        text: "^two$".to_string(),
        regex: true,
        ..SearchParams::default()
    };
    let Ok(Some((Pattern::Regex(regex), _))) = pattern(&anchored) else {
        panic!("no regex");
    };
    assert!(regex.is_match("one\r\ntwo\r\nthree"));
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn viewer(app: &mut App) -> &mut Viewer {
    match &mut app.screen {
        Screen::Viewer(viewer) => viewer,
        _ => panic!("no viewer"),
    }
}

/// Ticks until the search's work is done (its waiting window gone).
fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.tick();
        viewer(app).text.room.set((40, 5));
        if !app.is_waiting() && viewer(app).text.encoding().is_some() {
            return;
        }
        assert!(Instant::now() < deadline, "never done");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// F7 finds the text and selects it; Shift+F7 goes on; at the end the question, Yes goes round;
/// Alt+F7 goes back; a text that is not there says so.
#[test]
fn f7_finds_and_shift_f7_and_alt_f7_go_on() {
    let temp = tempfile::tempdir().unwrap();
    let lines: String = (1..=200).map(|n| format!("line {n} needle\n")).collect();
    std::fs::write(temp.path().join("v.txt"), &lines).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.view_file(&temp.path().join("v.txt"), false);
    settle(&mut app);
    app.viewer_search = SearchParams {
        text: "line 150 n".to_string(),
        ..SearchParams::default()
    };
    press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(_))),
        "the dialog"
    );
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    let at = lines.find("line 150 n").unwrap() as u64;
    assert_eq!(viewer(&mut app).searched.found, Some((at, 10)));
    // Going on: none after it; the question at the end, Yes: round to the same one.
    press(&mut app, KeyCode::F(7), KeyModifiers::SHIFT);
    settle(&mut app);
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the question");
    };
    assert_eq!(form.title, "Search");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert_eq!(
        viewer(&mut app).searched.found,
        Some((at, 10)),
        "round to it"
    );
    app.viewer_search.text = "needle".to_string();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    settle(&mut app);
    let before = lines[..at as usize].rfind("needle").unwrap() as u64;
    assert_eq!(viewer(&mut app).searched.found, Some((before, 6)), "back");
    app.viewer_search.text = "absent".to_string();
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    settle(&mut app);
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the question");
    };
    assert_eq!(form.title, "Search");
}

/// R7 review D7 (FAR's `Viewer::ProcessKey`): Ctrl+C copies what F7 found, Ctrl+U lets it go, Alt+BS
/// goes back to the place before the search, Ctrl+Z on to the one before; Num5 closes.
#[test]
fn ctrl_c_copies_the_found_text_and_alt_bs_goes_back() {
    let temp = tempfile::tempdir().unwrap();
    let lines: String = (1..=200).map(|n| format!("line {n} needle\n")).collect();
    std::fs::write(temp.path().join("v.txt"), &lines).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.view_file(&temp.path().join("v.txt"), false);
    settle(&mut app);
    let position = |app: &mut App| viewer(app).text.facts().unwrap().position;
    press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
    settle(&mut app);
    let paged = position(&mut app);
    assert!(paged > 0);
    app.viewer_search.text = "line 150 n".to_string();
    press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    let copied = app.clipboard.text().unwrap();
    assert_eq!(copied.as_deref(), Some("line 150 n"));
    press(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(viewer(&mut app).found_text(), None, "Ctrl+U: not marked");
    assert!(position(&mut app) > paged, "at the found text");
    press(&mut app, KeyCode::Backspace, KeyModifiers::ALT);
    settle(&mut app);
    assert_eq!(position(&mut app), paged, "back before the search");
    press(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);
    settle(&mut app);
    assert_eq!(position(&mut app), 0, "and before the page");
    press(&mut app, KeyCode::KeypadBegin, KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Panels), "Num5 closes");
}
