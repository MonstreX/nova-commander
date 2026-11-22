//! FAR's sort groups in a panel: Shift+F11 and "Use sort groups" of the Ctrl+F12 menu, the groups taken
//! from the highlighting groups' upper and lower lists and made again when those change.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::settings::{Filter, HighlightGroup};

fn group(mask: &str) -> HighlightGroup {
    HighlightGroup {
        filter: Filter {
            mask: mask.to_string(),
            ..Filter::default()
        },
        ..HighlightGroup::default()
    }
}

/// `a.txt`, `b.md`, `c.txt`, `z.log` read, `*.md` an upper group and `*.txt` a lower one.
fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.md", "c.txt", "z.log"] {
        std::fs::write(temp.path().join(name), "x").unwrap();
    }
    let mut app = app_in(temp.path());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.panels[0].entries.len() != 5 || app.panels[0].reading().is_some() {
        assert!(Instant::now() < deadline, "the panel was not read");
        app.tick();
    }
    app.highlighting.kept.upper = vec![group("*.md")];
    app.highlighting.kept.lower = vec![group("*.txt")];
    app.make_highlighting();
    (temp, app)
}

fn names(app: &App) -> Vec<&str> {
    (app.panels[0].entries.iter())
        .filter(|entry| !entry.is_parent)
        .map(|entry| entry.name.as_str())
        .collect()
}

#[test]
fn shift_f11_sorts_the_upper_groups_first_and_the_lower_last() {
    let (_temp, mut app) = app();
    assert_eq!(names(&app), ["a.txt", "b.md", "c.txt", "z.log"]);
    app.handle_key(KeyEvent::new(KeyCode::F(11), KeyModifiers::SHIFT));
    assert!(app.panels[0].sort.sort_groups);
    assert_eq!(names(&app), ["b.md", "z.log", "a.txt", "c.txt"]);
    app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::CONTROL));
    assert!(app.panels[0].sort.reverse, "by name again: reversed");
    assert_eq!(
        names(&app),
        ["b.md", "z.log", "c.txt", "a.txt"],
        "the groups keep their order, inside them the names turn round"
    );
    app.handle_key(KeyEvent::new(KeyCode::F(11), KeyModifiers::SHIFT));
    assert!(!app.panels[0].sort.sort_groups);
}

#[test]
fn by_size_the_groups_are_not_looked_at() {
    let (temp, mut app) = app();
    std::fs::write(temp.path().join("z.log"), "a longer text").unwrap();
    app.panels[0].reload();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.panels[0].reading().is_some() {
        assert!(Instant::now() < deadline, "the panel was not read");
        app.tick();
    }
    app.handle_key(KeyEvent::new(KeyCode::F(11), KeyModifiers::SHIFT));
    app.handle_key(KeyEvent::new(KeyCode::F(6), KeyModifiers::CONTROL));
    assert_eq!(
        names(&app)[0],
        "z.log",
        "the largest first, as without groups"
    );
    let mut sizes = app.panels[0].sort;
    sizes.sort_groups = false;
    let grouped = names(&app).join(",");
    app.panels[0].set_sort_order(sizes);
    assert_eq!(names(&app).join(","), grouped);
}

#[test]
fn the_sort_menu_shows_and_turns_the_option() {
    let (_temp, mut app) = app();
    app.open_sort_menu(Side::Left);
    let Some(Overlay::Popup(popup)) = &app.overlay else {
        panic!("no menu");
    };
    let at = (SORT_MENU.iter())
        .position(|(_, _, entry)| *entry == Some(SortEntry::SortGroups))
        .unwrap();
    assert_eq!(SORT_MENU[at].0, "Use sort groups");
    assert_eq!(SORT_MENU[at].1, "Shift+F11");
    assert_eq!(
        SORT_MENU[at - 1].2,
        None,
        "after the separator, before selected first"
    );
    assert_eq!(
        popup.items[at],
        PopupItem::new("Use sort groups", "Shift+F11", None)
    );
    app.overlay = None;
    app.finish_sort_menu(Side::Left, at, ChooseKey::Minus);
    assert!(!app.panels[0].sort.sort_groups, "- keeps it off");
    app.finish_sort_menu(Side::Left, at, ChooseKey::Enter);
    assert!(app.panels[0].sort.sort_groups);
    assert_eq!(names(&app)[0], "b.md");
    app.open_sort_menu(Side::Left);
    let Some(Overlay::Popup(popup)) = &app.overlay else {
        panic!("no menu");
    };
    assert_eq!(
        popup.items[at],
        PopupItem::new("Use sort groups", "Shift+F11", Some('√'))
    );
    app.finish_sort_menu(Side::Left, at, ChooseKey::Plus);
    assert!(app.panels[0].sort.sort_groups, "+ keeps it on");
}

#[test]
fn the_panel_sorts_again_when_the_groups_change() {
    let (_temp, mut app) = app();
    app.panels[0].toggle_sort_groups();
    assert_eq!(names(&app)[0], "b.md");
    app.highlighting.kept.upper = vec![group("z*")];
    app.make_highlighting();
    assert_eq!(names(&app), ["z.log", "b.md", "a.txt", "c.txt"]);
}
