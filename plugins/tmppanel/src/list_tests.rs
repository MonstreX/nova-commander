//! A temporary panel's list on files of a temporary folder: each path once as it is written, folders
//! with their contents, entries named by their whole paths with their real names (none in safe mode), the
//! gone dropped or, with any elements, kept.

use super::*;

/// A folder with `a.txt` (5 bytes) and `sub\b.txt`.
fn folder() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "hello").unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    std::fs::write(temp.path().join("sub").join("b.txt"), "b").unwrap();
    temp
}

#[test]
fn a_path_is_put_once_as_it_is_written_and_taken_off() {
    let temp = folder();
    let a = temp.path().join("a.txt");
    let mut list = List::default();
    assert!(list.add(a.clone()));
    assert!(!list.add(a.clone()), "there already");
    // 5d-3b: FAR's `RemoveDups` compares the names exactly (gh-1041, case-sensitive folders), so another
    // spelling stays as its own item; before, ours took it for the same file (the case ignored).
    let shouted = PathBuf::from(a.to_string_lossy().to_uppercase());
    assert!(
        list.add(shouted.clone()),
        "another spelling: FAR keeps both"
    );
    assert!(list.holds(&shouted) && list.holds(&a));
    assert_eq!(list.paths(), [a.clone(), shouted.clone()]);
    assert!(list.remove(&shouted));
    assert!(!list.remove(&shouted), "gone already");
    assert_eq!(list.paths(), std::slice::from_ref(&a), "the other stays");
    assert!(list.remove(&a));
    assert!(list.paths().is_empty());
}

#[test]
fn many_paths_are_taken_off_at_once_the_rest_in_order() {
    let mut list = List::default();
    let paths: Vec<PathBuf> = (0..20_000)
        .map(|at| PathBuf::from(format!(r"C:\many\{at}.txt")))
        .collect();
    for path in &paths {
        list.add(path.clone());
    }
    let sevens = |path: &&PathBuf| path.to_string_lossy().ends_with("7.txt");
    let gone = (paths.iter())
        .filter(|path| !sevens(path))
        .map(PathBuf::as_path)
        .chain([Path::new(r"C:\many\none.txt")]);
    assert_eq!(list.remove_all(gone), 18_000, "those there");
    let kept: Vec<&PathBuf> = paths.iter().filter(sevens).collect();
    assert_eq!(
        list.paths().iter().collect::<Vec<_>>(),
        kept,
        "the rest in order"
    );
    assert!(list.holds(kept[0]) && !list.holds(&paths[0]));
}

#[test]
fn a_folder_comes_with_its_contents_when_asked() {
    let temp = folder();
    let mut list = List::default();
    assert!(list.add_contents(temp.path(), &mut |_, _| true));
    let mut paths = list.paths().to_vec();
    paths.sort();
    let mut expected = vec![
        temp.path().to_path_buf(),
        temp.path().join("a.txt"),
        temp.path().join("sub"),
        temp.path().join("sub").join("b.txt"),
    ];
    expected.sort();
    assert_eq!(paths, expected);
    assert!(list.add_contents(temp.path(), &mut |_, _| true));
    assert_eq!(list.paths().len(), 4, "none twice");
}

#[test]
fn a_walk_asked_to_stop_says_so_at_once() {
    // ADR-0019 §5: the put that walks it then puts the list back as it was.
    let temp = folder();
    let mut list = List::default();
    let mut asked = 0;
    let walked = list.add_contents(temp.path(), &mut |_, _| {
        asked += 1;
        asked < 2
    });
    assert!(!walked);
    assert_eq!(asked, 2, "nothing asked after the stop");
}

