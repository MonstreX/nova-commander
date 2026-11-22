use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::waiting::finish_waiting;

const LOCAL: &str = "\
A:   Alpha
     set NC_MENU=alpha
--:  Group
T:   Tools
{
1:   One
     set NC_MENU=one
}
F5:  Five
     set NC_MENU=five
";

/// An app in a folder with `local` as its `UserMenu.ini` (none when empty) and a main menu beside.
fn app(local: &str) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    if !local.is_empty() {
        std::fs::write(work.join(MENU_FILE), local).unwrap();
    }
    std::fs::write(
        temp.path().join("main.ini"),
        "M:   Main item\n     set NC_MENU=main\n",
    )
    .unwrap();
    let mut app = app_in(&work);
    app.set_main_menu_file(temp.path().join("main.ini"));
    (temp, app)
}

fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    finish_waiting(app);
    // What came back after the work is shown on the next frame.
    app.tick();
}

fn press(app: &mut App, code: KeyCode) {
    key(app, code, KeyModifiers::NONE);
}

fn menu(app: &App) -> &UserMenu {
    match &app.overlay {
        Some(Overlay::UserMenu(menu)) => menu,
        _ => panic!("no user menu"),
    }
}

fn texts(app: &App) -> Vec<String> {
    menu(app)
        .lines
        .iter()
        .map(|line| line.text.clone())
        .collect()
}

fn variable(app: &App) -> Option<String> {
    app.environment.get("NC_MENU")
}

#[test]
fn f2_shows_the_folders_menu_as_far_writes_it() {
    let (_temp, mut app) = app(LOCAL);
    press(&mut app, KeyCode::F(2));
    assert_eq!(menu(&app).title, "Local menu");
    assert_eq!(
        texts(&app),
        ["&A   Alpha", "Group", "&T   Tools", "F5  Five"]
    );
    assert!(menu(&app).lines[1].separator && menu(&app).lines[2].submenu);
    assert_eq!(menu(&app).selected, 0);
    press(&mut app, KeyCode::Down);
    assert_eq!(menu(&app).selected, 2, "over the separator");
}

#[test]
fn without_a_menu_in_the_folder_the_main_one_comes() {
    let (_temp, mut app) = app("");
    press(&mut app, KeyCode::F(2));
    assert_eq!(menu(&app).title, "Main menu");
    assert_eq!(texts(&app), ["&M   Main item"]);
}

#[test]
fn enter_runs_the_items_commands_and_closes_the_menu() {
    let (_temp, mut app) = app(LOCAL);
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Enter);
    app.tick();
    assert!(app.overlay.is_none());
    assert_eq!(variable(&app).as_deref(), Some("alpha"));
}

#[test]
fn hotkeys_and_function_keys_run_their_items() {
    let (_temp, mut app) = app(LOCAL);
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::F(5));
    app.tick();
    assert_eq!(variable(&app).as_deref(), Some("five"));
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Char('a'));
    app.tick();
    assert_eq!(variable(&app).as_deref(), Some("alpha"));
}

#[test]
fn a_submenu_opens_and_closes_with_its_title_after_the_menus() {
    let (_temp, mut app) = app(LOCAL);
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Char('t'));
    assert_eq!(menu(&app).title, "Local menu \u{bb} Tools");
    assert_eq!(texts(&app), ["&1   One"]);
    press(&mut app, KeyCode::Esc);
    assert_eq!(menu(&app).title, "Local menu");
    assert_eq!(menu(&app).selected, 2, "back on its item");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Left);
    assert_eq!(menu(&app).title, "Local menu");
    press(&mut app, KeyCode::Right);
    key(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    assert!(app.overlay.is_none(), "Shift+F10 closes it all");
}

#[test]
fn shift_f2_goes_between_the_local_and_the_main_menu() {
    let (_temp, mut app) = app(LOCAL);
    press(&mut app, KeyCode::F(2));
    key(&mut app, KeyCode::F(2), KeyModifiers::SHIFT);
    assert_eq!(menu(&app).title, "Main menu");
    key(&mut app, KeyCode::F(2), KeyModifiers::SHIFT);
    assert_eq!(menu(&app).title, "Local menu");
}

