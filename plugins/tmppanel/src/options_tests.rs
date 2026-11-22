//! TmpPanel's options without a host: FAR's defaults, its own mode and SavLst's extension from them, the
//! configuration dialog as FAR lays it out and the options its OK leaves.

use nc_plugin_sdk::ItemKind;

use super::*;

#[test]
fn the_defaults_are_fars() {
    let options = Options::default();
    assert!(options.add_to_disks_menu && options.add_to_plugins_menu && options.common_panel);
    assert!(!options.safe_mode_panel && !options.any_in_panel);
    assert_eq!(options.copy_contents, 0);
    assert!(options.replace && options.list_utf8);
    assert!(!options.menu_for_filelist && !options.new_panel_for_search_results);
    assert!(!options.full_screen_panel);
    assert_eq!(
        [
            options.column_types.as_str(),
            &options.column_widths,
            &options.status_column_types,
            &options.status_column_widths,
            &options.mask,
            &options.prefix
        ],
        ["N,S", "0,8", "NR,SC,D,T", "0,8,0,5", "*.temp", "tmp"]
    );
}

#[test]
fn every_settings_key_names_its_field() {
    let mut options = Options::default();
    for key in FLAGS {
        assert!(options.flag(key).is_some(), "{key}");
    }
    for key in TEXTS {
        assert!(options.text(key).is_some(), "{key}");
    }
    assert!(options.flag("Other").is_none() && options.text("Other").is_none());
    *options.flag("Mode").unwrap() = false;
    assert!(!options.replace, "FAR's \"Mode\" holds Replace");
}

#[test]
fn its_own_mode_takes_the_columns_and_full_screen_from_the_options() {
    let mut options = Options::default();
    let mode = options.mode();
    assert_eq!(
        (mode.columns.as_str(), mode.widths.as_str()),
        ("N,S", "0,8")
    );
    assert_eq!(
        (mode.status_columns.as_str(), mode.status_widths.as_str()),
        ("NR,SC,D,T", "0,8,0,5")
    );
    assert!(mode.case_conversion && !mode.full_screen);
    options.column_types = "N,S,D".to_string();
    options.column_widths = "0,8,0".to_string();
    options.status_column_types = "N".to_string();
    options.status_column_widths = "0".to_string();
    options.full_screen_panel = true;
    let mode = options.mode();
    assert_eq!(
        (mode.columns.as_str(), mode.widths.as_str()),
        ("N,S,D", "0,8,0")
    );
    assert_eq!(
        (mode.status_columns.as_str(), mode.status_widths.as_str()),
        ("N", "0")
    );
    assert!(mode.full_screen && mode.case_conversion);
}

#[test]
fn the_list_extension_is_the_first_masks_without_wildcards() {
    let with = |mask: &str| Options {
        mask: mask.to_string(),
        ..Options::default()
    };
    assert_eq!(Options::default().list_extension(), Some("temp"));
    assert_eq!(with("*.lst,*.temp").list_extension(), Some("lst"));
    assert_eq!(with("*.*").list_extension(), None, "a wildcard");
    assert_eq!(with("*.t?p").list_extension(), None);
    assert_eq!(with("list").list_extension(), None, "no dot");
    assert_eq!(with("list.").list_extension(), None, "nothing after it");
    assert_eq!(
        with("*.a.b").list_extension(),
        Some("a.b"),
        "after the first dot"
    );
    assert_eq!(with("").list_extension(), None);
}

#[test]
fn the_dialog_has_fars_title_labels_and_buttons() {
    let dialog = dialog(&Options::default());
    assert_eq!(dialog.title, "Temporary panel");
    let labels: Vec<(usize, &str)> = [
        DISKS,
        COPY,
        COLUMN_TYPES - 1,
        FULL_SCREEN,
        MASK - 1,
        PREFIX - 1,
        OK,
        OK + 1,
    ]
    .into_iter()
    .map(|item| (item, dialog.items[item].text.as_str()))
    .collect();
    assert_eq!(
        labels,
        [
            (DISKS, "Add to &Disks menu"),
            (COPY, "Copy folder c&ontents"),
            (COLUMN_TYPES - 1, "Column &types"),
            (FULL_SCREEN, "F&ull screen mode"),
            (MASK - 1, "File masks for the file &lists:"),
            (PREFIX - 1, "Command line pre&fix:"),
            (OK, "Ok"),
            (OK + 1, "Cancel"),
        ]
    );
    assert_eq!(dialog.items.len(), OK + 2);
    assert!(dialog.items[OK].default);
}

#[test]
fn the_dialogs_items_are_laid_out_as_fars() {
    let dialog = dialog(&Options::default());
    let at = |item: usize| &dialog.items[item];
    assert!(at(PLUGINS).same_row, "FAR's two columns on one row");
    assert_eq!(at(COMMON - 1).kind, ItemKind::Separator);
    assert_eq!(at(COLUMN_TYPES).kind, ItemKind::Edit);
    assert_eq!(at(COLUMN_TYPES).width, 30);
    assert!(at(COPY).three_state);
}

#[test]
fn the_dialog_shows_the_options_values() {
    let options = Options {
        safe_mode_panel: true,
        mask: "*.lst".to_string(),
        ..Options::default()
    };
    let dialog = dialog(&options);
    assert_eq!(dialog.items[SAFE].state, 1);
    assert_eq!(dialog.items[ANY].state, 0);
    assert_eq!(dialog.items[STATUS_WIDTHS].text, "0,8,0,5");
    assert_eq!(dialog.items[MASK].text, "*.lst");
    // CopyContents: off, on, ask (FAR's [?]).
    for (copy, state) in [(0, 0), (1, 1), (2, 2)] {
        let options = Options {
            copy_contents: copy,
            ..Options::default()
        };
        assert_eq!(super::dialog(&options).items[COPY].state, state);
    }
}

#[test]
fn ok_takes_every_value_of_the_dialog_and_keeps_list_utf8() {
    let before = Options {
        list_utf8: false,
        ..Options::default()
    };
    let shown = dialog(&before);
    let mut values: Vec<Value> = (shown.items.iter())
        .map(|item| Value {
            state: item.state,
            text: item.text.clone(),
        })
        .collect();
    assert_eq!(from_dialog(&before, &values), before, "nothing changed");
    for item in [DISKS, PLUGINS, COMMON, REPLACE] {
        values[item].state = 0;
    }
    for item in [SAFE, ANY, MENU, NEW_PANEL, FULL_SCREEN] {
        values[item].state = 1;
    }
    values[COPY].state = 2;
    values[COLUMN_TYPES].text = "N".to_string();
    values[COLUMN_WIDTHS].text = "0".to_string();
    values[STATUS_TYPES].text = "S".to_string();
    values[STATUS_WIDTHS].text = "8".to_string();
    values[MASK].text = "*.lst".to_string();
    values[PREFIX].text = "tp".to_string();
    let after = from_dialog(&before, &values);
    assert_eq!(
        after,
        Options {
            add_to_disks_menu: false,
            add_to_plugins_menu: false,
            common_panel: false,
            safe_mode_panel: true,
            any_in_panel: true,
            copy_contents: 2,
            replace: false,
            menu_for_filelist: true,
            new_panel_for_search_results: true,
            full_screen_panel: true,
            list_utf8: false,
            column_types: "N".to_string(),
            column_widths: "0".to_string(),
            status_column_types: "S".to_string(),
            status_column_widths: "8".to_string(),
            mask: "*.lst".to_string(),
            prefix: "tp".to_string(),
        }
    );
}
