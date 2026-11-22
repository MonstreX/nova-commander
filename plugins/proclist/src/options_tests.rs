use super::*;
use nc_plugin_sdk::{ItemKind, Value};

#[test]
fn the_defaults_are_fars() {
    let options = Options::default();
    assert!(options.add_to_disks && options.add_to_plugins);
    assert!(options.export_environment && options.export_module_info);
    assert!(options.export_performance);
    assert!(!options.export_module_version && !options.export_handles);
    assert!(!options.export_handles_unnamed);
}

#[test]
fn the_dialog_is_at_fars_places() {
    let dialog = Options::default().dialog();
    assert_eq!(dialog.title, "Process list - config");
    assert_eq!(dialog.items.len(), 13);
    let place = |item: usize| dialog.items[item].place;
    assert_eq!(place(0), (5, 2, 27, 2));
    assert_eq!(dialog.items[2].kind, ItemKind::Separator);
    assert_eq!(
        dialog.items[3].text,
        "Include in the additional information:"
    );
    assert_eq!(place(3), (5, 5, 42, 5));
    assert_eq!(place(6), (5, 8, 42, 8));
    assert_eq!(place(9), (9, 11, 36, 11), "the unnamed handles indented");
    assert_eq!(place(OK), (15, 13, 20, 13));
    assert!(dialog.items[OK].default);
    assert_eq!(dialog.items[CHECKS[4]].state, 0, "module versions off");
    assert_eq!(dialog.items[CHECKS[5]].state, 1, "performance on");
}

#[test]
fn ok_gives_the_boxes_cancel_nothing() {
    let mut values = vec![Value::default(); 13];
    for item in [1, 6, 9] {
        values[item].state = 1;
    }
    let closed = |button| Closed {
        button,
        values: values.clone(),
    };
    let options = Options::from_dialog(&closed(Some(OK))).unwrap();
    assert_eq!(
        options,
        Options {
            add_to_disks: false,
            add_to_plugins: true,
            export_environment: false,
            export_module_info: false,
            export_module_version: true,
            export_performance: false,
            export_handles: false,
            export_handles_unnamed: true,
        }
    );
    assert_eq!(Options::from_dialog(&closed(Some(OK + 1))), None);
    assert_eq!(Options::from_dialog(&closed(None)), None);
}

#[test]
fn the_view_dialog_is_the_configurations_parts() {
    let dialog = Options::default().dialog();
    let view = Options::default().view_dialog();
    assert_eq!(view.title, "View with Options...");
    assert_eq!(view.items.len(), 10);
    assert_eq!(view.items[0].text, dialog.items[3].text);
    assert_eq!(view.items[0].place, (5, 2, 42, 2));
    assert_eq!(
        view.items[6].place,
        (9, 8, 36, 8),
        "the unnamed handles indented"
    );
    assert_eq!(view.items[VIEW_OK].place, (15, 10, 20, 10));
    assert!(view.items[VIEW_OK].default);
}

#[test]
fn the_view_dialog_gives_the_texts_parts_only() {
    let mut values = vec![Value::default(); 10];
    for item in [3, 5] {
        values[item].state = 1;
    }
    let closed = Closed {
        button: Some(VIEW_OK),
        values,
    };
    let base = Options {
        add_to_disks: false,
        ..Options::default()
    };
    let chosen = base.from_view_dialog(&closed).unwrap();
    assert!(
        !chosen.add_to_disks && chosen.add_to_plugins,
        "the menus' as they were"
    );
    assert!(chosen.export_module_version && chosen.export_handles);
    assert!(!chosen.export_environment && !chosen.export_performance);
    let cancelled = Closed {
        button: None,
        values: Vec::new(),
    };
    assert_eq!(base.from_view_dialog(&cancelled), None);
}
