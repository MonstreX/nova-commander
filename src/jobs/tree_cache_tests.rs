//! The trees' files as FAR keeps them: read in place of the disk, saved after a whole reading, changed
//! by what the operations make, move and delete. Every tree here has a test's own folder as its root.
//! Windows only: the files are named by the volume's facts, which only Windows gives here.
#![cfg(windows)]

use std::fs;

use super::*;
use crate::vfs::{Entry, VfsPath};

/// A test's folder as the root, its files in a folder beside it.
fn cache() -> (tempfile::TempDir, PathBuf, TreeCache) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("disk");
    fs::create_dir(&root).unwrap();
    let cache = TreeCache::under(temp.path().join("TreeCache"), root.clone());
    (temp, root, cache)
}

fn made(root: &Path, folders: &[&str]) -> Vec<PathBuf> {
    (folders.iter())
        .map(|folder| {
            let path = root.join(folder);
            fs::create_dir_all(&path).unwrap();
            path
        })
        .collect()
}

/// The file's lines, in order.
fn lines(cache: &TreeCache, root: &Path) -> Vec<String> {
    let text = fs::read_to_string(cache.file_of(root).unwrap()).unwrap();
    text.lines().map(str::to_string).collect()
}

fn sorted(mut lines: Vec<String>) -> Vec<String> {
    lines.sort();
    lines
}

#[test]
fn a_saved_tree_is_read_back_and_a_disk_without_a_file_has_none() {
    let (temp, root, cache) = cache();
    assert_eq!(cache.read(&root), None, "no file yet");
    let folders = made(&root, &["a", "a/b", "c"]);
    cache.save(&root, &folders, 4).unwrap();
    let file = cache.file_of(&root).unwrap();
    assert!(file.starts_with(temp.path().join("TreeCache")));
    assert_eq!(file.extension().unwrap(), "tree");
    assert_eq!(
        lines(&cache, &root),
        [r"\", r"\a", r"\a\b", r"\c"],
        "FAR's lines"
    );
    assert_eq!(cache.read(&root), Some(folders));
}

#[test]
fn a_tree_with_fewer_folders_than_the_least_is_not_saved() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["a", "b"]);
    cache.save(&root, &folders, 4).unwrap();
    assert_eq!(
        cache.read(&root),
        None,
        "the root and two folders: 3 lines of the 4"
    );
    cache.save(&root, &folders, 3).unwrap();
    assert_eq!(cache.read(&root).map(|read| read.len()), Some(2));
}

#[test]
fn a_file_that_cannot_be_written_says_so() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("disk");
    let folders = made(&root, &["a", "b", "c"]);
    // The folder of the files is a file.
    fs::write(temp.path().join("TreeCache"), "x").unwrap();
    let cache = TreeCache::under(temp.path().join("TreeCache"), root.clone());
    let unsaved = cache.save(&root, &folders, 1).unwrap_err();
    assert_eq!(Some(unsaved.file), cache.file_of(&root));
    assert!(!unsaved.error.is_empty());
}

