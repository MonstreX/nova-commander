//! The shipped archive plugin's "Create archive" dialog (arclite's `UpdateDialog` with `new_arc`) as its
//! handler answers the program: the items enabled and shown as the values say, the password carried
//! between its lines, OK refused with the reason, the profiles saved and deleted (a deleted one's values
//! emptied, its password too), a profile chosen filling the items (a level none of the list's as
//! Normal) and the one the values match chosen, Evaluate's whole path, "Save settings".

use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use super::{archive_host, create_dialog, one_file};
use crate::plugin_host::Event;
use crate::plugin_host::PluginHost;
use crate::plugin_host::windows::{
    Answer, Change, DialogEventKind, DialogMessage, DialogRequest, Handled, ItemValue, Request,
};

/// The dialog's items by index (the plugin's `create_dialog`).
const PROFILE: usize = 1;
const SAVE_PROFILE: usize = 2;
const DELETE_PROFILE: usize = 3;
const EVALUATE: usize = 5;
const APPEND_EXT: usize = 6;
const ARC_PATH: usize = 8;
const ZIP: usize = 12;
const LEVEL: usize = 16;
const METHOD_LABEL: usize = 17;
const METHOD: usize = 18;
const ENCRYPT: usize = 23;
const SHOW_PASSWORD: usize = 25;
const PASSWORD: usize = 27;
const PASSWORD_VERIFY: usize = 29;
const PASSWORD_VISIBLE: usize = 31;
const ENABLE_VOLUMES: usize = 36;
const VOLUME_SIZE: usize = 38;
const MOVE_FILES: usize = 40;
const OK: usize = 45;
const CANCEL: usize = 46;
const SAVE: usize = 47;

/// The levels' lines (arclite's): Store, Fastest, Fast, Normal, Maximum, Ultra.
const FASTEST: i32 = 1;
const NORMAL: i32 = 3;

/// The dialog up on the plugin's thread, its values as the program keeps them.
struct Shown<'a> {
    host: &'a PluginHost,
    talk: Sender<DialogMessage>,
    values: Vec<ItemValue>,
    /// The plugin's values set meanwhile, in order.
    settings: Vec<(String, String)>,
    /// The windows it asked: a message's title and lines, an input's title and prompt.
    asked: Vec<String>,
}

fn told(request: &Request) -> String {
    match request {
        Request::Message(message) => format!("{}: {}", message.title, message.lines.join("|")),
        Request::Input(input) => format!("{}: {} [{}]", input.title, input.prompt, input.text),
        Request::Menu(menu) => format!("menu {}", menu.title),
    }
}

impl<'a> Shown<'a> {
    fn new(host: &'a PluginHost, request: &DialogRequest, talk: Sender<DialogMessage>) -> Self {
        let values = (request.items.iter())
            .map(|item| ItemValue {
                state: item.state,
                text: item.text.clone(),
            })
            .collect();
        Self {
            host,
            talk,
            values,
            settings: Vec::new(),
            asked: Vec::new(),
        }
    }

    fn set(&mut self, item: usize, state: i32) {
        self.values[item].state = state;
    }

    fn type_in(&mut self, item: usize, text: &str) {
        self.values[item].text = text.to_string();
    }

    fn take(&mut self, event: Event, answer: &dyn Fn(&Request) -> Answer) {
        match event {
            Event::Ask { request, reply, .. } => {
                self.asked.push(told(&request));
                let _ = reply.send(answer(&request));
            }
            Event::Setting { key, value, .. } => self.settings.push((key, value)),
            // Questions about the panels and the screen left unanswered (none).
            _ => {}
        }
    }

