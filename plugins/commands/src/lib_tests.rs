use nc_plugin_sdk::ItemKind;

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
fn the_configuration_is_fars_at_its_places() {
    let dialog = dialog(&Options::default());
    assert_eq!(dialog.title, "Command prefixes");
    assert_eq!(dialog.size, Some((53, 19)));
    assert!(dialog.items[SCREEN].group && dialog.items[CATCH].group);
    assert_eq!(dialog.items[SCREEN].state, 1, "hidden by default");
    assert_eq!(dialog.items[MOST_LINE].kind, ItemKind::Fixed);
    assert_eq!(dialog.items[MOST_LINE].text, "1048576");
    assert_eq!(dialog.items[MOST_LINE].place, (28, 14, 37, 14));
    assert!(dialog.items[OK].default);
    assert_eq!(dialog.items.len(), OK + 2);
}

#[test]
fn ok_gives_the_options_and_no_size_is_fars_default() {
    let options = Options {
        screen: Screen::Both,
        catch: Catch::Apart,
        empty_files: false,
        new_files: false,
        most: 4096,
        separator: " ".to_string(),
    };
    let mut values = values(&dialog(&options));
    assert_eq!(from_dialog(&values, " "), options);
    values[MOST_LINE].text = "0".to_string();
    assert_eq!(from_dialog(&values, " ").most, 1_048_576);
}

#[test]
fn each_prefix_has_its_syntax() {
    for prefix in parse::PREFIXES {
        assert!(
            syntax(prefix)[0].starts_with(&format!("{prefix}:")),
            "{prefix}"
        );
    }
}
