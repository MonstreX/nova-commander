//! Shift+F2 and Shift+F3 on a disk's panel (S6.2e, FAR's `PluginHostGetFiles` and `ProcessOneHostFile`)
//! through the program and the shipped plugin: the selected files one after another, each opened by the
//! plugin with `OFP_EXTRACT` / `OFP_COMMANDS`, its root got (arclite's Extract dialog for its first,
//! the next silent with the same answers) or tested ("Test archive files"); a file done unselected, one
//! that is no archive passed over, a stop ending the run.

use super::*;

/// The left panel's selection, sorted.
fn selection(app: &App) -> Vec<String> {
    let mut names: Vec<String> = app.panels[0].selected.iter().cloned().collect();
    names.sort();
    names
}

fn select(app: &mut App, names: &[&str]) {
    for name in names {
        app.panels[0].selected.insert((*name).to_string());
    }
}

/// Shift+F2 on two zips: arclite's Extract dialog once, its destination the other panel's folder;
/// both roots of two entries into folders of their names (a disk's archives at once, "separate" asked
/// neither way), the second silently with the first's answers; both unselected after.
#[test]
fn shift_f2_extracts_every_selected_archive_with_one_dialog() {
    let (_plugins, _files, out, mut app) = app_with_archives("diskextract");
    select(&mut app, &["a.zip", "c.zip"]);
    shift(&mut app, 2);
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    assert_eq!(plugin_dialog(&app), Some("Extract files"));
    key(&mut app, KeyCode::Enter);
    let last = out.path().join("c").join("note.txt");
    tick_until(&mut app, |app| settled(app) && last.exists());
    let read = |parts: &[&str]| {
        let path = (parts.iter()).fold(out.path().to_path_buf(), |at, part| at.join(part));
        std::fs::read(path).unwrap()
    };
    assert_eq!(read(&["a", "docs", "readme.txt"]), b"hello");
    assert_eq!(read(&["a", "top.txt"]), b"top");
    assert_eq!(read(&["c", "note.txt"]), b"note");
    assert!(out.path().join("c").join("inner.zip").exists());
    assert!(selection(&app).is_empty(), "each done unselected");
}

/// A file that is no archive is passed over and stays selected; Esc in the dialog ends the run: the
/// archive after it is not asked about, and stays selected.
#[test]
fn shift_f2_passes_over_a_file_no_plugin_opens_and_esc_ends_the_run() {
    let (_plugins, _files, out, mut app) = app_with_archives("diskextractesc");
    select(&mut app, &["a.zip", "c.zip", "plain.txt"]);
    shift(&mut app, 2);
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    key(&mut app, KeyCode::Esc);
    tick_until(&mut app, settled);
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0, "nothing");
    assert_eq!(selection(&app), ["a.zip", "c.zip", "plain.txt"]);
    // b.docx (not of arclite's masks: the plugin passes it, as for Enter), plain.txt (no archive),
    // then c.zip: only the last asked.
    app.panels[0].selected.clear();
    select(&mut app, &["b.docx", "c.zip", "plain.txt"]);
    shift(&mut app, 2);
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    key(&mut app, KeyCode::Enter);
    let made = out.path().join("c").join("note.txt");
    tick_until(&mut app, |app| settled(app) && made.exists());
    assert_eq!(selection(&app), ["b.docx", "plain.txt"], "not opened: kept");
}

/// Nothing selected and the other panel hidden: the file's name without its extension, made whole from
/// the panel's folder (FAR's `strDestPath`), then a folder of the archive's own under it (arclite's).
#[test]
fn shift_f2_with_the_other_panel_hidden_extracts_beside_the_archive() {
    let (_plugins, files, out, mut app) = app_with_archives("diskextracthidden");
    app.panels[1].visible = false;
    point(&mut app, "a.zip");
    shift(&mut app, 2);
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    key(&mut app, KeyCode::Enter);
    let made = files.path().join("a").join("a").join("top.txt");
    tick_until(&mut app, |app| settled(app) && made.exists());
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
}

/// Shift+F3 on two archives: arclite's menu and "Archive test complete" for each, both unselected; Esc
/// in the menu ends the run (FAR's `ProcessOneHostFile` 0): the next is not asked about.
#[test]
fn shift_f3_tests_every_selected_archive_until_one_is_not_done() {
    let (_plugins, _files, _out, mut app) = app_with_archives("disktest");
    select(&mut app, &["a.zip", "c.zip"]);
    shift(&mut app, 3);
    for _ in 0..2 {
        tick_until(&mut app, |app| own_menu(app).is_some());
        assert_eq!(own_menu(&app).unwrap().1, ["Test archive files"]);
        key(&mut app, KeyCode::Enter);
        tick_until(&mut app, |app| message(app).is_some());
        assert_eq!(message(&app).unwrap().1, ["Archive test complete"]);
        key(&mut app, KeyCode::Enter);
    }
    tick_until(&mut app, settled);
    assert!(selection(&app).is_empty(), "both done");
    select(&mut app, &["a.zip", "c.zip"]);
    shift(&mut app, 3);
    tick_until(&mut app, |app| own_menu(app).is_some());
    key(&mut app, KeyCode::Esc);
    tick_until(&mut app, settled);
    assert_eq!(selection(&app), ["a.zip", "c.zip"], "the run ended");
}

