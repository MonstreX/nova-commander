//! "Menu from file list" as FAR's `ShowMenuFromList` builds it: labels, separators, commands, the title,
//! and what Enter on a command does.

use super::*;

fn texts(lines: &[&str]) -> Vec<(String, bool, String)> {
    let lines: Vec<String> = lines.iter().map(ToString::to_string).collect();
    (entries(&lines).into_iter())
        .map(|(item, command)| (item.text, item.separator, command))
        .collect()
}

#[test]
fn a_label_is_shown_and_its_command_run_a_bare_line_is_both() {
    let made = texts(&[r"|&Docs|  C:\docs", "|-|", r"C:\bare.txt", "|Only label|"]);
    let entry = |text: &str, separator: bool, command: &str| {
        (text.to_string(), separator, command.to_string())
    };
    assert_eq!(
        made,
        [
            entry("&Docs", false, r"C:\docs"),
            entry("", true, "|-|"),
            entry(r"C:\bare.txt", false, r"C:\bare.txt"),
            entry("Only label", false, "|Only label|"),
        ]
    );
}

#[cfg(windows)]
#[test]
fn variables_are_expanded_in_labels_and_commands() {
    // `OS` is "Windows_NT" on every Windows.
    let made = texts(&[r"|%OS%|C:\%OS%\a"]);
    assert_eq!(made[0].0, "Windows_NT");
    assert_eq!(made[0].2, r"C:\Windows_NT\a");
}

#[test]
fn the_title_is_the_files_name_without_its_extension() {
    assert_eq!(title(r"C:\lists\shortcuts.eng.temp"), "shortcuts.eng");
    assert_eq!(title(r"C:\lists\plain"), "plain");
    assert_eq!(title(""), "");
}

#[test]
fn enter_goes_to_a_folder_opens_a_file_or_fills_the_command_line() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    let folder = Some(temp.path());
    assert_eq!(
        chosen("sub", folder),
        KeyGo::ActiveFolder(temp.path().join("sub"))
    );
    // 5d-4 review: by its whole path, not from the program's own process folder.
    assert_eq!(
        chosen("a.txt", folder),
        KeyGo::ShellOpen(temp.path().join("a.txt"))
    );
    assert_eq!(shell_target("a.txt", folder), temp.path().join("a.txt"));
    assert_eq!(
        shell_target("https://example.com", folder),
        PathBuf::from("https://example.com"),
        "Shift+Enter: anything else as written"
    );
    assert_eq!(
        chosen("notepad readme", folder),
        KeyGo::CommandLine("notepad readme".to_string())
    );
    assert_eq!(
        chosen(r"\\.\C:", folder),
        KeyGo::ShellOpen(PathBuf::from(r"\\.\C:")),
        "a device is a file to FAR"
    );
}
