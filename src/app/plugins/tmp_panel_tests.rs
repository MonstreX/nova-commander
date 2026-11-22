//! The temporary panel (ADR-0016 §14, 5d-1) in the program's panels, its library as it is shipped: it
//! opens in its own mode, F5 into it adds paths (asking about folders' contents), F8 on it deletes the
//! real file, Enter on a folder goes to that folder on its disk. Each test names its plugin alone.

use super::*;
use crate::app::{ColumnKind, ViewMode};
use crate::plugin_host::tests::install_shipped;

/// The common panel's instance (`tmppanel::COMMON`).
const COMMON: &str = "common";

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// The temporary panel installed as `name`, opened on the left from the drive menu, the right panel in a
/// folder of files (`a.txt`, `sub\b.txt`), the right one active; the files' folder kept by the caller.
fn opened(name: &str) -> (tempfile::TempDir, tempfile::TempDir, App) {
    opened_with(name, &[], COMMON)
}

/// As [`opened`], the plugin's settings `values` (its keys and values, FAR's TmpPanel's), the panel
/// opened its instance `instance`.
/// The panel `side` shows the root of plugin `name`'s instance starting with `instance`, read.
fn shows_instance(app: &App, side: Side, name: &str, instance: &str) -> bool {
    let panel = &app.panels[side.index()];
    panel.reading().is_none()
        && panel.path.provider() == Some(name)
        && (panel.path.session_path())
            .is_some_and(|(shown, path)| shown.starts_with(instance) && path == "/")
}

fn opened_with(
    name: &str,
    values: &[(&str, &str)],
    instance: &str,
) -> (tempfile::TempDir, tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    install_shipped(temp.path(), "tmppanel", name);
    let mut app = app_in(temp.path());
    let settings: serde_json::Map<String, serde_json::Value> = (values.iter())
        .map(|(key, value)| (format!("plugins.{name}.settings.{key}"), (*value).into()))
        .collect();
    let host = crate::plugin_host::PluginHost::load(
        temp.path(),
        &temp.path().join("Logs"),
        &settings,
        &|_| true,
    );
    app.set_plugins(host);
    app.config.delete_settings.to_recycle_bin = false;
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("a.txt"), "hello").unwrap();
    std::fs::create_dir(files.path().join("sub")).unwrap();
    std::fs::write(files.path().join("sub").join("b.txt"), "b").unwrap();
    app.open_disk_for_test(0, 0, Side::Left.index());
    // 5d-3b: the common panel's one instance (FAR's one global current), not "0".."9"; a panel of its
    // own is `local<n>`, its number by the clock (`instance` its start then).
    tick_until(&mut app, |app| {
        shows_instance(app, Side::Left, name, instance)
    });
    let right = VfsPath::local(files.path());
    app.panels[1].change_dir(&right, None).unwrap();
    tick_until(&mut app, |app| shows(app, Side::Right, &right));
    app.active = Side::Right;
    (temp, files, app)
}

/// F5 or F6: FAR's copy dialog with the target "Temporary panel:" (its format), taken as it is.
fn copy(app: &mut App, code: KeyCode) {
    app.handle_key(key(code));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the copy dialog");
    };
    assert!(
        (form.items.iter()).any(|item| matches!(
            &item.control,
            crate::app::form::Control::Edit(edit) if edit.text() == "Temporary panel:"
        )),
        "the target is the panel's format"
    );
    app.handle_key(key(KeyCode::Enter));
}

/// The panel of `side` with its cursor on the entry named `name`.
fn point(app: &mut App, side: Side, name: &str) {
    let panel = &mut app.panels[side.index()];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == name)
        .unwrap();
}

fn names(app: &App, side: Side) -> Vec<String> {
    (app.panels[side.index()].entries.iter())
        .filter(|entry| !entry.is_parent)
        .map(|entry| entry.name.clone())
        .collect()
}

#[test]
fn it_opens_titled_in_its_own_mode_taking_paths() {
    let (_temp, _files, app) = opened("tmpone");
    let panel = &app.panels[0];
    assert_eq!(panel.look.title.as_deref(), Some("Temporary panel [0]"));
    assert!(panel.look.takes_paths);
    assert_eq!(
        panel.view_mode,
        ViewMode::Wide,
        "FAR's TmpPanel opens in its mode 4"
    );
    let spec = app.panel_modes.spec_in(panel.view_mode, &panel.look);
    let kinds: Vec<ColumnKind> = spec.panel.iter().map(|column| column.kind).collect();
    assert_eq!(kinds, [ColumnKind::Name, ColumnKind::Size]);
    assert!(names(&app, Side::Left).is_empty(), "empty at first");
}

