//! Shift+F2 / Shift+F3 on a disk's file at the plugin host with the shipped archive plugin (S6.2e,
//! `PluginHost::disk_file`, `Job::DiskFile`): a file not of arclite's include masks passed on for both,
//! as Enter (only Ctrl+PgDn may pass the masks by); a silent get writing the root with no dialog; the
//! test's end told as done; "Delete archive after extraction" at the top level deleting the archive with
//! no panel closed; a plugin or a path that cannot be handed over sending nothing. The program's masks
//! and confirmations answered as the program answers them; its other panel requests left unanswered.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use super::*;
use crate::plugin_host::disk_files::{DiskEnd, DiskUse};
use crate::plugin_host::panel_requests::{PanelAction, PanelAnswer, PanelRequest};
use crate::plugin_host::tests::next_event;
use crate::plugin_host::windows::{Answer, DialogMessage, ItemValue, Request};
use crate::plugin_host::{Event, PluginHost};

/// arclite's Extract dialog's "Delete archive after extraction" and OK (its `extract_dialog`).
const DELETE_ARCHIVE: usize = 11;
const EXTRACT_OK: usize = 17;

/// The shipped archive plugin installed as `name` with its values `settings`; a program for the
/// masks; a folder with `a.zip` and `b.docx` (the same zip), and an empty one.
struct Stand {
    _plugins: tempfile::TempDir,
    host: PluginHost,
    app: App,
    files: tempfile::TempDir,
    out: tempfile::TempDir,
}

fn stand(name: &str, settings: &[(&str, &str)]) -> Stand {
    let plugins = tempfile::tempdir().unwrap();
    crate::plugin_host::tests::install_shipped(plugins.path(), "archive", name);
    let values: serde_json::Map<String, serde_json::Value> = (settings.iter())
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
    let files = tempfile::tempdir().unwrap();
    let zip = zip_of(&[("docs/readme.txt", b"hello"), ("top.txt", b"top")]);
    std::fs::write(files.path().join("a.zip"), &zip).unwrap();
    std::fs::write(files.path().join("b.docx"), &zip).unwrap();
    let app = app_in(plugins.path());
    Stand {
        _plugins: plugins,
        host,
        app,
        files,
        out: tempfile::tempdir().unwrap(),
    }
}

impl Stand {
    fn file(&self, name: &str) -> PathBuf {
        self.files.path().join(name)
    }

    fn extract(&self, silent: bool) -> DiskUse {
        DiskUse::Extract {
            destination: self.out.path().display().to_string(),
            silent,
        }
    }

    /// `name` offered for `purpose` for the press `serial` on the left panel.
    fn offer(&self, name: &str, purpose: DiskUse, serial: u64) {
        let path = self.file(name);
        let sent = (self.host).disk_file(0, (&path, purpose), Some(self.files.path()), (0, serial));
        assert!(sent, "sent");
    }

    /// The next event that ends the call or asks the user: the program's masks and confirmations
    /// answered as it answers them, the panel requests heard kept in `asked` (their reply dropped: none),
    /// windows without buttons let be.
    fn next(&self, asked: &mut Vec<PanelRequest>) -> Event {
        loop {
            match next_event(&self.host) {
                Event::PanelAsk { request, reply, .. } => {
                    let answer = match &request {
                        PanelRequest::MatchMasks { masks, name } => Some(PanelAnswer::Confirms(
                            self.app.file_mask(masks).map(|mask| mask.matches(name)),
                        )),
                        PanelRequest::Confirmation(name) => {
                            Some(PanelAnswer::Confirms(self.app.confirmation(name)))
                        }
                        _ => None,
                    };
                    if let Some(answer) = answer {
                        let _ = reply.send(answer);
                    } else {
                        asked.push(request);
                    }
                }
                Event::Busy { .. } | Event::Done { .. } => {}
                Event::Setting { .. } | Event::Synchro { .. } => {}
                event => return event,
            }
        }
    }
}

/// The end of the press `serial` of `plugin`.
fn ended(event: Event, (plugin, serial): (&str, u64)) -> (DiskEnd, Option<String>) {
    match event {
        Event::DiskFile {
            plugin: by,
            serial: of,
            end,
            error,
        } => {
            assert_eq!((by.as_str(), of), (plugin, serial));
            (end, error)
        }
        other => panic!("a disk file's end: {other:?}"),
    }
}

fn passed_on(event: Event, (plugin, serial): (&str, u64)) {
    match event {
        Event::Key {
            plugin: by,
            side,
            serial: of,
            answer,
        } => {
            assert_eq!((by.as_str(), side, of), (plugin, 0, serial));
            assert!(answer.pass, "the next plugin's");
        }
        other => panic!("the press passed on: {other:?}"),
    }
}

/// A file of zip's contents named out of arclite's include masks is passed on by Shift+F2 and Shift+F3
/// as by Enter, though "Mask settings apply to Ctrl-PgDn" is off (only Ctrl+PgDn passes them by then);
/// a zip of the masks is taken (its commands' menu), Esc there not done.
#[test]
fn a_file_out_of_the_masks_is_passed_on_by_shift_f2_and_shift_f3() {
    let name = "diskhostmasks";
    let stand = stand(name, &[("pgdn_masks", "0")]);
    let mut asked = Vec::new();
    stand.offer("b.docx", stand.extract(false), 1);
    passed_on(stand.next(&mut asked), (name, 1));
    stand.offer("b.docx", stand.extract(true), 2);
    passed_on(stand.next(&mut asked), (name, 2));
    stand.offer("b.docx", DiskUse::Commands, 3);
    passed_on(stand.next(&mut asked), (name, 3));
    stand.offer("a.zip", DiskUse::Commands, 4);
    match stand.next(&mut asked) {
        Event::Ask {
            request: Request::Menu(menu),
            reply,
            ..
        } => {
            let lines: Vec<String> = (menu.lines.iter())
                .map(|line| line.text.replace('&', ""))
                .collect();
            assert_eq!(lines, ["Test archive files"]);
            reply.send(Answer::Esc).unwrap();
        }
        other => panic!("the commands' menu: {other:?}"),
    }
    assert_eq!(
        ended(stand.next(&mut asked), (name, 4)),
        (DiskEnd::NotDone, None)
    );
    assert!(stand.file("a.zip").exists() && stand.file("b.docx").exists());
}

