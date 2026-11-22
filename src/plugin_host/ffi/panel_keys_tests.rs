//! A plugin's answer to a key as the program's: where to go, the flags read one by one.

use super::*;

fn raw(flags: u32, path: &'static str) -> NcKeyAnswer {
    NcKeyAnswer {
        flags,
        path: NcStr::new(path),
    }
}

#[test]
fn the_answer_names_one_place_and_its_rereads() {
    assert_eq!(answer_of(&raw(0, "")), KeyAnswer::default());
    assert_eq!(
        answer_of(&raw(KEY_REREAD | KEY_REREAD_PASSIVE, "")),
        KeyAnswer {
            reread: true,
            reread_passive: true,
            // 5d-3b's fields, none here.
            ..KeyAnswer::default()
        }
    );
    let folder = answer_of(&raw(KEY_PASSIVE_FOLDER, "D:\\work"));
    assert_eq!(
        folder.go,
        Some(KeyGo::PassiveFolder(PathBuf::from("D:\\work")))
    );
    assert!(!folder.reread);
    let file = answer_of(&raw(KEY_PASSIVE_FILE, "C:\\a.txt"));
    assert_eq!(
        file.go,
        Some(KeyGo::PassiveFile(PathBuf::from("C:\\a.txt")))
    );
    let open = answer_of(&raw(KEY_OPEN | KEY_REREAD, "3"));
    assert_eq!(
        (open.go, open.reread),
        (Some(KeyGo::Open("3".to_string())), true)
    );
    assert_eq!(
        answer_of(&raw(KEY_PASSIVE_FOLDER, "")).go,
        None,
        "no path: nowhere to go"
    );
    assert_eq!(
        answer_of(&raw(KEY_REREAD_PASSIVE, "x")).go,
        None,
        "a path with no place"
    );
}

#[test]
fn the_active_panel_the_command_line_and_a_key_passed() {
    let folder = answer_of(&raw(KEY_ACTIVE_FOLDER, "D:\\work"));
    assert_eq!(
        folder.go,
        Some(KeyGo::ActiveFolder(PathBuf::from("D:\\work")))
    );
    let file = answer_of(&raw(KEY_ACTIVE_FILE | KEY_REREAD_QUICK_VIEW, "C:\\a.txt"));
    assert_eq!(
        (file.go, file.reread_quick_view),
        (Some(KeyGo::ActiveFile(PathBuf::from("C:\\a.txt"))), true)
    );
    assert_eq!(
        answer_of(&raw(KEY_COMMAND_LINE, "")).go,
        Some(KeyGo::CommandLine(String::new())),
        "an empty command line is one"
    );
    assert_eq!(
        answer_of(&raw(KEY_COMMAND_LINE, "echo x")).go,
        Some(KeyGo::CommandLine("echo x".to_string()))
    );
    let passed = answer_of(&raw(KEY_PASS | KEY_REREAD | KEY_ACTIVE_FOLDER, "D:\\work"));
    assert_eq!(
        passed,
        KeyAnswer {
            pass: true,
            ..KeyAnswer::default()
        },
        "a key passed is only that"
    );
}

#[test]
fn a_file_opened_by_the_system_is_a_place_too() {
    let opened = answer_of(&raw(KEY_SHELL_OPEN, r"C:\list.temp"));
    assert_eq!(
        opened.go,
        Some(KeyGo::ShellOpen(PathBuf::from(r"C:\list.temp")))
    );
    assert_eq!(
        answer_of(&raw(KEY_SHELL_OPEN, "")).go,
        None,
        "no path: nowhere"
    );
}
