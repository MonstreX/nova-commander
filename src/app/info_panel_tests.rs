//! Ctrl+L as FAR's `FilePanels::ProcessKey` switches it, and the info panel's own keys.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::popup::{PopupOutcome, handle_key};
use crate::app::waiting::finish_waiting;

fn ctrl(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::CONTROL));
}

fn read(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while app
        .info_panel
        .as_ref()
        .is_some_and(|info| info.info.is_none())
    {
        app.pump_info_panel();
        assert!(Instant::now() < deadline, "the info panel's reading");
        std::thread::yield_now();
    }
}

#[test]
fn ctrl_l_turns_the_passive_panel_into_info_and_back_from_either_side() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.active = Side::Left;
    ctrl(&mut app, KeyCode::Char('l'));
    assert_eq!(app.info_side(), Some(Side::Right));
    read(&mut app);
    let shown = app.info_panel.as_ref().unwrap().info.as_ref().unwrap();
    // The stubs of other systems know no names yet.
    assert!(cfg!(not(windows)) || !shown.system.user_name.is_empty());
    ctrl(&mut app, KeyCode::Char('l'));
    assert_eq!(app.info_side(), None, "again: files");

    ctrl(&mut app, KeyCode::Char('l'));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert!(app.info_is_active(), "Tab goes to it");
    ctrl(&mut app, KeyCode::Char('l'));
    assert_eq!(app.info_side(), None, "from the info panel itself");
    assert_eq!(app.active, Side::Right);
}

#[test]
fn ctrl_l_needs_the_active_panel_shown() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.panels[app.active.index()].visible = false;
    ctrl(&mut app, KeyCode::Char('l'));
    assert_eq!(app.info_side(), None);
}

#[test]
fn ctrl_f12_on_the_info_panel_folds_and_unfolds_its_sections() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    ctrl(&mut app, KeyCode::Char('l'));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    ctrl(&mut app, KeyCode::F(12));
    let Some(Overlay::Popup(popup)) = &app.overlay else {
        panic!("the Show Info menu");
    };
    assert_eq!(popup.title, "Show Info");
    let checks: Vec<Option<char>> = popup.items.iter().map(|item| item.check).collect();
    let labels: Vec<&str> = popup.items.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "&Disk",
            "&Memory",
            "&Description",
            "Plu&gin panel",
            "&Power status"
        ],
        "FAR's order"
    );
    assert_eq!(
        checks,
        [Some('+'), Some('+'), Some('+'), Some('+'), Some('+')],
        "power on: owner"
    );
    // `-` on Memory folds it, `*` switches it back.
    app.handle_key(KeyEvent::from(KeyCode::Down));
    app.handle_key(KeyEvent::from(KeyCode::Char('-')));
    assert!(!app.info_panel.as_ref().unwrap().sections.memory);
    ctrl(&mut app, KeyCode::F(12));
    let Some(Overlay::Popup(mut popup)) = app.overlay.take() else {
        panic!("the menu again");
    };
    popup.selected = 1;
    let PopupOutcome::Choose(index, key) =
        handle_key(&mut popup, KeyEvent::from(KeyCode::Char('*')))
    else {
        panic!("chosen");
    };
    app.finish_info_sections_menu(index, key);
    assert!(app.info_panel.as_ref().unwrap().sections.memory);
}

#[test]
fn the_sections_outlive_the_panel_and_ctrl_u_takes_it_along() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.active = Side::Left;
    ctrl(&mut app, KeyCode::Char('l'));
    app.info_panel.as_mut().unwrap().sections.disk = false;
    ctrl(&mut app, KeyCode::Char('u'));
    assert_eq!(app.info_side(), Some(Side::Left), "swapped with its side");
    assert_eq!(app.active, Side::Right);
    ctrl(&mut app, KeyCode::Char('l'));
    ctrl(&mut app, KeyCode::Char('l'));
    assert!(!app.info_panel.as_ref().unwrap().sections.disk, "kept");
}

#[test]
fn file_keys_do_nothing_on_the_info_panel_and_commands_run_in_the_other_folder() {
    let temp = tempfile::tempdir().unwrap();
    let other = temp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let mut app = app_in(temp.path());
    app.active = Side::Left;
    app.panels[0]
        .change_dir(&crate::vfs::VfsPath::local(&other), None)
        .unwrap();
    ctrl(&mut app, KeyCode::Char('l'));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    app.handle_key(KeyEvent::from(KeyCode::F(5)));
    assert!(app.overlay.is_none(), "no copy from the info panel");
    assert_eq!(app.active_folder(), other);
}

/// The info panel on the right with the keys, the left panel's files beside it.
fn info_active(app: &mut App) {
    app.active = Side::Left;
    ctrl(app, KeyCode::Char('l'));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert!(app.info_is_active());
}

#[test]
fn f7_ctrl_a_view_modes_and_name_insertions_do_nothing_on_the_info_panel() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    info_active(&mut app);
    app.handle_key(KeyEvent::from(KeyCode::F(7)));
    assert!(app.overlay.is_none(), "no folder made");
    ctrl(&mut app, KeyCode::Char('a'));
    assert!(app.overlay.is_none(), "no attributes");
    let mode = app.panels[1].view_mode;
    ctrl(&mut app, KeyCode::Char('6'));
    assert_eq!(
        app.panels[1].view_mode, mode,
        "the files under it keep their mode"
    );
    ctrl(&mut app, KeyCode::Enter);
    ctrl(&mut app, KeyCode::Char('f'));
    assert!(app.command_line.is_empty(), "no name put on the line");
}

