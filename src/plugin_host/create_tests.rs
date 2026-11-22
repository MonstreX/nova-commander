//! Shift+F1's new panel on the plugin's thread (FAR's `PluginPutFilesToNew`): `open_new` asked, none made
//! passing the press on (`Event::Key` with `pass`), one made getting the files put into its folder `/`
//! and closed, the end as `Event::Created` (or the put's failure, ending the press `serial`); a path
//! that cannot be handed over exactly sending nothing.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use crate::plugin_host::tests::{install_shipped, load, next_event, plugins_with_probe};
use crate::plugin_host::windows::{DialogMessage, DialogRequest, ItemValue};
use crate::plugin_host::{Event, PluginHost};

/// The archive's "Create archive" dialog's items by index (its `create_dialog`).
const ARC_PATH: usize = 8;
const OK: usize = 45;

/// The next event that ends Shift+F1's call or shows its dialog: windows without buttons and their end
/// let be, questions about the panels left unanswered (their reply dropped: none).
fn next_of_create(host: &PluginHost) -> Event {
    loop {
        match next_event(host) {
            Event::Busy { .. } | Event::Done { .. } | Event::PanelAsk { .. } => {}
            Event::Setting { .. } | Event::Synchro { .. } => {}
            event => return event,
        }
    }
}

/// A folder with `plain.txt` in it: the folder and the file.
fn one_file() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("plain.txt");
    std::fs::write(&file, "plain").unwrap();
    (folder, file)
}

/// The shipped archive plugin installed as `name`, its values `settings` (`key`, value).
fn archive_host(name: &str, settings: &[(&str, &str)]) -> (tempfile::TempDir, PluginHost) {
    let plugins = tempfile::tempdir().unwrap();
    install_shipped(plugins.path(), "archive", name);
    // These tests are a zip's: arclite's kept format (`update_arc_format_name`) zip, 7z by default.
    let zip = [("update_arc_format_name", "zip")];
    let values: serde_json::Map<String, serde_json::Value> = (zip.iter().chain(settings))
        .map(|(key, value)| {
            let key = format!("plugins.{name}.settings.{key}");
            (key, serde_json::Value::String((*value).to_string()))
        })
        .collect();
    let host = PluginHost::load(
        plugins.path(),
        &plugins.path().join("Logs"),
        &values,
        &|_| true,
    );
    assert_eq!(host.plugins.len(), 1, "{:?}", host.failures);
    (plugins, host)
}

/// The dialog's request and its line to the plugin, or what came instead.
fn create_dialog(host: &PluginHost) -> (DialogRequest, std::sync::mpsc::Sender<DialogMessage>) {
    match next_of_create(host) {
        Event::Dialog { request, talk, .. } => (request, talk),
        other => panic!("the plugin's dialog: {other:?}"),
    }
}

/// Every item's value as the dialog showed it, the archive's path typed as `path` when given.
fn values_of(request: &DialogRequest, path: Option<&Path>) -> Vec<ItemValue> {
    let mut values: Vec<ItemValue> = (request.items.iter())
        .map(|item| ItemValue {
            state: item.state,
            text: item.text.clone(),
        })
        .collect();
    if let Some(path) = path {
        values[ARC_PATH].text = path.display().to_string();
    }
    values
}

#[path = "create_dialog_host_tests.rs"]
mod dialog;

#[test]
fn a_plugin_that_makes_no_new_panel_passes_the_press_on() {
    let plugins = plugins_with_probe(|_| {});
    let host = load(plugins.path(), &|_| true);
    let (folder, file) = one_file();
    assert!(host.create(0, (&[file], folder.path()), (1, 42)));
    match next_of_create(&host) {
        Event::Key {
            plugin,
            side,
            serial,
            answer,
        } => {
            assert_eq!((plugin.as_str(), side, serial), ("probe", 1, 42));
            assert!(answer.pass, "the next plugin's");
            assert!(answer.go.is_none() && !answer.reread);
        }
        other => panic!("the press passed on: {other:?}"),
    }
}