    /// `kind` heard by the handler with the values now, its windows answered by `answer`: its answer,
    /// its texts and states taken into the values.
    fn hear(&mut self, kind: DialogEventKind, answer: &dyn Fn(&Request) -> Answer) -> Handled {
        let (reply, handled) = channel();
        let event = DialogMessage::Event {
            kind,
            values: self.values.clone(),
            reply,
        };
        self.talk.send(event).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let handled = loop {
            if let Ok(handled) = handled.try_recv() {
                break handled;
            }
            if let Some(event) = self.host.take_event() {
                self.take(event, answer);
                continue;
            }
            assert!(Instant::now() < deadline, "the handler did not answer");
            std::thread::sleep(Duration::from_millis(1));
        };
        // What it sent before its answer is there already.
        while let Some(event) = self.host.take_event() {
            self.take(event, answer);
        }
        for change in &handled.changes {
            match change {
                Change::Text(item, text) => self.values[*item].text.clone_from(text),
                Change::State(item, state) => self.values[*item].state = *state,
                _ => {}
            }
        }
        handled
    }

    /// The value the plugin set last for `key`.
    fn setting(&self, key: &str) -> Option<&str> {
        (self.settings.iter().rev())
            .find(|(own, _)| own == key)
            .map(|(_, value)| value.as_str())
    }
}

/// Windows answered by their first button (a message's OK, a question's Yes), an input by Esc.
fn first(request: &Request) -> Answer {
    match request {
        Request::Input(_) => Answer::Esc,
        _ => Answer::Button(0),
    }
}

fn has(handled: &Handled, change: &Change) -> bool {
    handled.changes.contains(change)
}

/// The dialog for `plain.txt` of a new folder, the plugin `name`'s values `settings`.
fn shown_with<'a>(
    host: &'a PluginHost,
    folder: &tempfile::TempDir,
    file: &std::path::Path,
) -> (DialogRequest, Shown<'a>) {
    assert!(host.create(0, (&[file.to_path_buf()], folder.path()), (0, 1)));
    let (request, talk) = create_dialog(host);
    let text = |item: usize| request.items[item].text.as_str();
    // The indexes this file names are the dialog's.
    assert_eq!(
        [text(SAVE_PROFILE), text(EVALUATE), text(OK), text(SAVE)],
        ["Save", "Eval&uate", "OK", "Save settings"]
    );
    assert_eq!(text(ZIP), "&zip");
    assert_eq!(text(ENABLE_VOLUMES), "Split into &volumes");
    let shown = Shown::new(host, &request, talk);
    (request, shown)
}

#[test]
fn the_items_follow_the_level_encrypt_and_show_password() {
    let (_plugins, host) = archive_host("arcitems", &[]);
    let (folder, file) = one_file();
    let (_request, mut shown) = shown_with(&host, &folder, &file);
    let init = shown.hear(DialogEventKind::Init, &first);
    assert!(has(&init, &Change::Focus(ARC_PATH)), "{init:?}");
    assert!(has(&init, &Change::Enabled(DELETE_PROFILE, false)));
    assert!(has(&init, &Change::Enabled(METHOD, true)));
    assert!(has(&init, &Change::Enabled(SHOW_PASSWORD, false)));
    assert!(
        has(&init, &Change::Enabled(ENABLE_VOLUMES, true)),
        "a zip split too"
    );
    // Store: no method.
    shown.set(LEVEL, 0);
    let store = shown.hear(DialogEventKind::Changed(LEVEL), &first);
    assert!(has(&store, &Change::Enabled(METHOD, false)));
    assert!(has(&store, &Change::Enabled(METHOD_LABEL, false)));
    // Encrypt: the password's lines, hidden ones shown.
    shown.set(ENCRYPT, 1);
    let encrypt = shown.hear(DialogEventKind::Changed(ENCRYPT), &first);
    assert!(has(&encrypt, &Change::Enabled(SHOW_PASSWORD, true)));
    assert!(has(&encrypt, &Change::Enabled(PASSWORD, true)));
    assert!(has(&encrypt, &Change::Visible(PASSWORD, true)));
    assert!(has(&encrypt, &Change::Visible(PASSWORD_VISIBLE, false)));
}

