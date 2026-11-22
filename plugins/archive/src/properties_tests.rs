use super::*;
use crate::test_archives::{make, opened, zip};

/// arclite's attribute letters, unknown bits in hex, a Unix mode in the high half.
#[test]
fn attributes_are_written_as_arclite_writes_them() {
    assert_eq!(attributes_text(0x21), "RA");
    assert_eq!(attributes_text(0x10), "D");
    assert_eq!(attributes_text(0), "");
    assert_eq!(attributes_text(0x10000), "00010000", "bit 16 has no letter");
    assert_eq!(attributes_text(0x21 | 0x10000), "RA 00010000");
    let unix = (0o100_644 << 16) | 0x8000;
    assert_eq!(attributes_text(unix), "-rw-r--r--");
    let folder = (0o040_755 << 16) | 0x8000 | 0x10;
    assert_eq!(attributes_text(folder), "D drwxr-xr-x");
    assert_eq!(posix_text(0o4755), "0rwsr-xr-x");
    assert_eq!(posix_text(0o1644), "0rw-r--r-T");
    assert_eq!(posix_text(0x1_0000), "0--------- 00010000");
}

/// A zip's item: the properties 7-Zip's zip handler gives, in arclite's names and forms; an encrypted
/// one's told.
#[test]
fn an_items_properties_are_7zips() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.zip");
    zip(&path, &[("docs/a.txt", Some(b"alpha alpha alpha"))]);
    let opened = opened(&path);
    let properties = item_properties(&opened, 0);
    let value = |properties: &[(String, String)], name: &str| {
        (properties.iter())
            .find(|(own, _)| own == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("{name}: {properties:?}"))
    };
    assert_eq!(value(&properties, "Path"), r"docs\a.txt");
    assert_eq!(value(&properties, "Folder"), "No");
    assert_eq!(value(&properties, "Size"), "17");
    assert_eq!(value(&properties, "Method"), "Store");
    assert_eq!(value(&properties, "CRC").len(), 8);
    assert!(!value(&properties, "Modified").is_empty());
    let secret = temp.path().join("s.zip");
    make(
        &secret,
        "zip",
        &[("s.txt", Some(b"hidden"))],
        (false, Some("pw")),
    );
    let locked = item_properties(&opened_secret(&secret), 0);
    assert_eq!(value(&locked, "Encrypted"), "Yes");
}

fn opened_secret(path: &std::path::Path) -> Opened {
    crate::test_archives::opened_with(path, Some("pw"))
}

/// arclite's dialog: the names in a column, the values read-only one wider than the longest, OK and
/// "..." in one row.
#[test]
fn the_dialog_is_arclites() {
    let properties = vec![
        ("Path".to_string(), "a.txt".to_string()),
        ("Packed Size".to_string(), "12".to_string()),
    ];
    let dialog = dialog(&properties, 80);
    assert_eq!(dialog.title, "Attributes");
    let value = &dialog.items[1];
    assert!(value.read_only && value.text == "a.txt");
    assert_eq!(
        value.place,
        (5 + 12, 2, 5 + 12 + 5, 2),
        "after 11 and a space, 6 wide"
    );
    let buttons: Vec<&str> = dialog
        .items
        .iter()
        .rev()
        .take(2)
        .map(|item| item.text.as_str())
        .collect();
    assert_eq!(buttons, ["...", "OK"]);
    let narrow = dialog_of_width(&[("N".repeat(50), "v".repeat(100))], 60);
    assert_eq!(
        narrow.size,
        Some((70, 7)),
        "within the width, half for the names"
    );
}

/// arclite's `show_attr`: ".." at the root the archive's own lines, ".." below it the folder's, a file's
/// own (before a folder of its name), a folder only implied none; a folder's attributes with its "D" and
/// no CRC.
#[test]
fn ctrl_a_shows_what_arclite_shows() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("a.zip");
    zip(
        &path,
        &[
            ("named", None),
            ("named/x.txt", Some(b"x")),
            ("implied/y.txt", Some(b"y")),
            ("top.txt", Some(b"t")),
        ],
    );
    let opened = opened(&path);
    let first = |rows: Vec<(String, String)>| rows.first().cloned().unwrap_or_default();
    let shown_for = |path, current| super::shown_for(&opened, (path, current));
    let root = shown_for("/", None);
    assert_eq!(first(root.clone()).0, "Path");
    assert!(
        root.iter().any(|(name, _)| name == "File count"),
        "the info lines"
    );
    let named = shown_for("/named", None);
    assert_eq!(first(named.clone()).1, "named", "the folder's own");
    let attributes = named.iter().find(|(name, _)| name == "Attributes").unwrap();
    assert!(attributes.1.contains('D'), "{attributes:?}");
    assert!(
        named
            .iter()
            .all(|(name, value)| name != "CRC" || value.is_empty()),
        "a folder has no CRC: {named:?}"
    );
    assert!(shown_for("/implied", None).is_empty(), "only implied");
    assert_eq!(first(shown_for("/", Some("top.txt"))).1, "top.txt");
    assert_eq!(first(shown_for("/named", Some("x.txt"))).1, r"named\x.txt");
    assert!(shown_for("/", Some("none")).is_empty());
}

fn dialog_of_width(properties: &[(String, String)], width: usize) -> Dialog {
    dialog(properties, width)
}