#[test]
fn folders_are_added_once_whatever_their_case_and_only_to_a_file_there_is() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["a", "b", "c"]);
    cache.add(&root, &folders);
    assert_eq!(
        cache.read(&root),
        None,
        "FAR's ReadCache: no file, no change"
    );
    cache.save(&root, &folders, 1).unwrap();
    cache.add(&root, &[root.join("A"), root.join("d")]);
    assert_eq!(lines(&cache, &root), [r"\", r"\a", r"\b", r"\c", r"\d"]);
}

#[test]
fn a_folder_gone_leaves_with_what_was_under_it_and_what_is_still_there_stays() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["a", "a/b", "a/c", "ab", "d"]);
    cache.save(&root, &folders, 1).unwrap();
    fs::remove_dir(root.join("a").join("b")).unwrap();
    cache.forget_gone(&root.join("a"));
    assert_eq!(
        lines(&cache, &root),
        [r"\", r"\a", r"\a\c", r"\ab", r"\d"],
        "a is still there, half deleted"
    );
    fs::remove_dir_all(root.join("a")).unwrap();
    cache.forget_gone(&root.join("A"));
    assert_eq!(lines(&cache, &root), [r"\", r"\ab", r"\d"], "not ab");
}

#[test]
fn a_deletion_takes_its_folders_gone_out() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["a", "a/b", "c"]);
    cache.save(&root, &folders, 1).unwrap();
    fs::remove_dir_all(root.join("a")).unwrap();
    let item = |name: &str, is_dir| delete::Item {
        path: VfsPath::local(root.join(name)),
        entry: Entry {
            name: name.to_string(),
            is_dir,
            ..Entry::default()
        },
    };
    let request = delete::Request {
        items: vec![item("a", true), item("c", false)],
        mode: delete::Mode::Remove,
        ask_folders: true,
        ask_read_only: true,
        count_totals: false,
    };
    cache.after_delete(&request);
    assert_eq!(
        lines(&cache, &root),
        [r"\", r"\c"],
        "c is no folder of the deletion"
    );
}

fn transfer(sources: &[PathBuf], target: &Path, move_: bool) -> transfer::Request {
    transfer::Request {
        sources: sources.iter().map(VfsPath::local).collect(),
        target: VfsPath::local(target),
        move_,
        existing: transfer::Existing::Ask,
        ask_read_only: true,
        keep: transfer::Keep::default(),
        retrying: Vec::new(),
        link: None,
        filter: None,
    }
}

/// What a copy of `from` to `to` does to the disk, as the operation would.
fn copy_folders(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        copy_folders(&entry.path(), &to.join(entry.file_name()));
    }
}

#[test]
fn a_copy_into_a_folder_puts_in_the_folders_it_made() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["src/x/y", "src", "src/x", "dst"]);
    cache.save(&root, &folders, 1).unwrap();
    let plan = cache.before_transfer(&transfer(&[root.join("src")], &root.join("dst"), false));
    copy_folders(&root.join("src"), &root.join("dst").join("src"));
    plan.apply();
    assert_eq!(
        sorted(lines(&cache, &root)),
        [
            r"\",
            r"\dst",
            r"\dst\src",
            r"\dst\src\x",
            r"\dst\src\x\y",
            r"\src",
            r"\src\x",
            r"\src\x\y"
        ]
    );
}

#[test]
fn a_move_to_a_new_name_takes_the_old_out_and_puts_the_new_in() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["old", "old/x", "z"]);
    cache.save(&root, &folders, 1).unwrap();
    let plan = cache.before_transfer(&transfer(&[root.join("old")], &root.join("new"), true));
    fs::rename(root.join("old"), root.join("new")).unwrap();
    plan.apply();
    assert_eq!(
        sorted(lines(&cache, &root)),
        [r"\", r"\new", r"\new\x", r"\z"],
        "the folders under the new name too, not only it (FAR's RenTreeName keeps it alone)"
    );
}

#[test]
fn a_link_to_a_folder_is_put_in_alone() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["a", "a/b", "c"]);
    cache.save(&root, &folders, 1).unwrap();
    let mut request = transfer(&[root.join("a")], &root.join("c"), false);
    request.link = Some(crate::platform::files::NewLink::Junction);
    let plan = cache.before_transfer(&request);
    // What the link leads to, as the tree would see it: a folder with b in it.
    copy_folders(&root.join("a"), &root.join("c").join("a"));
    plan.apply();
    assert_eq!(
        sorted(lines(&cache, &root)),
        [r"\", r"\a", r"\a\b", r"\c", r"\c\a"]
    );
}