#[test]
fn show_password_carries_the_text_between_its_lines() {
    let (_plugins, host) = archive_host("arcshown", &[]);
    let (folder, file) = one_file();
    let (_request, mut shown) = shown_with(&host, &folder, &file);
    shown.set(ENCRYPT, 1);
    shown.hear(DialogEventKind::Changed(ENCRYPT), &first);
    // Show password: the hidden line's text carried to the shown one, and back.
    shown.type_in(PASSWORD, "abc");
    shown.set(SHOW_PASSWORD, 1);
    let showing = shown.hear(DialogEventKind::Changed(SHOW_PASSWORD), &first);
    assert!(has(
        &showing,
        &Change::Text(PASSWORD_VISIBLE, "abc".to_string())
    ));
    assert!(has(&showing, &Change::Visible(PASSWORD_VISIBLE, true)));
    assert!(has(&showing, &Change::Visible(PASSWORD, false)));
    shown.type_in(PASSWORD_VISIBLE, "xyz");
    shown.set(SHOW_PASSWORD, 0);
    let hiding = shown.hear(DialogEventKind::Changed(SHOW_PASSWORD), &first);
    assert!(has(&hiding, &Change::Text(PASSWORD, "xyz".to_string())));
    assert!(has(
        &hiding,
        &Change::Text(PASSWORD_VERIFY, "xyz".to_string())
    ));
}

#[test]
fn ok_with_wrong_answers_says_why_and_stays() {
    let (_plugins, host) = archive_host("arcok", &[]);
    let (folder, file) = one_file();
    let (_request, mut shown) = shown_with(&host, &folder, &file);
    shown.set(ENCRYPT, 1);
    shown.type_in(PASSWORD, "a");
    shown.type_in(PASSWORD_VERIFY, "b");
    let refused = shown.hear(DialogEventKind::Close(Some(OK)), &first);
    assert!(refused.keep_open, "stays");
    assert_eq!(shown.asked, ["Create archive: Passwords don't match"]);
    // Cancel is not checked.
    let cancelled = shown.hear(DialogEventKind::Close(Some(CANCEL)), &first);
    assert!(!cancelled.keep_open);
    assert_eq!(shown.asked.len(), 1);
    shown.set(ENCRYPT, 0);
    shown.set(ZIP, 0);
    let no_format = shown.hear(DialogEventKind::Close(Some(OK)), &first);
    assert!(no_format.keep_open);
    assert_eq!(shown.asked[1], "Create archive: Select archive format");
    // Split into volumes of a size too small: arclite's reason, the dialog stays.
    shown.set(ZIP, 1);
    shown.set(ENABLE_VOLUMES, 1);
    shown.type_in(VOLUME_SIZE, "1");
    let volumes = shown.hear(DialogEventKind::Close(Some(OK)), &first);
    assert!(volumes.keep_open, "{:?}", shown.asked);
    assert_eq!(shown.asked[2], "Create archive: Invalid volume size");
}

#[test]
fn a_profile_saved_then_deleted_leaves_no_password_behind() {
    let (_plugins, host) = archive_host("arcprofile", &[]);
    let (folder, file) = one_file();
    let (_request, mut shown) = shown_with(&host, &folder, &file);
    shown.set(ENCRYPT, 1);
    shown.type_in(PASSWORD, "secret");
    shown.type_in(PASSWORD_VERIFY, "secret");
    let named = |request: &Request| match request {
        Request::Input(_) => Answer::Text("mine".to_string()),
        _ => Answer::Button(0),
    };
    let saved = shown.hear(DialogEventKind::Button(SAVE_PROFILE), &named);
    assert_eq!(shown.asked, ["Archive: Input profile name []"]);
    assert_eq!(shown.setting("profiles"), Some("mine"));
    assert_eq!(shown.setting(r"profiles\mine\password"), Some("secret"));
    assert_eq!(shown.setting(r"profiles\mine\arc_type"), Some("zip"));
    let lines = Change::Lines(PROFILE, vec!["mine".to_string(), String::new()]);
    assert!(has(&saved, &lines), "{saved:?}");
    assert!(
        has(&saved, &Change::State(PROFILE, 0)),
        "the one saved chosen"
    );
    assert!(has(&saved, &Change::Enabled(DELETE_PROFILE, true)));
    // A name that cannot name one: said, nothing kept.
    shown.settings.clear();
    let slashed = |request: &Request| match request {
        Request::Input(_) => Answer::Text(r"a\b".to_string()),
        _ => Answer::Button(0),
    };
    shown.hear(DialogEventKind::Button(SAVE_PROFILE), &slashed);
    assert_eq!(
        shown.asked[1..],
        [
            "Archive: Input profile name [mine]".to_string(),
            r"Archive: A profile's name has text and no \ in it".to_string()
        ],
        "the matching profile's name offered"
    );
    assert!(shown.settings.is_empty(), "{:?}", shown.settings);
}

