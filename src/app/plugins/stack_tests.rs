//! The plugins' panels one over another (ADR-0016 §15, FAR's `PushPlugin` / `PopPlugin`): ".." at the upper
//! one's root goes back to the one under it as it was, the lower one never closed meanwhile; a disk's
//! folder leaves them all; each brings back the view, the sort mode, its order and "directories first" the
//! panel had.

use super::*;
use crate::app::ViewMode;
use crate::vfs::{SortMode, SortOrder};

/// The probe installed twice, as `first` and `second` (`first` sorting before), each with the drive menu
/// item "Panel one", loaded into an app.
fn two_plugins(first: &str, second: &str) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    for name in [first, second] {
        let manifest =
            format!(r#"{{"name": "{name}", "version": "1", "api": 11, "library": "probe"}}"#);
        let place = install_probe(temp.path(), name, &manifest);
        std::fs::write(place.join("disks.txt"), "Panel one").unwrap();
    }
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    (temp, app)
}

fn sorted(mode: SortMode, reverse: bool, directories_first: bool) -> SortOrder {
    SortOrder {
        mode,
        reverse,
        directories_first,
        ..SortOrder::default()
    }
}

/// The left panel's view and sort as FAR keeps them for a plugin's panel.
fn looks(app: &App) -> (ViewMode, SortMode, bool, bool) {
    let panel = &app.panels[0];
    (
        panel.view_mode,
        panel.sort.mode,
        panel.sort.reverse,
        panel.sort.directories_first,
    )
}

#[test]
fn a_plugins_panel_opened_over_another_takes_its_place_with_the_disks_view() {
    // FAR's `ChangePanel` with `CreateNew` (diskmenu.cpp, filepanels.cpp): the plugin's panel shown closes
    // (`dispose`), the new one comes with what the panel had before it (`GetPrevViewMode`...), and ".."
    // at its root goes to that disk's folder.
    let (temp, mut app) = two_plugins("stacka", "stackb");
    app.active = Side::Left;
    let disk = app.panels[0].path.clone();
    app.panels[0].set_view_mode(ViewMode::Brief);
    app.panels[0].set_sort_order(sorted(SortMode::Size, true, false));
    app.open_disk_for_test(0, 0, Side::Left.index());
    let first = VfsPath::in_session("stacka", "one", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &first));
    app.panels[0].set_view_mode(ViewMode::Full);
    app.panels[0].set_sort_order(sorted(SortMode::Name, false, true));
    app.open_disk_for_test(1, 0, Side::Left.index());
    let second = VfsPath::in_session("stackb", "one", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &second));
    assert_eq!(
        looks(&app),
        (ViewMode::Brief, SortMode::Size, true, false),
        "what the panel had before the first"
    );
    assert_eq!(
        app.panels[0].return_to(),
        Some(&disk),
        "\"..\" leads to the disk"
    );
    settle(&mut app);
    assert!(
        log(&temp, "stacka").contains("closed one"),
        "the first one closed"
    );
    let saved = app.to_settings();
    assert_eq!(
        (saved.left_view.as_str(), saved.left_sort.as_str()),
        ("brief", SortMode::Size.id()),
        "saved as the disk's"
    );
    app.panels[0].set_view_mode(ViewMode::Wide);
    app.panels[0].go_parent().unwrap();
    tick_until(&mut app, |app| shows(app, Side::Left, &disk));
    assert_eq!(
        looks(&app),
        (ViewMode::Brief, SortMode::Size, true, false),
        "the disk's as it was"
    );
    assert_eq!(app.panels[0].return_to(), None);
    settle(&mut app);
    assert!(log(&temp, "stackb").contains("closed one"));
}

#[test]
fn dot_dot_whose_folder_is_gone_leaves_the_panel_where_it_is() {
    let (temp, mut app) = two_plugins("stacke", "stackf");
    app.active = Side::Left;
    let gone = temp.path().join("gone");
    std::fs::create_dir(&gone).unwrap();
    app.panels[0]
        .change_dir(&VfsPath::local(&gone), None)
        .unwrap();
    app.open_disk_for_test(0, 0, Side::Left.index());
    let panel = VfsPath::in_session("stacke", "one", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &panel));
    std::fs::remove_dir(&gone).unwrap();
    assert!(
        app.panels[0].go_parent().is_err(),
        "the folder came from is gone"
    );
    assert_eq!(app.panels[0].path, panel, "still on the plugin's panel");
    assert_eq!(
        app.panels[0].return_to(),
        Some(&VfsPath::local(&gone)),
        "the stack as it was"
    );
    let disk = VfsPath::local(temp.path());
    app.panels[0].change_dir(&disk, None).unwrap();
    tick_until(&mut app, |app| shows(app, Side::Left, &disk));
    assert_eq!(
        app.panels[0].return_to(),
        None,
        "no pop taken for the disk's folder"
    );
}

#[test]
fn a_disks_folder_leaves_them_all_with_what_the_panel_had_before() {
    let (temp, mut app) = two_plugins("stackc", "stackd");
    app.active = Side::Left;
    app.panels[0].set_view_mode(ViewMode::Wide);
    app.panels[0].set_sort_order(sorted(SortMode::Extension, false, true));
    app.open_disk_for_test(0, 0, Side::Left.index());
    let lower = VfsPath::in_session("stackc", "one", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &lower));
    app.panels[0].set_sort_order(sorted(SortMode::Unsorted, false, false));
    app.open_disk_for_test(1, 0, Side::Left.index());
    let upper = VfsPath::in_session("stackd", "one", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &upper));
    let elsewhere = VfsPath::local(temp.path().join("stackc"));
    app.panels[0].change_dir(&elsewhere, None).unwrap();
    tick_until(&mut app, |app| shows(app, Side::Left, &elsewhere));
    assert_eq!(
        looks(&app),
        (ViewMode::Wide, SortMode::Extension, false, true)
    );
    assert_eq!(app.panels[0].return_to(), None, "none under it any more");
    settle(&mut app);
    assert!(log(&temp, "stackc").contains("closed one"));
    assert!(log(&temp, "stackd").contains("closed one"));
}

#[test]
fn a_plugin_reading_and_deleting_its_real_names_itself_keeps_them() {
    // FAR's `UseInternalCommand`: a plugin with its own `GetFiles` / `DeleteFiles` keeps its real names.
    let (temp, mut app) = app_with("stackown", &[("disks.txt", "Panel ownreal")]);
    app.open_disk_for_test(0, 0, Side::Left.index());
    let root = VfsPath::in_session("stackown", "ownreal", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &root));
    app.active = Side::Left;
    app.config.delete_settings.to_recycle_bin = false;
    let folder = temp.path().join("stackown");
    let panel = &mut app.panels[0];
    assert!(panel.look.own_get && panel.look.own_delete);
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "disks.txt")
        .unwrap();
    let entry = panel.entries[panel.cursor].clone();
    assert!(entry.real_path.is_some());
    assert_eq!(
        panel.item_path(&entry),
        root.join("disks.txt"),
        "read through it"
    );
    assert_eq!(
        app.quick_view_target(Side::Right),
        Some((root.join("disks.txt"), false))
    );
    app.handle_key(KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE));
    tick_until(&mut app, |app| app.overlay.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |_| {
        log(&temp, "stackown").contains("removed ownreal:/disks.txt")
    });
    assert!(
        folder.join("disks.txt").is_file(),
        "deleted by it: the file stays"
    );
}