#[test]
fn cd_and_a_drive_on_the_command_line_move_the_files_panel_beside_it() {
    let temp = tempfile::tempdir().unwrap();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let mut app = app_in(temp.path());
    info_active(&mut app);
    app.command_line.set("cd sub");
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.panels[0].path, sub, "the visible files panel moved");
    assert_eq!(app.info_side(), Some(Side::Right), "the info panel stays");
}

#[test]
fn ctrl_l_shows_a_hidden_passive_panel_as_the_info_panel() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.active = Side::Left;
    app.panels[1].visible = false;
    ctrl(&mut app, KeyCode::Char('l'));
    assert_eq!(app.info_side(), Some(Side::Right));
    assert!(app.panels[1].visible);
}

#[test]
fn the_other_panel_moving_reads_it_again_and_a_hidden_one_is_not_read() {
    let temp = tempfile::tempdir().unwrap();
    let sub = temp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let mut app = app_in(temp.path());
    app.active = Side::Left;
    ctrl(&mut app, KeyCode::Char('l'));
    read(&mut app);
    let described = |app: &App| app.info_panel.as_ref().unwrap().folder.clone();
    assert_eq!(described(&app).as_deref(), Some(temp.path()));
    app.panels[0]
        .change_dir(&crate::vfs::VfsPath::local(&sub), None)
        .unwrap();
    assert!(app.info_panel.as_ref().unwrap().reading.is_none());
    app.pump_info_panel();
    assert!(
        app.info_panel.as_ref().unwrap().reading.is_some(),
        "moved: read again"
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while described(&app).as_deref() != Some(sub.as_path()) {
        app.pump_info_panel();
        assert!(Instant::now() < deadline, "the second reading");
        std::thread::yield_now();
    }
    app.panels[1].visible = false;
    app.info_panel.as_mut().unwrap().read_at = None;
    app.pump_info_panel();
    assert!(
        app.info_panel.as_ref().unwrap().reading.is_none(),
        "hidden: not read"
    );
    app.panels[1].visible = true;
    app.pump_info_panel();
    assert!(
        app.info_panel.as_ref().unwrap().reading.is_some(),
        "old: read again"
    );
}

#[test]
fn ctrl_r_reads_the_info_panel_again_and_keeps_its_meaning_on_files() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    info_active(&mut app);
    read(&mut app);
    ctrl(&mut app, KeyCode::Char('r'));
    assert!(app.info_panel.as_ref().unwrap().reading.is_some());
    // On the files panel Ctrl+F12 is still the sort menu.
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    ctrl(&mut app, KeyCode::F(12));
    let Some(Overlay::Popup(popup)) = &app.overlay else {
        panic!("a menu");
    };
    assert_eq!(popup.title, "Sort by");
}

#[test]
fn a_drive_chosen_for_its_side_brings_the_files_back() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.active = Side::Left;
    ctrl(&mut app, KeyCode::Char('l'));
    app.open_drive_menu(Side::Right);
    finish_waiting(&mut app);
    let Some(Overlay::DriveMenu(menu)) = app.overlay.take() else {
        panic!("the drive menu");
    };
    app.handle_drive_menu_key(menu, KeyEvent::from(KeyCode::Enter));
    finish_waiting(&mut app);
    assert_eq!(app.info_side(), None);
}

#[test]
fn a_change_the_panel_beside_takes_reads_it_again() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.active = Side::Left;
    ctrl(&mut app, KeyCode::Char('l'));
    read(&mut app);
    app.pump_info_panel();
    assert!(
        app.info_panel.as_ref().unwrap().reading.is_none(),
        "nothing changed: not read"
    );
    // A file written there, as a change notification has the panel read it: within a second of the
    // last reading it waits (a count of folders' sizes changes the list many times a second).
    std::fs::write(temp.path().join("ReadMe.txt"), "about").unwrap();
    app.panels[0].reload();
    finish_waiting(&mut app);
    app.info_panel.as_mut().unwrap().read_at = Some(Instant::now());
    app.pump_info_panel();
    assert!(
        app.info_panel.as_ref().unwrap().reading.is_none(),
        "at most once a second"
    );
    app.info_panel.as_mut().unwrap().read_at = Some(Instant::now() - Duration::from_millis(1500));
    app.pump_info_panel();
    assert!(
        app.info_panel.as_ref().unwrap().reading.is_some(),
        "FAR's info panel follows its folder's changes"
    );
}

#[test]
fn beside_a_plugins_panel_the_info_panel_takes_its_lines_and_a_disks_none() {
    // A name of its own: the providers are the process's, the tests side by side.
    let plugins = crate::plugin_host::tests::plugins_with_probe(|manifest| {
        manifest["name"] = "infoprobe".into()
    });
    std::fs::write(plugins.path().join("probe").join("disks.txt"), "Panel one").unwrap();
    let mut app = app_in(plugins.path());
    app.set_plugins(crate::plugin_host::tests::load(plugins.path(), &|_| true));
    ctrl(&mut app, KeyCode::Char('l'));
    assert_eq!(
        app.info_panel.as_ref().unwrap().plugin_lines,
        None,
        "beside a disk"
    );
    app.open_disk_for_test(0, 0, Side::Left.index());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.info_panel.as_ref().unwrap().plugin_lines.is_none() || app.panels[0].is_reading() {
        assert!(Instant::now() < deadline, "the plugin's panel");
        app.tick();
        std::thread::yield_now();
    }
    let lines = app
        .info_panel
        .as_ref()
        .unwrap()
        .plugin_lines
        .clone()
        .unwrap();
    let texts: Vec<(&str, &str, bool)> = (lines.iter())
        .map(|line| (line.text.as_str(), line.data.as_str(), line.separator))
        .collect();
    assert_eq!(texts, [("Probe one", "", true), ("Path", "/", false)]);
}
