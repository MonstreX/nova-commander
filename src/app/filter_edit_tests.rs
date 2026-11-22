//! Ins, F4, F5 in the filters menu through the filter's dialog, as FAR's `EditFilters`.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::filters_menu::FilterItem;
use crate::app::form::Control;

/// A folder with `a.txt`, `b.rs` and the folder `sub`, read.
fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.rs"] {
        std::fs::write(temp.path().join(name), "x").unwrap();
    }
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    let mut app = app_in(temp.path());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.panels[0].entries.len() != 4 || app.panels[0].reading().is_some() {
        assert!(Instant::now() < deadline, "the panel was not read");
        app.tick();
    }
    (temp, app)
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    app.tick();
}

fn plain(app: &mut App, code: KeyCode) {
    press(app, code, KeyModifiers::NONE);
}

fn dialog(app: &mut App) -> &mut Form {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Filter(_) | FormAction::FilterAttributes { .. },
        })) => form,
        _ => panic!("no filter dialog"),
    }
}

fn menu(app: &mut App) -> &mut FiltersMenu {
    match &mut app.overlay {
        Some(Overlay::Filters(menu)) => menu,
        _ => panic!("no menu"),
    }
}

/// The menu's cursor to the line of `item`.
fn go_to(app: &mut App, item: &FilterItem) {
    let menu = menu(app);
    menu.selected = (menu.lines.iter())
        .position(|line| &line.item == item)
        .unwrap();
}

/// Ctrl+I, Ins, the name and the mask, OK.
fn add(app: &mut App, name: &str, mask: &str) {
    press(app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    plain(app, KeyCode::Insert);
    dialog(app).set_edit_text(fd::NAME, name);
    dialog(app).set_edit_text(fd::MASK, mask);
    plain(app, KeyCode::Enter);
}

#[test]
fn ins_makes_a_filter_for_files_before_the_cursor() {
    let (_temp, mut app) = app();
    add(&mut app, "Text", "*.txt");
    assert_eq!(app.filters.list.len(), 1);
    let filter = &app.filters.list[0];
    assert_eq!(
        (filter.title.as_str(), filter.mask.as_str()),
        ("Text", "*.txt")
    );
    assert!(
        filter.use_attributes && filter.attributes_clear == DIRECTORY,
        "for files only"
    );
    assert_eq!(
        menu(&mut app).selected,
        0,
        "the new filter under the cursor"
    );
    assert!(menu(&mut app).lines[0].text.starts_with("Text"));
    // Another one before the first, the first one's mark kept with it.
    plain(&mut app, KeyCode::Char('+'));
    go_to(&mut app, &FilterItem::User(0));
    plain(&mut app, KeyCode::Insert);
    dialog(&mut app).set_edit_text(fd::NAME, "Rust");
    plain(&mut app, KeyCode::Enter);
    let titles: Vec<&str> = (app.filters.list.iter())
        .map(|filter| filter.title.as_str())
        .collect();
    assert_eq!(titles, ["Rust", "Text"]);
    let checks: Vec<Option<char>> = menu(&mut app).lines.iter().map(|line| line.check).collect();
    assert_eq!(&checks[..2], [None, Some('+')]);
    plain(&mut app, KeyCode::Esc);
    assert_eq!(
        (app.filters.list[1].marks.left_panel.as_str()),
        "+",
        "the marks kept as the menu closes"
    );
}

#[test]
fn f4_changes_the_users_filter_and_esc_leaves_it() {
    let (_temp, mut app) = app();
    add(&mut app, "Text", "*.txt");
    plain(&mut app, KeyCode::F(4));
    assert_eq!(dialog(&mut app).edit_text(fd::NAME), "Text");
    dialog(&mut app).set_edit_text(fd::MASK, "*.md");
    plain(&mut app, KeyCode::Esc);
    assert_eq!(app.filters.list[0].mask, "*.txt", "Esc keeps it");
    plain(&mut app, KeyCode::F(4));
    dialog(&mut app).set_edit_text(fd::MASK, "*.md");
    plain(&mut app, KeyCode::Enter);
    assert_eq!(app.filters.list[0].mask, "*.md");
    assert_eq!(app.filters.list.len(), 1);
}

#[test]
fn f5_copies_the_line_under_the_cursor() {
    let (_temp, mut app) = app();
    add(&mut app, "Text", "*.txt");
    plain(&mut app, KeyCode::Char('+'));
    go_to(&mut app, &FilterItem::User(0));
    plain(&mut app, KeyCode::F(5));
    assert_eq!(dialog(&mut app).edit_text(fd::NAME), "", "no title");
    assert_eq!(dialog(&mut app).edit_text(fd::MASK), "*.txt");
    plain(&mut app, KeyCode::Enter);
    assert_eq!(app.filters.list.len(), 2);
    assert_eq!(
        app.filters.list[0].marks,
        crate::settings::FilterMarks::default()
    );
    go_to(&mut app, &FilterItem::Type("*.rs".to_string()));
    plain(&mut app, KeyCode::F(5));
    assert_eq!(
        dialog(&mut app).edit_text(fd::MASK),
        "*.rs",
        "a type's filter"
    );
    plain(&mut app, KeyCode::Esc);
    go_to(&mut app, &FilterItem::Folders);
    plain(&mut app, KeyCode::F(5));
    assert!(
        !dialog(&mut app).is_checked(fd::MATCH_MASK),
        "FAR's folders: no mask"
    );
    plain(&mut app, KeyCode::Enter);
    let folders = app.filters.list.last().unwrap();
    assert_eq!(folders.attributes_set, DIRECTORY);
    assert_eq!(menu(&mut app).selected, 2, "after the user's filters");
}

#[test]
fn a_wrong_mask_or_size_warns_and_the_dialog_comes_back() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    plain(&mut app, KeyCode::Insert);
    dialog(&mut app).set_edit_text(fd::MASK, " , ");
    plain(&mut app, KeyCode::Enter);
    let Some(Overlay::Dialog(Dialog::Message { lines, title, .. })) = &app.overlay else {
        panic!("no warning");
    };
    assert_eq!(
        (title.as_str(), lines[0].as_str()),
        ("Warning", "File-mask string contains errors")
    );
    plain(&mut app, KeyCode::Enter);
    assert_eq!(
        dialog(&mut app).edit_text(fd::MASK),
        " , ",
        "back as it was"
    );
    dialog(&mut app).set_edit_text(fd::MASK, "*");
    if let Some(Control::Check { checked, .. }) = dialog(&mut app).control_mut(fd::MATCH_SIZE) {
        *checked = true;
    }
    dialog(&mut app).set_edit_text(fd::SIZE_FROM, "big");
    plain(&mut app, KeyCode::Enter);
    let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
        panic!("no warning");
    };
    assert_eq!(lines[0], fd::BAD_SIZE);
    plain(&mut app, KeyCode::Enter);
    assert_eq!(dialog(&mut app).edit_text(fd::SIZE_FROM), "big");
    assert!(app.filters.list.is_empty());
}

