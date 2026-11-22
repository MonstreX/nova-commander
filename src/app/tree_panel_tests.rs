//! Ctrl+T: the tree panel and its worker (ADR-0014). Each test's tree has its temporary folder for its
//! root (`TEST_ROOT`), so no test reads a real disk.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::{Command, ViewMode};

/// `root/a/b`, `root/a/c`, `root/d`, the panels in `root/a`, the tree's root `root`.
pub(super) fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    for folder in ["a/b", "a/c", "d"] {
        std::fs::create_dir_all(temp.path().join(folder)).unwrap();
    }
    TEST_ROOT.with(|root| *root.borrow_mut() = Some(temp.path().to_path_buf()));
    let app = app_in(&temp.path().join("a"));
    (temp, app)
}

pub(super) fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

pub(super) fn ctrl_t(app: &mut App) {
    press(app, KeyCode::Char('t'), KeyModifiers::CONTROL);
}

/// Ticks until the worker is done and the cursor went after the other panel.
pub(super) fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.pump_tree_panel();
        let tree = app.tree_panel.as_ref().unwrap();
        if !tree.reading && tree.synced.is_some() && tree.waiting.is_none() {
            return;
        }
        assert!(Instant::now() < deadline, "the tree was not read");
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn tree(app: &App) -> &TreePanel {
    app.tree_panel.as_ref().unwrap()
}

pub(super) fn names(app: &App) -> Vec<String> {
    let root = tree(app).root.clone();
    (tree(app).items.iter())
        .map(|item| {
            let rest = item.path.strip_prefix(&root).unwrap();
            rest.to_string_lossy().replace('\\', "/")
        })
        .collect()
}

pub(super) fn current(app: &App) -> String {
    names(app)[tree(app).cursor].clone()
}

