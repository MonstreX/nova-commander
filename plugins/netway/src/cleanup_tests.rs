use super::*;
use nc_plugin_sdk::ItemKind;

#[test]
fn the_parts_there_are_offered() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(present(temp.path()), [Part::Configuration]);
    std::fs::create_dir_all(temp.path().join(crate::SESSIONS_FOLDER)).unwrap();
    assert_eq!(
        present(temp.path()),
        [Part::Configuration],
        "no session in it"
    );
    std::fs::write(
        temp.path().join(crate::SESSIONS_FOLDER).join("a.netway"),
        "Host=h\n",
    )
    .unwrap();
    std::fs::write(temp.path().join("known_certificates"), "h:21 aa\n").unwrap();
    assert_eq!(
        present(temp.path()),
        [Part::Configuration, Part::Sessions, Part::Caches]
    );
}

/// NetBox's dialog: the check boxes, "Select all" / "Deselect all", OK disabled until one is checked.
#[test]
fn the_dialog_is_netboxs() {
    let parts = [Part::Configuration, Part::Sessions];
    let items = items(&parts);
    assert_eq!(items[0].text, "&Configuration");
    assert_eq!(items[1].text, "&Stored sessions");
    assert_eq!(items[3].text, "Select &all");
    assert_eq!(items[4].text, "&Deselect all");
    assert!(items[3].no_close && items[4].no_close);
    let ok = &items[parts.len() + 4];
    assert_eq!((ok.kind, ok.text.as_str()), (ItemKind::Button, "OK"));
    assert!(ok.disabled && ok.default);
    assert_eq!(Part::Caches.label(), "&Caches");
}
