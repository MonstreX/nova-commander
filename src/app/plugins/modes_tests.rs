//! A plugin's own panel modes in the program's panel (ADR-0016 §13, 5c-3), on the probe: Ctrl+3 and
//! Ctrl+6 show its modes with its entries' texts, the view the panel had comes back as it leaves the
//! plugin and is the one saved meanwhile; a plugin without modes, and a disk, keep the program's.

use super::*;
use crate::app::{ColumnKind, ViewMode};

fn ctrl(digit: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(digit), KeyModifiers::CONTROL)
}

/// The left panel's mode as it is drawn: its columns, titles and status columns.
fn drawn(app: &App) -> (Vec<ColumnKind>, Vec<Option<String>>, Vec<ColumnKind>, bool) {
    let panel = &app.panels[0];
    let spec = app.panel_modes.spec_in(panel.view_mode, &panel.look);
    let kinds =
        |columns: &[crate::app::ColumnDef]| columns.iter().map(|column| column.kind).collect();
    (
        kinds(&spec.panel),
        spec.titles.clone(),
        kinds(&spec.status),
        spec.full_screen,
    )
}

/// The probe `name`'s panel `instance` open on the left, active, the view Brief before it; the folder it
/// came from.
fn opened(name: &str, instance: &str) -> (tempfile::TempDir, App, VfsPath) {
    let disks = format!("Panel {instance}\nPanel other");
    let (temp, mut app) = app_with(name, &[("disks.txt", &disks)]);
    app.active = Side::Left;
    app.handle_key(ctrl('1'));
    let came_from = app.panels[0].path.clone();
    app.open_disk_for_test(0, 0, Side::Left.index());
    let root = VfsPath::in_session(name, instance, "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &root));
    (temp, app, came_from)
}

#[test]
fn ctrl_3_and_ctrl_6_show_the_plugins_modes_and_its_texts() {
    let (_temp, mut app, _) = opened("modesone", "one");
    assert_eq!(app.panels[0].view_mode, ViewMode::Brief, "the view it had");
    app.handle_key(ctrl('3'));
    use ColumnKind::*;
    let (kinds, titles, status, full_screen) = drawn(&app);
    assert_eq!(kinds, [Name, Custom(0), Description]);
    assert_eq!(
        titles,
        [
            Some("Item".to_string()),
            Some("Kind".to_string()),
            Some("About".to_string())
        ]
    );
    assert_eq!(status, [Name, Size, Date, Time], "the detailed status line");
    assert!(!full_screen);
    let readme = (app.panels[0].entries.iter())
        .find(|entry| entry.name == "readme.txt")
        .unwrap();
    let texts = readme.texts.as_deref().unwrap();
    assert_eq!(texts.columns, ["file", "10"]);
    assert_eq!(texts.description, "About readme.txt");
    app.handle_key(ctrl('6'));
    assert_eq!(app.panels[0].view_mode, ViewMode::Provider(6));
    let (kinds, titles, status, full_screen) = drawn(&app);
    assert_eq!(kinds, [Name, Custom(1)]);
    assert_eq!(titles, [None, Some("Letters".to_string())]);
    assert_eq!(status, [Custom(0), Name]);
    assert!(full_screen);
    app.handle_key(ctrl('2'));
    let (kinds, titles, ..) = drawn(&app);
    assert_eq!(
        kinds,
        [Name, Name],
        "none of its own at Ctrl+2: the program's"
    );
    assert!(titles.is_empty());
}

#[test]
fn the_view_from_before_is_saved_and_comes_back_as_the_panel_leaves_the_plugin() {
    let (_temp, mut app, came_from) = opened("modestwo", "one");
    app.handle_key(ctrl('6'));
    assert_eq!(app.to_settings().left_view, "brief", "saved as it was");
    // A panel of the plugin opened from the drive menu over this one comes with the view the panel had
    // before (FAR's `ChangePanel` with `CreateNew`: `GetPrevViewMode`), and keeps it to come back to.
    app.handle_key(ctrl('6'));
    app.open_disk_for_test(0, 1, Side::Left.index());
    let other = VfsPath::in_session("modestwo", "other", "/");
    tick_until(&mut app, |app| shows(app, Side::Left, &other));
    assert_eq!(app.panels[0].return_view(), Some(ViewMode::Brief));
    assert_eq!(
        app.panels[0].view_mode,
        ViewMode::Brief,
        "FAR's GetPrevViewMode"
    );
    app.handle_key(ctrl('6'));
    assert_eq!(app.panels[0].view_mode, ViewMode::Provider(6));
    app.panels[0].go_parent().unwrap();
    tick_until(&mut app, |app| shows(app, Side::Left, &came_from));
    assert_eq!(app.panels[0].view_mode, ViewMode::Brief, "back as it was");
    assert_eq!(app.panels[0].return_view(), None);
    app.handle_key(ctrl('6'));
    assert_eq!(
        app.panels[0].view_mode,
        ViewMode::Brief,
        "Ctrl+6 on a disk: no mode there"
    );
    app.handle_key(ctrl('3'));
    assert_eq!(app.panels[0].view_mode, ViewMode::Full);
    assert_eq!(app.to_settings().left_view, "full");
}

#[test]
fn a_start_mode_at_ctrl_6_to_9_opens_only_where_the_plugin_has_a_mode() {
    let (_temp, app, _) = opened("modesfive", "six");
    assert_eq!(
        app.panels[0].view_mode,
        ViewMode::Provider(6),
        "its own mode"
    );
    let (_temp, app, _) = opened("modessix", "seven");
    assert_eq!(
        app.panels[0].view_mode,
        ViewMode::Brief,
        "none of its own at Ctrl+7: the view stays"
    );
}

#[test]
fn a_start_sort_opens_the_panel_so_and_the_sort_it_had_comes_back() {
    // FAR's `StartSortMode` / `StartSortOrder` (API 2) and `PopPlugin`.
    let (_temp, mut app, came_from) = opened("modessort", "sorted");
    let sort = app.panels[0].sort;
    assert_eq!(
        (sort.mode, sort.reverse),
        (crate::vfs::SortMode::Size, true)
    );
    app.panels[0].change_dir(&came_from, None).unwrap();
    let sort = app.panels[0].sort;
    assert_eq!(
        (sort.mode, sort.reverse),
        (crate::vfs::SortMode::Name, false),
        "the panel's own again"
    );
}

#[test]
fn a_plugin_without_modes_shows_the_programs() {
    let (_temp, mut app, _) = opened("modesthree", "untitled");
    app.handle_key(ctrl('3'));
    let (kinds, titles, ..) = drawn(&app);
    assert_eq!(kinds, *ViewMode::Full.spec_kinds(&app));
    assert!(titles.is_empty());
    app.handle_key(ctrl('6'));
    assert_eq!(app.panels[0].view_mode, ViewMode::Full, "no mode at Ctrl+6");
}

impl ViewMode {
    /// The program's own columns of this mode.
    fn spec_kinds(self, app: &App) -> Vec<ColumnKind> {
        (app.panel_modes.spec(self).panel.iter())
            .map(|column| column.kind)
            .collect()
    }
}

#[test]
fn a_folder_read_again_takes_the_plugins_look_again() {
    let (_temp, mut app, _) = opened("modesfour", "one");
    app.panels[0].look = crate::vfs::PanelLook::default();
    app.panels[0].reload();
    tick_until(&mut app, |app| app.panels[0].look.mode(3).is_some());
    assert_eq!(app.panels[0].look.title.as_deref(), Some("Probe one:/"));
}

#[test]
fn shift_f5_on_a_plugins_file_offers_the_other_panels_folder() {
    let (_temp, mut app, _came_from) = opened("modesshift", "one");
    let disk = tempfile::tempdir().unwrap();
    (app.panels[1].change_dir(&VfsPath::local(disk.path()), None)).unwrap();
    let panel = &mut app.panels[0];
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "readme.txt")
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::SHIFT));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the copy dialog");
    };
    let line = (form.items.iter())
        .find_map(|item| match &item.control {
            crate::app::form::Control::Edit(edit) => Some(edit.text().to_string()),
            _ => None,
        })
        .unwrap();
    // FAR's `ProcessCopyKeys` for a file with no real one: F5's target, not the name itself.
    assert_eq!(line, format!("{}\\", disk.path().display()));
}
