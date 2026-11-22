use std::ffi::c_void;
use std::path::Path;

use crate::api::{
    self, API_VERSION, KEY_ACTIVE_FOLDER, KEY_OPEN, KEY_PASS, KEY_SHELL_OPEN, NcKeyAnswer,
    NcPlugin, NcStr, PLACE_FILE_MASKS, PLACE_PREFIXES,
};
use crate::tests::{host, text};
use crate::{CodePage, Error, Host, Info, KeyAnswer, KeyGo, Plugin, entry};

/// Opens by a command and by a file, telling what it was given.
struct Opener;

impl Plugin for Opener {
    fn new(_host: Host) -> Self {
        Self
    }

    fn info(&self) -> Info {
        Info::default()
    }

    fn prefixes(&self) -> Vec<String> {
        vec!["tmp".to_string(), "list".to_string()]
    }

    fn file_masks(&self) -> Vec<String> {
        vec!["*.temp".to_string()]
    }

    fn takes_find_list(&self) -> bool {
        true
    }

    fn find_list(&mut self, found: &[crate::FileEntry]) -> Result<KeyAnswer, Error> {
        // Each found object told back: its name, its size, "/" for a folder, "!" for its real name.
        let names: Vec<String> = (found.iter())
            .map(|entry| {
                let folder = if entry.is_dir { "/" } else { "" };
                let real = entry
                    .real_path
                    .as_ref()
                    .map(|path| path.display().to_string());
                let named = if real.as_deref() == Some(entry.name.as_str()) {
                    "!"
                } else {
                    ""
                };
                format!("{}={}{folder}{named}", entry.name, entry.size)
            })
            .collect();
        Ok(KeyAnswer::go(KeyGo::Open(names.join("|"))))
    }

    fn open_command(
        &mut self,
        prefix: &str,
        command: &str,
        folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        Ok(match command {
            "fail" => return Err(Error::from("no such list".to_string())),
            "panic" => panic!("the plugin panics on a command"),
            "folder" => KeyAnswer::go(KeyGo::ActiveFolder(folder.unwrap().to_path_buf())),
            _ => KeyAnswer::go(KeyGo::Open(format!("{prefix}|{command}|{folder:?}"))),
        })
    }

    fn open_file(
        &mut self,
        path: &Path,
        how: crate::OpenHow,
        _folder: Option<&Path>,
    ) -> Result<KeyAnswer, Error> {
        Ok(match path.extension().and_then(|ext| ext.to_str()) {
            Some("temp") => KeyAnswer::go(KeyGo::Open(format!("{}|{how:?}", path.display()))),
            Some("menu") => KeyAnswer::go(KeyGo::ShellOpen(path.to_path_buf())),
            Some("stop") => KeyAnswer::default(),
            _ => KeyAnswer::pass(),
        })
    }
}

fn table(host: &api::NcHost) -> &'static NcPlugin {
    // SAFETY: the host's table outlives the plugin in each test.
    unsafe { &*entry::<Opener>(host) }
}

fn items(table: &NcPlugin, place: u32) -> Vec<String> {
    // SAFETY: the table's own state; each item is copied before the next call.
    unsafe {
        let count = (table.item_count)(table.state, place);
        (0..count)
            .map(|index| text((table.item)(table.state, place, index)))
            .collect()
    }
}

fn command(table: &NcPlugin, prefix: &str, line: &str, folder: &str) -> (i32, u32, String) {
    let mut answer = NcKeyAnswer::NONE;
    // SAFETY: the table's own state; the texts and the answer's place live through the call.
    let code = unsafe {
        (table.open_command)(
            table.state,
            NcStr::new(prefix),
            NcStr::new(line),
            NcStr::new(folder),
            &raw mut answer,
        )
    };
    (code, answer.flags, text(answer.path))
}

fn file(table: &NcPlugin, path: &str, alternative: u32) -> (i32, u32, String) {
    let mut answer = NcKeyAnswer::NONE;
    // SAFETY: as in `command`.
    let code = unsafe {
        (table.open_file)(
            table.state,
            NcStr::new(path),
            alternative,
            NcStr::EMPTY,
            &raw mut answer,
        )
    };
    (code, answer.flags, text(answer.path))
}

