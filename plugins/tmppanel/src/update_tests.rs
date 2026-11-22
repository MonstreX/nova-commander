//! The panel's paths looked at again: those of one folder from one reading of it, the same entries as one
//! by one, the gone ones none.

use super::*;

fn folder() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "aaa").unwrap();
    std::fs::write(dir.path().join("b.txt"), "bb").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub").join("c.txt"), "c").unwrap();
    dir
}

#[test]
fn a_folders_paths_are_found_as_one_by_one_and_the_gone_are_none() {
    let dir = folder();
    let paths: Vec<PathBuf> = ["a.txt", "gone.txt", "b.txt", "sub"]
        .iter()
        .map(|name| dir.path().join(name))
        .collect();
    let looked = look(&paths);
    assert_eq!(looked.len(), 4);
    let alone: Vec<Option<FileEntry>> = paths.iter().map(|path| entry(path)).collect();
    for (grouped, single) in looked.iter().zip(&alone) {
        let pick = |entry: &Option<FileEntry>| {
            (entry.as_ref()).map(|entry| (entry.name.clone(), entry.size, entry.is_dir))
        };
        assert_eq!(pick(grouped), pick(single), "as one by one");
    }
    assert_eq!(looked[0].as_ref().unwrap().size, 3);
    assert_eq!(
        looked[0].as_ref().unwrap().real_path.as_deref(),
        Some(paths[0].as_path())
    );
    assert!(looked[1].is_none(), "gone");
    assert!(looked[3].as_ref().unwrap().is_dir, "a folder");
}

#[test]
fn paths_of_folders_in_turn_and_of_a_folder_gone() {
    let dir = folder();
    let paths = vec![
        dir.path().join("a.txt"),
        dir.path().join("sub").join("c.txt"),
        dir.path().join("b.txt"),
        dir.path().join("none").join("x.txt"),
        dir.path().join("none").join("y.txt"),
    ];
    let sizes: Vec<Option<u64>> = (look(&paths).iter())
        .map(|entry| entry.as_ref().map(|entry| entry.size))
        .collect();
    assert_eq!(sizes, [Some(3), Some(1), Some(2), None, None]);
}

#[test]
fn a_name_is_found_as_the_system_compares_names() {
    let dir = folder();
    let paths = vec![dir.path().join("A.TXT"), dir.path().join("b.txt")];
    let looked = look(&paths);
    assert_eq!(
        looked[0].is_some(),
        cfg!(windows),
        "without case on Windows"
    );
    if let Some(found) = &looked[0] {
        assert_eq!(found.name, paths[0].to_string_lossy(), "named as written");
    }
}

#[test]
fn two_spellings_of_one_name_are_both_found() {
    // Both can be on the list (its keys are exact); FAR's `try_emplace` keeps the first only (D22).
    let dir = folder();
    let paths = vec![dir.path().join("a.txt"), dir.path().join("A.TXT")];
    let found: Vec<bool> = look(&paths).iter().map(Option::is_some).collect();
    assert_eq!(found, [true, cfg!(windows)]);
}

#[test]
fn a_final_sigma_finds_the_file_written_with_another_sigma() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("fooσ.txt"), "x").unwrap();
    std::fs::write(dir.path().join("b.txt"), "x").unwrap();
    let paths = vec![dir.path().join("fooΣ.txt"), dir.path().join("b.txt")];
    let found: Vec<bool> = look(&paths).iter().map(Option::is_some).collect();
    assert_eq!(found, [cfg!(windows), true]);
}

#[test]
fn a_root_and_a_path_ending_in_a_separator_are_no_folders_names() {
    assert_eq!(folder_of(Path::new(r"C:\")), None);
    let dir = folder();
    let ended = PathBuf::from(format!(
        "{}{}",
        dir.path().join("sub").display(),
        std::path::MAIN_SEPARATOR
    ));
    assert_eq!(folder_of(&ended), None);
    assert_eq!(folder_of(&dir.path().join("a.txt")), Some(dir.path()));
    let looked = look(&[ended.clone(), dir.path().join("a.txt")]);
    assert!(
        looked[0].as_ref().is_some_and(|entry| entry.is_dir),
        "looked at alone"
    );
}