#[test]
fn the_attributes_go_through_their_own_dialog() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    plain(&mut app, KeyCode::Insert);
    dialog(&mut app).focus_item(fd::ATTRIBUTES);
    plain(&mut app, KeyCode::Enter);
    assert_eq!(dialog(&mut app).title, "Attributes");
    assert_eq!(
        dialog(&mut app).tristate(7),
        Some(false),
        "folders left out"
    );
    dialog(&mut app).set_tristate(2, Some(true));
    dialog(&mut app).focus_item(fd::FILTER_ATTRIBUTES.len() + 1);
    plain(&mut app, KeyCode::Enter);
    assert_eq!(dialog(&mut app).title, "Filter");
    dialog(&mut app).focus_item(fd::RESET);
    plain(&mut app, KeyCode::Enter);
    assert_eq!(
        dialog(&mut app).edit_text(fd::MASK),
        "*",
        "Reset stays in the dialog"
    );
    dialog(&mut app).focus_item(fd::ATTRIBUTES);
    plain(&mut app, KeyCode::Enter);
    assert_eq!(
        dialog(&mut app).tristate(2),
        None,
        "Reset took the hidden one off"
    );
    dialog(&mut app).set_tristate(2, Some(true));
    dialog(&mut app).focus_item(fd::FILTER_ATTRIBUTES.len() + 1);
    plain(&mut app, KeyCode::Enter);
    dialog(&mut app).focus_item(fd::OK);
    plain(&mut app, KeyCode::Enter);
    let filter = &app.filters.list[0];
    assert_eq!(
        (filter.attributes_set, filter.attributes_clear),
        (0x2, DIRECTORY)
    );
}

#[test]
fn f4_on_a_line_not_the_users_says_only_custom_ones() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    go_to(&mut app, &FilterItem::Folders);
    plain(&mut app, KeyCode::F(4));
    let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
        panic!("no message");
    };
    assert_eq!(lines[0], "Only custom filter can be edited");
}

#[test]
fn the_menu_opens_again_with_a_marked_filter() {
    let (_temp, mut app) = app();
    add(&mut app, "Texts", "*.txt");
    plain(&mut app, KeyCode::Char('+'));
    plain(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    assert!(menu(&mut app).lines[0].text.starts_with("Texts"));
}