#[test]
fn prefixes_and_masks_are_items_of_their_places() {
    let host = host(API_VERSION);
    let table = table(&host);
    assert_eq!(items(table, PLACE_PREFIXES), ["tmp", "list"]);
    assert_eq!(items(table, PLACE_FILE_MASKS), ["*.temp"]);
    assert_eq!(
        items(table, api::PLACE_FIND_LIST),
        [""],
        "it takes found files"
    );
}

#[test]
fn found_files_come_as_whole_paths_and_a_plugin_without_them_passes() {
    let host = host(API_VERSION);
    let table = table(&host);
    // What the search saw of each, named by its whole path, which is its real name.
    let found = [
        api::NcEntry {
            name: NcStr::new(r"C:\a.txt"),
            size: 12,
            real_path: NcStr::new(r"C:\a.txt"),
            ..api::NcEntry::EMPTY
        },
        api::NcEntry {
            name: NcStr::new(r"D:\sub"),
            flags: api::ENTRY_DIR,
            real_path: NcStr::new(r"D:\sub"),
            ..api::NcEntry::EMPTY
        },
    ];
    let mut answer = NcKeyAnswer::NONE;
    // SAFETY: the table's own state; the entries and the answer's place live through the call.
    let code = unsafe { (table.find_list)(table.state, found.as_ptr(), 2, &raw mut answer) };
    assert_eq!((code, answer.flags), (0, KEY_OPEN));
    assert_eq!(text(answer.path), r"C:\a.txt=12!|D:\sub=0/!");
    // A plugin of the SDK's defaults: no item, the files passed on.
    let plain = crate::tests::host(API_VERSION);
    // SAFETY: the host's table outlives the plugin in this test.
    let other = unsafe { &*entry::<crate::tests::Counter>(&raw const plain) };
    assert!(items(other, api::PLACE_FIND_LIST).is_empty());
    let mut answer = NcKeyAnswer::NONE;
    // SAFETY: as above, no paths.
    let code = unsafe { (other.find_list)(other.state, std::ptr::null(), 0, &raw mut answer) };
    assert_eq!((code, answer.flags), (0, KEY_PASS));
}

