use std::time::{Duration, Instant};

use super::*;
use crate::app::Overlay;
use crate::app::fileops::tests::app_in;

/// An app with `text` in the editor as `m.rs`, highlighted, the cursor at `(line, col)`.
fn editing(text: &str) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("m.rs");
    std::fs::write(&path, text).unwrap();
    let mut app = app_in(temp.path());
    app.edit_file(&path, false);
    until(
        &mut app,
        |app| matches!(&app.screen, Screen::Editor(editor) if editor.text.highlighted()),
    );
    (temp, app)
}

fn until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.tick();
        if let Screen::Editor(editor) = &mut app.screen {
            editor.text.room.set((40, 5));
        }
        let settled = matches!(&app.screen, Screen::Editor(editor) if editor.text.settled()
            && editor.pending.is_none());
        if settled && done(app) {
            return;
        }
        assert!(Instant::now() < deadline, "never came");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn view(app: &App) -> &EditView {
    match &app.screen {
        Screen::Editor(editor) => editor.text.view().unwrap(),
        _ => panic!("the editor"),
    }
}

fn step(app: &mut App, step: Step) {
    if let Screen::Editor(editor) = &mut app.screen {
        editor.text.press(step);
    }
    until(app, |_| true);
}

fn act(app: &mut App, action: Action) {
    app.colorer_parse_action(action);
    until(app, |_| true);
}

const TEXT: &str = "fn f() {\r\n    g(1);\r\n}\r\n";
/// Where the lines start.
const SECOND: u64 = 10;
const THIRD: u64 = 21;

#[test]
fn match_pair_goes_to_the_other_bracket_and_back() {
    let (_temp, mut app) = editing(TEXT);
    step(&mut app, Step::Place(place(0, 7)));
    act(&mut app, Action::MatchPair);
    assert_eq!(view(&app).cursor(), (THIRD, 0), "on the closing brace");
    act(&mut app, Action::MatchPair);
    assert_eq!(view(&app).cursor(), (0, 7), "back on the opening one");
}

#[test]
fn select_block_takes_the_brackets_and_select_pair_what_is_between() {
    let (_temp, mut app) = editing(TEXT);
    step(&mut app, Step::Place(place(0, 7)));
    act(&mut app, Action::SelectBlock);
    assert_eq!(view(&app).block().unwrap(), Some((7, THIRD + 1)));
    act(&mut app, Action::SelectPair);
    assert_eq!(view(&app).block().unwrap(), Some((8, THIRD)));
    // From the closing one the same block.
    step(&mut app, Step::Place(place(THIRD, 0)));
    act(&mut app, Action::SelectBlock);
    assert_eq!(view(&app).block().unwrap(), Some((7, THIRD + 1)));
}

#[test]
fn the_region_at_the_cursor_is_named_and_marked() {
    let (_temp, mut app) = editing(TEXT);
    step(&mut app, Step::Place(place(0, 0)));
    act(&mut app, Action::RegionInfo);
    let Some(Overlay::Dialog(Dialog::Message { title, lines, .. })) = &app.overlay else {
        panic!("Region name");
    };
    assert_eq!(title, "Region name");
    assert!(lines[0].starts_with("Region: "), "{lines:?}");
    assert!(lines[1].starts_with("Scheme: "), "{lines:?}");
    app.overlay = None;
    step(&mut app, Step::Place(place(SECOND, 4)));
    act(&mut app, Action::SelectRegion);
    assert!(view(&app).block().unwrap().is_some(), "a region marked");
}

fn place(line: u64, pos: usize) -> Place {
    Place {
        line: Some(line),
        pos: Some(pos),
        tab_pos: None,
        top: None,
        left: None,
        overtype: None,
    }
}
