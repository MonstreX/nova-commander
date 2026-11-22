use super::*;

fn folders(list: &[&str]) -> Vec<String> {
    list.iter().map(|folder| folder.to_string()).collect()
}

fn shown(lines: &[Line]) -> Vec<String> {
    (lines.iter())
        .map(|line| line.folder.clone().unwrap_or_else(|| "-".to_string()))
        .collect()
}

#[test]
fn bookmarks_are_kept_per_session() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("sub").join(FILE);
    let marks = |list: &[&str]| -> Vec<Mark> {
        folders(list)
            .iter()
            .map(|folder| Mark::remote(folder))
            .collect()
    };
    assert!(load(&file, "a").is_empty());
    save_marks(&file, "a", &marks(&["/x", "/y"])).unwrap();
    save_marks(&file, "b", &marks(&["/z"])).unwrap();
    save_marks(&file, "a", &marks(&["/w"])).unwrap();
    assert_eq!(load(&file, "a"), ["/w"]);
    assert_eq!(load(&file, "b"), ["/z"]);
    save_marks(&file, "b", &[]).unwrap();
    assert!(load(&file, "b").is_empty());
    // Location profiles' bookmarks keep their names and local folders; S6.6d's lines still read.
    let profile = Mark {
        name: "work".to_string(),
        remote: "/srv".to_string(),
        local: r"C:\work".to_string(),
    };
    save_marks(&file, "", std::slice::from_ref(&profile)).unwrap();
    assert_eq!(load_marks(&file, ""), [profile]);
    assert_eq!(load(&file, "a"), ["/w"], "the other keys kept");
    std::fs::write(&file, "old\t/plain\n").unwrap();
    assert_eq!(load_marks(&file, "old"), [Mark::remote("/plain")]);
}

/// A name, a folder or a session's key with a tab, a line end or `%` in it reads back as it was; the
/// other lines are not moved.
#[test]
fn tabs_and_line_ends_in_a_bookmark_are_kept() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join(FILE);
    let odd = Mark {
        name: "tab\there\nand 100% %09".to_string(),
        remote: "/a\tb".to_string(),
        local: r"C:\temp\new".to_string(),
    };
    save_marks(&file, "key\twith tab", std::slice::from_ref(&odd)).unwrap();
    save_marks(&file, "other", &[Mark::remote("/o")]).unwrap();
    assert_eq!(load_marks(&file, "key\twith tab"), [odd]);
    assert_eq!(load(&file, "other"), ["/o"]);
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.lines().count(), 2);
    assert_eq!(unescaped("50% off %zz"), "50% off %zz", "a lone % stays");
}

/// Each kept character has its own code (a carriage return too); a `%` that starts no code stays, at the
/// end, before one character only, before a lower-case or a non-ASCII one; a key with a tab saved again
/// replaces its old lines.
#[test]
fn every_escape_reads_back_and_odd_percents_stay() {
    assert_eq!(escaped("a%b\tc\nd\re"), "a%25b%09c%0Ad%0De");
    assert_eq!(unescaped("a%25b%09c%0Ad%0De"), "a%b\tc\nd\re");
    for odd in ["100%", "%0", "x%2", "%0a", "%0d", "%%", "%é", "%0é", "%Z9"] {
        assert_eq!(unescaped(odd), odd, "{odd:?}");
    }
    assert_eq!(unescaped("%%25"), "%%", "a lone % then a code");
    assert_eq!(unescaped("%2525"), "%25", "read once");
    for field in ["", "%", "%25", "\r\n", "50%\t%0A", "é%é"] {
        assert_eq!(unescaped(&escaped(field)), field, "{field:?}");
    }
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join(FILE);
    let key = "user@host\twith tab";
    save_marks(&file, key, &[Mark::remote("/one")]).unwrap();
    save_marks(&file, key, &[Mark::remote("/two\r")]).unwrap();
    assert_eq!(load(&file, key), ["/two\r"]);
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.lines().count(), 1, "{text:?}");
}

/// NetBox's menu: the history, a separator, the bookmarks sorted once each, the cursor on the current
/// folder's bookmark.
#[test]
fn the_menu_lists_the_history_then_the_bookmarks() {
    let history = folders(&["/a", "/b"]);
    let bookmarks = folders(&["/z", "/c", "/z", ""]);
    let (lines, focused) = lines(&history, &bookmarks, "/z");
    assert_eq!(shown(&lines), ["/a", "/b", "-", "/c", "/z"]);
    assert_eq!(focused, 4);
    assert!(lines[3].bookmark && !lines[0].bookmark);
    let (lines, focused) = lines_of(&history, &bookmarks, "/elsewhere");
    assert_eq!((focused, lines.len()), (3, 5), "the first bookmark");
    let (lines, focused) = lines_of(&history, &[], "/a");
    assert_eq!(shown(&lines), ["/a", "/b", ""]);
    assert_eq!(focused, 2, "the empty line");
    let (lines, focused) = lines_of(&[], &folders(&["/c"]), "/q");
    assert_eq!((shown(&lines), focused), (vec!["/c".to_string()], 0));
}

fn lines_of(history: &[String], bookmarks: &[String], current: &str) -> (Vec<Line>, usize) {
    lines(history, bookmarks, current)
}

#[test]
fn only_the_last_forty_of_the_history_are_shown() {
    let history: Vec<String> = (0..50).map(|at| format!("/h{at}")).collect();
    // NetBox's: the last 39 past 40, and the empty line.
    let (shown, _) = lines_of(&history, &[], "/");
    assert_eq!(shown.len(), MAX_HISTORY - 1 + 1);
    assert_eq!(shown[0].folder.as_deref(), Some("/h11"));
    let (whole, _) = lines_of(&history[..40], &[], "/");
    assert_eq!(whole.len(), 41, "40 kept whole");
}