#[test]
fn a_command_comes_with_its_prefix_and_folder_and_answers_where_to_go() {
    let host = host(API_VERSION);
    let table = table(&host);
    let (code, flags, path) = command(table, "list", "+menu a.temp", r"C:\work");
    assert_eq!(code, 0);
    assert_eq!(flags, KEY_OPEN);
    assert_eq!(path, r#"list|+menu a.temp|Some("C:\\work")"#);
    // An empty folder is none.
    assert_eq!(command(table, "tmp", "", "").2, "tmp||None");
    assert_eq!(
        command(table, "tmp", "folder", r"C:\work"),
        (0, KEY_ACTIVE_FOLDER, r"C:\work".to_string())
    );
}

#[test]
fn a_commands_failure_and_panic_are_its_error() {
    let host = host(API_VERSION);
    let table = table(&host);
    let (code, _, _) = command(table, "tmp", "fail", "");
    assert_ne!(code, 0);
    // SAFETY: the table's own state.
    let error = text(unsafe { (table.last_error)(table.state) });
    assert_eq!(error, "no such list");
    let (code, _, _) = command(table, "tmp", "panic", "");
    assert_eq!(code, api::ERROR_OTHER, "a panic is the call's failure");
    // SAFETY: as above.
    let error = text(unsafe { (table.last_error)(table.state) });
    assert!(error.starts_with("The plugin failed"), "{error}");
}

#[test]
fn a_file_is_opened_passed_on_or_done_by_the_plugin() {
    let host = host(API_VERSION);
    let table = table(&host);
    assert_eq!(
        file(table, r"C:\a.temp", 1),
        (0, KEY_OPEN, r"C:\a.temp|PgDn".to_string())
    );
    assert_eq!(file(table, r"C:\a.temp", 0).2, r"C:\a.temp|Enter");
    assert_eq!(file(table, r"C:\a.temp", 2).2, r"C:\a.temp|Extract");
    assert_eq!(file(table, r"C:\a.temp", 3).2, r"C:\a.temp|Commands");
    assert_eq!(
        file(table, r"C:\a.temp", 9).2,
        r"C:\a.temp|Enter",
        "unknown"
    );
    assert_eq!(file(table, r"C:\a.txt", 0), (0, KEY_PASS, String::new()));
    assert_eq!(file(table, r"C:\a.stop", 0), (0, 0, String::new()));
    assert_eq!(
        file(table, r"C:\a.menu", 0),
        (0, KEY_SHELL_OPEN, r"C:\a.menu".to_string())
    );
}

/// The host's decode: the bytes as Latin-1 for page 1 (the OEM one here), none for another page.
unsafe extern "C" fn decode(
    context: *mut c_void,
    code_page: u32,
    bytes: *const u8,
    len: usize,
    out: *mut NcStr,
) -> i32 {
    if code_page != api::CODE_PAGE_OEM {
        return -1;
    }
    // SAFETY: the tests' context is a live `String`; the plugin's bytes live through the call.
    unsafe {
        let kept = &mut *context.cast::<String>();
        *kept = std::slice::from_raw_parts(bytes, len)
            .iter()
            .map(|byte| char::from(*byte))
            .collect();
        *out = NcStr::new(kept);
    }
    0
}

#[test]
fn decode_asks_the_host_for_its_code_pages() {
    let mut kept = String::new();
    let table = api::NcHost {
        context: (&raw mut kept).cast(),
        decode,
        ..host(API_VERSION)
    };
    let host = Host {
        table: &raw const table,
    };
    assert_eq!(
        host.decode(CodePage::Oem, &[0x61, 0xE9]).as_deref(),
        Some("a\u{e9}")
    );
    assert_eq!(host.decode(CodePage::Ansi, b"a"), None);
    assert_eq!(host.decode(CodePage::Number(866), b"a"), None);
}

#[test]
fn a_found_entry_tells_its_times_and_flags() {
    // The host's milliseconds from 1970 (before it negative, none as TIME_NONE) and its flags.
    let raw = api::NcEntry {
        name: NcStr::new(r"C:\x.txt"),
        flags: api::ENTRY_HIDDEN | api::ENTRY_READ_ONLY,
        size: 3,
        modified: 1_500,
        created: -2_500,
        accessed: api::TIME_NONE,
        real_path: NcStr::EMPTY,
        ..api::NcEntry::EMPTY
    };
    // SAFETY: the entry's texts live through the call.
    let found = unsafe { super::found_entry(&raw) };
    let epoch = std::time::UNIX_EPOCH;
    assert_eq!(
        found.modified,
        Some(epoch + std::time::Duration::from_millis(1_500))
    );
    assert_eq!(
        found.created,
        Some(epoch - std::time::Duration::from_millis(2_500))
    );
    assert_eq!(found.accessed, None);
    assert_eq!(
        (found.hidden, found.system, found.read_only, found.is_dir),
        (true, false, true, false)
    );
    assert_eq!(found.real_path, None, "no real name given");
    assert_eq!(found.size, 3);
}

#[test]
fn each_open_code_is_its_reason() {
    use crate::OpenHow;
    let reasons = [
        (api::OPEN_ENTER, OpenHow::Enter),
        (api::OPEN_PGDN, OpenHow::PgDn),
        (api::OPEN_EXTRACT, OpenHow::Extract),
        (api::OPEN_COMMANDS, OpenHow::Commands),
        (api::OPEN_SEARCH, OpenHow::Search),
        (99, OpenHow::Enter),
    ];
    for (code, how) in reasons {
        assert_eq!(OpenHow::of(code), how, "code {code}");
    }
    assert_eq!(api::OPEN_SEARCH, 4, "API 6's code");
}
