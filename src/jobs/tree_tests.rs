//! The folders of a tree laid out as FAR's `FillLastData`.

use super::*;

#[test]
fn the_lines_know_which_folder_is_the_last_of_its_parent() {
    let root = Path::new("R:\\");
    let folders = ["R:\\d", "R:\\a\\c", "R:\\a", "R:\\a\\b", "R:\\a\\b"].map(PathBuf::from);
    let shown: Vec<(String, usize, Vec<bool>)> = (make_items(root, folders.to_vec()).iter())
        .map(|item| {
            let path = item.path.to_string_lossy().into_owned();
            (path, item.depth, item.last.clone())
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("R:\\".to_string(), 0, vec![]),
            ("R:\\a".to_string(), 1, vec![false]),
            ("R:\\a\\b".to_string(), 2, vec![false, false]),
            ("R:\\a\\c".to_string(), 2, vec![false, true]),
            ("R:\\d".to_string(), 1, vec![true]),
        ],
        "each once, a folder before what is in it"
    );
}

#[test]
fn one_folder_in_two_cases_is_one_and_numbers_written_apart_stay_apart() {
    let root = Path::new("R:\\");
    let folders = ["R:\\a\\x", "R:\\A\\y", "R:\\a", "R:\\A", "R:\\01", "R:\\1"].map(PathBuf::from);
    let items = make_items(root, folders.to_vec());
    let paths: Vec<String> = (items.iter().skip(1))
        .map(|item| item.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        paths,
        ["R:\\01", "R:\\1", "R:\\a", "R:\\a\\x", "R:\\A\\y"],
        "FAR's tree keeps names whatever their case once; the first given stays"
    );
    assert_eq!(items[3].last, [true]);
    assert_eq!(items[4].last, [true, false]);
    assert_eq!(items[5].last, [true, true]);
}

#[test]
fn every_folder_under_one_is_found_for_the_files() {
    let temp = tempfile::tempdir().unwrap();
    for folder in ["a/b", "c"] {
        std::fs::create_dir_all(temp.path().join(folder)).unwrap();
    }
    std::fs::write(temp.path().join("file.txt"), "x").unwrap();
    let mut found = folders_under(temp.path());
    found.sort();
    let a = temp.path().join("a");
    assert_eq!(found, [a.clone(), a.join("b"), temp.path().join("c")]);
}

#[test]
#[cfg(windows)]
fn names_deeper_down_are_in_far_s_order_too() {
    let root = Path::new(r"R:\");
    let folders = [r"R:\b", r"R:\a\x10", r"R:\a\X2", r"R:\a", r"R:\a\x10\1"].map(PathBuf::from);
    let paths: Vec<String> = (make_items(root, folders.to_vec()).iter().skip(1))
        .map(|item| item.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        paths,
        [r"R:\a", r"R:\a\X2", r"R:\a\x10", r"R:\a\x10\1", r"R:\b"]
    );
}