#[test]
fn f5_adds_the_paths_and_a_folder_without_its_contents() {
    let (_temp, files, mut app) = opened("tmptwo");
    point(&mut app, Side::Right, "a.txt");
    copy(&mut app, KeyCode::F(5));
    let a = files.path().join("a.txt").display().to_string();
    tick_until(&mut app, |app| names(app, Side::Left) == [a.clone()]);
    // FAR's "Sending files to temporary panel" while it puts.
    tick_until(&mut app, |app| app.overlay.is_none());
    assert_eq!(
        app.panels[0].entries[1].real_path.as_deref(),
        Some(files.path().join("a.txt").as_path())
    );
    // A folder: TmpPanel's default `CopyContents = 0` puts it alone and asks nothing.
    point(&mut app, Side::Right, "sub");
    app.handle_key(key(KeyCode::Insert));
    copy(&mut app, KeyCode::F(6));
    let sub = files.path().join("sub");
    tick_until(&mut app, |app| names(app, Side::Left).len() == 2);
    tick_until(&mut app, |app| {
        app.overlay.is_none() && app.plugins.putting.is_none()
    });
    let listed = names(&app, Side::Left);
    assert!(listed.contains(&sub.display().to_string()));
    assert!(
        !listed.contains(&sub.join("b.txt").display().to_string()),
        "without its contents"
    );
    assert!(app.panels[1].selected.is_empty(), "the selection went");
    assert!(sub.join("b.txt").is_file(), "a move removes nothing");
}

#[test]
fn f8_deletes_the_real_file_and_its_name_goes() {
    let (_temp, files, mut app) = opened("tmpthree");
    point(&mut app, Side::Right, "a.txt");
    copy(&mut app, KeyCode::F(5));
    let a = files.path().join("a.txt");
    tick_until(&mut app, |app| names(app, Side::Left).len() == 1);
    // The put's message gone before F8.
    tick_until(&mut app, |app| app.overlay.is_none());
    app.active = Side::Left;
    point(&mut app, Side::Left, &a.display().to_string());
    app.handle_key(key(KeyCode::F(8)));
    tick_until(&mut app, |app| app.overlay.is_some());
    app.handle_key(key(KeyCode::Enter));
    tick_until(&mut app, |app| {
        !a.exists() && names(app, Side::Left).is_empty()
    });
}

#[test]
fn enter_on_a_folder_goes_to_it_on_its_disk_in_the_view_it_had() {
    let (_temp, files, mut app) = opened("tmpfour");
    point(&mut app, Side::Right, "sub");
    copy(&mut app, KeyCode::F(5));
    let sub = files.path().join("sub");
    tick_until(&mut app, |app| {
        names(app, Side::Left) == [sub.display().to_string()] && app.overlay.is_none()
    });
    app.active = Side::Left;
    point(&mut app, Side::Left, &sub.display().to_string());
    app.handle_key(key(KeyCode::Enter));
    let real = VfsPath::local(&sub);
    tick_until(&mut app, |app| shows(app, Side::Left, &real));
    assert_eq!(
        app.panels[0].view_mode,
        ViewMode::Medium,
        "the view from before"
    );
    assert_eq!(names(&app, Side::Left), ["b.txt"]);
}

#[test]
fn a_folder_put_by_default_goes_alone_and_takes_the_selection() {
    let (_temp, files, mut app) = opened("tmpfive");
    point(&mut app, Side::Right, "sub");
    app.handle_key(key(KeyCode::Insert));
    copy(&mut app, KeyCode::F(5));
    tick_until(&mut app, |app| app.plugins.putting.is_none());
    let sub = files.path().join("sub").display().to_string();
    assert_eq!(names(&app, Side::Left), [sub], "FAR's default: no question");
    assert!(app.panels[1].selected.is_empty(), "put: the selection went");
}

#[test]
fn an_edited_target_is_an_ordinary_copy_there() {
    let (_temp, files, mut app) = opened("tmpseven");
    let elsewhere = tempfile::tempdir().unwrap();
    point(&mut app, Side::Right, "a.txt");
    app.handle_key(key(KeyCode::F(5)));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("the copy dialog");
    };
    let target = (form.items.iter_mut())
        .find_map(|item| match &mut item.control {
            crate::app::form::Control::Edit(edit) if edit.text() == "Temporary panel:" => {
                Some(edit)
            }
            _ => None,
        })
        .unwrap();
    *target = crate::app::LineInput::with_text(&elsewhere.path().display().to_string());
    app.handle_key(key(KeyCode::Enter));
    let copied = elsewhere.path().join("a.txt");
    tick_until(&mut app, |_| copied.is_file());
    assert!(files.path().join("a.txt").is_file(), "copied, not put");
    assert!(
        names(&app, Side::Left).is_empty(),
        "nothing on the temporary panel"
    );
}