#[test]
fn made_puts_in_the_first_folder_made_and_those_in_it() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["a", "b", "c"]);
    cache.save(&root, &folders, 1).unwrap();
    made(&root, &["a/new/deep"]);
    cache.made(&root.join("a").join("new"));
    assert_eq!(
        lines(&cache, &root),
        [r"\", r"\a", r"\b", r"\c", r"\a\new", r"\a\new\deep"]
    );
}

#[test]
fn the_root_of_a_path_is_its_disks_or_the_tests_own() {
    let (_temp, root, cache) = cache();
    assert_eq!(cache.root_of(&root.join("a")), root);
    let other = TreeCache::new(PathBuf::from("cache"));
    assert_eq!(other.root_of(Path::new(r"C:\a\b")), PathBuf::from(r"C:\"));
}

#[test]
fn a_folder_goes_in_with_the_folders_above_it() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["a", "b", "c"]);
    cache.save(&root, &folders, 1).unwrap();
    cache.add(&root, &[root.join("x").join("y")]);
    assert_eq!(
        lines(&cache, &root),
        [r"\", r"\a", r"\b", r"\c", r"\x", r"\x\y"]
    );
}

#[test]
fn a_case_only_rename_gives_the_folder_and_those_under_it_the_new_case() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["dir", "dir/in", "z"]);
    cache.save(&root, &folders, 1).unwrap();
    let plan = cache.before_transfer(&transfer(&[root.join("dir")], &root.join("Dir"), true));
    fs::rename(root.join("dir"), root.join("Dir")).unwrap();
    plan.apply();
    assert_eq!(lines(&cache, &root), [r"\", r"\Dir", r"\Dir\in", r"\z"]);
}

#[test]
fn a_move_out_of_the_local_disks_still_takes_its_folders_out() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["a", "a/b", "c"]);
    cache.save(&root, &folders, 1).unwrap();
    let mut request = transfer(&[root.join("a")], &root, true);
    request.target = crate::vfs::test_tree::root("tree-cache-move");
    let plan = cache.before_transfer(&request);
    fs::remove_dir_all(root.join("a")).unwrap();
    plan.apply();
    assert_eq!(lines(&cache, &root), [r"\", r"\c"]);
}

#[test]
fn f7_names_with_dots_make_lines_of_the_folders_themselves() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["a", "b", "c"]);
    cache.save(&root, &folders, 1).unwrap();
    let path = root.join("a").join("..").join("x").join(".").join("y");
    let top = first_missing(&path).unwrap();
    assert_eq!(top, root.join("x"), "a/.. is there: x is the first made");
    fs::create_dir_all(root.join("x").join("y")).unwrap();
    cache.made(&top);
    assert_eq!(
        lines(&cache, &root),
        [r"\", r"\a", r"\b", r"\c", r"\x", r"\x\y"]
    );
    assert_eq!(first_missing(&root.join("a")), None, "all there");
}

#[test]
fn a_disk_without_a_file_gets_none_from_a_copy() {
    let (_temp, root, cache) = cache();
    made(&root, &["src/x", "dst"]);
    let plan = cache.before_transfer(&transfer(&[root.join("src")], &root.join("dst"), false));
    copy_folders(&root.join("src"), &root.join("dst").join("src"));
    plan.apply();
    assert_eq!(cache.read(&root), None);
    assert!(!cache.file_of(&root).unwrap().exists());
}

#[test]
fn a_line_no_folder_could_have_is_passed_over() {
    let (_temp, root, cache) = cache();
    let folders = made(&root, &["a", "b", "c"]);
    cache.save(&root, &folders, 4).unwrap();
    let file = cache.file_of(&root).unwrap();
    let text = fs::read_to_string(&file).unwrap()
        + "\n\\a\\..\\..\\x\n\\C:\\y\n\\b/z\n\\.\n\\x/\n\\ab:c\n";
    fs::write(&file, text).unwrap();
    assert_eq!(
        cache.read(&root),
        Some(folders),
        "nothing outside the root, nothing new"
    );
}