/// A kept profile "mine" with a password, which the dialog's options match.
const MINE: [(&str, &str); 6] = [
    ("profiles", "mine"),
    (r"profiles\mine\arc_type", "zip"),
    (r"profiles\mine\level", "5"),
    (r"profiles\mine\method", "Deflate"),
    (r"profiles\mine\encrypt", "1"),
    (r"profiles\mine\password", "secret"),
];

#[test]
fn a_profile_deleted_leaves_no_password_behind() {
    let (_plugins, host) = archive_host("arcdelete", &MINE);
    let (folder, file) = one_file();
    let (_request, mut shown) = shown_with(&host, &folder, &file);
    shown.set(PROFILE, 0);
    shown.hear(DialogEventKind::Changed(PROFILE), &first);
    // Delete answered No: kept.
    shown.asked.clear();
    let no = |_: &Request| Answer::Button(1);
    shown.hear(DialogEventKind::Button(DELETE_PROFILE), &no);
    assert_eq!(
        shown.asked,
        ["Archive: Do you want to delete selected profile?"]
    );
    assert!(shown.settings.is_empty());
    // Yes: gone, its values emptied (arclite deletes its key).
    let deleted = shown.hear(DialogEventKind::Button(DELETE_PROFILE), &first);
    assert_eq!(shown.setting("profiles"), Some(""));
    assert_eq!(shown.setting(r"profiles\mine\password"), Some(""));
    assert_eq!(shown.setting(r"profiles\mine\level"), Some(""));
    assert!(has(&deleted, &Change::Lines(PROFILE, vec![String::new()])));
    assert!(has(&deleted, &Change::Enabled(DELETE_PROFILE, false)));
}

#[test]
fn delete_with_no_profile_chosen_asks_nothing() {
    let (_plugins, host) = archive_host("arcnodelete", &[]);
    let (folder, file) = one_file();
    let (request, mut shown) = shown_with(&host, &folder, &file);
    assert_eq!(
        request.items[PROFILE].state, 0,
        "the empty line: no profiles"
    );
    shown.hear(DialogEventKind::Button(DELETE_PROFILE), &first);
    assert!(shown.asked.is_empty() && shown.settings.is_empty());
}

const FAST: [(&str, &str); 7] = [
    ("profiles", "fast\nodd"),
    (r"profiles\fast\arc_type", "zip"),
    (r"profiles\fast\level", "1"),
    (r"profiles\fast\method", "Deflate"),
    (r"profiles\odd\arc_type", "zip"),
    (r"profiles\odd\level", "4"),
    (r"profiles\odd\method", "PPMD"),
];

