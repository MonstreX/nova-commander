//! The info panel's folder description: found by FAR's names, shown, moved, viewed and edited.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Side;
use crate::app::fileops::tests::app_in;
use crate::settings::Settings;

#[test]
fn names_are_split_by_commas_and_semicolons_outside_quotes() {
    assert_eq!(
        description_names(r#"DirInfo, File_Id.diz;"a,b.txt" ;; ReadMe.*"#),
        ["DirInfo", "File_Id.diz", "a,b.txt", "ReadMe.*"]
    );
    assert!(description_names(" , ").is_empty());
}

#[test]
fn the_first_name_that_finds_a_file_wins_folders_passed_over() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("DirInfo")).unwrap();
    std::fs::write(temp.path().join("readme.md"), "read me").unwrap();
    let defaults = Settings::default().info_panel.description_files;
    let found = description_file(temp.path(), &defaults);
    assert!(
        cfg!(not(windows)) || found == Some(temp.path().join("readme.md")),
        "{found:?}"
    );
    std::fs::write(temp.path().join("File_Id.diz"), "diz").unwrap();
    let found = description_file(temp.path(), &defaults);
    assert!(cfg!(not(windows)) || found == Some(temp.path().join("File_Id.diz")));
    assert_eq!(description_file(temp.path(), "nothing.*"), None);
}

/// The info panel on the right with the keys, the left panel in `folder`, read to its description.
fn info_on(folder: &Path) -> App {
    let mut app = app_in(folder);
    app.active = Side::Left;
    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert!(app.info_has_keys());
    let deadline = Instant::now() + Duration::from_secs(20);
    while app
        .info_panel
        .as_ref()
        .is_some_and(|info| info.info.is_none())
    {
        app.pump_info_panel();
        assert!(Instant::now() < deadline, "the info panel's reading");
        std::thread::sleep(Duration::from_millis(2));
    }
    app
}

/// The description's first row once it is read, in a room of 30 by 3 (other systems find no
/// description file yet).
#[cfg(windows)]
fn first_row(app: &mut App) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let info = app.info_panel.as_ref().unwrap();
        let description = info.description.as_ref().expect("a description file");
        description.text.room.set((30, 3));
        let rows = description.text.screen(30, 3).map(|screen| screen.rows);
        if let Some(row) = rows.and_then(|rows| rows.first().cloned()) {
            return row;
        }
        app.pump_info_panel();
        assert!(Instant::now() < deadline, "the description's rows");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(windows)]
#[test]
fn the_description_is_shown_and_moved_by_the_viewer_s_keys() {
    let temp = tempfile::tempdir().unwrap();
    let lines: String = (1..=10).map(|n| format!("note {n}\n")).collect();
    std::fs::write(temp.path().join("Descript.ion"), lines).unwrap();
    let mut app = info_on(temp.path());
    assert_eq!(first_row(&mut app), "note 1");
    assert_eq!(
        app.description_next_page(),
        Some("OEM"),
        "ASCII: the ANSI page"
    );
    app.handle_key(KeyEvent::from(KeyCode::Down));
    assert_eq!(first_row(&mut app), "note 2");
    app.handle_key(KeyEvent::from(KeyCode::F(3)));
    assert!(
        matches!(app.screen, super::super::Screen::Viewer(_)),
        "F3 views it"
    );
}

#[test]
fn f4_without_a_description_edits_a_new_dirinfo() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = info_on(temp.path());
    assert!(app.info_panel.as_ref().unwrap().description.is_none());
    assert_eq!(app.description_next_page(), None);
    app.handle_key(KeyEvent::from(KeyCode::F(4)));
    let super::super::Screen::Editor(editor) = &app.screen else {
        panic!("the editor");
    };
    let folder = crate::platform::files::exact_path(temp.path());
    let path = &editor.path;
    assert_eq!(
        path.file_name().unwrap(),
        "DirInfo",
        "FAR's first plain name"
    );
    assert!(
        !temp.path().join("DirInfo").exists(),
        "made only when saved"
    );
    assert!(path.starts_with(temp.path()) || path.starts_with(folder));
}

