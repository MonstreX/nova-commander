use super::*;
use nc_plugin_sdk::{ItemKind, Value};

#[test]
fn the_defaults_are_fars() {
    let options = Options::default();
    assert!(!options.wait && !options.delete_with_f8 && !options.silent);
    assert!(options.clear_selection);
    assert_eq!(options.gui, Gui::Ask);
    assert_eq!(options.extra, Extra::Nothing);
    assert!(!options.different_only);
    assert!(!options.at_mouse, "the window's centre");
}

#[test]
fn the_dialog_is_at_fars_places() {
    let dialog = Options::default().dialog();
    assert_eq!(dialog.title, "EMenu");
    assert_eq!(dialog.items.len(), 16);
    let place = |item: usize| dialog.items[item].place;
    assert_eq!(place(SHOW_MESSAGE), (5, 2, 38, 2));
    assert_eq!(place(SHOW_GUI), (5, 3, 26, 3));
    assert!(dialog.items[SHOW_GUI].three_state);
    assert_eq!(dialog.items[SHOW_GUI].state, 2, "[?]");
    assert_eq!(place(CLEAR_ITEM), (5, 5, 45, 5));
}

#[test]
fn the_boxes_are_titled_lines_with_their_radio_buttons() {
    let dialog = Options::default().dialog();
    let place = |item: usize| dialog.items[item].place;
    assert_eq!(dialog.items[5].kind, ItemKind::Separator);
    assert_eq!(dialog.items[5].text, "Additional information in text menu");
    assert_eq!(place(NOTHING_ITEM), (7, 8, 16, 8));
    assert!(dialog.items[NOTHING_ITEM].group && dialog.items[MOUSE_ITEM].group);
    assert!(dialog.items[DIFFERENT_ITEM].disabled, "nothing added: off");
    assert_eq!(place(12), (7, 15, 39, 15));
    assert_eq!(dialog.items[12].state, 1, "the centre");
    assert_eq!(place(SAVE), (16, 18, 23, 18));
    assert!(dialog.items[SAVE].default);
}

#[test]
fn silence_turns_the_message_after_a_command_off() {
    let silent = Options {
        silent: true,
        ..Options::default()
    };
    assert!(silent.dialog().items[SHOW_MESSAGE].disabled);
}

fn closed(button: usize, states: &[i32]) -> Closed {
    Closed {
        button: Some(button),
        values: (states.iter())
            .map(|&state| Value {
                state,
                text: String::new(),
            })
            .collect(),
    }
}

#[test]
fn saved_values_are_read_back() {
    let all = closed(SAVE, &[1, 1, 1, 0, 0, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0, 0]);
    assert_eq!(
        Options::from_dialog(&all),
        Some(Options {
            wait: true,
            gui: Gui::Gui,
            delete_with_f8: true,
            clear_selection: false,
            silent: false,
            extra: Extra::Verb,
            different_only: true,
            at_mouse: true,
        })
    );
    let states = [0, 2, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0];
    let ask = Options::from_dialog(&closed(SAVE, &states)).unwrap();
    assert_eq!(
        (ask.gui, ask.extra, ask.at_mouse),
        (Gui::Ask, Extra::Help, false)
    );
    assert_eq!(Options::from_dialog(&closed(SAVE + 1, &states)), None);
}
