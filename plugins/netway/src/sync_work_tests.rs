use super::*;

fn item(path: &str, action: Action) -> Item {
    Item {
        path: path.to_string(),
        is_dir: false,
        action,
        checked: true,
        local: Some((5, Some(1_000_000_000))),
        remote: None,
    }
}

#[test]
fn the_local_tree_is_read() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("sub")).unwrap();
    std::fs::write(temp.path().join("a.txt"), "alpha").unwrap();
    std::fs::write(temp.path().join("sub").join("b.txt"), "be").unwrap();
    let mut tree = local_tree(temp.path()).unwrap();
    tree.sort_by(|left, right| left.name.cmp(&right.name));
    assert_eq!(tree[0].name, "a.txt");
    assert_eq!(tree[0].size, 5);
    assert!(tree[0].modified.is_some());
    assert!(tree[1].is_dir);
    assert_eq!(tree[1].children[0].name, "b.txt");
}

#[test]
fn listings_are_assembled_into_a_tree() {
    let node = |name: &str, is_dir: bool| Node {
        name: name.to_string(),
        is_dir,
        ..Node::default()
    };
    let mut listed = vec![
        ("/srv/sub".to_string(), vec![node("deep.txt", false)]),
        (
            "/srv".to_string(),
            vec![node("sub", true), node("a.txt", false)],
        ),
    ];
    let tree = assembled("/srv", &mut listed);
    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].children[0].name, "deep.txt");
}

/// The checklist's menu: a line per item with its arrow, checked as it is, " N of M " below; Space / Ins
/// toggle, "+" checks all, "-" unchecks all.
#[test]
fn the_checklist_is_a_menu_of_checked_lines() {
    let mut items = vec![
        item("a.txt", Action::UploadNew),
        item("sub/b.txt", Action::DeleteLocal),
    ];
    items[1].checked = false;
    let menu = checklist_menu(&items, 1);
    assert_eq!(menu.title, "Synchronization checklist");
    assert_eq!(menu.bottom, " 1 of 2 ");
    assert!(menu.items[0].checked && !menu.items[1].checked);
    assert!(menu.items[0].text.starts_with("a.txt"));
    assert!(menu.items[0].text.contains("*->"));
    assert!(menu.items[1].text.contains("X  "));
    toggled(&mut items, Some(1), 0);
    assert!(items[1].checked);
    toggled(&mut items, None, 3);
    assert!(items.iter().all(|item| !item.checked));
    toggled(&mut items, None, 2);
    assert!(items.iter().all(|item| item.checked));
    assert_eq!(split("sub/deep/b.txt"), ("sub/deep", "b.txt"));
    assert_eq!(split("a.txt"), ("", "a.txt"));
}