/// A folder among the selected is offered to no plugin (FAR's `OpenPluginForFile`) and stays selected;
/// the other panel hidden but something selected: its folder still the destination (FAR's rule).
#[test]
fn a_selected_folder_is_passed_over_and_a_hidden_panel_takes_the_files_when_selected() {
    let (_plugins, files, out, mut app) = app_with_archives("diskfolder");
    std::fs::create_dir(files.path().join("sub.zip")).unwrap();
    app.panels[0].reload();
    tick_until(&mut app, |app| app.panels[0].reading().is_none());
    app.panels[1].visible = false;
    select(&mut app, &["sub.zip", "a.zip"]);
    shift(&mut app, 2);
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    key(&mut app, KeyCode::Enter);
    let made = out.path().join("a").join("top.txt");
    tick_until(&mut app, |app| settled(app) && made.exists());
    assert_eq!(selection(&app), ["sub.zip"], "a folder: not offered");
}

/// The run goes on only while its panel stays on its folder: gone elsewhere under the first archive's
/// dialog, the next archive is not asked about.
#[test]
fn the_run_ends_when_its_panel_leaves_the_folder() {
    let (_plugins, _files, out, mut app) = app_with_archives("diskleft");
    select(&mut app, &["a.zip", "c.zip"]);
    shift(&mut app, 2);
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    app.panels[0]
        .change_dir(&VfsPath::local(out.path()), None)
        .unwrap();
    key(&mut app, KeyCode::Enter);
    let first = out.path().join("a").join("top.txt");
    tick_until(&mut app, |app| settled(app) && first.exists());
    assert!(!out.path().join("c").exists(), "c.zip not asked about");
}

/// "Go to target directory" kept on: a lone archive of one entry goes into the other panel's folder and
/// the active panel goes there, onto that entry (arclite's `panel_go_to_file` at the top level).
#[test]
fn shift_f2_goes_to_the_target_when_asked() {
    let plugins = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(plugins.path(), "archive", "diskgoto");
    let kept: serde_json::Map<String, serde_json::Value> = [(
        "plugins.diskgoto.settings.extract_open_dir".to_string(),
        serde_json::Value::String("1".to_string()),
    )]
    .into_iter()
    .collect();
    let host = crate::plugin_host::PluginHost::load(
        plugins.path(),
        &plugins.path().join("Logs"),
        &kept,
        &|_| true,
    );
    let files = tempfile::tempdir().unwrap();
    let one = zip_of(&[("only.txt", b"only")]);
    std::fs::write(files.path().join("one.zip"), one).unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut app = app_in(plugins.path());
    app.set_plugins(host);
    app.panels[0]
        .change_dir(&VfsPath::local(files.path()), None)
        .unwrap();
    app.panels[1]
        .change_dir(&VfsPath::local(out.path()), None)
        .unwrap();
    app.active = Side::Left;
    tick_until(&mut app, |app| {
        app.panels.iter().all(|panel| panel.reading().is_none())
            && (app.plugins.host.plugins.iter()).any(|plugin| plugin.file_masks == ["*"])
    });
    point(&mut app, "one.zip");
    shift(&mut app, 2);
    tick_until(&mut app, |app| plugin_dialog(app).is_some());
    key(&mut app, KeyCode::Enter);
    let there = VfsPath::local(out.path());
    tick_until(&mut app, |app| {
        settled(app)
            && shows(app, Side::Left, &there)
            && (app.panels[0].current()).is_some_and(|entry| entry.name == "only.txt")
    });
    assert_eq!(std::fs::read(out.path().join("only.txt")).unwrap(), b"only");
}

/// Shift+F3 with nothing selected: the archive under the cursor tested ("Archive test complete"), the
/// run then over with nothing selected after.
#[test]
fn shift_f3_with_nothing_selected_tests_the_archive_under_the_cursor() {
    let (_plugins, _files, _out, mut app) = app_with_archives("disktestone");
    point(&mut app, "c.zip");
    shift(&mut app, 3);
    tick_until(&mut app, |app| own_menu(app).is_some());
    assert_eq!(own_menu(&app).unwrap().1, ["Test archive files"]);
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| message(app).is_some());
    assert_eq!(message(&app).unwrap().1, ["Archive test complete"]);
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, settled);
    assert!(selection(&app).is_empty());
    assert_eq!(
        app.panels[0].current().map(|entry| entry.name.as_str()),
        Some("c.zip"),
        "the cursor stays"
    );
}

#[path = "disk_host_tests.rs"]
mod host;

#[path = "disk_run_tests.rs"]
mod run;