#[cfg(windows)]
#[test]
fn other_names_given_are_looked_for_again() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("notes.txt"), "mine\n").unwrap();
    let mut app = info_on(temp.path());
    assert!(app.info_panel.as_ref().unwrap().description.is_none());
    app.set_description_files("*.txt");
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.info_panel.as_ref().unwrap().description.is_none() {
        app.pump_info_panel();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(first_row(&mut app), "mine");
}

#[cfg(windows)]
#[test]
fn the_mouse_views_with_the_left_button_and_edits_with_the_right() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("Read.Me"), "x\n").unwrap();
    let mut app = info_on(temp.path());
    first_row(&mut app);
    let info = app.info_panel.as_ref().unwrap();
    info.description.as_ref().unwrap().area.set((60, 20, 30, 5));
    assert!(!app.description_click(10, 21, true), "outside it");
    assert!(app.description_click(61, 21, false));
    assert!(
        matches!(app.screen, super::super::Screen::Editor(_)),
        "right: F4"
    );
}

#[test]
fn f4_on_a_folded_section_edits_the_dirinfo_there_never_an_empty_one() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("DirInfo"), "keep me").unwrap();
    let mut app = info_on(temp.path());
    let info = app.info_panel.as_mut().unwrap();
    info.sections.description = false;
    // Whether the reading found it yet or not, F4 loads the file.
    info.description = None;
    app.handle_key(KeyEvent::from(KeyCode::F(4)));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        app.tick();
        let super::super::Screen::Editor(editor) = &app.screen else {
            panic!("the editor");
        };
        editor.text.room.set((40, 5));
        if let Some(screen) = editor.text.screen(40, 5)
            && screen.rows.first().is_some_and(|row| row == "keep me")
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "its text");
    }
}

#[test]
fn the_description_goes_with_the_disk_under_the_other_panel() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("Read.Me"), "x\n").unwrap();
    let mut app = info_on(temp.path());
    let info = app.info_panel.as_mut().unwrap();
    info.description = Some(Description::open(temp.path().join("Read.Me")));
    app.panels[0].path = crate::vfs::VfsPath::parse("nosuch://host/dir");
    app.reread_info_panel();
    let info = app.info_panel.as_ref().unwrap();
    assert!(!info.describes);
    assert!(info.description.is_none(), "none left of the old folder");
}

#[cfg(windows)]
#[test]
fn a_button_going_down_on_the_description_views_or_edits_it() {
    use crate::app::console_input::{Mouse, MouseAction};
    use crate::app::mouse::PanelArea;
    use crate::platform::input::MouseButtons;

    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("Read.Me"), "x\n").unwrap();
    let mut app = info_on(temp.path());
    first_row(&mut app);
    app.mouse.areas[1] = Some(PanelArea {
        x: 50,
        y: 0,
        width: 50,
        height: 30,
        list: None,
    });
    let info = app.info_panel.as_ref().unwrap();
    info.description.as_ref().unwrap().area.set((51, 20, 48, 9));
    let buttons = |left, right| MouseButtons {
        left,
        right,
        middle: false,
    };
    let record = |held, pressed| Mouse {
        column: 60,
        row: 22,
        buttons: held,
        pressed,
        action: MouseAction::Button,
        modifiers: KeyModifiers::NONE,
    };
    // Left let go while the right is held: nothing went down.
    app.handle_mouse(record(buttons(false, true), buttons(false, false)));
    assert!(matches!(app.screen, super::super::Screen::Panels));
    app.handle_mouse(record(buttons(false, true), buttons(false, true)));
    assert!(
        matches!(app.screen, super::super::Screen::Editor(_)),
        "right: F4"
    );
}

#[cfg(windows)]
#[test]
fn the_menus_symbols_of_the_info_panel_name_its_description_file() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("Descript.ion"), "notes").unwrap();
    std::fs::write(temp.path().join("other.txt"), "x").unwrap();
    let mut app = info_on(temp.path());
    let _ = first_row(&mut app);
    let facts = app.menu_facts();
    let info = if app.active == Side::Right {
        &facts.active
    } else {
        &facts.passive
    };
    assert_eq!(info.dir, temp.path().display().to_string());
    assert_eq!(
        info.current,
        temp.path().join("Descript.ion").display().to_string()
    );
    assert!(info.selected.is_empty(), "nothing is selected there");
}
