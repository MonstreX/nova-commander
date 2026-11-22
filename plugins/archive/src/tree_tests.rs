use super::*;

fn file(path: &[&str]) -> Item {
    Item {
        path: path.iter().map(|part| (*part).to_string()).collect(),
        ..Item::default()
    }
}

fn folder(path: &[&str]) -> Item {
    Item {
        is_dir: true,
        ..file(path)
    }
}

fn names(tree: &Tree, node: usize) -> Vec<(&str, bool, Option<usize>)> {
    (tree.nodes[node].children.iter())
        .map(|&child| {
            let node = &tree.nodes[child];
            (node.name.as_str(), node.is_dir, node.item)
        })
        .collect()
}

/// arclite's `make_index`: a folder only implied is there without an item; one named later takes that
/// item; a folder whatever its case one (its first spelling), files told apart by case both kept.
#[test]
fn implied_folders_are_listed_and_named_ones_kept() {
    let items = [
        file(&["a", "b", "c.txt"]),
        folder(&["A"]),
        file(&["A.txt"]),
        file(&["a.txt"]),
        folder(&["a", "B"]),
    ];
    let tree = Tree::new(&items);
    assert_eq!(
        names(&tree, 0),
        [
            ("a", true, Some(1)),
            ("A.txt", false, Some(2)),
            ("a.txt", false, Some(3))
        ]
    );
    let a = tree.find_dir("/a").unwrap();
    assert_eq!(names(&tree, a), [("b", true, Some(4))]);
    let b = tree.find_dir("/a/b/").unwrap();
    assert_eq!(names(&tree, b), [("c.txt", false, Some(0))]);
    assert_eq!(tree.find_dir("/"), Some(0));
    assert_eq!(tree.find_dir("/A/B"), Some(b), "lstrcmpiW: case not told");
    assert_eq!(tree.find_dir("/a.txt"), None, "a file is no folder");
    assert_eq!(
        tree.find("/A.txt"),
        Some(4),
        "a file by its name as written"
    );
    assert_eq!(tree.find("/a.txt"), Some(5));
    assert_eq!(
        tree.find("/a/b/C.TXT"),
        Some(b + 1),
        "else whatever its case"
    );
}

/// arclite's `make_index`: `.` passed over, `..` back to the folder before, never above the root; a name
/// `.` or `..` itself not listed.
#[test]
fn dots_on_a_path_are_folded() {
    let items = [
        file(&[".", "a.txt"]),
        file(&["d", "..", "b.txt"]),
        file(&["..", "..", "c.txt"]),
        file(&["d", "e", "..", "f.txt"]),
        file(&["d", ".."]),
    ];
    let tree = Tree::new(&items);
    let root: Vec<&str> = names(&tree, 0).iter().map(|(name, _, _)| *name).collect();
    assert_eq!(root, ["a.txt", "d", "b.txt", "c.txt"]);
    let d = tree.find_dir("/d").unwrap();
    let in_d: Vec<&str> = names(&tree, d).iter().map(|(name, _, _)| *name).collect();
    assert_eq!(in_d, ["e", "f.txt"]);
}

/// A file found by its path; a folder first when both are named so; nothing outside.
#[test]
fn entries_are_found_by_their_paths() {
    let items = [file(&["x"]), file(&["x", "y"]), file(&["d", "f"])];
    let tree = Tree::new(&items);
    let x_file = tree.find("/x").unwrap();
    assert!(tree.nodes[x_file].is_dir, "the folder x first");
    let f = tree.find("/d/f").unwrap();
    assert_eq!(tree.nodes[f].item, Some(2));
    assert_eq!(tree.find("/d/g"), None);
    assert_eq!(tree.find("/z/f"), None);
    assert_eq!(names(&tree, 0).len(), 3, "x as a file and as a folder, d");
}

/// An item with no name (`/` alone) is not listed.
#[test]
fn an_empty_name_is_let_be() {
    let tree = Tree::new(&[file(&[])]);
    assert!(tree.nodes[0].children.is_empty());
}
