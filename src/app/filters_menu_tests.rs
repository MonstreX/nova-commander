//! Ctrl+I: the filters menu and the panel read through it.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;

/// A folder with `a.txt`, `b.TXT`, `c.rs`, `noext` and the folder `sub`, read.
fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.TXT", "c.rs", "noext"] {
        std::fs::write(temp.path().join(name), "x").unwrap();
    }
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    let mut app = app_in(temp.path());
    settle(&mut app, 6);
    (temp, app)
}

/// Ticks until the active panel has `count` entries.
fn settle(app: &mut App, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.panels[0].entries.len() != count || app.panels[0].reading().is_some() {
        assert!(Instant::now() < deadline, "{:?}", names(app));
        app.tick();
    }
}

fn names(app: &App) -> Vec<String> {
    let mut names: Vec<String> = (app.panels[0].entries.iter())
        .filter(|entry| !entry.is_parent)
        .map(|entry| entry.name.clone())
        .collect();
    names.sort();
    names
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn menu(app: &App) -> &FiltersMenu {
    let Some(Overlay::Filters(menu)) = &app.overlay else {
        panic!("no menu");
    };
    menu
}

/// The cursor to the line of `item`.
fn go_to(app: &mut App, item: &FilterItem) {
    let Some(Overlay::Filters(menu)) = &mut app.overlay else {
        panic!("no menu");
    };
    menu.selected = menu
        .lines
        .iter()
        .position(|line| &line.item == item)
        .unwrap();
}

#[test]
fn the_menu_lists_folders_and_the_panels_file_types_once_each() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    let menu = menu(&app);
    let items: Vec<&FilterItem> = menu.lines.iter().map(|line| &line.item).collect();
    assert_eq!(
        items,
        [
            &FilterItem::Separator,
            &FilterItem::Folders,
            &FilterItem::Type("*.".to_string()),
            &FilterItem::Type("*.rs".to_string()),
            &FilterItem::Type("*.txt".to_string()),
        ]
    );
    assert_eq!(menu.selected, 1, "not on the separator");
    assert_eq!(
        menu.lines[1].text,
        "&0. Folders                \u{2502} +D          ... \u{2502} "
    );
    assert_eq!(
        menu.lines[3].text,
        "&2. Panel file type        \u{2502} -D          ... \u{2502} *.rs"
    );
}

#[test]
fn a_type_marked_plus_shows_only_its_files_and_the_folders() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    go_to(&mut app, &FilterItem::Type("*.txt".to_string()));
    press(&mut app, KeyCode::Char('+'), KeyModifiers::NONE);
    assert_eq!(menu(&app).lines[4].check, Some('+'));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    settle(&mut app, 4);
    assert_eq!(names(&app), ["a.txt", "b.TXT", "sub"]);
    assert!(app.panels[0].filter_is_on());
    assert!(
        !app.panels[1].filter_is_on(),
        "the other panel's area is its own"
    );

    // The hidden types stay in the menu; the mark goes off again with the same key.
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    assert_eq!(menu(&app).lines.len(), 5);
    go_to(&mut app, &FilterItem::Type("*.txt".to_string()));
    press(&mut app, KeyCode::Char('+'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app, 6);
    assert!(app.filters.types.is_empty(), "marked nowhere: dropped");
}

#[test]
fn minus_hides_a_type_and_the_folders_line_hides_folders() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    go_to(&mut app, &FilterItem::Folders);
    press(&mut app, KeyCode::Char('-'), KeyModifiers::NONE);
    // Gone down to `*.`; Space marks it `+`... and BS takes it off.
    press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
    go_to(&mut app, &FilterItem::Type("*.".to_string()));
    assert_eq!(menu(&app).lines[2].check, Some('+'));
    press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    settle(&mut app, 4);
    assert_eq!(names(&app), ["a.txt", "b.TXT", "noext"]);
    assert_eq!(app.filters.folders.left_panel, "-");
    assert_eq!(app.filters.types[0].mask, "*.rs");
    assert_eq!(app.filters.types[0].marks.left_panel, "X");

    // Shift+BS takes every mark off.
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Backspace, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    settle(&mut app, 6);
    assert!(!app.panels[0].filter_is_on());
}