#[test]
fn the_lines_know_which_folder_is_the_last_of_its_parent() {
    let root = Path::new("R:\\");
    let folders = ["R:\\d", "R:\\a\\c", "R:\\a", "R:\\a\\b", "R:\\a\\b"].map(PathBuf::from);
    let items = make_items(root, folders.to_vec());
    let shown: Vec<(String, usize, Vec<bool>)> = (items.iter())
        .map(|item| {
            (
                item.path.to_string_lossy().into_owned(),
                item.depth,
                item.last.clone(),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("R:\\".to_string(), 0, vec![]),
            ("R:\\a".to_string(), 1, vec![false]),
            ("R:\\a\\b".to_string(), 2, vec![false, false]),
            ("R:\\a\\c".to_string(), 2, vec![false, true]),
            ("R:\\d".to_string(), 1, vec![true]),
        ],
        "each once, a folder before what is in it"
    );
}

#[test]
fn ctrl_t_makes_the_passive_panel_the_tree_on_the_active_ones_folder() {
    let (_temp, mut app) = app();
    ctrl_t(&mut app);
    assert_eq!(app.tree_side(), Some(app.active.other()));
    assert!(!app.shows_files(app.active.other()));
    settle(&mut app);
    assert_eq!(names(&app), ["", "a", "a/b", "a/c", "d"]);
    assert_eq!(current(&app), "a", "the folder of the panel beside it");
    assert_eq!(tree(&app).work, Some(1));
    ctrl_t(&mut app);
    assert_eq!(app.tree_side(), None, "again: files");
}

#[test]
fn ctrl_t_on_the_tree_itself_gives_its_files_back_and_an_info_panel_gives_way() {
    let (_temp, mut app) = app();
    app.execute(Command::InfoPanel(app.active.other()));
    ctrl_t(&mut app);
    assert_eq!(app.info_side(), None, "the tree in its place");
    assert_eq!(app.tree_side(), Some(app.active.other()));
    settle(&mut app);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert!(app.tree_is_active());
    ctrl_t(&mut app);
    assert_eq!(app.tree_side(), None);
}

#[test]
fn turned_off_completely_there_is_no_tree() {
    let (_temp, mut app) = app();
    app.prefs.tree.turn_off_completely = true;
    ctrl_t(&mut app);
    app.execute(Command::TreePanel(app.active.other()));
    assert_eq!(app.tree_side(), None);
}

pub(super) fn on_tree(app: &mut App) {
    ctrl_t(app);
    settle(app);
    press(app, KeyCode::Tab, KeyModifiers::NONE);
    assert!(app.tree_is_active());
}

#[test]
fn enter_takes_the_other_panel_there_and_a_folder_gone_leaves_the_tree() {
    let (temp, mut app) = app();
    on_tree(&mut app);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    assert_eq!(current(&app), "d");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let other = app.tree_side().unwrap().other().index();
    assert_eq!(
        app.panels[other].path,
        VfsPath::local(temp.path().join("d"))
    );
    assert_eq!(tree(&app).work, Some(4));
    std::fs::remove_dir_all(temp.path().join("a")).unwrap();
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(names(&app), ["", "d"], "a with what was in it");
}

#[test]
fn gray_plus_and_minus_go_to_the_folders_of_the_same_depth() {
    let (_temp, mut app) = app();
    on_tree(&mut app);
    let gray = |code| {
        let mut key = KeyEvent::new(code, KeyModifiers::NONE);
        key.state = KeyEventState::KEYPAD;
        key
    };
    app.handle_key(gray(KeyCode::Char('+')));
    assert_eq!(current(&app), "d", "past a's folders");
    app.handle_key(gray(KeyCode::Char('-')));
    assert_eq!(current(&app), "a");
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    app.handle_key(gray(KeyCode::Char('+')));
    assert_eq!(current(&app), "a/c");
    app.handle_key(gray(KeyCode::Char('+')));
    assert_eq!(current(&app), "a/c", "none after it");
}

#[test]
fn auto_change_folder_makes_each_move_an_enter() {
    let (temp, mut app) = app();
    app.prefs.tree.auto_change_folder = true;
    on_tree(&mut app);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    let other = app.tree_side().unwrap().other().index();
    assert_eq!(
        app.panels[other].path,
        VfsPath::local(temp.path().join("a").join("b"))
    );
}

#[test]
fn the_cursor_follows_the_other_panel_and_a_new_folder_is_read_in() {
    let (temp, mut app) = app();
    ctrl_t(&mut app);
    settle(&mut app);
    let files = app.active.index();
    std::fs::create_dir_all(temp.path().join("d/new/deep")).unwrap();
    app.panels[files]
        .change_dir(&VfsPath::local(temp.path().join("d").join("new")), None)
        .unwrap();
    settle(&mut app);
    assert_eq!(
        names(&app),
        ["", "a", "a/b", "a/c", "d", "d/new", "d/new/deep"]
    );
    assert_eq!(current(&app), "d/new");
    assert_eq!(tree(&app).work, Some(5));
    app.panels[files]
        .change_dir(&VfsPath::local(temp.path().join("a").join("c")), None)
        .unwrap();
    settle(&mut app);
    assert_eq!(current(&app), "a/c");
}

#[test]
fn the_tree_goes_with_its_side_and_a_view_mode_there_gives_the_files_back() {
    let (_temp, mut app) = app();
    ctrl_t(&mut app);
    let side = app.tree_side().unwrap();
    app.execute(Command::SwapPanels);
    assert_eq!(app.tree_side(), Some(side.other()));
    app.execute(Command::SetViewMode(side.other(), ViewMode::Full));
    assert_eq!(app.tree_side(), None);
}

#[test]
fn the_keys_work_while_the_worker_reads_and_the_tree_fills_as_it_goes() {
    let (_temp, mut app) = app();
    ctrl_t(&mut app);
    assert!(tree(&app).reading, "the worker has just started");
    assert_eq!(names(&app), [""], "the root at once");
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(app.command_line.text(), "x", "no message holds the keys");
    app.command_line.set("");
    ctrl_t(&mut app);
    assert_eq!(
        app.tree_side(),
        None,
        "Ctrl+T too: closing it stops the worker"
    );
    ctrl_t(&mut app);
    settle(&mut app);
    assert_eq!(names(&app), ["", "a", "a/b", "a/c", "d"]);
    assert!(!tree(&app).reading);
    assert_eq!(tree(&app).count, 4);
}

#[test]
fn ctrl_l_or_the_menus_info_panel_on_the_trees_side_puts_the_info_panel_there() {
    let (_temp, mut app) = app();
    ctrl_t(&mut app);
    settle(&mut app);
    let side = app.tree_side().unwrap();
    press(&mut app, KeyCode::Char('l'), KeyModifiers::CONTROL);
    assert_eq!((app.info_side(), app.tree_side()), (Some(side), None));
    ctrl_t(&mut app);
    assert_eq!((app.info_side(), app.tree_side()), (None, Some(side)));
    app.execute(Command::InfoPanel(side));
    assert_eq!((app.info_side(), app.tree_side()), (Some(side), None));
}

#[test]
fn enter_with_a_command_typed_is_the_command_lines() {
    let (_temp, mut app) = app();
    on_tree(&mut app);
    let other = app.tree_side().unwrap().other().index();
    let before = app.panels[other].path.clone();
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    app.command_line.set("echo one");
    let taken = app.handle_tree_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!taken, "it goes on to the command line");
    assert_eq!(app.panels[other].path, before);
}

