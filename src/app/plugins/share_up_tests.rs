//! Up from a share's root without the network plugin: ".." and Ctrl+PgUp there open the drive menu
//! (FAR's `ChangeDisk`); a network drive's root with `PgUpChangeDisk` 2 too. The plugin's part is the
//! e2e stand's (`e2e_network_up_body`).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Overlay;
use crate::vfs::{Entry, VfsPath};

/// The program with its active panel at `root`, not read, ".." under the cursor.
fn at_root(root: &str) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    let panel = app.active_panel_mut();
    panel.path = VfsPath::local(Path::new(root));
    panel.entries = vec![Entry::parent()];
    panel.cursor = 0;
    (temp, app)
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
    crate::app::waiting::finish_waiting(app);
}

fn drive_menu_up(app: &App) -> bool {
    matches!(app.overlay, Some(Overlay::DriveMenu(_)))
}

/// Enter on ".." at a share's root with no plugin going up from it: the drive menu, the panel where it
/// was; Ctrl+PgUp there too, and nothing with "Use Ctrl+PgUp to change drive" off.
#[test]
fn without_the_plugin_a_shares_root_goes_to_the_drive_menu() {
    let share = r"\\server\share\";
    let (_temp, mut app) = at_root(share);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(drive_menu_up(&app), "FAR's ChangeDisk");
    assert_eq!(app.active_panel().path, VfsPath::local(Path::new(share)));
    let (_temp, mut app) = at_root(share);
    press(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
    assert!(drive_menu_up(&app));
    let (_temp, mut app) = at_root(share);
    app.prefs.ctrl_pg_up_changes_drive = 0;
    press(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
    assert!(app.overlay.is_none(), "off: nothing");
    app.sync_share_parent();
    assert!(
        !app.active_panel().share_parent(),
        "no plugin: a share's root shows no '..'"
    );
}

/// The shipped network plugin loaded: the panels show ".." at a share's root from then on; unloaded,
/// no more.
#[cfg(windows)]
#[test]
fn the_network_plugin_loaded_gives_a_shares_root_its_parent() {
    let plugins = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(plugins.path(), "network", "network");
    let (_temp, mut app) = at_root(r"C:\");
    let host = crate::plugin_host::PluginHost::load(
        plugins.path(),
        &plugins.path().join("Logs"),
        &serde_json::Map::new(),
        &|_| true,
    );
    app.set_plugins(host);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.plugins.host.prefix_plugin(SHARE_UP).is_none() {
        assert!(std::time::Instant::now() < deadline, "not loaded in time");
        app.tick();
        std::thread::yield_now();
    }
    app.sync_share_parent();
    assert!(
        app.panels
            .iter()
            .all(super::super::super::Panel::share_parent)
    );
    app.plugins.host = crate::plugin_host::PluginHost::default();
    app.sync_share_parent();
    assert!(!app.active_panel().share_parent(), "gone with the plugin");
}

/// The plugin's "not mine" (a share it does not go up from): the drive menu while the panel is as it
/// was; after the panel moved, or for an answer no longer waited for, nothing.
#[test]
fn a_share_the_plugin_leaves_opens_the_drive_menu_only_where_it_was() {
    let passed = || crate::plugin_host::KeyAnswer {
        pass: true,
        ..crate::plugin_host::KeyAnswer::default()
    };
    let up_key = KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL);
    let (_temp, mut app) = at_root(r"\\server\share\");
    let side = app.active;
    let serial = app.wait_for_plugin(WaitKind::ShareUp, "network", side, up_key);
    app.plugin_key_done("network", side.index(), serial, passed());
    crate::app::waiting::finish_waiting(&mut app);
    assert!(drive_menu_up(&app), "FAR's ChangeDisk");
    assert!(app.plugins.key_wait.is_none());

    let (_temp, mut app) = at_root(r"\\server\share\");
    let serial = app.wait_for_plugin(WaitKind::ShareUp, "network", side, up_key);
    app.active_panel_mut().path = VfsPath::local(Path::new(r"\\server\other\"));
    app.plugin_key_done("network", side.index(), serial, passed());
    crate::app::waiting::finish_waiting(&mut app);
    assert!(app.overlay.is_none(), "the panel moved: nothing");

    let (_temp, mut app) = at_root(r"\\server\share\");
    let serial = app.wait_for_plugin(WaitKind::ShareUp, "network", side, up_key);
    app.plugins.key_wait = None;
    app.plugin_key_done("network", side.index(), serial, passed());
    crate::app::waiting::finish_waiting(&mut app);
    assert!(app.overlay.is_none(), "not waited for any more: nothing");
}

/// A drive's root with `PgUpChangeDisk` 2: not a network drive here, the drive menu.
#[test]
fn a_drives_root_with_the_third_state_still_opens_the_drive_menu() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp
        .path()
        .ancestors()
        .last()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let (_temp, mut app) = at_root(&root);
    app.prefs.ctrl_pg_up_changes_drive = 2;
    press(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
    assert!(drive_menu_up(&app));
}

/// FAR's `NetRoot`: a share's root, a slash at its end or not; a folder in it, a drive, a long path's
/// drive are none.
#[test]
fn a_shares_root_is_named_as_far_names_it() {
    let share = |path: &str| crate::vfs::local::share_of(Path::new(path));
    assert_eq!(
        share(r"\\server\share"),
        Some(r"\\server\share".to_string())
    );
    assert_eq!(
        share(r"\\server\share\"),
        Some(r"\\server\share".to_string())
    );
    assert_eq!(
        share(r"\\?\UNC\server\share\"),
        Some(r"\\server\share".to_string())
    );
    assert_eq!(share(r"\\server\share\folder"), None);
    assert_eq!(share(r"C:\"), None);
    assert_eq!(share(r"\\?\C:\"), None);
}
