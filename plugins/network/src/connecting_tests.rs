use super::*;

fn label<'a>(keys: &'a [PanelKey], key: &str) -> &'a str {
    keys.iter()
        .find(|one| one.key == key)
        .map_or("missing", |one| one.label.as_str())
}

/// FAR's key bar: on a computer's shares Map, MapTo, TMap, TMapTo, Unmap and ManCon; on a domain only
/// ManCon; elsewhere none - the keys still the plugin's (F5 and F6 do nothing there, as FAR's).
#[test]
fn the_labels_are_fars_by_the_folder() {
    let server = connection_keys(Some(DISPLAY_SERVER), false);
    let labels: Vec<&str> = ["F4", "F5", "F6", "Shift+F5", "Shift+F6", "F8"]
        .iter()
        .map(|key| label(&server, key))
        .collect();
    assert_eq!(
        labels,
        ["ManCon", "Map", "MapTo", "TMap", "TMapTo", "Unmap"]
    );
    assert_eq!(label(&server, "Del"), "");
    assert_eq!(label(&server, "Ctrl+F"), "");
    let domain = connection_keys(Some(DISPLAY_DOMAIN), false);
    assert_eq!(label(&domain, "F4"), "ManCon");
    assert_eq!(label(&domain, "F5"), "");
    let favorites = connection_keys(Some(DISPLAY_DOMAIN), true);
    assert_eq!(label(&favorites, "F8"), "Remove", "FAR's MF8Fav");
    assert_eq!(label(&favorites, "F4"), "ManCon");
    let top = connection_keys(None, false);
    assert!(top.iter().all(|one| one.label.is_empty()));
    assert_eq!(top.len(), server.len(), "the same keys everywhere");
}

/// Ctrl+F: the name under the cursor quoted when it has a space and a space after it, ".." the folder's
/// own; Alt+Shift+Ins: the selection's names a line each, ".." the folder's.
#[test]
fn names_for_the_command_line_and_the_clipboard() {
    let on = |current: Option<&str>, selected: &[&str]| KeyPress {
        key: "Ctrl+F".to_string(),
        current: current.map(ToString::to_string),
        selected: selected.iter().map(ToString::to_string).collect(),
        ..KeyPress::default()
    };
    let own = r"\\SERVER";
    assert_eq!(command_line_text(&on(Some("Music"), &[]), own), "Music ");
    assert_eq!(
        command_line_text(&on(Some("My Music"), &[]), own),
        "\"My Music\" "
    );
    assert_eq!(command_line_text(&on(None, &[]), own), r"\\SERVER ");
    assert_eq!(
        clipboard_text(&on(Some("x"), &["Music", "My Music"]), own),
        "Music\r\n\"My Music\""
    );
    assert_eq!(clipboard_text(&on(Some("Music"), &[]), own), "Music");
    assert_eq!(clipboard_text(&on(None, &[]), ".\\"), ".\\");
}

/// Shift+F8 acts on the share under the cursor only, the other keys on the selection or else it.
#[test]
fn shift_f8_takes_the_current_share_only() {
    let press = |key: &str| KeyPress {
        key: key.to_string(),
        current: Some("Music".to_string()),
        selected: vec!["Films".to_string(), "Books".to_string()],
        ..KeyPress::default()
    };
    assert_eq!(targets(&press("Shift+F8")), ["Music"]);
    assert_eq!(targets(&press("F8")), ["Films", "Books"]);
    let parent = KeyPress {
        key: "Shift+F8".to_string(),
        ..KeyPress::default()
    };
    assert!(targets(&parent).is_empty(), "nothing on ..");
}