#[test]
fn bs_goes_to_the_menu_of_a_folder_above() {
    let (temp, mut app) = app("");
    std::fs::write(
        temp.path().join(MENU_FILE),
        "U:   Up here\n     set NC_MENU=up\n",
    )
    .unwrap();
    let deep = temp.path().join("work").join("deep");
    std::fs::create_dir(&deep).unwrap();
    std::fs::write(deep.join(MENU_FILE), "D:   Deep\n").unwrap();
    app.panels[0]
        .change_dir(&crate::vfs::VfsPath::local(&deep), None)
        .unwrap();
    press(&mut app, KeyCode::F(2));
    assert_eq!(texts(&app), ["&D   Deep"]);
    press(&mut app, KeyCode::Backspace);
    assert_eq!(
        texts(&app),
        ["&U   Up here"],
        "work has none: the one above it"
    );
}

#[test]
fn labels_show_the_files_names() {
    let (_temp, mut app) = app("V:   View !.!\n     rem\n");
    std::fs::write(app.active_folder().join("a.txt"), "x").unwrap();
    app.panels[0].reload();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !app.panels[0]
        .entries
        .iter()
        .any(|entry| entry.name == "a.txt")
    {
        assert!(std::time::Instant::now() < deadline);
        app.tick();
    }
    app.panels[0].go_to_name("a.txt");
    press(&mut app, KeyCode::F(2));
    assert_eq!(texts(&app), ["&V   View a.txt"]);
}

#[test]
fn the_commands_menu_edits_the_main_or_this_folders_menu() {
    let (_temp, mut app) = app(LOCAL);
    app.execute(crate::app::Command::EditUserMenu);
    press(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).title, "Main menu");
    press(&mut app, KeyCode::Esc);
    app.execute(crate::app::Command::EditUserMenu);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).title, "Local menu");
}

#[test]
fn an_items_f4_or_f10_runs_it_and_ctrl_with_its_key_does_not() {
    let (_temp, mut app) =
        app("F4:  Four\n     set NC_MENU=four\nF5:  Five\n     set NC_MENU=five\n");
    press(&mut app, KeyCode::F(2));
    key(&mut app, KeyCode::F(5), KeyModifiers::CONTROL);
    assert!(app.overlay.is_some(), "Ctrl+F5 is no hotkey");
    assert_eq!(variable(&app), None);
    press(&mut app, KeyCode::F(4));
    app.tick();
    assert_eq!(
        variable(&app).as_deref(),
        Some("four"),
        "F4 runs the F4 item"
    );
}

#[test]
fn a_separators_label_is_as_written_and_labels_get_the_environment() {
    let (_temp, mut app) = app("--:  Files !.!\nA:   Home %NC_LABEL_HOME%\n");
    app.environment.set("NC_LABEL_HOME", "here");
    press(&mut app, KeyCode::F(2));
    assert_eq!(texts(&app), ["Files !.!", "&A   Home here"]);
    assert_eq!(menu(&app).selected, 1, "not on the separator");
}

#[test]
fn a_submenus_title_has_no_hotkey_marks() {
    let (_temp, mut app) = app("T:   &Tools && more\n{\n1:   One\n}\n");
    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).title, "Local menu \u{bb} Tools & more");
}

#[test]
fn a_local_menu_that_cannot_be_read_is_said() {
    let (_temp, mut app) = app("");
    std::fs::create_dir(app.active_folder().join(MENU_FILE)).unwrap();
    press(&mut app, KeyCode::F(2));
    let Some(Overlay::Dialog(crate::app::Dialog::Message { title, .. })) = &app.overlay else {
        panic!("no error");
    };
    assert_eq!(title, "User menu");
}

#[test]
fn a_plugins_entry_of_a_real_file_names_that_file() {
    // P2.4 (FAR's real names): `!.!` of a TmpPanel entry is the file it stands for.
    let mut panel = Panel::new(crate::vfs::VfsPath::in_session("tmp", "one", "/"));
    panel.entries = vec![crate::vfs::Entry {
        name: "shown".to_string(),
        real_path: Some(std::path::PathBuf::from(r"C:\data\real.txt")),
        ..crate::vfs::Entry::default()
    }];
    panel.cursor = 0;
    let facts = panel_facts(&panel);
    assert_eq!(facts.current, r"C:\data\real.txt");
    assert_eq!(facts.selected, [r"C:\data\real.txt"]);
}