#[test]
fn the_marks_go_with_the_panels_and_outlive_the_program() {
    let (temp, mut app) = app();
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    go_to(&mut app, &FilterItem::Type("*.rs".to_string()));
    press(&mut app, KeyCode::Char('-'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    settle(&mut app, 5);
    press(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(app.filters.types[0].marks.right_panel, "-");
    assert!(app.filters.types[0].marks.left_panel.is_empty());
    assert!(app.panels[1].filter_is_on());

    let saved = app.to_settings();
    assert_eq!(saved.type_filters, app.filters.types);
    let again = App::new(temp.path(), 100, 40, &saved);
    assert!(again.panels[1].filter_is_on());
    assert!(!again.panels[0].filter_is_on());
}

#[test]
fn a_users_filter_is_marked_moved_and_deleted() {
    let (_temp, mut app) = app();
    let filter = |title: &str, mask: &str| Filter {
        title: title.to_string(),
        mask: mask.to_string(),
        ..Filter::default()
    };
    app.filters.list = vec![filter("Rust", "*.rs"), filter("Text & co", "*.txt")];
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    assert_eq!(
        menu(&app).lines[1].text,
        // FAR's `NameAmpFix`: a title's `&` is a hotkey's mark, one column more.
        "Text & co              \u{2502}             ... \u{2502} *.txt"
    );
    go_to(&mut app, &FilterItem::User(0));
    press(&mut app, KeyCode::Char('I'), KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert_eq!(app.filters.list[1].title, "Rust");
    assert_eq!(
        menu(&app).lines[1].check,
        Some('I'),
        "the mark goes with it"
    );

    // F4 elsewhere says only these can be edited; Del on one asks.
    go_to(&mut app, &FilterItem::Folders);
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Message { error: true, .. }))
    ));
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.tick();
    go_to(&mut app, &FilterItem::User(0));
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    let Some(Overlay::Dialog(Dialog::Confirm { lines, .. })) = &app.overlay else {
        panic!("no question");
    };
    assert_eq!(lines[1], "\"Text & co\"");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.tick();
    assert_eq!(app.filters.list.len(), 1);
    assert_eq!(
        menu(&app).lines[0].check,
        Some('I'),
        "the other keeps its mark"
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    settle(&mut app, 2);
    assert_eq!(
        names(&app),
        ["c.rs"],
        "a strong include of the files and folders"
    );
}

#[test]
fn a_sort_keeps_the_hidden_types_and_a_marked_type_stays_with_no_file() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    go_to(&mut app, &FilterItem::Type("*.txt".to_string()));
    press(&mut app, KeyCode::Char('+'), KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    settle(&mut app, 4);
    press(&mut app, KeyCode::F(4), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    let types: Vec<FilterItem> = (menu(&app).lines.iter())
        .map(|line| line.item.clone())
        .filter(|item| matches!(item, FilterItem::Type(_)))
        .collect();
    assert_eq!(types.len(), 3, "the hidden *.rs and *. stay after the sort");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    // A type marked for this panel is listed with no file of it here.
    app.filters.types[0].mask = "*.zip".to_string();
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    let zip = (menu(&app).lines.iter())
        .find(|line| line.item == FilterItem::Type("*.zip".to_string()))
        .unwrap();
    assert_eq!(zip.check, Some('+'));
}

#[test]
fn hotkeys_take_the_cursor_and_ctrl_minus_marks_nothing() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('3'), KeyModifiers::NONE);
    assert_eq!(
        menu(&app).lines[menu(&app).selected].item,
        FilterItem::Type("*.txt".to_string())
    );
    press(&mut app, KeyCode::Char('0'), KeyModifiers::ALT);
    assert_eq!(
        menu(&app).lines[menu(&app).selected].item,
        FilterItem::Folders
    );
    press(&mut app, KeyCode::Char('-'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char(' '), KeyModifiers::SHIFT);
    assert!(menu(&app).lines.iter().all(|line| line.check.is_none()));
}

#[test]
fn the_menus_filter_does_nothing_on_the_info_panel() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::Char('l'), KeyModifiers::CONTROL);
    if !app.info_is_active() {
        press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    }
    app.handle_overlay_key(
        Overlay::Menu(crate::app::MenuState {
            menu: 2,
            item: 12,
            no_tree: false,
            provider_modes: [0; 2],
        }),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );
    assert!(app.overlay.is_none());
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    assert!(app.overlay.is_none());
}

#[test]
fn attributes_show_four_letters_and_long_titles_end_with_an_ellipsis() {
    assert_eq!(attributes_text(0x1 | 0x2 | 0x4 | 0x20, 0x10), "+RHSA -D   ");
    assert_eq!(
        attributes_text(0x1 | 0x2 | 0x4 | 0x20 | 0x100, 0),
        "+RHS\u{2026}      "
    );
    assert_eq!(
        fit_to_left("a very long title of a filter", 21),
        "a very long title of\u{2026}"
    );
}
