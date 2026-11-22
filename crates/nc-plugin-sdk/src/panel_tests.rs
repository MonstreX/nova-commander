//! A plugin's panels through the table: entries, their flags and times, failures by kind, a panic, the
//! title, the end, and a command that opens a panel.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;
use crate::api::{API_VERSION, NcPlugin, PLACE_COMMANDS, PLACE_DISKS};
use crate::tests::{host, text};
use crate::{Host, Info, entry};

#[derive(Default)]
struct Shelf {
    closed: Vec<String>,
}

impl Plugin for Shelf {
    fn new(_host: Host) -> Self {
        Self::default()
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn run_command(&mut self, index: usize) -> Result<Option<String>, String> {
        Ok((index == 0).then(|| "shelf".to_string()))
    }

    fn open_disk(&mut self, _index: usize) -> Result<Option<String>, String> {
        Ok(Some(String::new()))
    }

    fn list(&mut self, instance: &str, path: &str) -> Result<Vec<FileEntry>, Error> {
        match path {
            "/" => Ok(vec![
                FileEntry::folder(format!("{instance} books")),
                FileEntry {
                    real_path: Some(std::path::PathBuf::from(r"C:\files\note.txt")),
                    hidden: true,
                    system: true,
                    read_only: true,
                    modified: Some(UNIX_EPOCH + Duration::from_millis(1_500)),
                    created: Some(UNIX_EPOCH - Duration::from_millis(2_000)),
                    ..FileEntry::file("note.txt", 42)
                },
            ]),
            "/gone" => Err(Error::new(ErrorKind::NotFound, "no such shelf")),
            "/locked" => Err(Error::new(ErrorKind::Denied, "locked")),
            "/full" => Err(Error::new(ErrorKind::Exists, "there")),
            "/odd" => Err(Error::new(ErrorKind::Unsupported, "odd")),
            "/broken" => Err("broken".to_string().into()),
            _ => panic!("the shelf panics at {path}"),
        }
    }

    fn panel_title(&self, instance: &str, path: &str) -> String {
        assert_ne!(instance, "panic", "the shelf panics titling");
        format!("Shelf {instance}:{path}")
    }

    fn close_panel(&mut self, instance: &str) {
        self.closed.push(instance.to_string());
        assert_ne!(instance, "panic", "the shelf panics closing");
    }
}

/// The shelf's table for the tests' host; released by the caller.
fn shelf(host: &crate::api::NcHost) -> &'static NcPlugin {
    // SAFETY: the host's table outlives the plugin in each test, released at its end.
    unsafe { &*entry::<Shelf>(host) }
}

/// The shelf's own state, to look at what it heard.
fn heard(table: &NcPlugin) -> &Shelf {
    // SAFETY: the table's own state, not released yet, no call running.
    unsafe { &(*table.state.cast::<crate::State<Shelf>>()).plugin }
}

fn listed(table: &NcPlugin, path: &str) -> (i32, Vec<NcEntry>) {
    let mut entries: *const NcEntry = std::ptr::null();
    let mut count = 0;
    // SAFETY: the table's own state; the answer's places live through the call; the entries are
    // copied before the next call.
    unsafe {
        let code = (table.list)(
            table.state,
            NcStr::new("main"),
            NcStr::new(path),
            &raw mut entries,
            &raw mut count,
        );
        let copied = if entries.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(entries, count).to_vec()
        };
        (code, copied)
    }
}

