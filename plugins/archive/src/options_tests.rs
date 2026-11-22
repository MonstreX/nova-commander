use nc_plugin_sdk::{ItemKind, Value};

use super::*;

/// arclite's defaults: Enter by its masks, Ctrl+PgDn by contents, its own view mode 2, the prefix `arc`.
#[test]
fn the_defaults_are_arclites() {
    let options = Options::default();
    assert!(options.handle_commands && options.use_include_masks && !options.pgdn_masks);
    assert!(options.include_masks.starts_with("*.zip,*.rar,*.[7bgxl]z,"));
    assert!(options.include_masks.ends_with(",*.zst"));
    assert_eq!(
        (options.plugin_prefix.as_str(), options.panel_view_mode),
        ("arc", 2)
    );
    assert_eq!(options.oem_cp, 0, "the system's");
}

/// The items the indexes name, in arclite's order.
#[test]
fn the_dialog_is_arclites() {
    let dialog = Options::default().dialog();
    let label = |item: usize| dialog.items[item].text.as_str();
    assert_eq!(label(HANDLE_CREATE), "Handle archive creation (Shift-F&1)");
    assert_eq!(
        label(HANDLE_COMMANDS),
        "Handle archive commands (Ctrl-PgDn, Shift-F&2)"
    );
    assert_eq!(label(OWN_VIEW_MODE), "Use o&wn panel view mode");
    assert_eq!(label(OEM_CP - 1), "OEM &Codepage:");
    assert_eq!(label(ANSI_CP - 1), "&ANSI Codepage:");
    assert_eq!(label(SAVE_CP), "Sa&ve Codepages");
    assert_eq!(label(USE_INCLUDE - 1), "&Open files matching masks:");
    assert_eq!(label(INCLUDE), DEFAULT_MASKS);
    assert_eq!(label(USE_EXCLUDE - 1), "&Exclude files matching masks:");
    assert_eq!(label(PGDN_MASKS), "Ma&sk settings apply to Ctrl-PgDn");
    assert_eq!(label(DEFAULT_MASKS_BUTTON), "Default &masks");
    assert_eq!(label(OK), "OK");
    assert_eq!(dialog.items[EXCLUDE].kind, ItemKind::Edit);
    assert_eq!(dialog.title, TITLE);
}

/// OK gives what the items say (a code page not a number the system's); Esc nothing.
#[test]
fn the_dialog_gives_the_settings() {
    let options = Options::default();
    let mut values: Vec<Value> = options
        .dialog()
        .items
        .iter()
        .map(|item| Value {
            state: item.state,
            text: item.text.clone(),
        })
        .collect();
    values[PGDN_MASKS].state = 1;
    values[USE_EXCLUDE].state = 1;
    values[EXCLUDE].text = "*.docx".to_string();
    values[OEM_CP].text = " 866 ".to_string();
    values[ANSI_CP].text = "x".to_string();
    let closed = Closed {
        button: Some(OK),
        values: values.clone(),
    };
    let got = options.from_dialog(&closed).unwrap();
    assert!(got.pgdn_masks && got.use_exclude_masks);
    assert_eq!(got.exclude_masks, "*.docx");
    assert_eq!((got.oem_cp, got.ansi_cp), (866, 0));
    assert_eq!(got.include_masks, DEFAULT_MASKS);
    assert_eq!(got.plugin_prefix, "arc", "not in the dialog: kept");
    let esc = Closed {
        button: None,
        values,
    };
    assert_eq!(options.from_dialog(&esc), None);
}

/// The formats' part and its buttons, in arclite's order and words.
#[test]
fn the_dialogs_formats_part_is_arclites() {
    let dialog = Options::default().dialog();
    let label = |item: usize| dialog.items[item].text.as_str();
    assert_eq!(label(EDIT_INCLUDE), "Edit");
    assert_eq!(label(EDIT_EXCLUDE), "Edit");
    assert_eq!(label(GENERATE_MASKS), "&Generate masks");
    assert_eq!(label(USE_ENABLED - 1), "E&nable archive formats:");
    assert_eq!(label(USE_ENABLED), "Active");
    assert_eq!(label(EDIT_ENABLED), "Edit");
    assert_eq!(dialog.items[ENABLED].kind, ItemKind::Edit);
    assert_eq!(label(USE_DISABLED - 1), "&Disable archive formats:");
    assert_eq!(dialog.items[DISABLED].kind, ItemKind::Edit);
    assert_eq!(label(PGDN_FORMATS), "&Format settings apply to Ctrl-PgDn");
    assert_eq!(label(LIBRARY_INFO), "&7z library info");
    assert!(dialog.items[LIBRARY_INFO].no_close && dialog.items[EDIT_ENABLED].no_close);
    let options = Options::default();
    assert!(!options.use_enabled_formats && !options.use_disabled_formats && !options.pgdn_formats);
}