#[test]
fn a_drive_chosen_for_the_trees_side_gives_its_files_back() {
    let (temp, mut app) = app();
    ctrl_t(&mut app);
    let side = app.tree_side().unwrap();
    app.enter_drive_folder(side, &VfsPath::local(temp.path().join("d")), "Change drive");
    assert_eq!(app.tree_side(), None);
}

#[test]
fn a_view_mode_key_on_the_tree_gives_the_files_back_in_that_mode() {
    let (_temp, mut app) = app();
    on_tree(&mut app);
    let side = app.active;
    press(&mut app, KeyCode::Char('3'), KeyModifiers::CONTROL);
    assert_eq!(app.tree_side(), None);
    assert_eq!(app.panels[side.index()].view_mode, ViewMode::Full);
}

#[test]
fn ctrl_r_reads_the_tree_again() {
    let (temp, mut app) = app();
    on_tree(&mut app);
    std::fs::create_dir(temp.path().join("e")).unwrap();
    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    settle(&mut app);
    assert_eq!(names(&app), ["", "a", "a/b", "a/c", "d", "e"]);
}

#[test]
fn the_cursor_keys_and_the_pages_as_fars() {
    let (temp, mut app) = app();
    for at in 0..20 {
        std::fs::create_dir(temp.path().join("d").join(format!("{at:02}"))).unwrap();
    }
    on_tree(&mut app);
    app.tree_panel.as_mut().unwrap().set_rows(5);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    assert_eq!(tree(&app).cursor, 0);
    press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
    assert_eq!(
        (tree(&app).cursor, tree(&app).top),
        (5, 5),
        "both a page on"
    );
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!((tree(&app).cursor, tree(&app).top), (4, 4));
    press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
    assert_eq!((tree(&app).cursor, tree(&app).top), (0, 0));
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    assert_eq!(current(&app), "d/19");
}

#[test]
fn a_hidden_tree_does_not_follow_and_the_menu_moves_it() {
    let (temp, mut app) = app();
    ctrl_t(&mut app);
    settle(&mut app);
    let side = app.tree_side().unwrap();
    app.panels[side.index()].visible = false;
    let files = side.other().index();
    app.panels[files]
        .change_dir(&VfsPath::local(temp.path().join("d")), None)
        .unwrap();
    app.pump_tree_panel();
    assert_eq!(current(&app), "a", "FAR's hidden panel waits");
    app.panels[side.index()].visible = true;
    settle(&mut app);
    assert_eq!(current(&app), "d");
    app.execute(Command::TreePanel(side.other()));
    assert_eq!(app.tree_side(), Some(side.other()));
}

#[cfg(windows)]
/// The trees' files in a folder of their own; the tree's root is the test's.
fn keep_files(app: &mut App, root: &Path) -> tempfile::TempDir {
    let files = tempfile::tempdir().unwrap();
    let cache = crate::jobs::tree_cache::TreeCache::under(files.path().into(), root.into());
    app.tasks.tree_cache = Some(cache);
    files
}

#[cfg(windows)]
/// The folders of the file's lines, without the times of those read.
fn file_lines(app: &App, root: &Path) -> Vec<String> {
    let cache = app.tasks.tree_cache.as_ref().unwrap();
    let text = std::fs::read_to_string(cache.file_of(root).unwrap()).unwrap_or_default();
    (text.lines())
        .map(|line| line.split('\t').next().unwrap_or(line).to_string())
        .collect()
}

