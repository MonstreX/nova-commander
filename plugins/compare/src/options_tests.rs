use super::*;
use nc_plugin_sdk::{ItemKind, Value};

#[test]
fn the_defaults_are_fars() {
    let options = Options::default();
    assert!(options.time && options.low_precision && options.ignore_time_zone && options.size);
    assert!(!options.subfolders && !options.contents && !options.selected_only);
    assert_eq!(options.max_depth, 99);
    assert!(!options.whitespace, "new lines ignored");
}

#[test]
fn the_dialog_is_at_fars_places() {
    let dialog = Options::default().dialog(false, true);
    assert_eq!(dialog.title, "Advanced compare");
    assert_eq!(dialog.items.len(), 20);
    let place = |item: usize| dialog.items[item].place;
    assert_eq!(place(SUBFOLDERS), (5, 3, 20, 3));
    assert_eq!(place(USE_MAX_DEPTH), (9, 4, 35, 4));
    assert_eq!(place(MAX_DEPTH), (35, 4, 39, 4));
    assert_eq!(dialog.items[MAX_DEPTH].kind, ItemKind::Fixed);
    assert_eq!(dialog.items[MAX_DEPTH].text, "99");
    assert_eq!(place(TIME), (5, 8, 14, 8));
    assert_eq!(place(NEW_LINES).0, 13);
    assert!(dialog.items[NEW_LINES].group);
    assert_eq!(place(NO_DIFFERENCE), (5, 17, 56, 17));
    assert_eq!(place(OK), (24, 19, 29, 19));
    assert!(dialog.items[OK].default);
    assert!(dialog.items[SUBFOLDERS].focus, "the first box");
}

#[test]
fn a_plugins_panel_and_no_selection_turn_their_boxes_off() {
    let all = Options {
        subfolders: true,
        contents: true,
        contents_ignore: true,
        selected_only: true,
        ..Options::default()
    };
    let dialog = all.dialog(true, false);
    for item in [
        SUBFOLDERS,
        USE_MAX_DEPTH,
        MAX_DEPTH,
        CONTENTS,
        CONTENTS_IGNORE,
        NEW_LINES,
    ] {
        assert!(dialog.items[item].disabled, "{item}");
    }
    assert_eq!(dialog.items[SUBFOLDERS].state, 0);
    assert_eq!(dialog.items[CONTENTS].state, 0);
    assert!(dialog.items[SELECTED_ONLY].disabled);
    assert_eq!(dialog.items[SELECTED_ONLY].state, 0);
    assert!(dialog.items[TIME].focus, "the first box on");
    let open = all.dialog(false, true);
    assert!(!open.items[NEW_LINES].disabled && !open.items[MAX_DEPTH].disabled);
}

#[test]
fn the_values_give_the_options() {
    let mut values = vec![Value::default(); 20];
    for item in [SUBFOLDERS, TIME, CONTENTS, CONTENTS_IGNORE, WHITESPACE] {
        values[item].state = 1;
    }
    values[MAX_DEPTH].text = " 7".to_string();
    let closed = Closed {
        button: Some(OK),
        values,
    };
    let options = Options::of_values(&closed);
    assert!(options.subfolders && options.time && options.contents && options.whitespace);
    assert!(!options.size && !options.low_precision);
    assert_eq!(options.max_depth, 7);
}

fn closed_with(on: &[usize], depth: &str) -> Closed {
    let mut values = vec![Value::default(); 20];
    for &item in on {
        values[item].state = 1;
    }
    values[MAX_DEPTH].text = depth.to_string();
    Closed {
        button: Some(OK),
        values,
    }
}

#[test]
fn the_settings_saved_are_the_items_enabled_as_the_dialog_closed() {
    // Subfolders and contents turned on in the dialog: what they let on is saved too.
    let closed = closed_with(
        &[
            SUBFOLDERS,
            USE_MAX_DEPTH,
            CONTENTS,
            CONTENTS_IGNORE,
            WHITESPACE,
        ],
        "3",
    );
    let saved = saved(&closed, false, false);
    let value = |key: &str| {
        saved
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(value("UseMaxScanDepth"), Some("1"));
    assert_eq!(value("MaxScanDepth"), Some("3"));
    assert_eq!(value("CompareContentsIgnore"), Some("1"));
    assert_eq!(value("IgnoreWhitespace"), Some("1"));
    assert_eq!(value("ProcessSelected"), None, "off without a selection");
    assert_eq!(value("LowPrecisionTime"), None, "time off");
    // Turned off in the dialog: what it held is kept as it was.
    let off = closed_with(&[TIME], "5");
    let saved = super::saved(&off, false, true);
    assert!(saved.iter().all(|(name, _)| *name != "MaxScanDepth"));
    assert!(
        saved
            .iter()
            .any(|(name, value)| *name == "ProcessSelected" && value == "0")
    );
    let plugins = super::saved(&closed, true, false);
    assert!(
        plugins
            .iter()
            .all(|(name, _)| *name != "ProcessSubfolders" && *name != "CompareContents")
    );
}
