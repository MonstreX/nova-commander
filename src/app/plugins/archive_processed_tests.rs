//! Shift+F3's end as the program hears it (FAR's `ProcessHostFileW` returned, S6.2d): the answer the
//! shipped plugin never gives for a panel's call (`HOST_FILE_CHANGED`) and the others, crafted as the
//! plugin's thread sends them, for a wait set up as Shift+F3 sets it: the selection gone and both panels
//! read again when it changed something; only the entries handed unselected (Ctrl+M back) when it says
//! so; a failure said with the plugin's title; an answer no longer waited for reads only its plugin's
//! panels again.

use std::collections::BTreeSet;

use super::*;
use crate::app::plugins::keys::WaitKind;
use crate::plugin_host::Event;
use crate::plugin_host::getting::{HOST_FILE_CHANGED, HOST_FILE_UNSELECT};

/// `a.zip` opened on the left panel at its root, `top.txt` and `docs` selected.
fn opened(name: &str) -> (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir, App) {
    let (plugins, files, out, mut app) = app_with_archives(name);
    point(&mut app, "a.zip");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    app.panels[0].selected.insert("top.txt".to_string());
    app.panels[0].selected.insert("docs".to_string());
    (plugins, files, out, app)
}

/// Shift+F3's wait for the panel `side` with the entries `names` handed: its serial.
fn host_file_wait(app: &mut App, plugin: &str, side: Side, names: &[&str]) -> u64 {
    let names = names.iter().map(|name| (*name).to_string()).collect();
    let kind = WaitKind::Get(Getting::HostFile { names });
    let key = KeyEvent::new(KeyCode::F(3), KeyModifiers::SHIFT);
    app.wait_for_plugin(kind, plugin, side, key)
}

fn processed(plugin: &str, serial: u64, result: Result<u32, String>) -> Event {
    Event::Processed {
        plugin: plugin.to_string(),
        serial,
        result,
    }
}

fn selected(app: &App, side: Side) -> BTreeSet<String> {
    app.panels[side.index()].selected.to_set()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

/// The program's error window up: its title and lines.
fn error_shown(app: &App) -> Option<(String, Vec<String>)> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Message {
            title,
            lines,
            error: true,
        })) => Some((title.clone(), lines.clone())),
        _ => None,
    }
}

/// The left panel made stale: an entry no reading of the archive has, gone once it is read again.
fn stale(app: &mut App) {
    let panel = &mut app.panels[0];
    let mut phantom = (panel.entries.iter())
        .find(|entry| entry.name == "top.txt")
        .cloned()
        .unwrap();
    phantom.name = "phantom.txt".to_string();
    panel.entries.push(phantom);
}

/// The left panel read again (or being read) since [`stale`].
fn left_read_again(app: &App) -> bool {
    app.panels[0].is_reading() || !names(app).contains(&"phantom.txt".to_string())
}

/// `late.txt` written into the right panel's folder, seen only by a reading of that panel.
fn late_file(app: &App, out: &tempfile::TempDir) {
    std::fs::write(out.path().join("late.txt"), "late").unwrap();
    assert!(!right_read_again(app));
}

/// The right panel read again (or being read) since [`late_file`].
fn right_read_again(app: &App) -> bool {
    app.panels[1].is_reading()
        || (app.panels[1].entries.iter()).any(|entry| entry.name == "late.txt")
}

/// `HOST_FILE_CHANGED` (FAR's TRUE): the whole selection goes, not only the entries handed, even with
/// `HOST_FILE_UNSELECT` too (Ctrl+M brings it back), and both panels are read again; the wait is over
/// and the keys held meanwhile go on.
#[test]
fn changed_clears_the_selection_and_reads_both_panels_again() {
    let name = "archiveprocchanged";
    let (_plugins, _files, out, mut app) = opened(name);
    late_file(&app, &out);
    stale(&mut app);
    assert!(!left_read_again(&app));
    let serial = host_file_wait(&mut app, name, Side::Left, &["top.txt"]);
    // Tab held while waiting: it goes on to the active disk's panel once the wait is over.
    app.active = Side::Right;
    let wait = app.plugins.key_wait.as_mut().unwrap();
    wait.held.keys.push(KeyEvent::from(KeyCode::Tab));
    let bits = HOST_FILE_CHANGED | HOST_FILE_UNSELECT;
    app.plugin_processed(processed(name, serial, Ok(bits)));
    assert!(selected(&app, Side::Left).is_empty(), "the selection gone");
    assert!(left_read_again(&app), "the plugin's panel read again");
    assert!(right_read_again(&app), "the other panel read again too");
    assert!(app.plugins.key_wait.is_none(), "the wait over");
    assert!(app.overlay.is_none(), "nothing to tell");
    assert_eq!(app.active, Side::Left, "the key held went on");
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    app.panels[0].restore_selection();
    assert_eq!(
        selected(&app, Side::Left),
        set(&["docs", "top.txt"]),
        "Ctrl+M: the selection before"
    );
}

