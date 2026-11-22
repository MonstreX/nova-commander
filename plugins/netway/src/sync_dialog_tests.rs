use super::*;
use nc_plugin_sdk::ItemKind;

fn values_of(items: &[DialogItem]) -> Vec<Value> {
    (items.iter())
        .map(|item| Value {
            text: item.text.clone(),
            state: item.state,
        })
        .collect()
}

#[test]
fn the_items_are_where_the_dialog_reads_them() {
    let items = items((r"C:\work", "/srv"), (Mode::Local, PREVIEW), (true, false));
    assert_eq!(items[LOCAL].text, r"C:\work");
    assert_eq!(items[REMOTE].text, "/srv");
    assert_eq!(items[TO_LOCAL].state, 1);
    assert_eq!(items[FILES].text, "Synchronize &files");
    assert_eq!(items[TIMESTAMPS].text, "Synchronize &timestamps");
    assert_eq!(items[PREVIEW_ITEM].state, 1);
    assert!(items[SELECTED].disabled, "no selection");
    assert_eq!(items[BY_TIME].state, 1);
    assert_eq!(
        (items[OK].kind, items[OK].text.as_str()),
        (ItemKind::Button, "OK")
    );
    let no_times = items_of(false);
    assert!(no_times[TIMESTAMPS].disabled);
}

fn items_of(timestamps_too: bool) -> Vec<DialogItem> {
    items(("", ""), (Mode::Remote, TIMESTAMP), (timestamps_too, true))
}

/// The choice and its `sp*` flags go round.
#[test]
fn the_choice_goes_round_its_params() {
    let params = DELETE | MIRROR | BY_SIZE | SELECTED_ONLY | NOT_BY_TIME;
    let items = items(("l", "r"), (Mode::Remote, params), (true, true));
    let chosen = chosen(&values_of(&items));
    assert_eq!(chosen.settings.mode, Mode::Remote);
    assert!(chosen.settings.delete && chosen.settings.mirror && chosen.settings.by_size);
    assert!(!chosen.settings.by_time && chosen.selected_only && !chosen.preview);
    assert_eq!(params_of(&chosen), params);
    assert_eq!((chosen.local.as_str(), chosen.remote.as_str()), ("l", "r"));
    let stamps = chosen_of(TIMESTAMP | EXISTING_ONLY);
    assert!(stamps.settings.timestamps && stamps.settings.existing_only);
}

fn chosen_of(params: i64) -> Chosen {
    chosen(&values_of(&items(
        ("", ""),
        (Mode::Both, params),
        (true, false),
    )))
}

#[test]
fn the_modes_have_winscps_values() {
    for mode in [Mode::Remote, Mode::Local, Mode::Both] {
        assert_eq!(mode_of(mode_value(mode)), Some(mode));
    }
    assert_eq!(mode_of(-1), None, "from the panel");
}
