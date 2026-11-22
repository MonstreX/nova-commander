use super::*;
use nc_plugin_sdk::ItemKind;

fn mark(name: &str) -> Mark {
    Mark {
        name: name.to_string(),
        remote: format!("/{name}"),
        local: format!(r"C:\{name}"),
    }
}

#[test]
fn bookmarks_are_added_once_by_name() {
    let mut profiles = Profiles::default();
    assert!(profiles.add(false, mark("work")));
    assert!(!profiles.add(false, mark("work")), "a name taken");
    assert!(
        profiles.add(true, mark("work")),
        "the shared list is another"
    );
    assert!(
        !profiles.add(false, Mark::default()),
        "an empty name is no bookmark"
    );
    assert_eq!(profiles.session.len(), 1);
    assert_eq!(profiles.list(true)[0].local, r"C:\work");
}

#[test]
fn the_dialog_is_netboxs() {
    let profiles = Profiles {
        session: vec![mark("a"), mark("b")],
        shared: vec![mark("c")],
    };
    let items = items(&profiles);
    assert_eq!(items[SESSION].text, "&Session profiles");
    assert_eq!(items[SESSION].state, 1);
    assert_eq!(items[LIST].kind, ItemKind::ListBox);
    assert_eq!(items[LIST].lines, ["a", "b"]);
    assert!(items[LOCAL].read_only && items[REMOTE].read_only);
    assert_eq!(items[ADD].text, "&Add");
    assert!(items[ADD].no_close && !items[OPEN].no_close);
    assert_eq!(items[OK].text, "OK");
    assert_eq!(remove_question("a"), "Do you wish to remove bookmark 'a'");
}
