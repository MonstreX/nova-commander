use super::*;

#[test]
fn variables_are_expanded_and_unknown_ones_kept() {
    let path = std::env::var("PATH").unwrap();
    assert_eq!(expand("a%PATH%b"), format!("a{path}b"));
    assert_eq!(expand("%NO_SUCH_VC_VARIABLE%x"), "%NO_SUCH_VC_VARIABLE%x");
    assert_eq!(expand("100% sure"), "100% sure");
    assert_eq!(expand("%%PATH%"), format!("%{path}"));
}

#[test]
fn names_split_at_spaces_out_of_quotes_and_at_quotes() {
    assert_eq!(names("a.txt  b.txt"), ["a.txt", "b.txt"]);
    assert_eq!(names("\"my file.txt\"\tb"), ["my file.txt", "b"]);
    assert_eq!(names("open\"x y\"z"), ["open", "x y", "z"]);
    assert!(names("   ").is_empty());
}

#[test]
fn a_name_without_a_folder_is_in_the_current_one() {
    let current = Path::new(r"E:\work");
    assert_eq!(whole("a.txt", current), PathBuf::from(r"E:\work\a.txt"));
    assert_eq!(whole(r"C:\x\b.txt", current), PathBuf::from(r"C:\x\b.txt"));
    assert_eq!(whole("C:", current), PathBuf::from("C:"));
    assert_eq!(drive_root("c:"), Some(r"c:\".to_string()));
    assert_eq!(drive_root(r"D:\"), Some(r"D:\".to_string()));
    assert_eq!(drive_root(r"D:\x"), None);
    assert_eq!(drive_root("ab"), None);
}

fn target(path: &str, folder: bool) -> Target {
    Target {
        path: PathBuf::from(path),
        folder,
    }
}

#[test]
fn files_of_two_folders_are_refused() {
    let same = [target(r"E:\w\a.txt", false), target(r"e:\W\sub", true)];
    assert!(one_folder(&same), "case does not matter");
    let apart = [target(r"E:\w\a.txt", false), target(r"E:\v\b.txt", false)];
    assert!(!one_folder(&apart));
    assert!(!one_folder(&[
        target(r"C:\", true),
        target(r"E:\w\a.txt", false)
    ]));
    assert!(
        one_folder(&[target(r"C:\", true), target(r"D:\", true)]),
        "drives: This PC's"
    );
}

#[test]
fn the_title_is_the_one_name_or_the_counts() {
    assert_eq!(title(&[target(r"E:\w\a.txt", false)]), "a.txt");
    assert_eq!(title(&[target(r"E:\w\sub", true)]), r"sub\");
    assert_eq!(title(&[target(r"C:\", true)]), r"C:\");
    let three = [
        target(r"E:\w\a", false),
        target(r"E:\w\b", false),
        target(r"E:\w\c", true),
    ];
    assert_eq!(title(&three), "2 file(s), 1 folder(s)");
    assert_eq!(title(&three[..2]), "2 file(s)");
    assert_eq!(
        title(&[target(r"E:\w\c", true), target(r"E:\w\d", true)]),
        "2 folder(s)"
    );
}

#[test]
fn texts_compare_but_for_hotkeys_and_case() {
    assert!(same_but_hotkeys("&Open", "open"));
    assert!(same_but_hotkeys("Pr&operties", "PROPERTIES"));
    assert!(!same_but_hotkeys("Open", "Open with"));
}
