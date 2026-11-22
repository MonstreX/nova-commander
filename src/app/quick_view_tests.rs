//! Ctrl+Q: FAR's quick view panel. The panels start in the test's own folder: `sub` (two files in it),
//! `a.bat`, `b.txt`.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

use super::*;
use crate::app::Screen;
use crate::app::command::Command;

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    std::fs::write(temp.path().join("sub").join("x"), "12345").unwrap();
    std::fs::write(temp.path().join("sub").join("y"), "123").unwrap();
    std::fs::write(temp.path().join("a.bat"), "echo").unwrap();
    std::fs::write(temp.path().join("b.txt"), "text").unwrap();
    let app = App::new(temp.path(), 100, 40, &crate::settings::Settings::default());
    (temp, app)
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn ctrl_q(app: &mut App) {
    press(app, KeyCode::Char('q'), KeyModifiers::CONTROL);
}

/// The active panel's cursor on `name`.
fn cursor_on(app: &mut App, name: &str) {
    let path = app.active_panel().path.clone();
    app.active_panel_mut()
        .change_dir(&path, Some(name))
        .unwrap();
    app.pump_quick_view();
}

fn shown(app: &App) -> &Shown {
    (app.quick_view.as_ref())
        .and_then(|view| view.shown.as_ref())
        .expect("something shown")
}

fn shown_name(app: &App) -> String {
    shown(app).path.file_name().unwrap_or_default()
}

/// Ticks until the folder's count ends.
fn counted(app: &mut App) -> Counted {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.pump_quick_view();
        if let Showing::Folder { seen, .. } = &shown(app).what
            && seen.over
        {
            return seen.clone();
        }
        assert!(Instant::now() < deadline, "the count did not end");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn ctrl_q_shows_the_item_under_the_cursor_on_the_passive_panel() {
    let (_temp, mut app) = app();
    cursor_on(&mut app, "sub");
    ctrl_q(&mut app);
    assert_eq!(app.quick_view_side(), Some(Side::Right));
    assert!(!app.shows_files(Side::Right));
    assert_eq!(shown_name(&app), "sub");
    let counted = counted(&mut app);
    assert_eq!((counted.info.folders, counted.info.files), (0, 2));
    assert_eq!(counted.info.bytes, 8);
    assert_eq!(counted.link, None);
}

#[test]
fn a_file_shows_its_type_and_the_parent_entry_its_folder() {
    let (temp, mut app) = app();
    ctrl_q(&mut app);
    cursor_on(&mut app, "a.bat");
    let Showing::File { type_name, .. } = &shown(&app).what else {
        panic!("a file")
    };
    assert!(type_name.as_ref().is_some_and(|name| !name.is_empty()));
    cursor_on(&mut app, "..");
    assert_eq!(shown(&app).path, VfsPath::local(temp.path()));
    assert!(matches!(shown(&app).what, Showing::Folder { .. }));
}

#[test]
fn the_same_item_keeps_its_count() {
    let (_temp, mut app) = app();
    cursor_on(&mut app, "sub");
    ctrl_q(&mut app);
    counted(&mut app);
    assert!(!app.pump_quick_view(), "nothing new to draw");
    cursor_on(&mut app, "sub");
    assert!(matches!(&shown(&app).what, Showing::Folder { seen, .. } if seen.over));
}

#[test]
fn ctrl_q_again_on_either_side_gives_the_files_back() {
    let (_temp, mut app) = app();
    ctrl_q(&mut app);
    ctrl_q(&mut app);
    assert_eq!(app.quick_view_side(), None);
    ctrl_q(&mut app);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert!(app.quick_view_is_active());
    ctrl_q(&mut app);
    assert_eq!(app.quick_view_side(), None);
    assert!(app.shows_files(Side::Right));
}

#[test]
fn the_info_panel_the_tree_and_a_view_mode_take_its_place_and_it_takes_theirs() {
    let (_temp, mut app) = app();
    ctrl_q(&mut app);
    press(&mut app, KeyCode::Char('l'), KeyModifiers::CONTROL);
    assert_eq!(app.quick_view_side(), None);
    assert_eq!(app.info_side(), Some(Side::Right));
    app.execute(Command::QuickView(Side::Right));
    assert_eq!(app.info_side(), None, "the menu's Quick view on that side");
    assert_eq!(app.quick_view_side(), Some(Side::Right));
    app.execute(Command::SetViewMode(
        Side::Right,
        crate::app::ViewMode::Brief,
    ));
    assert_eq!(app.quick_view_side(), None);
    app.execute(Command::QuickView(Side::Left));
    assert_eq!(app.quick_view_side(), Some(Side::Left));
}

#[test]
fn on_the_quick_view_gray_plus_and_minus_move_the_other_panel_s_cursor() {
    let (_temp, mut app) = app();
    ctrl_q(&mut app);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    cursor_on_left(&mut app, "a.bat");
    let mut plus = KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE);
    plus.state = KeyEventState::KEYPAD;
    app.handle_key(plus);
    app.pump_quick_view();
    assert_eq!(shown_name(&app), "b.txt");
    assert!(app.quick_view_is_active(), "the keys stay");
    let mut minus = KeyEvent::new(KeyCode::Char('-'), KeyModifiers::NONE);
    minus.state = KeyEventState::KEYPAD;
    app.handle_key(minus);
    app.pump_quick_view();
    assert_eq!(shown_name(&app), "a.bat");
}

fn cursor_on_left(app: &mut App, name: &str) {
    let path = app.panels[0].path.clone();
    app.panels[0].change_dir(&path, Some(name)).unwrap();
    app.pump_quick_view();
}

