//! A plugin's requests of the panels (API 3) on the probe: SameFolder's reading of one panel and the other
//! sent to its folder with the cursor on the same name (answered once read), the drive menu's panel as
//! the call's own, what the probe reads of both panels and its changes of the command line, the selection
//! and the cursor.
#![cfg(windows)]

use std::time::{Duration, Instant};

use super::super::super::fileops::tests::app_in;
use super::super::super::{App, Side};
use crate::plugin_host::tests::{install_probe, load as load_plugins};

/// The probe as `name` with its commands, loaded into an app whose left panel is in `left` (a.txt,
/// b.txt, c.txt) and the right one in `right`, the left one active with its cursor on b.txt.
fn app_with(name: &str, commands: &str) -> (tempfile::TempDir, App, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let manifest =
        format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
    let place = install_probe(temp.path(), name, &manifest);
    std::fs::write(place.join("commands.txt"), commands).unwrap();
    std::fs::write(place.join("disks.txt"), commands).unwrap();
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    std::fs::create_dir_all(&right).unwrap();
    std::fs::create_dir_all(&left).unwrap();
    for file in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(left.join(file), file).unwrap();
    }
    let mut app = app_in(&left);
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    app.panels[1]
        .change_dir(&crate::vfs::VfsPath::local(&right), None)
        .unwrap();
    tick_until(&mut app, |app| {
        !app.panels[1].is_reading() && !app.panels[0].is_reading()
    });
    app.active = Side::Left;
    app.panels[0].go_to_name("b.txt");
    (temp, app, place)
}

fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < deadline, "not in time");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn current(app: &App, side: Side) -> Option<String> {
    app.panels[side.index()]
        .current()
        .map(|entry| entry.name.clone())
}

#[test]
fn same_folder_from_f11_sends_the_other_panel_to_this_folder_on_the_same_name() {
    let (temp, mut app, _) = app_with("samefone", "Same folder");
    app.run_plugin_command(&[(0, 0)], 0);
    let left = temp.path().join("left");
    tick_until(&mut app, |app| {
        app.panels[1].path == left && current(app, Side::Right).as_deref() == Some("b.txt")
    });
    assert_eq!(app.panels[0].path, left, "the active panel stays");
}

#[test]
fn same_folder_from_the_drive_menu_sends_the_menus_panel() {
    let (temp, mut app, _) = app_with("samefmenu", "Same folder");
    // The right panel's drive menu (Alt+F2) while the left one is active: the right one goes.
    app.open_disk_for_test(0, 0, Side::Right.index());
    let left = temp.path().join("left");
    tick_until(&mut app, |app| app.panels[1].path == left);
    tick_until(&mut app, |app| {
        current(app, Side::Right).as_deref() == Some("b.txt")
    });
    // From the left panel's menu the left one goes to the right one's folder.
    app.panels[1].go_to_name("..");
    let right = temp.path().join("right");
    app.panels[1]
        .change_dir(&crate::vfs::VfsPath::local(&right), None)
        .unwrap();
    tick_until(&mut app, |app| {
        app.panels[1].path == right && !app.panels[1].is_reading()
    });
    app.open_disk_for_test(0, 0, Side::Left.index());
    tick_until(&mut app, |app| app.panels[0].path == right);
}

#[test]
fn the_probe_reads_both_panels_and_changes_the_command_line_selection_and_cursor() {
    let (temp, mut app, place) = app_with("panelreport", "Report panels");
    app.command_line.set("dir");
    app.run_plugin_command(&[(0, 0)], 0);
    tick_until(&mut app, |app| app.command_line.text() == "probe was here");
    tick_until(&mut app, |app| {
        current(app, Side::Left).as_deref() == Some("c.txt")
    });
    let said = std::fs::read_to_string(place.join("panels-said.txt")).unwrap();
    let left = temp.path().join("left").display().to_string();
    let right = temp.path().join("right").display().to_string();
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(
        lines[0],
        format!(
            "Active left=true active=true target=true kind=Files folder={left} items=4 \
             names=[\"..\", \"a.txt\", \"b.txt\", \"c.txt\"] selected=[\"b.txt\"]"
        ),
        "nothing selected: the current item"
    );
    assert_eq!(
        lines[1],
        format!(
            "Passive left=false active=false target=false kind=Files folder={right} items=1 \
             names=[\"..\"] selected=[]"
        ),
        "on ..: nothing"
    );
    assert_eq!(lines[2], "command line=dir");
    assert!(app.panels[0].selected.contains("b.txt"));
}
