//! A panel's own keys through the table: named with their labels in the panel's look, one pressed reaching
//! the plugin with the cursor's entry, the selection and the other panel's folder, the answer's flags and
//! path, a failure and a panic as failures.

use super::*;
use crate::api::{API_VERSION, ERROR_DENIED, ERROR_OTHER, NcPlugin};
use crate::tests::{host, text};
use crate::{Host, Info, entry};

/// Answers each key as its name asks; keeps what it heard.
#[derive(Default)]
struct Keys {
    heard: Vec<KeyPress>,
}

impl Plugin for Keys {
    fn new(_host: Host) -> Self {
        Self::default()
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn panel_keys(&self, _instance: &str) -> Vec<PanelKey> {
        vec![
            PanelKey::new("F7", "Remove"),
            PanelKey::new("Alt+Shift+0", ""),
        ]
    }

    fn panel_key(
        &mut self,
        _instance: &str,
        path: &str,
        press: &KeyPress,
    ) -> Result<KeyAnswer, Error> {
        self.heard.push(press.clone());
        Ok(match press.key.as_str() {
            "F7" => KeyAnswer::reread(),
            "folder" => KeyAnswer::go(KeyGo::PassiveFolder(PathBuf::from(path))),
            "file" => KeyAnswer {
                reread_passive: true,
                ..KeyAnswer::go(KeyGo::PassiveFile(PathBuf::from(r"C:\a\b.txt")))
            },
            "open" => KeyAnswer::go(KeyGo::Open("3".to_string())),
            "pass" => KeyAnswer::pass(),
            "active folder" => KeyAnswer::go(KeyGo::ActiveFolder(PathBuf::from(r"C:\a"))),
            "active file" => KeyAnswer {
                reread_quick_view: true,
                ..KeyAnswer::go(KeyGo::ActiveFile(PathBuf::from(r"C:\a\b.txt")))
            },
            "command" => KeyAnswer::go(KeyGo::CommandLine("echo".to_string())),
            "empty command" => KeyAnswer::go(KeyGo::CommandLine(String::new())),
            "denied" => return Err(Error::new(crate::ErrorKind::Denied, "no")),
            "panic" => panic!("the plugin panics on a key"),
            _ => KeyAnswer::default(),
        })
    }
}

fn table(host: &crate::api::NcHost) -> &'static NcPlugin {
    // SAFETY: the host's table outlives the plugin in each test, released at its end.
    unsafe { &*entry::<Keys>(host) }
}

/// The key `key` pressed with `current`, `selected` and `passive`: the code and the answer.
fn press_key(
    table: &NcPlugin,
    key: &str,
    current: &str,
    selected: &[&str],
    passive: &str,
) -> (i32, u32, String) {
    let names: Vec<NcStr> = selected.iter().map(|name| NcStr::new(name)).collect();
    let event = NcKeyEvent {
        key: NcStr::new(key),
        current: NcStr::new(current),
        selected: if names.is_empty() {
            std::ptr::null()
        } else {
            names.as_ptr()
        },
        selected_count: names.len(),
        passive: NcStr::new(passive),
        folder: NcStr::new(r"C:\work"),
    };
    let mut answer = NcKeyAnswer::NONE;
    // SAFETY: the table's own state; the texts, the event and the answer's place live through the call.
    let code = unsafe {
        (table.panel_key)(
            table.state,
            NcStr::new("main"),
            NcStr::new("/here"),
            &raw const event,
            &raw mut answer,
        )
    };
    (code, answer.flags, text(answer.path))
}

