use super::*;
use nc_plugin_sdk::ItemKind;

/// The values the items start with, as the dialog hands them back unchanged.
fn values(items: &[DialogItem]) -> Vec<Value> {
    (items.iter())
        .map(|item| Value {
            text: item.text.clone(),
            state: item.state,
        })
        .collect()
}

#[test]
fn the_items_stand_where_netbox_puts_them() {
    let items = items(&CopyParams::default(), Attrs::default(), false);
    assert_eq!(items.len(), OK + 2);
    type Place = (i32, i32, i32, i32);
    let kinds_titles: Vec<(&str, Place)> = (items.iter())
        .filter(|item| item.kind == ItemKind::Box)
        .map(|item| (item.text.as_str(), item.place))
        .collect();
    assert_eq!(
        kinds_titles,
        [
            ("Transfer mode", (4, 2, 42, 2)),
            ("Upload options", (44, 2, 74, 2)),
            ("Filename modification", (4, 8, 42, 8)),
            ("Download options", (4, 12, 42, 12)),
            ("Common options", (44, 11, 74, 11)),
        ]
    );
    assert_eq!(items[TEXT_MODE].text, "&Text (plain text, html, ...)");
    assert!(items[TEXT_MODE].group && items[NO_CHANGE].group);
    assert_eq!(items[BINARY].state, 1, "binary by default");
    assert!(items[ASCII_MASK].disabled, "the masks only with Automatic");
    assert_eq!(items[REPLACE].text, "&Replace \\:*\"?");
    assert_eq!(items[OCTAL].text, "0644");
    assert!(
        items[OCTAL].disabled && items[right_item(0)].disabled,
        "rights only when set"
    );
    assert_eq!(items[right_item(0)].text, "R");
    assert_eq!(items[right_item(4)].text, "W");
    assert_eq!(items[SPEED].text, "Unlimited");
    assert!(items[TEMP_DIR].disabled);
    assert_eq!(items[OK].text, "&Ok");
}

#[test]
fn what_a_direction_cannot_use_is_disabled() {
    let params = CopyParams {
        preserve_rights: true,
        ..CopyParams::default()
    };
    let down = items(&params, Attrs::download(), false);
    assert!(down[RIGHTS].disabled && down[OCTAL].disabled && down[CLEAR_ARCHIVE].disabled);
    assert!(down[IGNORE].disabled && !down[READ_ONLY].disabled);
    let up = items(&params, Attrs::upload(false), false);
    assert!(up[READ_ONLY].disabled && !up[RIGHTS].disabled && !up[OCTAL].disabled);
    assert!(!up[CLEAR_ARCHIVE].disabled && !up[IGNORE].disabled);
    assert!(
        items(&params, Attrs::upload(false), true)[CLEAR_ARCHIVE].disabled,
        "an editor's"
    );
    let all_x = CopyParams {
        rights: 0o755,
        ..params
    };
    let shown = items(&all_x, Attrs::default(), false);
    assert!(
        shown[ADD_X].disabled && shown[ADD_X].state == 0,
        "X everywhere: nothing to add"
    );
}

#[test]
fn the_values_give_the_parameters_back() {
    let start = CopyParams::default();
    let mut values = values(&items(&start, Attrs::default(), false));
    assert_eq!(params_of(&values, &start), start, "unchanged");
    values[BINARY].state = 0;
    values[AUTOMATIC].state = 1;
    values[ASCII_MASK].text = "*.txt".to_string();
    values[NO_CHANGE].state = 0;
    values[NO_CHANGE + 3].state = 1;
    values[RIGHTS].state = 1;
    values[OCTAL].text = "0750".to_string();
    values[PRESERVE_TIME].state = 0;
    values[FILE_MASK].text = "*.log".to_string();
    values[SPEED].text = "64".to_string();
    values[USE_TEMP].state = 1;
    values[TEMP_DIR].text = " C:\\Temp ".to_string();
    let got = params_of(&values, &start);
    assert_eq!(got.transfer_mode, TransferMode::Automatic);
    assert_eq!(got.ascii_mask, "*.txt");
    assert_eq!(got.name_case, NameCase::Lower);
    assert!(got.preserve_rights && !got.preserve_time);
    assert_eq!(got.rights, 0o750);
    assert_eq!(
        (got.include_mask.as_str(), got.speed_limit),
        ("*.log", 64 * 1024)
    );
    assert_eq!(got.temp_path, "C:\\Temp");
    values[OCTAL].text = "    ".to_string();
    values[right_item(0)].state = 1;
    values[right_item(1)].state = 0;
    assert_eq!(
        params_of(&values, &start).rights,
        0o644 & !0o200,
        "the boxes without octal"
    );
}

#[test]
fn ok_refuses_a_wrong_mask_speed_or_octal() {
    let start = CopyParams::default();
    let fresh = values(&items(&start, Attrs::default(), false));
    assert_eq!(wrong(&fresh), None);
    let mut bad = fresh.clone();
    bad[FILE_MASK].text = "a|b|c".to_string();
    assert_eq!(wrong(&bad).map(|(item, _)| item), Some(FILE_MASK));
    let mut bad = fresh.clone();
    bad[SPEED].text = "fast".to_string();
    assert_eq!(
        wrong(&bad),
        Some((SPEED, "'fast' is not a valid speed limit.".to_string()))
    );
    let mut bad = fresh.clone();
    bad[OCTAL].text = "99".to_string();
    assert_eq!(wrong(&bad), None, "the rights not set: not read");
    bad[RIGHTS].state = 1;
    assert_eq!(wrong(&bad).map(|(item, _)| item), Some(OCTAL));
    let mut bad = fresh;
    bad[ASCII_MASK].text = "x>y".to_string();
    assert_eq!(wrong(&bad), None, "the text masks only with Automatic");
    bad[AUTOMATIC].state = 1;
    assert_eq!(wrong(&bad).map(|(item, _)| item), Some(ASCII_MASK));
}
