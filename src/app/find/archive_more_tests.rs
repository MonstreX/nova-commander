//! More of Alt+F7's "Search in archives" through the program: Go to from a panel elsewhere and onto a
//! folder gone from the archive, F3 on what a plugin no longer loaded found, each plugin asked with its
//! own masks, and the box following the panel's kind (a drive changed from the dialog, real names).

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::super::FindState;
use super::super::dialog::{self, DRIVE};
use super::super::tests::{form, press, until};
use super::{app_with_zip, find_in_archives, lines, opens_alone, point_found};
use crate::app::dialog::Dialog;
use crate::app::fileops::tests::app_in;
use crate::app::{App, Overlay, Screen, Side};
use crate::plugin_host::tests::{install_shipped, load, plugins_with_probe};
use crate::vfs::VfsPath;
use archive::support::zip_of;

/// Whether the left panel is read and inside the archive at `inner`.
fn inside_at(app: &App, inner: &str) -> bool {
    let panel = &app.panels[0];
    panel.reading().is_none() && (panel.path.session_path()).is_some_and(|(_, at)| at == inner)
}

fn current_name(app: &App) -> Option<String> {
    app.panels[0].current().map(|entry| entry.name.clone())
}

/// Go to takes the panel to the archive's folder from wherever it is, the plugin named in any case.
#[test]
fn go_to_takes_the_panel_to_the_archive_from_elsewhere() {
    let (_plugins, _files, mut app) = app_with_zip("FindArcAway");
    find_in_archives(&mut app, "readme.txt", "");
    let elsewhere = tempfile::tempdir().unwrap();
    let away = VfsPath::local(elsewhere.path());
    app.panels[0].change_dir(&away, None).unwrap();
    until(&mut app, |app| app.panels[0].reading().is_none());
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| inside_at(app, "/docs"));
    assert_eq!(current_name(&app).as_deref(), Some("readme.txt"));
}

/// Until the search has closed the archive `zip` (the plugin lets its file go).
fn wait_closed(app: &mut App, zip: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !opens_alone(zip) {
        assert!(std::time::Instant::now() < deadline, "still open");
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Go to when the archive's folder is gone: the panel cannot get there (its error says why), and no
/// plugin is asked to open anything.
#[test]
fn go_to_where_the_archives_folder_is_gone_asks_no_plugin() {
    let (_plugins, files, mut app) = app_with_zip("findarcnodir");
    find_in_archives(&mut app, "readme.txt", "");
    wait_closed(&mut app, &files.path().join("a.zip"));
    let elsewhere = tempfile::tempdir().unwrap();
    let away = VfsPath::local(elsewhere.path());
    for panel in &mut app.panels {
        panel.change_dir(&away, None).unwrap();
    }
    until(&mut app, |app| {
        app.panels.iter().all(|panel| panel.reading().is_none())
    });
    std::fs::remove_dir_all(files.path()).unwrap();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(!app.plugin_key_waits(), "nothing opened");
    assert!(app.panels[0].path.same_place(&away), "the panel stays");
}

/// TmpPanel opens no list for the search: its list file is found, nothing inside it.
#[test]
fn a_temporary_panels_list_is_found_and_not_searched_in() {
    let plugins = tempfile::tempdir().unwrap();
    install_shipped(plugins.path(), "tmppanel", "findarclistall");
    let files = tempfile::tempdir().unwrap();
    let plain = files.path().join("plain.txt");
    std::fs::write(&plain, "plain").unwrap();
    std::fs::write(files.path().join("list.temp"), plain.display().to_string()).unwrap();
    let mut app = app_in(files.path());
    app.set_plugins(load(plugins.path(), &|_| true));
    app.active = Side::Left;
    until(&mut app, |app| {
        app.panels[0].reading().is_none()
            && (app.panels[0].entries.iter()).any(|entry| entry.name == "list.temp")
            && (app.plugins.host.plugins.iter()).any(|plugin| !plugin.file_masks.is_empty())
    });
    find_in_archives(&mut app, "*", "");
    assert_eq!(lines(&app, files.path()), ["T", "list.temp", "plain.txt"]);
}

/// A folder no longer in the archive (changed since the search): its root, as FAR's
/// `SetPluginDirectory` failing leaves it, with no error.
#[test]
fn go_to_on_a_folder_gone_from_the_archive_stays_at_its_root() {
    let (_plugins, files, mut app) = app_with_zip("findarcgonedir");
    find_in_archives(&mut app, "readme.txt", "");
    let zip = files.path().join("a.zip");
    wait_closed(&mut app, &zip);
    std::fs::write(&zip, zip_of(&[("top.txt", b"top")])).unwrap();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| inside_at(app, "/"));
    let names: Vec<&str> = (app.panels[0].entries.iter())
        .map(|entry| entry.name.as_str())
        .collect();
    assert!(names.contains(&"top.txt"), "{names:?}");
    assert!(
        !matches!(app.overlay, Some(Overlay::Dialog(_))),
        "no error: {:?}",
        app.overlay.is_some()
    );
}

/// F3 on what a plugin found that is no longer loaded: no other plugin opens the archive for it.
#[test]
fn f3_on_what_a_plugin_gone_found_says_the_archive_is_not_opened() {
    let (_plugins, files, mut app) = app_with_zip("findarcghost");
    find_in_archives(&mut app, "*.txt", "");
    let Some(Overlay::FindResults(found)) = &mut app.overlay else {
        panic!("no results");
    };
    for item in &mut found.found {
        let ghost = (item.folder.file_path())
            .map(|(archive, inner)| VfsPath::in_file("ghost", archive.clone(), inner));
        if let Some(ghost) = ghost {
            item.folder = ghost;
        }
    }
    point_found(&mut app, "top.txt");
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    until(&mut app, |app| {
        matches!(
            app.overlay,
            Some(Overlay::Dialog(Dialog::Message { error: true, .. }))
        )
    });
    let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
        unreachable!();
    };
    let zip = files.path().join("a.zip");
    assert_eq!(lines[0], format!("Cannot open {}", zip.display()));
    assert!(!matches!(app.screen, Screen::Viewer(_)));
}