#[test]
fn a_profile_chosen_fills_the_items_and_the_matching_one_is_chosen() {
    let (_plugins, host) = archive_host("arcchoose", &FAST);
    let (folder, file) = one_file();
    let (request, mut shown) = shown_with(&host, &folder, &file);
    assert_eq!(
        request.items[PROFILE].lines,
        ["fast", "odd", ""],
        "kept profiles"
    );
    assert_eq!(
        request.items[PROFILE].state, 2,
        "none matches: the empty line"
    );
    // "odd" chosen: its level is none of the list's, Normal shown; its method.
    shown.set(PROFILE, 1);
    let odd = shown.hear(DialogEventKind::Changed(PROFILE), &first);
    assert!(has(&odd, &Change::State(LEVEL, NORMAL)), "{odd:?}");
    assert!(has(&odd, &Change::State(METHOD, 2)), "PPMD");
    // Fastest with Deflate (arclite's fourth method): the values are "fast"'s, so it is chosen.
    shown.set(LEVEL, FASTEST);
    shown.set(METHOD, 3);
    let fast = shown.hear(DialogEventKind::Changed(LEVEL), &first);
    assert!(has(&fast, &Change::State(PROFILE, 0)), "{fast:?}");
    assert!(has(&fast, &Change::Enabled(DELETE_PROFILE, true)));
    // Another method: none matches any more.
    shown.set(METHOD, 2);
    let none = shown.hear(DialogEventKind::Changed(METHOD), &first);
    assert!(has(&none, &Change::State(PROFILE, 2)), "{none:?}");
}

#[test]
fn a_kept_level_none_of_the_lists_starts_as_normal() {
    let (_plugins, host) = archive_host("arclevel", &[("update_level", "4")]);
    let (folder, file) = one_file();
    let (request, _shown) = shown_with(&host, &folder, &file);
    assert_eq!(request.items[LEVEL].state, NORMAL);
    let (_plugins, host) = archive_host("arclevel9", &[("update_level", "9")]);
    let (request, _shown) = shown_with(&host, &folder, &file);
    assert_eq!(request.items[LEVEL].state, 5, "Ultra kept");
}

#[test]
fn evaluate_shows_the_whole_path_with_dot_dot_resolved() {
    let (_plugins, host) = archive_host("arceval", &[]);
    let (folder, file) = one_file();
    let (_request, mut shown) = shown_with(&host, &folder, &file);
    shown.type_in(ARC_PATH, r"..\sub\..\x");
    shown.set(APPEND_EXT, 1);
    let evaluated = shown.hear(DialogEventKind::Button(EVALUATE), &first);
    assert!(has(&evaluated, &Change::Focus(ARC_PATH)));
    let up = folder.path().parent().unwrap().join("x.zip");
    let [told] = shown.asked.as_slice() else {
        panic!("one window: {:?}", shown.asked);
    };
    // Its lines cut to the screen's width: the path whole once they are joined.
    let lines = told.strip_prefix(": ").unwrap_or(told).replace('|', "");
    assert_eq!(lines, up.display().to_string());
}

#[test]
fn save_settings_keeps_the_answers_under_arclites_names() {
    let (_plugins, host) = archive_host("arcsave", &[]);
    let (folder, file) = one_file();
    let (_request, mut shown) = shown_with(&host, &folder, &file);
    shown.hear(DialogEventKind::Init, &first);
    shown.set(LEVEL, FASTEST);
    shown.hear(DialogEventKind::Changed(LEVEL), &first);
    shown.set(METHOD, 2);
    shown.set(MOVE_FILES, 1);
    let saved = shown.hear(DialogEventKind::Button(SAVE), &first);
    assert!(has(&saved, &Change::Focus(OK)));
    assert_eq!(shown.asked, ["Add files to archive: Settings saved"]);
    assert_eq!(shown.setting("update_arc_format_name"), Some("zip"));
    assert_eq!(shown.setting("update_level"), Some("1"));
    assert_eq!(shown.setting("update_levels"), Some("zip=1"));
    assert_eq!(shown.setting("update_method"), Some("PPMD"));
    assert_eq!(shown.setting("update_move_files"), Some("1"));
    assert_eq!(shown.setting("update_enable_volumes"), Some("0"));
    assert_eq!(shown.setting("update_password"), Some(""), "not encrypting");
    // Answers that cannot be taken: nothing kept, nothing said.
    shown.settings.clear();
    shown.set(ZIP, 0);
    shown.hear(DialogEventKind::Button(SAVE), &first);
    assert!(shown.settings.is_empty());
    assert_eq!(shown.asked.len(), 1);
}