#[cfg(windows)]
#[test]
fn a_tree_kept_in_its_file_gets_what_changed_and_ctrl_r_reads_the_disk_again() {
    let (temp, mut app) = app();
    let _files = keep_files(&mut app, temp.path());
    on_tree(&mut app);
    assert_eq!(
        file_lines(&app, temp.path()),
        [r"\", r"\a", r"\a\b", r"\a\c", r"\d"],
        "the first reading is saved"
    );
    std::fs::create_dir(temp.path().join("e")).unwrap();
    ctrl_t(&mut app);
    ctrl_t(&mut app);
    settle(&mut app);
    assert_eq!(
        names(&app),
        ["", "a", "a/b", "a/c", "d", "e"],
        "the root's time changed: it was listed again"
    );
    std::fs::create_dir(temp.path().join("f")).unwrap();
    // The tree came on the other side: to it, for its Ctrl+R.
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert!(app.tree_is_active());
    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert!(tree(&app).reading);
    settle(&mut app);
    assert_eq!(names(&app), ["", "a", "a/b", "a/c", "d", "e", "f"]);
    assert!(file_lines(&app, temp.path()).contains(&r"\f".to_string()));
}

#[cfg(windows)]
#[test]
fn a_tree_of_fewer_folders_than_the_least_is_not_kept() {
    let (temp, mut app) = app();
    let _files = keep_files(&mut app, temp.path());
    app.prefs.tree.min_tree_count = 6;
    on_tree(&mut app);
    assert!(file_lines(&app, temp.path()).is_empty(), "5 lines of the 6");
}

#[cfg(windows)]
#[test]
fn a_folder_made_goes_into_the_file_and_the_tree() {
    let (temp, mut app) = app();
    let _files = keep_files(&mut app, temp.path());
    ctrl_t(&mut app);
    settle(&mut app);
    app.make_dir("new\\deep");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !names(&app).contains(&"a/new/deep".to_string()) {
        assert!(
            Instant::now() < deadline,
            "the tree did not get the new folder"
        );
        app.pump_waiting();
        app.pump_tree_panel();
        std::thread::sleep(Duration::from_millis(5));
    }
    let lines = file_lines(&app, temp.path());
    assert!(lines.contains(&r"\a\new".to_string()) && lines.contains(&r"\a\new\deep".to_string()));
}

#[cfg(windows)]
#[test]
fn enter_on_a_folder_gone_takes_it_out_of_the_file_too() {
    let (temp, mut app) = app();
    let _files = keep_files(&mut app, temp.path());
    on_tree(&mut app);
    std::fs::remove_dir(temp.path().join("d")).unwrap();
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let deadline = Instant::now() + Duration::from_secs(10);
    while file_lines(&app, temp.path()).contains(&r"\d".to_string()) {
        assert!(Instant::now() < deadline, "d stayed in the file");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(names(&app), ["", "a", "a/b", "a/c"]);
}

#[cfg(windows)]
#[test]
fn a_file_that_cannot_be_saved_says_so() {
    let (temp, mut app) = app();
    let files = tempfile::tempdir().unwrap();
    // The folder of the files is a file.
    let not_a_folder = files.path().join("TreeCache");
    std::fs::write(&not_a_folder, "x").unwrap();
    let cache = crate::jobs::tree_cache::TreeCache::under(not_a_folder, temp.path().into());
    app.tasks.tree_cache = Some(cache);
    ctrl_t(&mut app);
    settle(&mut app);
    let Some(crate::app::Overlay::Dialog(crate::app::Dialog::Message { title, lines, .. })) =
        &app.overlay
    else {
        panic!("no message");
    };
    assert_eq!(title, "Error");
    assert_eq!(lines[0], "Cannot save folders tree to file");
    assert_eq!(
        names(&app),
        ["", "a", "a/b", "a/c", "d"],
        "the tree is shown"
    );
}

#[test]
fn the_tree_losing_the_keys_on_a_folder_gone_lets_it_go() {
    let (temp, mut app) = app();
    on_tree(&mut app);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    assert_eq!(current(&app), "d");
    // A tick with the keys on the tree, as the program has between keys.
    app.pump_tree_panel();
    std::fs::remove_dir(temp.path().join("d")).unwrap();
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    let deadline = Instant::now() + Duration::from_secs(10);
    while names(&app).contains(&"d".to_string()) {
        assert!(Instant::now() < deadline, "d stayed in the tree");
        app.pump_tree_panel();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(names(&app), ["", "a", "a/b", "a/c"]);
}

#[test]
fn enter_waits_for_a_slow_disk_and_esc_leaves_the_other_panel_where_it_was() {
    // ADR-0019, review R22: the folder's disk asked off the screen's thread.
    use crate::app::waiting::{finish_waiting, hold_next_work, shows_stopped};
    let (temp, mut app) = app();
    on_tree(&mut app);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    let other = app.tree_side().unwrap().other().index();
    let before = app.panels[other].path.clone();
    let release = hold_next_work();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.is_waiting(), "asked off the screen's thread");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(shows_stopped(&app));
    drop(release);
    assert!(!app.pump_waiting(), "its late end is not done");
    assert_eq!(app.panels[other].path, before);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    // The disk answers late: the other panel goes there; a folder gone leaves the tree.
    let release = hold_next_work();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let _ = release.send(());
    finish_waiting(&mut app);
    assert_eq!(
        app.panels[other].path,
        VfsPath::local(temp.path().join("d"))
    );
    std::fs::remove_dir_all(temp.path().join("d")).unwrap();
    let release = hold_next_work();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    // A mouse hold ending meanwhile starts no second wait.
    app.tree_enter();
    let _ = release.send(());
    finish_waiting(&mut app);
    assert!(!names(&app).contains(&"d".to_string()), "d left the tree");
}