/// TmpPanel first (its masks take no zip, and it passes the search's files), the archive plugin after
/// it with its own masks: the zip opened by it.
#[test]
fn each_plugin_is_offered_the_file_with_its_own_masks() {
    let plugins = tempfile::tempdir().unwrap();
    install_shipped(plugins.path(), "tmppanel", "findarcmasks1");
    install_shipped(plugins.path(), "archive", "findarcmasks2");
    let files = tempfile::tempdir().unwrap();
    let zip = zip_of(&[("top.txt", b"top")]);
    std::fs::write(files.path().join("a.zip"), zip).unwrap();
    let mut app = app_in(files.path());
    app.set_plugins(load(plugins.path(), &|_| true));
    app.active = Side::Left;
    until(&mut app, |app| {
        app.panels[0].reading().is_none()
            && (app.panels[0].entries.iter()).any(|entry| entry.name == "a.zip")
            && (app.plugins.host.plugins.iter()).any(|plugin| plugin.file_masks == ["*"])
    });
    let hosted = &app.plugins.host.plugins;
    let at = (hosted.iter()).position(|plugin| plugin.name == "findarcmasks2");
    assert_eq!(at, Some(1), "the archive's plugin second");
    find_in_archives(&mut app, "top.txt", "");
    assert_eq!(lines(&app, files.path()), [r"T\a.zip:\", "top.txt"]);
}

#[test]
fn the_box_stays_on_a_plugins_panel_with_real_names_and_comes_back_off_it() {
    let state = FindState {
        archives: true,
        ..FindState::default()
    };
    let mut form = state.form("root", &crate::settings::CodePageSettings::default());
    let shown = |form: &crate::app::form::Form| {
        let disabled = form.items[dialog::ARCHIVES].disabled;
        (form.is_checked(dialog::ARCHIVES), disabled)
    };
    state.set_plugin_mode(&mut form, true, true);
    assert_eq!(shown(&form), (true, false), "a list's panel: real names");
    state.set_plugin_mode(&mut form, true, false);
    assert_eq!(shown(&form), (false, true), "an archive's panel");
    state.set_plugin_mode(&mut form, false, true);
    assert_eq!(shown(&form), (true, false), "as given, back on a disk");
}

/// The drive menu from the dialog `app` shows; `choose` stands for the item chosen, then the dialog
/// back for the panel's new place.
fn change_drive(app: &mut App, choose: impl FnOnce(&mut App)) {
    form(app).focus_item(DRIVE);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    until(app, |app| {
        matches!(app.overlay, Some(Overlay::DriveMenu(_)))
    });
    app.overlay = None;
    choose(app);
    until(app, |app| {
        !app.panels[0].is_reading()
            && matches!(app.overlay, Some(Overlay::Dialog(Dialog::Form { .. })))
    });
}

/// The box follows the panel the dialog's drive menu changed: off and disabled on a plugin's panel
/// without real names, back as given on a disk.
#[test]
fn the_box_follows_the_panel_the_drive_menu_changed() {
    let plugins = plugins_with_probe(|manifest| manifest["name"] = "findarcdrive".into());
    std::fs::write(plugins.path().join("probe").join("disks.txt"), "Panel one").unwrap();
    let mut app = app_in(plugins.path());
    app.set_plugins(load(plugins.path(), &|_| true));
    app.active = Side::Left;
    app.find.archives = true;
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    assert!(form(&mut app).is_checked(dialog::ARCHIVES));
    change_drive(&mut app, |app| {
        app.open_disk_for_test(0, 0, Side::Left.index());
        until(app, |app| app.panels[0].path.provider().is_some());
    });
    let shown = form(&mut app);
    assert!(shown.items[dialog::ARCHIVES].disabled);
    assert!(!shown.is_checked(dialog::ARCHIVES));
    let disk = VfsPath::local(plugins.path());
    change_drive(&mut app, |app| {
        app.panels[0].change_dir(&disk, None).unwrap()
    });
    let shown = form(&mut app);
    assert!(!shown.items[dialog::ARCHIVES].disabled);
    assert!(shown.is_checked(dialog::ARCHIVES), "as given");
}
