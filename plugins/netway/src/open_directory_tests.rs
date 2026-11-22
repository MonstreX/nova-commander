use super::*;

fn line(folder: Option<&str>, bookmark: bool) -> Line {
    Line {
        folder: folder.map(str::to_string),
        bookmark,
    }
}

/// NetBox's keys: Enter goes, Del / F8 remove a bookmark or clear the history, Ctrl+Enter the command
/// line, Ctrl+C / Ctrl+Ins the clipboard; the empty line and Esc do nothing.
#[test]
fn the_keys_ask_what_netbox_does() {
    let mark = line(Some("/b"), true);
    let past = line(Some("/h"), false);
    assert_eq!(asked(Some(&mark), None), Asked::Go("/b".to_string()));
    assert_eq!(asked(Some(&mark), Some(0)), Asked::Remove("/b".to_string()));
    assert_eq!(asked(Some(&mark), Some(1)), Asked::Remove("/b".to_string()));
    assert_eq!(asked(Some(&past), Some(1)), Asked::ClearHistory);
    assert_eq!(
        asked(Some(&past), Some(2)),
        Asked::CommandLine("/h".to_string())
    );
    assert_eq!(
        asked(Some(&past), Some(4)),
        Asked::Clipboard("/h".to_string())
    );
    assert_eq!(asked(Some(&line(Some(""), false)), None), Asked::Nothing);
    assert_eq!(asked(None, None), Asked::Nothing);
}

#[test]
fn the_edit_history_lists_the_files_and_an_empty_line() {
    let edits = [
        ("/a".to_string(), "x.txt".to_string()),
        ("/".to_string(), "y.txt".to_string()),
    ];
    let menu = edit_history_menu(&edits);
    let texts: Vec<&str> = menu.items.iter().map(|item| item.text.as_str()).collect();
    assert_eq!(texts, ["/a/x.txt", "/y.txt", ""]);
    assert_eq!((menu.title.as_str(), menu.selected), ("Edit history", 2));
    assert_eq!(menu.break_keys, ["F4"]);
}

#[test]
fn the_menu_is_netboxs() {
    let lines = [
        line(Some("/h"), false),
        line(None, false),
        line(Some("/b"), true),
    ];
    let menu = menu_of(&lines, 2, false);
    assert_eq!(menu.title, "Open directory");
    assert_eq!(
        menu.bottom,
        "Del - remove, Ctrl-Enter - command line, Ctrl-Ins - clipboard"
    );
    assert!(menu.items[1].separator);
    assert_eq!((menu.selected, menu.break_keys.len()), (2, 5));
    assert!(menu.show_ampersand && menu.reverse_auto_hotkeys && menu.wrap);
    assert_eq!(menu_of(&lines, 0, true).title, "Manage bookmarks");
}