#[test]
fn a_panels_keys_and_labels_are_in_its_look() {
    let host = host(API_VERSION);
    let table = table(&host);
    // SAFETY: the table's own state; the texts live through the call.
    let info = unsafe { (table.panel_info)(table.state, NcStr::new("main"), NcStr::new("/")) };
    // SAFETY: the plugin's keys, alive until its next call.
    let keys = unsafe { std::slice::from_raw_parts(info.keys, info.key_count) };
    let named: Vec<(String, String)> = (keys.iter())
        .map(|key| (text(key.key), text(key.label)))
        .collect();
    assert_eq!(
        named,
        [
            ("F7".to_string(), "Remove".to_string()),
            ("Alt+Shift+0".to_string(), String::new())
        ]
    );
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_key_reaches_the_plugin_with_the_cursor_the_selection_and_the_other_folder() {
    let host = host(API_VERSION);
    let table = table(&host);
    assert_eq!(
        press_key(table, "F7", r"C:\x.txt", &["a", "b"], r"D:\work"),
        (0, KEY_REREAD, String::new())
    );
    assert_eq!(
        press_key(table, "F7", "", &[], ""),
        (0, KEY_REREAD, String::new())
    );
    // SAFETY: the table's own state, no call running.
    let heard = unsafe { &(*table.state.cast::<crate::State<Keys>>()).plugin.heard };
    assert_eq!(
        heard[0],
        KeyPress {
            key: "F7".to_string(),
            current: Some(r"C:\x.txt".to_string()),
            selected: vec!["a".to_string(), "b".to_string()],
            passive: Some(PathBuf::from(r"D:\work")),
            folder: Some(PathBuf::from(r"C:\work")),
        }
    );
    assert_eq!(heard[0].targets(), ["a", "b"], "the selection");
    assert_eq!(
        (heard[1].current.clone(), heard[1].passive.clone()),
        (None, None),
        "on \"..\""
    );
    assert!(heard[1].targets().is_empty(), "nothing on \"..\"");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn the_answer_tells_where_to_go_and_a_failure_its_kind() {
    let host = host(API_VERSION);
    let table = table(&host);
    assert_eq!(
        press_key(table, "folder", "", &[], ""),
        (0, KEY_PASSIVE_FOLDER, "/here".to_string())
    );
    assert_eq!(
        press_key(table, "file", "", &[], ""),
        (
            0,
            KEY_PASSIVE_FILE | KEY_REREAD_PASSIVE,
            r"C:\a\b.txt".to_string()
        )
    );
    assert_eq!(
        press_key(table, "open", "", &[], ""),
        (0, KEY_OPEN, "3".to_string())
    );
    assert_eq!(
        press_key(table, "other", "", &[], ""),
        (0, 0, String::new()),
        "nothing to do"
    );
    assert_eq!(
        press_key(table, "pass", "", &[], ""),
        (0, crate::api::KEY_PASS, String::new())
    );
    assert_eq!(
        press_key(table, "active folder", "", &[], ""),
        (0, crate::api::KEY_ACTIVE_FOLDER, r"C:\a".to_string())
    );
    assert_eq!(
        press_key(table, "active file", "", &[], ""),
        (
            0,
            crate::api::KEY_ACTIVE_FILE | crate::api::KEY_REREAD_QUICK_VIEW,
            r"C:\a\b.txt".to_string()
        )
    );
    assert_eq!(
        press_key(table, "command", "", &[], ""),
        (0, crate::api::KEY_COMMAND_LINE, "echo".to_string())
    );
    assert_eq!(
        press_key(table, "empty command", "", &[], ""),
        (0, crate::api::KEY_COMMAND_LINE, String::new()),
        "an empty command line is one"
    );
    assert_eq!(press_key(table, "denied", "", &[], "").0, ERROR_DENIED);
    assert_eq!(press_key(table, "panic", "", &[], "").0, ERROR_OTHER);
    let mut answer = NcKeyAnswer::NONE;
    // SAFETY: the table's own state; a null event is refused.
    let code = unsafe {
        (table.panel_key)(
            table.state,
            NcStr::new("main"),
            NcStr::new("/"),
            std::ptr::null(),
            &raw mut answer,
        )
    };
    assert_eq!(code, crate::api::ERROR_UNSUPPORTED);
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}