#[test]
fn a_plugin_or_a_path_that_cannot_be_handed_over_sends_nothing() {
    let plugins = plugins_with_probe(|_| {});
    let host = load(plugins.path(), &|_| true);
    let (folder, file) = one_file();
    // An unpaired surrogate: no Unicode text names it.
    let broken = PathBuf::from(OsString::from_wide(&[u16::from(b'x'), 0xD800]));
    assert!(!host.create(5, (std::slice::from_ref(&file), folder.path()), (0, 1)));
    let some = [file.clone(), folder.path().join(&broken)];
    assert!(!host.create(0, (&some, folder.path()), (0, 2)), "a source");
    let inside = folder.path().join(&broken);
    assert!(
        !host.create(0, (std::slice::from_ref(&file), &inside), (0, 3)),
        "the folder"
    );
    // The jobs go in order: the first answer is this press's, none was sent before it.
    assert!(host.create(0, (&[file], folder.path()), (0, 4)));
    match next_of_create(&host) {
        Event::Key { serial, .. } => assert_eq!(serial, 4),
        other => panic!("the press passed on: {other:?}"),
    }
}

#[test]
fn the_archive_with_handle_create_off_makes_no_new_panel() {
    let (_plugins, host) = archive_host("arcoff", &[("handle_create", "0")]);
    let (folder, file) = one_file();
    assert!(host.create(0, (&[file], folder.path()), (0, 9)));
    match next_of_create(&host) {
        Event::Key {
            plugin,
            serial,
            answer,
            ..
        } => {
            assert_eq!((plugin.as_str(), serial), ("arcoff", 9));
            assert!(answer.pass);
        }
        other => panic!("the press passed on: {other:?}"),
    }
}

#[test]
fn a_new_panel_gets_the_files_and_its_end_is_told() {
    let (_plugins, host) = archive_host("arcon", &[]);
    let (folder, file) = one_file();
    // Esc in its dialog: put stopped, the end told as not done.
    assert!(host.create(0, (std::slice::from_ref(&file), folder.path()), (0, 11)));
    let (request, talk) = create_dialog(&host);
    assert_eq!(request.title, "Create archive", "put into the new panel");
    assert_eq!(
        request.items[ARC_PATH].text, "plain.txt.zip",
        "named after the file put"
    );
    drop(talk);
    match next_of_create(&host) {
        Event::Created {
            plugin,
            serial,
            done,
        } => assert_eq!((plugin.as_str(), serial, done), ("arcon", 11, false)),
        other => panic!("its end: {other:?}"),
    }
    assert!(!folder.path().join("plain.txt.zip").exists());
    // OK: the archive made of the file, the end told as done.
    assert!(host.create(0, (std::slice::from_ref(&file), folder.path()), (1, 12)));
    let (request, talk) = create_dialog(&host);
    assert_eq!(request.items[OK].text, "OK");
    let closed = DialogMessage::Closed {
        button: Some(OK),
        values: values_of(&request, None),
    };
    talk.send(closed).unwrap();
    match next_of_create(&host) {
        Event::Created {
            plugin,
            serial,
            done,
        } => assert_eq!((plugin.as_str(), serial, done), ("arcon", 12, true)),
        other => panic!("its end: {other:?}"),
    }
    let made = folder.path().join("plain.txt.zip");
    let Some(opened) = archive::support::opened(&made) else {
        panic!("no archive made");
    };
    let names: Vec<String> = (opened.items.iter())
        .map(|item| item.path.join("/"))
        .collect();
    assert_eq!(names, ["plain.txt"]);
    assert!(file.exists(), "copied, not moved");
}

#[test]
fn a_new_panels_put_that_fails_ends_its_press_and_the_plugin_goes_on() {
    let (_plugins, host) = archive_host("arcfail", &[]);
    let (folder, file) = one_file();
    assert!(host.create(0, (std::slice::from_ref(&file), folder.path()), (0, 21)));
    let (request, talk) = create_dialog(&host);
    // Its folder cannot be made: a file is where a folder would be.
    let under_a_file = file.join("sub").join("x.zip");
    let closed = DialogMessage::Closed {
        button: Some(OK),
        values: values_of(&request, Some(&under_a_file)),
    };
    talk.send(closed).unwrap();
    match next_of_create(&host) {
        Event::Failed {
            plugin,
            error,
            serial,
        } => {
            assert_eq!((plugin.as_str(), serial), ("arcfail", Some(21)));
            assert!(!error.is_empty());
        }
        other => panic!("the put's failure: {other:?}"),
    }
    // The panel closed after the failure: the next press gets a panel of its own and goes on.
    assert!(host.create(0, (std::slice::from_ref(&file), folder.path()), (0, 22)));
    let (_request, talk) = create_dialog(&host);
    drop(talk);
    assert!(matches!(
        next_of_create(&host),
        Event::Created {
            serial: 22,
            done: false,
            ..
        }
    ));
}
