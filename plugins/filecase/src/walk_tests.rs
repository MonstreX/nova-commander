use super::*;
use crate::case::Mode;

fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = (std::fs::read_dir(folder).unwrap().flatten())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn upper() -> Rules {
    Rules {
        name: Mode::Upper,
        extension: Mode::Lower,
        skip_mixed: false,
        ..Rules::default()
    }
}

#[test]
fn a_file_changes_its_case_in_place() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("read me.TXT");
    std::fs::write(&file, "kept").unwrap();
    assert_eq!(convert(&file, false, &upper(), false, &|| false), 1);
    assert_eq!(names(temp.path()), ["READ ME.txt"]);
    let read = std::fs::read_to_string(temp.path().join("READ ME.txt")).unwrap();
    assert_eq!(read, "kept");
}

#[test]
fn subfolders_go_every_level_down_the_folders_names_as_asked() {
    let temp = tempfile::tempdir().unwrap();
    let top = temp.path().join("top");
    std::fs::create_dir_all(top.join("inner").join("deep")).unwrap();
    std::fs::write(top.join("a.c"), "").unwrap();
    std::fs::write(top.join("inner").join("deep").join("b.h"), "").unwrap();
    let rules = upper();
    assert_eq!(
        convert(&top, true, &rules, true, &|| false),
        2,
        "files only"
    );
    assert_eq!(names(&top), ["A.c", "inner"]);
    assert_eq!(names(&top.join("inner").join("deep")), ["B.h"]);
    let folders = Rules {
        folder_names: true,
        ..rules
    };
    assert_eq!(convert(&top, true, &folders, true, &|| false), 3);
    assert_eq!(names(temp.path()), ["TOP"]);
    assert_eq!(names(&temp.path().join("TOP")), ["A.c", "INNER"]);
    assert_eq!(names(&temp.path().join("TOP").join("INNER")), ["DEEP"]);
}

#[test]
fn without_subfolders_a_folders_content_stays() {
    let temp = tempfile::tempdir().unwrap();
    let top = temp.path().join("top");
    std::fs::create_dir(&top).unwrap();
    std::fs::write(top.join("a.c"), "").unwrap();
    assert_eq!(convert(&top, true, &upper(), false, &|| false), 0);
    assert_eq!(names(&top), ["a.c"]);
}

#[test]
fn asked_to_stop_it_changes_nothing_more() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("a.c");
    std::fs::write(&file, "").unwrap();
    assert_eq!(convert(&file, false, &upper(), false, &|| true), 0);
    assert_eq!(names(temp.path()), ["a.c"]);
}

/// A junction at `link` to `target` (made without elevation).
#[cfg(windows)]
fn junction(link: &Path, target: &Path) {
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(made.status.success(), "{made:?}");
}

#[cfg(windows)]
#[test]
fn a_link_to_a_folder_is_never_gone_into() {
    let temp = tempfile::tempdir().unwrap();
    let outside = temp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("out.txt"), "").unwrap();
    let top = temp.path().join("top");
    std::fs::create_dir(&top).unwrap();
    junction(&top.join("link"), &outside);
    let folders = Rules {
        folder_names: true,
        ..upper()
    };
    assert_eq!(
        convert(&top, true, &folders, true, &|| false),
        2,
        "the link and the folder"
    );
    assert_eq!(names(&temp.path().join("TOP")), ["LINK"]);
    assert_eq!(names(&outside), ["out.txt"], "nothing renamed through it");
    // A link selected itself: renamed, not gone into.
    let selected = temp.path().join("selected");
    junction(&selected, &outside);
    assert_eq!(convert(&selected, true, &folders, true, &|| false), 1);
    assert_eq!(names(&outside), ["out.txt"]);
    for link in [
        temp.path().join("TOP").join("LINK"),
        temp.path().join("SELECTED"),
    ] {
        std::fs::remove_dir(link).unwrap();
    }
}

#[test]
fn a_folder_inside_keeps_its_name_without_folders_names() {
    let temp = tempfile::tempdir().unwrap();
    let top = temp.path().join("top");
    std::fs::create_dir_all(top.join("inner")).unwrap();
    assert_eq!(convert(&top, true, &upper(), true, &|| false), 0);
    assert_eq!(names(&top), ["inner"]);
}

#[test]
fn stopped_inside_a_folder_the_folders_above_stay() {
    let temp = tempfile::tempdir().unwrap();
    let top = temp.path().join("top");
    std::fs::create_dir(&top).unwrap();
    std::fs::write(top.join("a.c"), "").unwrap();
    let folders = Rules {
        folder_names: true,
        ..upper()
    };
    let calls = std::cell::Cell::new(0);
    // Stopped after the folder's content was looked at and its file renamed.
    let stop = || {
        calls.set(calls.get() + 1);
        calls.get() > 2
    };
    assert_eq!(convert(&top, true, &folders, true, &stop), 1);
    assert_eq!(names(temp.path()), ["top"], "the folder above left");
    assert_eq!(names(&top), ["A.c"]);
}