#[test]
fn a_folder_is_listed_with_its_entries_flags_and_times() {
    let host = host(API_VERSION);
    let table = shelf(&host);
    let (code, entries) = listed(table, "/");
    assert_eq!(code, 0);
    assert_eq!(entries.len(), 2);
    assert_eq!(text(entries[0].name), "main books", "the instance asked");
    assert_eq!(entries[0].flags, ENTRY_DIR);
    assert_eq!(
        [entries[0].modified, entries[0].created, entries[0].accessed],
        [TIME_NONE; 3]
    );
    assert_eq!(text(entries[1].name), "note.txt");
    assert_eq!(
        text(entries[1].real_path),
        r"C:\files\note.txt",
        "its real file"
    );
    assert_eq!(text(entries[0].real_path), "", "none");
    assert_eq!(entries[1].size, 42);
    assert_eq!(
        entries[1].flags,
        ENTRY_HIDDEN | ENTRY_SYSTEM | ENTRY_READ_ONLY
    );
    assert_eq!(entries[1].modified, 1_500);
    assert_eq!(entries[1].created, -2_000, "before 1970: negative");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_failure_tells_its_kind_and_a_panic_is_another_failure() {
    let host = host(API_VERSION);
    let table = shelf(&host);
    let error = |path: &str| {
        let (code, entries) = listed(table, path);
        assert!(entries.is_empty(), "{path}: no entries");
        // SAFETY: the table's own state.
        (code, text(unsafe { (table.last_error)(table.state) }))
    };
    assert_eq!(
        error("/gone"),
        (ERROR_NOT_FOUND, "no such shelf".to_string())
    );
    assert_eq!(error("/locked"), (ERROR_DENIED, "locked".to_string()));
    assert_eq!(error("/full"), (ERROR_EXISTS, "there".to_string()));
    assert_eq!(error("/odd"), (ERROR_UNSUPPORTED, "odd".to_string()));
    assert_eq!(error("/broken"), (ERROR_OTHER, "broken".to_string()));
    let (code, why) = error("/elsewhere");
    assert_eq!(code, ERROR_OTHER);
    assert!(why.starts_with("The plugin failed: "), "{why}");
    assert_eq!(listed(table, "/").1.len(), 2, "it goes on");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn the_title_and_the_end_of_a_panel_reach_the_plugin() {
    let host = host(API_VERSION);
    let table = shelf(&host);
    // SAFETY: the table's own state, one call at a time; released once at the end.
    unsafe {
        let info = (table.panel_info)(table.state, NcStr::new("main"), NcStr::new("/a"));
        assert_eq!(text(info.title), "Shelf main:/a");
        let info = (table.panel_info)(table.state, NcStr::new("panic"), NcStr::new("/a"));
        assert_eq!(text(info.title), "", "a panic titling: the path stands");
        let info = (table.panel_info)(table.state, NcStr::new("other"), NcStr::new("/b"));
        assert_eq!(text(info.title), "Shelf other:/b", "it goes on");
        (table.close_panel)(table.state, NcStr::new("main"));
        (table.close_panel)(table.state, NcStr::new("panic"));
        assert_eq!(
            heard(table).closed,
            ["main", "panic"],
            "a panic closing stays there"
        );
        (table.release)(table.state);
    }
}

#[test]
fn a_command_or_a_drive_item_names_the_panel_it_opens() {
    let host = host(API_VERSION);
    let table = shelf(&host);
    let run = |place: u32, index: usize| {
        let mut panel = NcStr::EMPTY;
        // SAFETY: the table's own state; the answer's place lives through the call.
        let code = unsafe { (table.run_item)(table.state, place, index, &raw mut panel) };
        (code, text(panel))
    };
    assert_eq!(run(PLACE_COMMANDS, 0), (0, "shelf".to_string()));
    assert_eq!(run(PLACE_COMMANDS, 1), (0, String::new()), "no panel");
    assert_eq!(
        run(PLACE_DISKS, 0),
        (0, String::new()),
        "an empty name opens none"
    );
    // SAFETY: a null place for the answer is let be.
    let code = unsafe { (table.run_item)(table.state, PLACE_COMMANDS, 0, std::ptr::null_mut()) };
    assert_eq!(code, 0);
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_null_place_for_the_answer_is_let_be() {
    let host = host(API_VERSION);
    let table = shelf(&host);
    let mut entries: *const NcEntry = std::ptr::null();
    let mut count = 0;
    // SAFETY: the table's own state; the places given live through the calls, the null ones are not
    // written; released once at the end.
    unsafe {
        let call = |entries: *mut *const NcEntry, count: *mut usize| {
            (table.list)(
                table.state,
                NcStr::new("main"),
                NcStr::new("/"),
                entries,
                count,
            )
        };
        assert_eq!(call(&raw mut entries, std::ptr::null_mut()), 0);
        assert!(entries.is_null(), "no count: no answer");
        assert_eq!(call(std::ptr::null_mut(), &raw mut count), 0);
        assert_eq!(count, 0, "no entries' place: no answer");
        assert_eq!(call(std::ptr::null_mut(), std::ptr::null_mut()), 0);
        (table.release)(table.state);
    }
}

/// How many texts the shelf's state keeps now.
fn kept(table: &NcPlugin) -> usize {
    // SAFETY: the table's own state, not released yet, no call running.
    unsafe { (*table.state.cast::<crate::State<Shelf>>()).kept.len() }
}

#[test]
fn what_a_call_hands_out_lives_until_the_next_call_only() {
    let host = host(API_VERSION);
    let table = shelf(&host);
    for _ in 0..3 {
        assert_eq!(listed(table, "/").1.len(), 2);
        assert_eq!(
            kept(table),
            3,
            "the names and the real file of the last listing"
        );
    }
    for _ in 0..3 {
        // SAFETY: the table's own state; the texts live through the call.
        let info = unsafe { (table.panel_info)(table.state, NcStr::new("m"), NcStr::new("/")) };
        assert_eq!(text(info.title), "Shelf m:/");
        assert_eq!(kept(table), 2, "the last title and format");
    }
    for _ in 0..3 {
        let mut panel = NcStr::EMPTY;
        // SAFETY: the table's own state; the answer's place lives through the call.
        let code = unsafe { (table.run_item)(table.state, PLACE_COMMANDS, 0, &raw mut panel) };
        assert_eq!((code, text(panel)), (0, "shelf".to_string()));
        assert_eq!(kept(table), 1, "the last panel's name");
    }
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

/// A plugin that has no panels: the trait's own answers.
struct Bare;

impl Plugin for Bare {
    fn new(_host: Host) -> Self {
        Self
    }

    fn info(&self) -> Info {
        Info::default()
    }
}

#[test]
fn a_plugin_without_panels_lists_none_and_has_no_title() {
    let host = host(API_VERSION);
    // SAFETY: the host's table outlives the plugin; one call at a time; released once at the end.
    unsafe {
        let table = &*entry::<Bare>(&raw const host);
        let mut entries: *const NcEntry = std::ptr::null();
        let mut count = 0;
        let code = (table.list)(
            table.state,
            NcStr::new("main"),
            NcStr::new("/"),
            &raw mut entries,
            &raw mut count,
        );
        assert_eq!(code, ERROR_UNSUPPORTED);
        assert_eq!(
            text((table.last_error)(table.state)),
            "The plugin has no panels"
        );
        assert!(entries.is_null() && count == 0);
        let info = (table.panel_info)(table.state, NcStr::new("main"), NcStr::new("/"));
        assert_eq!(text(info.title), "", "the path stands");
        assert!(
            info.info_lines.is_null() && info.info_line_count == 0,
            "no lines"
        );
        (table.close_panel)(table.state, NcStr::new("main"));
        let mut panel = NcStr::EMPTY;
        let code = (table.run_item)(table.state, PLACE_COMMANDS, 0, &raw mut panel);
        assert_eq!((code, text(panel)), (0, String::new()), "no panel opened");
        (table.release)(table.state);
    }
}

#[test]
fn times_before_and_after_1970_and_far_off_keep_their_sign() {
    assert_eq!(millis(None), TIME_NONE);
    assert_eq!(millis(Some(UNIX_EPOCH)), 0);
    assert_eq!(millis(Some(UNIX_EPOCH + Duration::from_millis(7))), 7);
    assert_eq!(millis(Some(UNIX_EPOCH - Duration::from_millis(7))), -7);
    let now = millis(Some(SystemTime::now()));
    assert!(now > 1_700_000_000_000, "{now}");
}

/// A plugin whose entries carry texts and whose panel has modes; `panic` panics giving them, `many`
/// gives twelve.
struct Shown;

impl Plugin for Shown {
    fn new(_host: Host) -> Self {
        Self
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn list(&mut self, _instance: &str, _path: &str) -> Result<Vec<FileEntry>, Error> {
        Ok(vec![
            FileEntry {
                columns: vec!["kind".to_string(), String::new(), "3".to_string()],
                description: "About".to_string(),
                ..FileEntry::file("a", 1)
            },
            FileEntry::file("b", 2),
        ])
    }

    fn panel_title(&self, _instance: &str, _path: &str) -> String {
        "Shown".to_string()
    }

    fn panel_info_lines(&self, instance: &str, path: &str) -> Vec<crate::InfoLine> {
        assert_ne!(instance, "panic", "the plugin panics giving its lines");
        vec![
            crate::InfoLine::separator("Server"),
            crate::InfoLine::new("Path", path),
        ]
    }

    fn panel_modes(&self, instance: &str, _path: &str) -> Vec<Option<PanelMode>> {
        assert_ne!(instance, "panic", "the plugin panics giving its modes");
        let mode = PanelMode {
            columns: "N,C0".to_string(),
            widths: "0,5".to_string(),
            titles: vec![None, Some(String::new()), Some("Kind".to_string())],
            status_columns: "C0".to_string(),
            status_widths: "6".to_string(),
            full_screen: true,
            case_conversion: true,
            ..PanelMode::default()
        };
        if instance == "many" {
            return vec![Some(mode); 12];
        }
        let empty = PanelMode {
            columns: String::new(),
            ..mode.clone()
        };
        let detailed = PanelMode {
            detailed_status: true,
            align_extensions: true,
            titles: Vec::new(),
            ..mode.clone()
        };
        vec![None, Some(mode), Some(empty), Some(detailed)]
    }
}

/// `count` texts at `list` (null: none).
fn texts(list: *const NcStr, count: usize) -> Vec<String> {
    if list.is_null() {
        return Vec::new();
    }
    // SAFETY: the plugin's list, alive until its next call; copied at once.
    unsafe { std::slice::from_raw_parts(list, count) }
        .iter()
        .map(|raw| text(*raw))
        .collect()
}

#[test]
fn an_entrys_texts_go_with_it_and_none_are_null() {
    let host = host(API_VERSION);
    // SAFETY: the host's table outlives the plugin; released at the end.
    let table = unsafe { &*entry::<Shown>(&raw const host) };
    let (code, entries) = listed(table, "/");
    assert_eq!(code, 0);
    assert_eq!(
        texts(entries[0].columns, entries[0].column_count),
        ["kind", "", "3"]
    );
    assert_eq!(text(entries[0].description), "About");
    assert!(entries[1].columns.is_null() && entries[1].column_count == 0);
    assert_eq!(entries[1].description.len, 0, "none: empty");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_panels_modes_go_by_key_with_their_flags_and_titles() {
    let host = host(API_VERSION);
    // SAFETY: the host's table outlives the plugin; released at the end.
    let table = unsafe { &*entry::<Shown>(&raw const host) };
    let info = |instance: &str| {
        // SAFETY: the table's own state; the texts live through the call.
        unsafe { (table.panel_info)(table.state, NcStr::new(instance), NcStr::new("/")) }
    };
    let given = info("main");
    assert_eq!(text(given.title), "Shown");
    // SAFETY: the plugin's modes, alive until its next call.
    let modes = unsafe { std::slice::from_raw_parts(given.modes, given.mode_count) }.to_vec();
    assert_eq!(modes.len(), 4);
    for at in [0, 2] {
        assert_eq!(text(modes[at].columns), "", "{at}: none there");
        assert!(modes[at].titles.is_null() && modes[at].flags == 0);
    }
    let first = modes[1];
    assert_eq!(
        [
            first.columns,
            first.widths,
            first.status_columns,
            first.status_widths
        ]
        .map(text),
        ["N,C0", "0,5", "C0", "6"]
    );
    assert_eq!(texts(first.titles, first.title_count), ["", "", "Kind"]);
    // SAFETY: the plugin's list, alive until its next call.
    let raw = unsafe { std::slice::from_raw_parts(first.titles, first.title_count) };
    let null: Vec<bool> = raw.iter().map(|title| title.ptr.is_null()).collect();
    assert_eq!(
        null,
        [true, false, false],
        "none null, an empty one not (FAR's ColumnTitles)"
    );
    assert_eq!(first.flags, MODE_FULL_SCREEN | MODE_CASE_CONVERSION);
    let detailed = modes[3];
    assert_eq!(
        detailed.flags,
        MODE_FULL_SCREEN | MODE_DETAILED_STATUS | MODE_ALIGN_EXTENSIONS | MODE_CASE_CONVERSION
    );
    assert!(detailed.titles.is_null() && detailed.title_count == 0);
    assert_eq!(info("many").mode_count, 10, "Ctrl+0..9");
    let panicked = info("panic");
    assert!(
        panicked.modes.is_null() && panicked.mode_count == 0,
        "a panic: none"
    );
    assert_eq!(text(panicked.title), "Shown", "the title stands");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn one_entry_asked_carries_its_texts_too() {
    let host = host(API_VERSION);
    // SAFETY: the host's table outlives the plugin; released at the end.
    let table = unsafe { &*entry::<Shown>(&raw const host) };
    let mut found = NcEntry::EMPTY;
    // SAFETY: the table's own state; the texts and the answer's place live through the call.
    let code = unsafe {
        (table.stat)(
            table.state,
            NcStr::new("main"),
            NcStr::new("/a"),
            &raw mut found,
        )
    };
    assert_eq!(code, 0);
    assert_eq!(text(found.name), "a");
    assert_eq!(texts(found.columns, found.column_count), ["kind", "", "3"]);
    assert_eq!(text(found.description), "About");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}

#[test]
fn a_panels_info_lines_go_with_their_separators_and_a_panic_gives_none() {
    let host = host(API_VERSION);
    // SAFETY: the host's table outlives the plugin; released at the end.
    let table = unsafe { &*entry::<Shown>(&raw const host) };
    // SAFETY: the table's own state; the texts live through the call.
    let given = unsafe { (table.panel_info)(table.state, NcStr::new("main"), NcStr::new("/a")) };
    // SAFETY: the plugin's lines, alive until its next call.
    let lines = unsafe { std::slice::from_raw_parts(given.info_lines, given.info_line_count) };
    let read: Vec<(String, String, u32)> = (lines.iter())
        .map(|line| (text(line.text), text(line.data), line.flags))
        .collect();
    assert_eq!(
        read,
        [
            (
                "Server".to_string(),
                String::new(),
                crate::api::INFO_SEPARATOR
            ),
            ("Path".to_string(), "/a".to_string(), 0),
        ]
    );
    // SAFETY: as above.
    let panicked = unsafe { (table.panel_info)(table.state, NcStr::new("panic"), NcStr::new("/")) };
    assert_eq!(panicked.info_line_count, 0, "a panic: none");
    // SAFETY: the table's own state, released once.
    unsafe { (table.release)(table.state) };
}
