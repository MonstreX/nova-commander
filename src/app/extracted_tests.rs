//! F3 / F4 on a plugin's file (review R55), on the probe: the file copied for the viewer and the editor,
//! the copy's folder gone once they close, an edited copy put back in the plugin's panel. Each test names
//! its probe alone: the providers' list is the program's.
#![cfg(windows)]

use std::io::Read;

use super::super::fileops::tests::app_in;
use super::super::waiting::finish_waiting;
use super::*;
use crate::plugin_host::tests::{install_probe, load as load_plugins};

/// The probe as plugin `name`, its panel "one" in the active panel, the cursor on `readme.txt`.
fn on_probes_file(name: &str) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    let manifest = format!(
        r#"{{"name": "{name}", "version": "1", "api": {}, "library": "probe"}}"#,
        nc_plugin_api::API_VERSION
    );
    install_probe(temp.path(), name, &manifest);
    let mut app = app_in(temp.path());
    app.set_plugins(load_plugins(temp.path(), &|_| true));
    let panel = app.active_panel_mut();
    panel
        .change_dir(&VfsPath::in_session(name, "one", "/"), None)
        .unwrap();
    panel.cursor = (panel.entries.iter())
        .position(|entry| entry.name == "readme.txt")
        .expect("the probe's file");
    (temp, app)
}

fn read_back(name: &str) -> String {
    let at = VfsPath::in_session(name, "one", "/readme.txt");
    let mut text = String::new();
    vfs::provider_for(&at)
        .unwrap()
        .open_read(&at)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn f3_views_a_copy_of_a_plugins_file_and_its_folder_goes_after() {
    let (_temp, mut app) = on_probes_file("extractview");
    assert!(app.open_extracted(false));
    finish_waiting(&mut app);
    assert!(
        matches!(app.screen, Screen::Viewer(_)),
        "the viewer on the copy"
    );
    let folder = app.extracted[0].folder.clone();
    assert_eq!(
        std::fs::read_to_string(&app.extracted[0].copy).unwrap(),
        "Hello probe!"
    );
    app.screen = Screen::Panels;
    assert!(!app.pump_extracted(), "nothing to put back");
    assert!(app.extracted.is_empty());
    assert!(!folder.exists(), "the copy's folder went");
}

#[test]
fn f4_puts_an_edited_copy_back_and_one_not_saved_is_left_alone() {
    let (_temp, mut app) = on_probes_file("extractedit");
    assert!(app.open_extracted(true));
    finish_waiting(&mut app);
    assert!(matches!(app.screen, Screen::Editor(_)));
    let copy = app.extracted[0].copy.clone();
    // Saved by the editor: another length and time.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&copy, "Changed text").unwrap();
    app.screen = Screen::Panels;
    assert!(app.pump_extracted(), "put back");
    finish_waiting(&mut app);
    assert_eq!(read_back("extractedit"), "Changed text");
    assert!(!copy.exists(), "its folder went once it was back");

    assert!(app.open_extracted(true));
    finish_waiting(&mut app);
    app.screen = Screen::Panels;
    assert!(!app.pump_extracted(), "not saved: nothing goes back");
    assert_eq!(read_back("extractedit"), "Changed text");
}

/// API 8's `PANEL_PUTS_ON_SAVE`: a save in the editor puts the copy back at once (the editor and the copy
/// staying), its time and length then counted as made once taken; a panel without it waits for the
/// editor to be left.
#[test]
fn a_save_puts_the_copy_back_when_its_panel_asks() {
    let (_temp, mut app) = on_probes_file("extractsave");
    assert!(app.open_extracted(true));
    finish_waiting(&mut app);
    let copy = app.extracted[0].copy.clone();
    let made = app.extracted[0].made;
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&copy, "Saved once").unwrap();
    app.active_panel_mut().look.takes_paths = true;
    app.put_on_save(&copy);
    assert!(app.plugins.putting.is_none(), "no put at each save asked");
    app.active_panel_mut().look.puts_on_save = true;
    app.put_on_save(&copy.with_file_name("other.txt"));
    assert!(app.plugins.putting.is_none(), "not a copy the editor has");
    app.put_on_save(&copy);
    let putting = app.plugins.putting.clone().expect("put back now");
    let (sent, now) = putting.saved.expect("a saved copy");
    assert_eq!((sent.as_path(), putting.edited), (copy.as_path(), None));
    assert_ne!(now, made);
    app.put_ended("extractsave", true);
    assert_eq!(app.extracted[0].made, now, "taken: made now");
    assert!(copy.exists() && matches!(app.screen, Screen::Editor(_)));
}