/// OK gives the formats' lists too.
#[test]
fn the_dialog_gives_the_formats_settings() {
    let options = Options::default();
    let mut values: Vec<Value> = (options.dialog().items.iter())
        .map(|item| Value {
            state: item.state,
            text: item.text.clone(),
        })
        .collect();
    values[USE_ENABLED].state = 1;
    values[ENABLED].text = "7z,zip".to_string();
    values[USE_DISABLED].state = 1;
    values[DISABLED].text = "Rar".to_string();
    values[PGDN_FORMATS].state = 1;
    let closed = Closed {
        button: Some(OK),
        values,
    };
    let got = options.from_dialog(&closed).unwrap();
    assert!(got.use_enabled_formats && got.use_disabled_formats && got.pgdn_formats);
    assert_eq!(
        (got.enabled_formats.as_str(), got.disabled_formats.as_str()),
        ("7z,zip", "Rar")
    );
}

fn named(names: &[&str]) -> Vec<nc_plugin_sdk::sevenzip::Format> {
    (names.iter())
        .map(|name| nc_plugin_sdk::sevenzip::Format {
            name: (*name).to_string(),
            ..nc_plugin_sdk::sevenzip::Format::default()
        })
        .collect()
}

/// arclite's `analyse_open`: the enabled list (whatever its case) less the disabled; for Ctrl+PgDn only
/// with "Format settings apply to Ctrl-PgDn"; no list on: every format.
#[test]
fn the_formats_tried_follow_the_lists() {
    let formats = named(&["7z", "Rar", "zip", "gzip"]);
    let mut options = Options::default();
    assert_eq!(options.tried_formats(&formats, false), None);
    options.use_enabled_formats = true;
    options.enabled_formats = "ZIP,7z,Rar".to_string();
    assert_eq!(options.tried_formats(&formats, false), Some(vec![0, 1, 2]));
    options.use_disabled_formats = true;
    options.disabled_formats = "rar".to_string();
    assert_eq!(options.tried_formats(&formats, false), Some(vec![0, 2]));
    assert_eq!(
        options.tried_formats(&formats, true),
        None,
        "not for Ctrl+PgDn"
    );
    options.pgdn_formats = true;
    assert_eq!(options.tried_formats(&formats, true), Some(vec![0, 2]));
    options.use_enabled_formats = false;
    assert_eq!(options.tried_formats(&formats, false), Some(vec![0, 2, 3]));
    options.disabled_formats = "7z,Rar,zip,gzip".to_string();
    assert_eq!(options.tried_formats(&formats, false), Some(vec![]));
}

/// A list counts only while its "Active" is on, whatever it holds; Ctrl+PgDn with neither on tries
/// every format too.
#[test]
fn a_list_not_active_does_not_count() {
    let formats = named(&["7z", "Rar", "zip"]);
    let mut options = Options {
        enabled_formats: "zip".to_string(),
        disabled_formats: "7z".to_string(),
        pgdn_formats: true,
        ..Options::default()
    };
    assert_eq!(options.tried_formats(&formats, false), None);
    assert_eq!(options.tried_formats(&formats, true), None);
    options.use_enabled_formats = true;
    assert_eq!(options.tried_formats(&formats, false), Some(vec![2]));
    options.enabled_formats = "zip,7z".to_string();
    assert_eq!(
        options.tried_formats(&formats, false),
        Some(vec![0, 2]),
        "the disabled list not active"
    );
    options.use_enabled_formats = false;
    options.use_disabled_formats = true;
    assert_eq!(options.tried_formats(&formats, true), Some(vec![1, 2]));
}

/// "Edit" beside a list as arclite's handlers enable it: never with "Active" off; a formats list's
/// always with it on; a masks list's with masks in its line, or as "Active" is clicked whatever it holds.
#[test]
fn edit_is_enabled_as_arclite_enables_it() {
    for (on, clicked, masks, empty, enabled) in [
        (false, true, false, false, false),
        (false, false, true, false, false),
        (true, false, false, true, true),
        (true, false, true, false, true),
        (true, false, true, true, false),
        (true, true, true, true, true),
    ] {
        assert_eq!(
            edit_enabled(on, clicked, masks, empty),
            enabled,
            "{on} {clicked} {masks} {empty}"
        );
    }
}
