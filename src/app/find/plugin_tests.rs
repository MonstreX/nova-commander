//! Alt+F7 on a plugin's panel as FAR's `PluginMode`: the drives' areas grey, the links and streams off,
//! the drives, PATH and the root searched from the plugin's root through the plugin, its files' texts
//! read through it and a file found viewed as a copy.

use super::*;
use crate::app::{Screen, Side};
use crate::plugin_host::tests::{load, plugins_with_probe};

/// The probe as `name` (one of each test's own: the providers are the process's, the tests side by
/// side), its panel `one` on the left at `/docs` (its tree: `/readme.txt` "Hello probe!", `/docs/a.txt`
/// "a").
fn on_probe(name: &str) -> (tempfile::TempDir, App) {
    let plugins = plugins_with_probe(|manifest| manifest["name"] = name.into());
    std::fs::write(plugins.path().join("probe").join("disks.txt"), "Panel one").unwrap();
    let mut app = app_in(plugins.path());
    app.set_plugins(load(plugins.path(), &|_| true));
    app.open_disk_for_test(0, 0, Side::Left.index());
    let docs = VfsPath::in_session(name, "one", "/docs");
    until(&mut app, |app| {
        app.panels[0].path.provider().is_some() && !app.panels[0].is_reading()
    });
    app.panels[0].change_dir(&docs, None).unwrap();
    until(&mut app, |app| {
        app.panels[0].path == docs && !app.panels[0].is_reading()
    });
    (plugins, app)
}

#[test]
fn on_a_plugins_panel_the_drives_are_grey_and_links_and_streams_off() {
    let (_plugins, mut app) = on_probe("findgrey");
    app.find.area = Some(Area::AllDrives);
    app.find.links = true;
    app.find.streams = true;
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    let form = form(&mut app);
    let Some(Control::DropDown { lines, chosen, .. }) = form.control(AREA) else {
        panic!("the area list");
    };
    assert!(matches!(lines[0], ListLine::Grayed(_)));
    assert!(matches!(lines[1], ListLine::Grayed(_)));
    assert!(matches!(lines[2], ListLine::Choice(_)), "PATH stays");
    assert_eq!(
        Area::ALL[*chosen],
        Area::Root,
        "the drives give way to the root"
    );
    assert!(form.items[STREAMS].disabled && !form.is_checked(STREAMS));
    assert!(
        form.items[STREAMS - 1].disabled && !form.is_checked(STREAMS - 1),
        "links"
    );
    // A search on the plugin's panel neither takes the boxes' "off" for the disks nor walks links.
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| results(app).finalized);
    assert!(app.find.links && app.find.streams, "the disks' choice kept");
    let mask = app.file_mask("*").unwrap();
    let request = app.find_request(mask);
    assert!(
        !request.links && !request.streams,
        "none on the plugin's panel"
    );
}

#[test]
fn the_root_is_the_plugins_and_its_texts_are_read_through_it() {
    let (_plugins, mut app) = on_probe("findroot");
    app.find.area = Some(Area::Root);
    find(&mut app, "*.txt", "");
    let mut names = found_names(&app);
    names.sort();
    assert_eq!(names, ["a.txt", "readme.txt"]);
    let root = VfsPath::in_session("findroot", "one", "/");
    let results = results(&app);
    assert!(results.found.iter().any(|item| item.folder == root));
    assert!(!results.find_list, "no Panel without real names");
    app.overlay = None;
    app.find.area = Some(Area::Current);
    find(&mut app, "*", "a");
    assert_eq!(found_names(&app), ["a.txt"], "the current folder only");
    app.overlay = None;
    app.find.area = Some(Area::Path);
    find(&mut app, "*", "Hello probe");
    assert_eq!(
        found_names(&app),
        ["readme.txt"],
        "PATH is the plugin's root too"
    );
}

#[test]
fn f3_on_a_plugins_file_found_views_a_copy_of_it() {
    let (_plugins, mut app) = on_probe("findview");
    app.find.area = Some(Area::Root);
    find(&mut app, "readme.txt", "");
    assert_eq!(found_names(&app), ["readme.txt"]);
    let at = (results(&app).lines.iter())
        .position(|line| matches!(line, crate::app::find::results::ResultLine::Found(_)))
        .unwrap();
    if let Some(Overlay::FindResults(results)) = &mut app.overlay {
        results.cursor = at;
    }
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    until(&mut app, |app| matches!(app.screen, Screen::Viewer(_)));
    let Screen::Viewer(viewer) = &app.screen else {
        unreachable!()
    };
    let shown = viewer.path.clone();
    assert!(
        shown.is_absolute() && shown.ends_with("readme.txt"),
        "{}",
        shown.display()
    );
    assert_eq!(std::fs::read_to_string(&shown).unwrap(), "Hello probe!");
}
