//! The nodes of a tree: folders put in with those above them, taken out with those under them.

use std::time::{Duration, UNIX_EPOCH};

use super::*;
use crate::platform::files::NAMES_DIFFER_BY_CASE;

fn root() -> PathBuf {
    PathBuf::from("R:").join("root")
}

#[test]
fn a_folder_comes_in_with_those_above_it_once_whatever_its_case() {
    let root = root();
    let mut nodes = Nodes::new(&root);
    assert!(nodes.insert(&root.join("a").join("b")));
    assert_eq!(nodes.len(), 3, "the root, a and b");
    // Where case matters (Unix) `A` is a folder of its own.
    let differs = nodes.insert(&root.join("A").join("B"));
    assert_eq!(
        differs, NAMES_DIFFER_BY_CASE,
        "the same folder where case does not matter"
    );
    if !NAMES_DIFFER_BY_CASE {
        assert_eq!(
            nodes
                .get(&root.join("A").join("b"))
                .map(|node| node.path.clone()),
            Some(root.join("a").join("b")),
            "the first spelling stays"
        );
        assert_eq!(nodes.children(&root), [root.join("a")]);
    }
    assert!(!nodes.insert(Path::new("elsewhere")), "not under the root");
}

#[test]
fn a_folder_leaves_with_what_is_under_it_and_the_root_stays() {
    let root = root();
    let mut nodes = Nodes::new(&root);
    for folder in ["a/b/c", "a/d", "e"] {
        nodes.insert(&root.join(folder));
    }
    let a = if NAMES_DIFFER_BY_CASE { "a" } else { "A" };
    assert!(nodes.remove(&root.join(a)));
    let mut left = nodes.folders();
    left.sort();
    assert_eq!(left, [root.join("e")]);
    assert_eq!(nodes.children(&root), [root.join("e")]);
    assert!(!nodes.remove(&root), "never the root");
    assert!(!nodes.remove(&root.join("a")), "gone already");
}

#[test]
fn times_are_kept_and_ctrl_r_forgets_them() {
    let root = root();
    let mut nodes = Nodes::new(&root);
    nodes.insert(&root.join("a"));
    let time = UNIX_EPOCH + Duration::from_secs(5);
    nodes.set_read(&root.join("a"), Some(time));
    assert_eq!(nodes.get(&root.join("a")).unwrap().read, Some(time));
    nodes.forget_times();
    assert_eq!(nodes.get(&root.join("a")).unwrap().read, None);
}

#[test]
fn the_orders_put_a_folder_before_those_in_it() {
    let root = root();
    let mut nodes = Nodes::new(&root);
    for folder in ["b/x/y", "a", "b"] {
        nodes.insert(&root.join(folder));
    }
    let keys: Vec<PathBuf> = nodes
        .in_order()
        .iter()
        .map(|node| node.path.clone())
        .collect();
    assert_eq!(
        keys,
        [
            root.clone(),
            root.join("a"),
            root.join("b"),
            root.join("b").join("x"),
            root.join("b").join("x").join("y")
        ]
    );
    assert_eq!(
        nodes.by_depth(),
        [
            root.join("a"),
            root.join("b"),
            root.join("b").join("x"),
            root.join("b").join("x").join("y")
        ]
    );
}

#[test]
fn a_folder_renamed_in_its_case_takes_its_new_name_with_those_under_it() {
    let root = root();
    let mut nodes = Nodes::new(&root);
    nodes.insert(&root.join("work").join("in"));
    nodes.insert(&root.join("z"));
    nodes.respell(&root.join("work"), &root.join("Work"));
    assert_eq!(
        nodes.get(&root.join("work").join("in")).unwrap().path,
        root.join("Work").join("in")
    );
    assert_eq!(nodes.get(&root.join("z")).unwrap().path, root.join("z"));
}

#[test]
fn what_is_under_a_folder_is_found_by_its_key() {
    let root = root();
    let mut nodes = Nodes::new(&root);
    for folder in ["a/b/c", "ab", "d"] {
        nodes.insert(&root.join(folder));
    }
    let under: Vec<String> = nodes.under("a").into_iter().map(|(key, _)| key).collect();
    assert_eq!(
        under,
        [format!("a{SEP}b"), format!("a{SEP}b{SEP}c")],
        "not ab"
    );
    assert_eq!(nodes.under("").len(), 5, "the root's: every folder");
    assert!(is_under(&format!("a{SEP}b"), "a") && is_under("a", "a") && !is_under("ab", "a"));
}

#[test]
fn a_line_of_a_file_puts_its_folder_in_with_those_above_it_and_its_time() {
    let root = root();
    let mut nodes = Nodes::new(&root);
    let time = UNIX_EPOCH + Duration::from_secs(7);
    nodes.put(&["a", "B"], Some(time));
    nodes.put(&[], Some(time));
    let b = root.join("a").join("B");
    assert_eq!(nodes.get(&root.join("A").join("b")).unwrap().path, b);
    assert_eq!(nodes.get(&b).unwrap().read, Some(time));
    assert_eq!(
        nodes.get(&root.join("a")).unwrap().read,
        None,
        "only its own"
    );
    assert_eq!(nodes.children(&root.join("a")), [b]);
    assert_eq!(
        nodes.get(&root).unwrap().read,
        Some(time),
        "the root's line"
    );
}
