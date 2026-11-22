use super::*;
use nc_plugin_sdk::ItemKind;

fn values_of(items: &[DialogItem]) -> Vec<Value> {
    (items.iter())
        .map(|item| Value {
            text: item.text.clone(),
            state: item.state,
        })
        .collect()
}

#[test]
fn the_dialog_is_netboxs_and_gives_the_watch() {
    let params = DELETE_FLAG | SELECTED_FLAG;
    let shown = items((r"C:\work", "/srv"), (params, RECURSE_OPTION), true);
    assert_eq!(
        shown[0].text,
        "&Watch for changes in the local directory ..."
    );
    assert_eq!(
        shown[2].text,
        "... &and automatically reflect them on the remote directory"
    );
    assert_eq!(
        (shown[START].kind, shown[START].text.as_str()),
        (ItemKind::Button, "&Start")
    );
    let watch = watch_of(&values_of(&shown));
    assert_eq!(
        (watch.local.as_str(), watch.remote.as_str()),
        (r"C:\work", "/srv")
    );
    assert!(watch.delete && watch.recursive && watch.selected_only);
    assert!(!watch.existing_only && !watch.on_start && !watch.save);
    let settings = settings_of(&watch);
    assert_eq!(settings.mode, Mode::Remote);
    assert!(settings.delete && settings.by_time);
    let unselected = items(("", ""), (params, 0), false);
    assert!(unselected[SELECTED].disabled && unselected[SELECTED].state == 0);
}

/// The upload options' boxes, in the watch's order.
const BOXES: [usize; 6] = [DELETE, EXISTING, RECURSIVE, SELECTED, ON_START, SAVE];

fn flags(watch: &Watch) -> [bool; 6] {
    [
        watch.delete,
        watch.existing_only,
        watch.recursive,
        watch.selected_only,
        watch.on_start,
        watch.save,
    ]
}

/// Each box is its own flag of the watch; the folders are taken trimmed; no values give an empty watch.
#[test]
fn each_box_is_its_own_flag() {
    for one in BOXES {
        let mut values = values_of(&items(("", ""), (0, 0), true));
        values[one].state = 1;
        assert_eq!(
            flags(&watch_of(&values)),
            BOXES.map(|other| other == one),
            "item {one}"
        );
    }
    let mut values = values_of(&items((r"  C:\work ", " /srv  "), (0, 0), true));
    values[DELETE].state = 2;
    let watch = watch_of(&values);
    assert_eq!(
        (watch.local.as_str(), watch.remote.as_str()),
        (r"C:\work", "/srv")
    );
    assert!(!watch.delete, "only a checked box is on");
    let empty = watch_of(&[]);
    assert_eq!(flags(&empty), [false; 6]);
    assert!(empty.local.is_empty() && empty.remote.is_empty());
}

/// Each kept bit checks its own box (the params' Delete files, Existing files only, Selected files only;
/// the options' Update subdirectories, Synchronize on start); "Use same options next time" never.
#[test]
fn each_kept_bit_checks_its_own_box() {
    let shown = |params: i64, options: i64| -> [bool; 6] {
        let items = items(("", ""), (params, options), true);
        BOXES.map(|item| items[item].state == 1)
    };
    let only = |at: usize| std::array::from_fn::<bool, 6, _>(|n| n == at);
    assert_eq!(shown(DELETE_FLAG, 0), only(0));
    assert_eq!(shown(EXISTING_FLAG, 0), only(1));
    assert_eq!(shown(0, RECURSE_OPTION), only(2));
    assert_eq!(shown(SELECTED_FLAG, 0), only(3));
    assert_eq!(shown(0, SYNCHRONIZE_OPTION), only(4));
    assert_eq!(shown(0, 0), [false; 6]);
    assert_eq!(shown(-1, -1), [true, true, true, true, true, false]);
    let settings = settings_of(&Watch {
        existing_only: true,
        ..watch_of(&[])
    });
    assert!(settings.existing_only && !settings.delete);
    assert!(!settings.timestamps && !settings.mirror);
}

fn node(name: &str, is_dir: bool) -> Node {
    Node {
        name: name.to_string(),
        is_dir,
        children: vec![Node::default()],
        ..Node::default()
    }
}

/// Without subdirectories only the first level's files are watched; the selection keeps its names.
#[test]
fn the_trees_are_trimmed_as_asked() {
    let watch = |recursive: bool| Watch {
        local: String::new(),
        remote: String::new(),
        delete: false,
        existing_only: false,
        recursive,
        selected_only: false,
        on_start: false,
        save: false,
    };
    let nodes = || {
        vec![
            node("a.txt", false),
            node("dir", true),
            node("b.txt", false),
        ]
    };
    let flat = trimmed(nodes(), &watch(false), &[]);
    assert_eq!(flat.len(), 2);
    assert!(flat.iter().all(|node| node.children.is_empty()));
    let deep = trimmed(nodes(), &watch(true), &[]);
    assert_eq!(deep.len(), 3);
    let chosen = trimmed(nodes(), &watch(true), &["DIR".to_string()]);
    assert_eq!(chosen.len(), 1);
    assert_eq!(chosen[0].name, "dir");
}