#[test]
fn alt_f6_is_a_link_not_a_put_and_a_hidden_panel_takes_no_paths() {
    let (_temp, _files, mut app) = opened("tmpsix");
    point(&mut app, Side::Right, "a.txt");
    app.handle_key(KeyEvent::new(KeyCode::F(6), KeyModifiers::ALT));
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Form { .. }))),
        "the link's dialog"
    );
    assert!(app.plugins.putting.is_none());
    app.overlay = None;
    app.panels[0].visible = false;
    assert!(
        app.plugin_target().is_none(),
        "hidden: an ordinary copy's target"
    );
}

#[test]
fn the_programs_modes_show_its_names_only_sorted_by_them() {
    let (_temp, files, mut app) = opened("tmpeight");
    std::fs::create_dir(files.path().join("z")).unwrap();
    std::fs::write(files.path().join("z").join("0.txt"), "0").unwrap();
    point(&mut app, Side::Right, "a.txt");
    copy(&mut app, KeyCode::F(5));
    tick_until(&mut app, |app| {
        names(app, Side::Left).len() == 1 && app.overlay.is_none()
    });
    let z = VfsPath::local(files.path().join("z"));
    app.panels[1].change_dir(&z, None).unwrap();
    tick_until(&mut app, |app| shows(app, Side::Right, &z));
    point(&mut app, Side::Right, "0.txt");
    copy(&mut app, KeyCode::F(5));
    tick_until(&mut app, |app| {
        names(app, Side::Left).len() == 2 && app.overlay.is_none()
    });
    let shown = |app: &App| {
        let panel = &app.panels[0];
        let spec = app.panel_modes.spec_in(panel.view_mode, &panel.look);
        spec.panel[0].flags.name_only
    };
    assert!(!shown(&app), "its own mode at Ctrl+4: whole paths");
    let zero = files.path().join("z").join("0.txt").display().to_string();
    let a = files.path().join("a.txt").display().to_string();
    let files_only = |app: &App| -> Vec<String> {
        (app.panels[0].entries.iter())
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.name.clone())
            .collect()
    };
    assert_eq!(
        files_only(&app),
        [a.clone(), zero.clone()],
        "by whole paths"
    );
    app.active = Side::Left;
    app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::CONTROL));
    assert!(
        shown(&app),
        "the program's Medium: names only (OPIF_SHOWNAMESONLY)"
    );
    assert_eq!(files_only(&app), [zero, a], "sorted by the names shown");
}

/// The files' folder as a tree on the left (its root the test's own folder, no real disk read), the
/// temporary panel on the right opposite it, the tree's cursor on `sub`.
fn tree_opposite(name: &str) -> (tempfile::TempDir, tempfile::TempDir, App) {
    let (temp, files, mut app) = opened(name);
    crate::app::tree_panel::TEST_ROOT
        .with(|root| *root.borrow_mut() = Some(files.path().to_path_buf()));
    app.active = Side::Right;
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
    tick_until(&mut app, |app| {
        app.tree_panel
            .as_ref()
            .is_some_and(|tree| !tree.reading && tree.items.len() == 2)
    });
    app.open_disk_for_test(0, 0, Side::Right.index());
    // 5d-3b: the common panel's one instance (FAR's one global current), not "0".."9".
    let root = VfsPath::in_session(name, COMMON, "/");
    tick_until(&mut app, |app| shows(app, Side::Right, &root));
    app.active = Side::Left;
    app.handle_key(key(KeyCode::End));
    assert!(app.tree_is_active());
    (temp, files, app)
}

/// The target line of the copy dialog up, then the dialog away.
fn offered(app: &mut App) -> Option<String> {
    let target = match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => {
            (form.items.iter()).find_map(|item| match &item.control {
                crate::app::form::Control::Edit(edit) => Some(edit.text().to_string()),
                _ => None,
            })
        }
        _ => None,
    };
    app.overlay = None;
    target
}

#[test]
fn the_trees_f5_offers_the_plugin_its_shift_f5_the_folder_and_alt_f6_nothing() {
    let (_temp, files, mut app) = tree_opposite("tmpnine");
    let sub = files.path().join("sub").display().to_string();
    app.handle_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::SHIFT));
    let own = offered(&mut app).unwrap();
    assert!(
        own.contains(&sub),
        "FAR's tree: Shift+F5 never to the plugin: {own}"
    );
    app.handle_key(key(KeyCode::F(5)));
    assert_eq!(offered(&mut app).as_deref(), Some("Temporary panel:"));
    app.handle_key(KeyEvent::new(KeyCode::F(6), KeyModifiers::ALT));
    assert!(
        app.overlay.is_none(),
        "FAR's tree: Alt+F6 into a plugin's panel does nothing"
    );
}

#[path = "tmp_panel_keys_tests.rs"]
mod keys;

#[path = "tmp_panel_config_tests.rs"]
mod config;

#[path = "tmp_panel_options_tests.rs"]
mod options;

#[path = "tmp_panel_lists_tests.rs"]
mod lists;