#[test]
fn f3_on_the_quick_view_views_the_other_panel_s_file() {
    let (_temp, mut app) = app();
    ctrl_q(&mut app);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    cursor_on_left(&mut app, "b.txt");
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Viewer(_)));
    assert_eq!(app.active, Side::Right, "the quick view keeps the keys");
}

#[test]
fn the_file_keys_do_nothing_on_the_quick_view() {
    let (temp, mut app) = app();
    ctrl_q(&mut app);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
    assert!(app.overlay.is_none(), "no question, no new folder");
    assert!(temp.path().join("sub").exists());
}

#[test]
fn ctrl_u_takes_it_along_and_ctrl_r_counts_again() {
    let (temp, mut app) = app();
    cursor_on(&mut app, "sub");
    ctrl_q(&mut app);
    assert_eq!(counted(&mut app).info.files, 2);
    app.swap_panels();
    assert_eq!(app.quick_view_side(), Some(Side::Left));
    assert_eq!(
        app.active,
        Side::Right,
        "the files keep the keys, moved with them"
    );
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    std::fs::write(temp.path().join("sub").join("z"), "1").unwrap();
    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert!(app.quick_view_is_active());
    assert_eq!(counted(&mut app).info.files, 3, "counted again");
}

#[test]
fn a_hidden_quick_view_follows_nothing() {
    let (_temp, mut app) = app();
    cursor_on(&mut app, "a.bat");
    ctrl_q(&mut app);
    app.toggle_panel(Side::Right);
    cursor_on(&mut app, "b.txt");
    assert_eq!(shown_name(&app), "a.bat");
}

#[test]
fn beside_the_tree_it_shows_the_tree_s_folder() {
    let (temp, mut app) = app();
    crate::app::tree_panel::TEST_ROOT
        .with(|root| *root.borrow_mut() = Some(temp.path().to_path_buf()));
    press(&mut app, KeyCode::Char('t'), KeyModifiers::CONTROL);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.tree_panel.as_ref().is_none_or(|tree| tree.reading) {
        app.pump_tree_panel();
        assert!(Instant::now() < deadline, "the tree was not read");
        std::thread::sleep(Duration::from_millis(5));
    }
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    app.execute(Command::QuickView(Side::Left));
    app.pump_quick_view();
    assert_eq!(shown(&app).path, VfsPath::local(temp.path().join("sub")));
}

#[test]
fn f3_on_the_quick_view_counts_a_folder_on_the_panel_beside_it() {
    let (_temp, mut app) = app();
    ctrl_q(&mut app);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    cursor_on_left(&mut app, "sub");
    press(&mut app, KeyCode::KeypadBegin, KeyModifiers::NONE);
    assert!(
        app.panels[0]
            .current()
            .is_some_and(|entry| entry.folder_size != crate::vfs::FolderSize::Unknown),
        "Num5 is F3: the folder's size counted"
    );
    assert_eq!(app.active, Side::Right);
}

#[test]
fn associations_of_the_file_shown_run_with_the_quick_view_s_facts() {
    let (temp, mut app) = app();
    ctrl_q(&mut app);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    cursor_on_left(&mut app, "b.txt");
    let facts = app.menu_facts();
    let b = temp.path().join("b.txt").display().to_string();
    assert_eq!(
        facts.active.current, b,
        "FAR's QuickView::GetCurName: the whole path"
    );
    assert_eq!(facts.active.dir, temp.path().display().to_string());
    assert_eq!(app.quick_view_file().as_deref(), Some("b.txt"));
    cursor_on_left(&mut app, "sub");
    assert_eq!(app.quick_view_file(), None, "a folder");
}

#[test]
fn an_association_chosen_later_runs_on_the_file_shown() {
    let (temp, mut app) = app();
    let view = |command: &str| crate::settings::Association {
        masks: "*.txt".to_string(),
        view: crate::settings::AssociationCommand {
            command: command.to_string(),
            on: true,
        },
        ..crate::settings::Association::default()
    };
    app.config.associations = vec![view("set NC_ASSOC=first"), view("set NC_ASSOC=!.!")];
    ctrl_q(&mut app);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    cursor_on_left(&mut app, "b.txt");
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    assert!(
        matches!(app.overlay, Some(crate::app::Overlay::Dialog(_))),
        "Select association"
    );
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    for _ in 0..5 {
        app.tick();
    }
    let b = temp.path().join("b.txt").display().to_string();
    assert_eq!(
        app.environment.get("NC_ASSOC"),
        Some(b),
        "not the hidden list's item"
    );
}

#[test]
fn beside_the_tree_its_folder_is_the_tree_s() {
    let (temp, mut app) = app();
    crate::app::tree_panel::TEST_ROOT
        .with(|root| *root.borrow_mut() = Some(temp.path().to_path_buf()));
    press(&mut app, KeyCode::Char('t'), KeyModifiers::CONTROL);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.tree_panel.as_ref().is_none_or(|tree| tree.reading) {
        app.pump_tree_panel();
        assert!(Instant::now() < deadline, "the tree was not read");
        std::thread::sleep(Duration::from_millis(5));
    }
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    app.execute(Command::QuickView(Side::Left));
    app.pump_quick_view();
    let sub = temp.path().join("sub").display().to_string();
    let facts = app.quick_view_facts(Side::Left).unwrap();
    assert_eq!(
        (facts.dir.as_str(), facts.current.as_str()),
        (sub.as_str(), sub.as_str())
    );
}
