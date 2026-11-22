//! "Use filter" and Filter of F5 / F6 / Alt+F6 as FAR's copy dialog has them.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::transfer_dialog::COPY_BUTTON;

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    app.tick();
}

fn form(app: &mut App) -> &mut Form {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Transfer { .. },
        })) => form,
        _ => panic!("no copy dialog"),
    }
}

/// A folder with `a.txt`, `b.rs` and the folder `sub` (with `c.txt`), read, and a target folder.
fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir_all(source.join("sub")).unwrap();
    std::fs::write(source.join("a.txt"), "a").unwrap();
    std::fs::write(source.join("b.rs"), "b").unwrap();
    std::fs::write(source.join("sub").join("c.txt"), "c").unwrap();
    std::fs::create_dir(temp.path().join("target")).unwrap();
    let mut app = app_in(&source);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.panels[0].entries.len() != 4 || app.panels[0].reading().is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "the panel was not read"
        );
        app.tick();
    }
    (temp, app)
}

#[test]
fn use_filter_is_off_enables_filter_and_none_for_a_single_file() {
    let (_temp, mut app) = app();
    app.panels[0].go_to_name("b.rs");
    press(&mut app, KeyCode::F(5), KeyModifiers::NONE);
    assert!(
        form(&mut app).items[USE_FILTER].disabled,
        "FAR: no filter for one file"
    );
    assert!(form(&mut app).items[FILTER].disabled);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.panels[0].go_to_name("sub");
    press(&mut app, KeyCode::F(5), KeyModifiers::NONE);
    assert!(!form(&mut app).is_checked(USE_FILTER) && !form(&mut app).items[USE_FILTER].disabled);
    assert!(form(&mut app).items[FILTER].disabled, "FAR's automation");
    press(&mut app, KeyCode::Char('u'), KeyModifiers::ALT);
    assert!(!form(&mut app).items[FILTER].disabled);
    // FAR's `m_UseFilter` is not kept: each dialog starts with the box off.
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    assert!(!form(&mut app).is_checked(USE_FILTER), "off again");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(6), KeyModifiers::ALT);
    assert!(form(&mut app).items[LINK_FILTER].disabled);
    press(&mut app, KeyCode::Char('u'), KeyModifiers::ALT);
    assert!(
        !form(&mut app).items[LINK_FILTER].disabled,
        "the link dialog too"
    );
}

#[test]
fn filter_opens_copys_menu_and_the_copy_leaves_out_what_it_marks() {
    let (temp, mut app) = app();
    app.panels[0].go_to_name("sub");
    press(&mut app, KeyCode::F(5), KeyModifiers::NONE);
    form(&mut app).set_edit_text(
        super::super::transfer_dialog::TARGET,
        &temp.path().join("target").display().to_string(),
    );
    press(&mut app, KeyCode::Char('u'), KeyModifiers::ALT);
    press(&mut app, KeyCode::Char('e'), KeyModifiers::ALT);
    let Some(Overlay::Filters(menu)) = &mut app.overlay else {
        panic!("no filters menu");
    };
    assert_eq!(menu.area, Area::Copy);
    let at = (menu.lines.iter())
        .position(|line| {
            line.item == super::super::filters_menu::FilterItem::Type("*.txt".to_string())
        })
        .expect("the panel's types");
    menu.selected = at;
    press(&mut app, KeyCode::Char('-'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(
        form(&mut app).is_checked(USE_FILTER),
        "the dialog back as it was"
    );
    assert_eq!(app.filters.types[0].marks.copy, "-");
    form(&mut app).focus_item(COPY_BUTTON);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.tasks.active() > 0 || app.shown_task.is_some() {
        assert!(std::time::Instant::now() < deadline, "the copy did not end");
        app.tick();
    }
    let copied = temp.path().join("target").join("sub");
    assert!(copied.is_dir(), "the folder is copied");
    assert!(!copied.join("c.txt").exists(), "its *.txt left out");
}