/// The editor left by its "Save" as it closes: the panels are back before the save's put ends; the copy
/// waits for it (no "kept" error from a second put), then goes as one taken.
#[test]
fn a_save_as_the_editor_closes_waits_for_its_put() {
    let (_temp, mut app) = on_probes_file("extractsave");
    assert!(app.open_extracted(true));
    finish_waiting(&mut app);
    let copy = app.extracted[0].copy.clone();
    let folder = app.extracted[0].folder.clone();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&copy, "Saved as it closed").unwrap();
    app.active_panel_mut().look.takes_paths = true;
    app.active_panel_mut().look.puts_on_save = true;
    app.put_on_save(&copy);
    assert!(app.plugins.putting.is_some(), "the save's put under way");
    app.screen = Screen::Panels;
    assert!(!app.pump_extracted(), "it waits for the put");
    assert!(app.overlay.is_none(), "no error shown");
    assert_eq!(app.extracted.len(), 1);
    app.put_ended("extractsave", true);
    assert!(!app.pump_extracted(), "taken: nothing more to put");
    assert!(app.extracted.is_empty() && !folder.exists());
}

#[test]
fn a_file_on_a_disk_is_not_copied() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "x").unwrap();
    let mut app = app_in(temp.path());
    app.active_panel_mut().reload();
    assert!(
        !app.open_extracted(false),
        "the viewer opens it where it is"
    );
}

/// R7 review C5: a copy's folder is made new, never one an earlier run left with the same number (a kept
/// edited copy in it stays as it was).
#[test]
fn a_copys_folder_is_never_one_left_there() {
    let temp = tempfile::tempdir().unwrap();
    let first = new_folder_in(temp.path()).unwrap();
    let name = first.file_name().unwrap().to_string_lossy().into_owned();
    let (stem, number) = name.rsplit_once('-').unwrap();
    let number: u64 = number.parse().unwrap();
    // The next numbers taken by an earlier run's folders, each with its kept copy.
    for next in number + 1..number + 50 {
        let left = temp.path().join(format!("{stem}-{next}"));
        std::fs::create_dir(&left).unwrap();
        std::fs::write(left.join("kept.txt"), "kept").unwrap();
    }
    let made = new_folder_in(temp.path()).unwrap();
    assert_eq!(std::fs::read_dir(&made).unwrap().count(), 0, "made new");
    for next in number + 1..number + 50 {
        let kept = temp.path().join(format!("{stem}-{next}")).join("kept.txt");
        assert_eq!(std::fs::read_to_string(kept).unwrap(), "kept");
    }
}

/// R7 review D1: Shift+F4's new file on a plugin's panel is edited as a copy not there yet; saved, it
/// goes to the panel; not saved, nothing does; a name that is no entry's of the panel's folder is
/// refused.
#[test]
fn a_new_file_on_a_plugins_panel_goes_to_it_once_saved() {
    let (_temp, mut app) = on_probes_file("extractnew");
    app.edit_new_copy("fresh.txt").unwrap();
    let Screen::Editor(editor) = &app.screen else {
        panic!("the editor");
    };
    let copy = editor.path.clone();
    assert_eq!(app.extracted[0].copy, copy);
    assert!(!copy.exists(), "made by its save");
    std::fs::write(&copy, "new text").unwrap();
    app.screen = Screen::Panels;
    assert!(app.pump_extracted(), "put back");
    finish_waiting(&mut app);
    let at = VfsPath::in_session("extractnew", "one", "/fresh.txt");
    let mut text = String::new();
    let provider = vfs::provider_for(&at).unwrap();
    provider
        .open_read(&at)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "new text");
    app.edit_new_copy("unsaved.txt").unwrap();
    let folder = app.extracted[0].folder.clone();
    app.screen = Screen::Panels;
    assert!(!app.pump_extracted(), "not saved: nothing goes");
    assert!(!folder.exists());
    assert!(app.edit_new_copy("a/b.txt").is_err());
}

#[path = "extracted_archive_tests.rs"]
mod archive;
