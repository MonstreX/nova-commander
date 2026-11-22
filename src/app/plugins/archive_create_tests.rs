//! New archives (S6.2c, FAR's `PluginPutFilesToNew` with arclite's `put_files` of an archive not open)
//! through the program and the shipped plugin: Shift+F1 on a disk's panel asks arclite's "Create
//! archive" (not the program's copy dialog), OK writes the zip beside the files, the cursor on it, the
//! selection gone; Esc makes nothing and keeps the selection.

use super::*;

/// The plugin's dialog up: its title.
fn create_dialog(app: &App) -> Option<String> {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Plugin(PluginForm::Dialog),
        })) => Some(form.title.clone()),
        _ => None,
    }
}

fn shift_f1(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::SHIFT));
}

#[test]
fn shift_f1_makes_a_7z_by_arclites_dialog() {
    let (_plugins, files, _out, mut app) = app_with_archives("archivecreate");
    point(&mut app, "plain.txt");
    app.panels[0].selected.insert("plain.txt".to_string());
    app.panels[0].selected.insert("b.docx".to_string());
    shift_f1(&mut app);
    tick_until(&mut app, |app| create_dialog(app).is_some());
    assert_eq!(create_dialog(&app).as_deref(), Some("Create archive"));
    // Esc: nothing made, the selection kept.
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        app.plugins.key_wait.is_none() && app.plugins.key_late.is_empty() && app.overlay.is_none()
    });
    let folder_name = files
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let made = files.path().join(format!("{folder_name}.7z"));
    assert!(!made.exists());
    assert_eq!(app.panels[0].selected.len(), 2);
    // OK: the folder's name with arclite's default format's extension (7z), both files in it.
    shift_f1(&mut app);
    tick_until(&mut app, |app| create_dialog(app).is_some());
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    // Its end heard: neither waited for nor a late wait (the plugin's window may let the keys go).
    tick_until(&mut app, |app| {
        app.plugins.key_wait.is_none()
            && app.plugins.key_late.is_empty()
            && made.exists()
            && app.panels[0].reading().is_none()
            && names(app).contains(&format!("{folder_name}.7z"))
    });
    assert!(app.panels[0].selected.is_empty(), "FAR's ClearSelection");
    assert_eq!(
        app.panels[0].current().map(|entry| entry.name.clone()),
        Some(format!("{folder_name}.7z")),
        "arclite's panel_go_to_file"
    );
    assert!(files.path().join("plain.txt").exists(), "copied, not moved");
    assert_eq!(&std::fs::read(&made).unwrap()[..2], b"7z");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    tick_until(&mut app, |app| in_archive(app, "/"));
    let mut inside = names(&app);
    inside.sort();
    assert_eq!(inside, ["b.docx", "plain.txt"]);
}

/// A plugin that makes no new panel passes Shift+F1 on (FAR's `OpenFilePlugin` asks the next): the
/// first archive plugin has "Handle archive creation" off, the second asks "Create archive".
#[test]
fn shift_f1_goes_on_to_the_next_plugin_when_one_makes_no_panel() {
    let plugins = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(plugins.path(), "archive", "arcoff");
    crate::plugin_host::tests::install_shipped(plugins.path(), "archive", "arcon");
    let off: serde_json::Map<String, serde_json::Value> = [(
        "plugins.arcoff.settings.handle_create".to_string(),
        serde_json::Value::String("0".to_string()),
    )]
    .into_iter()
    .collect();
    let host = crate::plugin_host::PluginHost::load(
        plugins.path(),
        &plugins.path().join("Logs"),
        &off,
        &|_| true,
    );
    assert_eq!(host.plugins.len(), 2, "{:?}", host.failures);
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("one.txt"), "one").unwrap();
    let mut app = app_in(plugins.path());
    app.set_plugins(host);
    app.panels[0]
        .change_dir(&VfsPath::local(files.path()), None)
        .unwrap();
    app.active = Side::Left;
    tick_until(&mut app, |app| {
        app.panels[0].reading().is_none()
            && (app.plugins.host.plugins.iter()).all(|plugin| plugin.file_masks == ["*"])
    });
    let first = app.plugins.host.plugins[0].name.clone();
    assert_eq!(first, "arcoff", "asked first");
    point(&mut app, "one.txt");
    shift_f1(&mut app);
    tick_until(&mut app, |app| create_dialog(app).is_some());
    let asking = app
        .plugins
        .key_wait
        .as_ref()
        .map(|wait| wait.plugin.clone());
    assert_eq!(
        asking.as_deref(),
        Some("arcon"),
        "the second plugin's dialog"
    );
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    tick_until(&mut app, |app| {
        app.plugins.key_wait.is_none() && app.plugins.key_late.is_empty()
    });
    assert!(!files.path().join("one.txt.zip").exists());
}

/// Shift+F1 on ".." alone (nothing under the cursor to take) asks nothing.
#[test]
fn shift_f1_on_dot_dot_does_nothing() {
    let (_plugins, _files, _out, mut app) = app_with_archives("archivecreatenone");
    app.panels[0].cursor = 0;
    assert!(app.panels[0].current().is_some_and(|entry| entry.is_parent));
    shift_f1(&mut app);
    assert!(app.plugins.key_wait.is_none());
    assert!(create_dialog(&app).is_none());
}