/// After the plugin's first file (`OPM_SILENT`): the root written into the destination as given, with no
/// dialog, and done.
#[test]
fn a_silent_get_writes_the_root_with_no_dialog() {
    let name = "diskhostsilent";
    let stand = stand(name, &[]);
    let mut asked = Vec::new();
    stand.offer("a.zip", stand.extract(true), 7);
    assert_eq!(
        ended(stand.next(&mut asked), (name, 7)),
        (DiskEnd::Done, None),
        "no dialog asked"
    );
    let out = stand.out.path();
    assert_eq!(std::fs::read(out.join("top.txt")).unwrap(), b"top");
    assert_eq!(
        std::fs::read(out.join("docs").join("readme.txt")).unwrap(),
        b"hello"
    );
    assert!(stand.file("a.zip").exists(), "the archive kept");
}

/// Shift+F3's "Test archive files": the archive tested, "Archive test complete" said, the file done.
#[test]
fn shift_f3s_test_is_done() {
    let name = "diskhosttest";
    let stand = stand(name, &[]);
    let mut asked = Vec::new();
    stand.offer("a.zip", DiskUse::Commands, 5);
    match stand.next(&mut asked) {
        Event::Ask {
            request: Request::Menu(_),
            reply,
            ..
        } => reply
            .send(Answer::Chosen {
                item: Some(0),
                key: None,
            })
            .unwrap(),
        other => panic!("the commands' menu: {other:?}"),
    }
    match stand.next(&mut asked) {
        Event::Ask {
            request: Request::Message(message),
            reply,
            ..
        } => {
            assert_eq!(message.lines, ["Archive test complete"]);
            reply.send(Answer::Button(0)).unwrap();
        }
        other => panic!("the test's end: {other:?}"),
    }
    assert_eq!(
        ended(stand.next(&mut asked), (name, 5)),
        (DiskEnd::Done, None)
    );
}

/// "Delete archive after extraction" checked in the Extract dialog of a disk's archive: its root written
/// out, the archive file deleted, and no panel closed (the archive's panel is never shown: arclite's
/// `close_panel` of it closes none, the program's panels stay).
#[test]
fn delete_archive_at_the_top_level_deletes_the_file_and_closes_no_panel() {
    let name = "diskhostdelete";
    let stand = stand(name, &[]);
    let mut asked = Vec::new();
    stand.offer("a.zip", stand.extract(false), 9);
    let (request, talk) = match stand.next(&mut asked) {
        Event::Dialog { request, talk, .. } => (request, talk),
        other => panic!("the Extract dialog: {other:?}"),
    };
    assert_eq!(request.title, "Extract files");
    assert!(
        request.items[DELETE_ARCHIVE].text.contains("lete archive"),
        "{:?}",
        request.items[DELETE_ARCHIVE]
    );
    assert_eq!(request.items[EXTRACT_OK].text, "OK");
    let mut values: Vec<ItemValue> = (request.items.iter())
        .map(|item| ItemValue {
            state: item.state,
            text: item.text.clone(),
        })
        .collect();
    values[DELETE_ARCHIVE].state = 1;
    let closed = DialogMessage::Closed {
        button: Some(EXTRACT_OK),
        values,
    };
    talk.send(closed).unwrap();
    assert_eq!(
        ended(stand.next(&mut asked), (name, 9)),
        (DiskEnd::Done, None)
    );
    assert!(!stand.file("a.zip").exists(), "the archive deleted");
    assert!(stand.file("b.docx").exists());
    let written = walkdir_files(stand.out.path());
    assert!(
        written.iter().any(|path| path.ends_with("top.txt")),
        "its root written out first: {written:?}"
    );
    let closing = (asked.iter())
        .find(|request| matches!(request, PanelRequest::Do(_, PanelAction::Close { .. })));
    assert!(closing.is_none(), "no panel closed: {asked:?}");
}

/// Every file under `folder`.
fn walkdir_files(folder: &std::path::Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut left = vec![folder.to_path_buf()];
    while let Some(at) = left.pop() {
        for entry in std::fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                left.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

/// No plugin at that number, or a path no Unicode text names: nothing sent (the next press's answer the
/// first to come).
#[test]
fn a_plugin_or_a_path_that_cannot_be_handed_over_sends_nothing() {
    let name = "diskhostbroken";
    let stand = stand(name, &[]);
    let folder = Some(stand.files.path());
    let zip = stand.file("a.zip");
    assert!(
        !(stand.host).disk_file(5, (&zip, DiskUse::Commands), folder, (0, 1)),
        "no such plugin"
    );
    // An unpaired surrogate: no Unicode text names it.
    let broken = stand.files.path().join(PathBuf::from(OsString::from_wide(&[
        u16::from(b'x'),
        0xD800,
    ])));
    assert!(!(stand.host).disk_file(0, (&broken, DiskUse::Commands), folder, (0, 2)));
    assert!(!(stand.host).disk_file(0, (&broken, stand.extract(false)), folder, (0, 3)));
    stand.offer("b.docx", DiskUse::Commands, 4);
    let mut asked = Vec::new();
    passed_on(stand.next(&mut asked), (name, 4));
}
