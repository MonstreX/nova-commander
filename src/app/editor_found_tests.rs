//! F7's "All" list and the dialog's Word / Selection pickers.

use std::time::{Duration, Instant};

use super::*;
use crate::app::editor_search_form::{self, ALL, SELECTION, TEXT, WORD};
use crate::app::viewer::search::SearchParams;
use crate::app::{Dialog, dialog::FormAction};

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.tick();
        let ready = match &mut app.screen {
            Screen::Editor(editor) => {
                editor.text.room.set((40, 5));
                editor.text.settled() && editor.text.place().is_some()
            }
            _ => true,
        };
        if ready && !app.is_waiting() {
            return;
        }
        assert!(Instant::now() < deadline, "never done");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn opened(text: &str) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("e.txt");
    std::fs::write(&path, text).unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.edit_file(&path, false);
    settle(&mut app);
    (temp, app)
}

fn the_list(app: &App) -> &PluginMenu {
    match &app.overlay {
        Some(Overlay::PluginMenu(menu)) if menu.found_all.is_some() => menu,
        _ => panic!("no list"),
    }
}

fn place(app: &App) -> (u64, usize) {
    match &app.screen {
        Screen::Editor(editor) => editor.text.place().map(|it| (it.0, it.2)).unwrap(),
        _ => panic!("no editor"),
    }
}

/// FAR's "All": the list's title, its columns and marks; Ctrl+Enter goes and keeps it, Shift+F5 and
/// F5 show it otherwise, Enter goes and closes it.
#[test]
fn all_lists_every_match_and_goes_to_the_one_chosen() {
    let (_temp, mut app) = opened("one two\r\nthree\r\ntwo two\r\n");
    let params = SearchParams {
        text: "two".to_string(),
        ..SearchParams::default()
    };
    app.finish_editor_search(&editor_search_form::form(&params), Some(ALL));
    settle(&mut app);
    let list = the_list(&app);
    assert_eq!(list.title, "Occurrences: 3, lines: 2");
    let texts: Vec<&str> = list.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(texts, ["1│5│one two", "3│1│two two", "3│5│two two"]);
    assert_eq!(list.marks, [Some((8, 11)), Some((4, 7)), Some((8, 11))]);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
    settle(&mut app);
    assert_eq!(place(&app), (3, 1), "gone to the second");
    assert_eq!(the_list(&app).selected, 1, "the list kept");
    press(&mut app, KeyCode::F(5), KeyModifiers::SHIFT);
    assert_eq!(the_list(&app).lines[0].text, "one two", "no columns");
    assert_eq!(the_list(&app).marks[0], Some((4, 7)));
    press(&mut app, KeyCode::F(5), KeyModifiers::NONE);
    assert_eq!(the_list(&app).max_height, None, "zoomed");
    press(&mut app, KeyCode::Up, KeyModifiers::CONTROL);
    settle(&mut app);
    assert_eq!(place(&app).0, 2, "the editor scrolled under the list");
    assert!(app.overlay.is_some());
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app);
    assert!(app.overlay.is_none());
    assert_eq!(place(&app), (3, 5));
}

/// FAR's pickers: "Word" the word at the cursor, "Selection" the cursor's line without a block; the
/// dialog comes back with it in "Search for".
#[test]
fn word_and_selection_put_the_editor_s_text_into_the_dialog() {
    let (_temp, mut app) = opened("alpha beta,gamma\r\n");
    for _ in 0..7 {
        press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    }
    settle(&mut app);
    let form = editor_search_form::form(&SearchParams::default());
    app.finish_editor_search(&form, Some(WORD));
    let Some(Overlay::Dialog(Dialog::Form {
        form,
        action: FormAction::EditorSearch,
    })) = &app.overlay
    else {
        panic!("no dialog");
    };
    assert_eq!(form.edit_text(TEXT), "beta");
    let form = editor_search_form::form(&SearchParams::default());
    app.overlay = None;
    app.finish_editor_search(&form, Some(SELECTION));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no dialog");
    };
    assert_eq!(form.edit_text(TEXT), "alpha beta,gamma");
    let line: Vec<char> = "a b".chars().collect();
    assert_eq!(editor_search_form::picked(&line, 1, None, false), "a");
    assert_eq!(
        editor_search_form::picked(&line, 9, None, false),
        "",
        "past the end"
    );
    assert_eq!(
        editor_search_form::picked(&line, 0, Some("x\ny"), true),
        "x"
    );
}

/// FAR's F4 in the list (`SaveFoundItemsToNewEditor`): each line found once in a new editor window
/// on "<name>.Filtered<.ext>", the cursor on the item chosen; the editor it came from behind.
#[test]
fn f4_puts_the_lines_found_in_a_new_editor_window() {
    let (temp, mut app) = opened("one two\r\nthree\r\ntwo two\r\n");
    let params = SearchParams {
        text: "two".to_string(),
        ..SearchParams::default()
    };
    app.finish_editor_search(&editor_search_form::form(&params), Some(ALL));
    settle(&mut app);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    settle(&mut app);
    assert!(app.overlay.is_none(), "the list closed");
    let Screen::Editor(editor) = &app.screen else {
        panic!("no editor");
    };
    assert_eq!(editor.path, temp.path().join("e.Filtered.txt"));
    assert!(!editor.path.exists(), "made by its save");
    let screen = editor.text.screen(40, 5).unwrap();
    assert_eq!(&screen.rows[..3], ["one two", "two two", ""]);
    assert_eq!(
        place(&app),
        (2, 5),
        "on the third item: line 2, its second match (counted from 1)"
    );
    assert_eq!(app.window_lines().len(), 2, "the first editor behind");
}

#[test]
fn the_new_editors_name_follows_fars_formats() {
    let name = |path: &str| filtered_name(std::path::Path::new(path));
    assert_eq!(
        name(r"C:\d\a.txt"),
        std::path::Path::new(r"C:\d\a.Filtered.txt")
    );
    assert_eq!(
        name(r"C:\d\x.rs"),
        std::path::Path::new(r"C:\d\x.rs.Filtered.txt")
    );
    assert_eq!(
        name(r"C:\d\noext"),
        std::path::Path::new(r"C:\d\noext.Filtered.txt")
    );
    assert_eq!(
        name(r"C:\d\a.LOG"),
        std::path::Path::new(r"C:\d\a.Filtered.LOG")
    );
}
