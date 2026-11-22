use nc_plugin_sdk::{ItemKind, Value};

use super::*;

fn values(dialog: &Dialog) -> Vec<Value> {
    (dialog.items.iter())
        .map(|item| Value {
            state: item.state,
            text: item.text.clone(),
        })
        .collect()
}

#[test]
fn the_dialog_is_fars_at_its_places() {
    let options = Options {
        rules: Rules {
            name: Mode::Title,
            extension: Mode::None,
            ..Rules::default()
        },
        subfolders: true,
    };
    let dialog = dialog(&options);
    assert_eq!(dialog.title, "Case conversion");
    assert_eq!(dialog.size, Some((53, 21)));
    let at = |item: usize| &dialog.items[item];
    assert_eq!(at(NAME_MODES + 3).place, (5, 6, 23, 6));
    assert!(at(NAME_MODES).group && at(EXTENSION_MODES).group);
    assert!(at(NAME_MODES + 3).focus && at(NAME_MODES + 3).state == 1);
    assert_eq!(at(EXTENSION_MODES + 4).place.0, 28);
    assert_eq!(at(EXTENSION_MODES + 4).state, 1);
    assert_eq!(at(SUBFOLDERS).state, 1);
    assert_eq!(at(THIS_RUN).state, 0);
    assert_eq!(at(DELIMITERS_LINE).text, " _");
    assert!(at(RESET).no_close);
    assert_eq!(at(OK).kind, ItemKind::Button);
    assert!(at(OK).default);
    assert_eq!(dialog.items.len(), OK + 2);
}

#[test]
fn ok_gives_the_options_as_set() {
    let options = Options {
        rules: Rules {
            name: Mode::Upper,
            extension: Mode::FirstCapital,
            skip_mixed: false,
            folder_names: true,
            delimiters: "-".to_string(),
        },
        subfolders: true,
    };
    let mut values = values(&dialog(&options));
    assert_eq!(from_dialog(&values), (options.clone(), false));
    values[THIS_RUN].state = 1;
    assert!(from_dialog(&values).1);
}