/// `HOST_FILE_UNSELECT` alone (arclite after testing): only the entries handed are unselected, the
/// selection before saved for Ctrl+M; nothing is read again.
#[test]
fn unselect_unselects_only_the_handed_entries_and_ctrl_m_brings_them_back() {
    let name = "archiveprocunselect";
    let (_plugins, _files, out, mut app) = opened(name);
    app.panels[0].selected.insert("phantom.txt".to_string());
    late_file(&app, &out);
    stale(&mut app);
    let serial = host_file_wait(&mut app, name, Side::Left, &["top.txt", "phantom.txt"]);
    app.plugin_processed(processed(name, serial, Ok(HOST_FILE_UNSELECT)));
    assert_eq!(
        selected(&app, Side::Left),
        set(&["docs"]),
        "the others kept"
    );
    assert!(!left_read_again(&app), "the plugin's panel not read again");
    assert!(!right_read_again(&app), "nor the other one");
    assert!(app.plugins.key_wait.is_none(), "the wait over");
    assert!(app.overlay.is_none());
    app.panels[0].restore_selection();
    assert_eq!(
        selected(&app, Side::Left),
        set(&["docs", "phantom.txt", "top.txt"]),
        "Ctrl+M: the selection before"
    );
}

/// Nothing done (0, also the user's stop): the selection stays, nothing is read again, the wait is over.
#[test]
fn nothing_done_keeps_the_selection() {
    let name = "archiveprocnothing";
    let (_plugins, _files, out, mut app) = opened(name);
    late_file(&app, &out);
    stale(&mut app);
    let serial = host_file_wait(&mut app, name, Side::Left, &["top.txt"]);
    app.plugin_processed(processed(name, serial, Ok(0)));
    assert_eq!(selected(&app, Side::Left), set(&["docs", "top.txt"]));
    assert!(!left_read_again(&app));
    assert!(!right_read_again(&app));
    assert!(app.plugins.key_wait.is_none(), "the wait over");
    assert!(app.overlay.is_none());
}

/// A failure: the plugin's reason in an error window titled with its title; the selection stays and
/// nothing is read again; the wait is over.
#[test]
fn a_failure_is_said_with_the_plugins_title() {
    let name = "archiveprocfailed";
    let (_plugins, _files, out, mut app) = opened(name);
    late_file(&app, &out);
    stale(&mut app);
    let serial = host_file_wait(&mut app, name, Side::Left, &["top.txt"]);
    app.plugin_processed(processed(name, serial, Err("no room".to_string())));
    let title = app.plugin_title(name);
    assert!(!title.is_empty());
    assert_eq!(
        error_shown(&app),
        Some((title, vec!["no room".to_string()]))
    );
    assert_eq!(selected(&app, Side::Left), set(&["docs", "top.txt"]));
    assert!(!left_read_again(&app));
    assert!(!right_read_again(&app));
    assert!(app.plugins.key_wait.is_none(), "the wait over");
}

/// A wait let go by a window of its call (`key_late`): its answer applied all the same.
#[test]
fn a_wait_let_go_still_has_its_entries_unselected() {
    let name = "archiveproclate";
    let (_plugins, _files, _out, mut app) = opened(name);
    let serial = host_file_wait(&mut app, name, Side::Left, &["docs"]);
    let wait = app.plugins.key_wait.take().unwrap();
    app.plugins.key_late.push(wait);
    app.plugin_processed(processed(name, serial, Ok(HOST_FILE_UNSELECT)));
    assert_eq!(selected(&app, Side::Left), set(&["top.txt"]));
    assert!(app.plugins.key_late.is_empty(), "taken");
}

