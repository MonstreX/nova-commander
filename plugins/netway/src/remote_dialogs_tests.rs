use super::*;

fn values(items: &[DialogItem]) -> Vec<Value> {
    (items.iter())
        .map(|item| Value {
            text: item.text.clone(),
            state: item.state,
        })
        .collect()
}

#[test]
fn the_questions_and_lines_are_netboxs() {
    let one = vec!["a.txt".to_string()];
    let two = vec!["a".to_string(), "b".to_string()];
    assert_eq!(
        delete_question(&one),
        "Do you wish to delete the file a.txt"
    );
    assert_eq!(delete_question(&two), "Do you wish to delete 2 items");
    assert_eq!(
        transfer_prompt(&one, false),
        "Duplicate file 'a.txt' to remote directory:"
    );
    assert_eq!(
        transfer_prompt(&two, true),
        "Move 2 files to remote directory:"
    );
    assert_eq!(transfer_value("/home/", &one), "/home/a.txt");
    assert_eq!(transfer_value("/", &two), "/*.*");
    assert_eq!(
        split_value("/home/x/*.bak"),
        ("/home/x".to_string(), "*.bak".to_string())
    );
    assert_eq!(split_value("/new"), ("/".to_string(), "new".to_string()));
    assert_eq!(split_value("new"), (String::new(), "new".to_string()));
}

#[test]
fn make_folder_gives_its_name_and_rights() {
    let items = folder_items("docs", Some(0o750));
    assert_eq!(items[FOLDER_NAME].text, "docs");
    assert_eq!(items[SET_RIGHTS].state, 1);
    assert_eq!(items[FOLDER_BOX.octal()].text, "0750");
    let made = folder_of(&values(&items));
    assert_eq!(
        made,
        NewFolder {
            name: "docs".to_string(),
            rights: Some(0o750),
            save: false
        }
    );
    let plain = folder_items("x", None);
    assert!(plain[FOLDER_BOX.octal()].disabled, "no rights: the box off");
    assert_eq!(folder_of(&values(&plain)).rights, None);
    assert_eq!(plain[FOLDER_BOX.end()].text, "Use &same settings next time");
}

#[test]
fn attributes_tell_what_changed() {
    let current = Current {
        names: vec!["docs".to_string()],
        uid: Some(1000),
        owner: Some("user".to_string()),
        gid: Some(100),
        group: Some("users".to_string()),
        rights: Rights::of(0o755),
        any_folder: true,
        ..Current::default()
    };
    let slots = attribute_slots(&current);
    let items = attribute_items(&current);
    assert_eq!(items[slots.uid].text, "1000");
    assert_eq!(items[slots.uid + 1].text, "[ user ]");
    assert_eq!(items[slots.ok].text, "&Ok");
    assert!(items[slots.ok].disabled, "nothing changed yet");
    let mut read = values(&items);
    assert_eq!(changed(&current, &read), None);
    read[slots.gid].text = "50".to_string();
    read[slots.rights.check(2, 2)].state = 0;
    read[slots.recursive.unwrap()].state = 1;
    let properties = changed(&current, &read).unwrap();
    assert_eq!(properties.gid, Some(50));
    assert_eq!(properties.uid, None);
    assert_eq!(properties.rights, Some(Rights::of(0o754)));
    assert!(properties.recursive);
    let linked = Current {
        link_to: Some("/target".to_string()),
        ..current
    };
    let items = attribute_items(&linked);
    assert_eq!(items[3].text, "/target");
    assert!(items[3].read_only);
    assert_eq!(items[attribute_slots(&linked).uid].text, "1000");
}

#[test]
fn the_link_dialog_needs_both_lines() {
    let link = Link {
        name: String::new(),
        point_to: "docs".to_string(),
        symbolic: true,
    };
    let items = link_items(&link, false);
    assert!(items[LINK_OK].disabled);
    assert_eq!(items[SYMBOLIC].state, 1);
    let edit = link_items(
        &Link {
            name: "ln".to_string(),
            ..link
        },
        true,
    );
    assert!(!edit[LINK_OK].disabled && edit[LINK_NAME].disabled && edit[SYMBOLIC].disabled);
}