#[test]
fn entries_are_named_by_their_whole_paths_and_the_gone_are_dropped() {
    let temp = folder();
    let (a, sub) = (temp.path().join("a.txt"), temp.path().join("sub"));
    let gone = temp.path().join("gone.txt");
    let mut list = List::default();
    for path in [&a, &gone, &sub] {
        list.add(path.clone());
    }
    let entries = list.entries(false, false);
    assert_eq!(entries.len(), 2);
    let file = &entries[0];
    assert_eq!(file.name, a.to_string_lossy());
    assert_eq!(file.real_path.as_deref(), Some(a.as_path()));
    assert!(!file.is_dir);
    assert_eq!(file.size, 5);
    assert!(file.modified.is_some());
    assert!(entries[1].is_dir && entries[1].size == 0);
    assert_eq!(
        list.paths(),
        [a, sub],
        "the gone one dropped, the order kept"
    );
    assert!(entry(&gone).is_none());
}

#[test]
fn safe_mode_gives_no_real_names_and_any_elements_keep_the_gone() {
    let temp = folder();
    let (a, gone) = (temp.path().join("a.txt"), temp.path().join("gone.txt"));
    let mut list = List::default();
    for path in [&a, &gone] {
        list.add(path.clone());
    }
    let entries = list.entries(true, true);
    assert_eq!(entries.len(), 2, "the gone one kept with any elements");
    assert_eq!(entries[0].name, a.to_string_lossy());
    assert_eq!(entries[0].real_path, None, "safe mode: no real names");
    assert_eq!(entries[0].size, 5, "its file's otherwise");
    assert_eq!(entries[1].name, gone.to_string_lossy(), "as it is written");
    assert!(entries[1].real_path.is_none() && !entries[1].is_dir);
    assert_eq!(list.paths().len(), 2, "none dropped");
    let entries = list.entries(true, false);
    assert_eq!(entries[0].real_path.as_deref(), Some(a.as_path()));
    assert_eq!(list.entries(false, false).len(), 1, "without any: dropped");
    assert_eq!(list.paths(), std::slice::from_ref(&a));
}

#[test]
fn found_files_show_first_as_the_search_saw_them_then_as_the_disks_say() {
    // FAR's `SetFindList` and `m_UpdateNotNeeded`: no looking at the disks at the first reading.
    let temp = folder();
    let a = temp.path().join("a.txt");
    let gone = temp.path().join("gone.txt");
    let entry = |path: &Path, size: u64| FileEntry {
        size,
        real_path: Some(path.to_path_buf()),
        ..FileEntry::file(path.display().to_string(), 0)
    };
    let mut list = List::default();
    list.add_found(&[entry(&a, 99), entry(&gone, 7), entry(&a, 99)]);
    assert_eq!(list.paths().len(), 2, "each once");
    let first = list.entries(false, false);
    let sizes: Vec<u64> = first.iter().map(|entry| entry.size).collect();
    assert_eq!(sizes, [99, 7], "as the search saw them, the gone one too");
    let next = list.entries(false, false);
    assert_eq!(next.len(), 1, "then the disk's: the gone one dropped");
    assert_eq!(next[0].size, 5, "a.txt as it is");
}

#[test]
fn found_files_in_safe_mode_have_no_real_names_and_a_change_drops_what_the_search_saw() {
    let temp = folder();
    let a = temp.path().join("a.txt");
    let gone = temp.path().join("gone.txt");
    let entry = |path: &Path, size: u64| FileEntry {
        size,
        real_path: Some(path.to_path_buf()),
        ..FileEntry::file(path.display().to_string(), 0)
    };
    let mut list = List::default();
    list.add(temp.path().join("old.txt"));
    list.add_found(&[entry(&a, 99), entry(&gone, 7)]);
    assert_eq!(
        list.paths().len(),
        2,
        "in place of what it held (FAR's clear)"
    );
    let safe = list.entries(false, true);
    assert!(
        safe.iter().all(|entry| entry.real_path.is_none()),
        "safe mode"
    );
    assert_eq!(safe[0].size, 99, "as the search saw it");
    // A change before the first reading: the disks are looked at.
    list.add_found(&[entry(&a, 99), entry(&gone, 7)]);
    list.remove(&a);
    assert!(
        list.entries(false, false).is_empty(),
        "gone.txt is not there, a.txt taken off"
    );
}