/// The panel gone elsewhere since the press, or a wait that is not Shift+F3's: its selection is not
/// the plugin's to change (both panels still read again when it changed something).
#[test]
fn another_folder_or_another_wait_keeps_the_selection() {
    let name = "archiveprocelsewhere";
    let (_plugins, files, out, mut app) = opened(name);
    std::fs::write(out.path().join("o.txt"), "o").unwrap();
    // Shift+F3 pressed with the right panel on `out`, which then went to `files`.
    let serial = host_file_wait(&mut app, name, Side::Right, &["plain.txt"]);
    app.panels[1]
        .change_dir(&VfsPath::local(files.path()), None)
        .unwrap();
    tick_until(&mut app, |app| app.panels[1].reading().is_none());
    app.panels[1].selected.insert("plain.txt".to_string());
    app.plugin_processed(processed(name, serial, Ok(HOST_FILE_UNSELECT)));
    assert_eq!(selected(&app, Side::Right), set(&["plain.txt"]), "unselect");
    // Pressed on `files`, gone back to `out`.
    let serial = host_file_wait(&mut app, name, Side::Right, &["o.txt"]);
    app.panels[1]
        .change_dir(&VfsPath::local(out.path()), None)
        .unwrap();
    tick_until(&mut app, |app| app.panels[1].reading().is_none());
    app.panels[1].selected.insert("o.txt".to_string());
    stale(&mut app);
    app.plugin_processed(processed(name, serial, Ok(HOST_FILE_CHANGED)));
    assert!(left_read_again(&app), "read again all the same");
    tick_until(&mut app, |app| app.panels[1].reading().is_none());
    assert_eq!(selected(&app, Side::Right), set(&["o.txt"]), "changed");
    let serial = app.wait_for_plugin(
        WaitKind::Key,
        name,
        Side::Left,
        KeyEvent::from(KeyCode::Char('x')),
    );
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    app.panels[0].selected.insert("top.txt".to_string());
    app.plugin_processed(processed(name, serial, Ok(HOST_FILE_UNSELECT)));
    assert!(
        selected(&app, Side::Left).contains("top.txt"),
        "a panel key's wait"
    );
}

/// An answer no longer waited for (Esc let it go): only its plugin's panels are read again, and only
/// when it changed something; no selection changes; a wait for another press stays; its failure said.
#[test]
fn an_answer_not_waited_for_reads_only_its_plugins_panels_again() {
    let name = "archiveprocgone";
    let (_plugins, _files, out, mut app) = opened(name);
    let kept = host_file_wait(&mut app, name, Side::Left, &["top.txt"]);
    let gone = kept + 1000;
    late_file(&app, &out);
    stale(&mut app);
    app.plugin_processed(processed(name, gone, Ok(HOST_FILE_UNSELECT)));
    assert!(!left_read_again(&app), "nothing changed: not read again");
    app.plugin_processed(processed("someotherplugin", gone, Ok(HOST_FILE_CHANGED)));
    assert!(!left_read_again(&app), "another plugin's answer");
    assert_eq!(selected(&app, Side::Left), set(&["docs", "top.txt"]));
    app.plugin_processed(processed(name, gone, Ok(HOST_FILE_CHANGED)));
    assert!(left_read_again(&app), "the plugin's panel read again");
    assert!(!right_read_again(&app), "a disk's panel not read again");
    assert_eq!(
        selected(&app, Side::Left),
        set(&["docs", "top.txt"]),
        "the selection not the late answer's"
    );
    assert_eq!(
        (app.plugins.key_wait.as_ref()).map(|wait| wait.serial),
        Some(kept),
        "the wait for another press stays"
    );
    assert!(app.overlay.is_none());
    app.plugin_processed(processed(name, gone, Err("no room".to_string())));
    let title = app.plugin_title(name);
    assert_eq!(
        error_shown(&app),
        Some((title, vec!["no room".to_string()]))
    );
}
