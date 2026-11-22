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
fn the_titles_are_netbox_s_with_the_plugins_name() {
    assert_eq!(title_of(ITEMS[0]), "NetWay - Transfer settings");
    assert_eq!(title_of(ITEMS[1]), "NetWay - Endurance settings");
    assert_eq!(title_of(ITEMS[2]), "NetWay - Editor/viewer settings");
}

#[test]
fn the_editor_dialog_shows_and_gives_back_its_options() {
    let defaults = EditorOptions::default();
    let items = editor_items(defaults);
    assert_eq!(items[UPLOAD_ON_SAVE].text, "Upload after every &save");
    assert_eq!(items[OK].text, "&Ok");
    assert!(
        items[UPLOAD_SAME].disabled,
        "every save uploads: nothing to ask"
    );
    assert_eq!(editor_options_of(&values(&items)), defaults);
    let asked = EditorOptions {
        download_default: false,
        upload_same: false,
        upload_on_save: false,
    };
    let items = editor_items(asked);
    assert!(!items[UPLOAD_SAME].disabled && items[UPLOAD_OPTIONS].state == 1);
    assert_eq!(editor_options_of(&values(&items)), asked);
}
