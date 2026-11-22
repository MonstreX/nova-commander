//! The names in the question before deleting: highlighted, as many as half the screen shows.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::dialog::Dialog;
use super::super::fileops::tests::app_in;
use super::super::form::{Control, Form};
use super::super::{App, Overlay};
use crate::vfs::test_tree::{self as tree, file};

fn app_on_tree(session: &str, names: &[&str], cursor: &str) -> App {
    let root = tree::root(session);
    for name in names {
        file(&root.join(name));
    }
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.active_panel_mut()
        .change_dir(&root, Some(cursor))
        .unwrap();
    app
}

fn form(app: &App) -> &Form {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("expected a form"),
    }
}

fn labels(form: &Form) -> Vec<String> {
    (form.items.iter())
        .filter_map(|item| match &item.control {
            Control::Label(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_names_are_highlighted_and_a_low_screen_lists_half_its_height() {
    let names: Vec<String> = (1..=13).map(|n| format!("{n:02}.txt")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut app = app_on_tree("app-highlight", &refs, "01.txt");
    app.active_panel_mut()
        .selected
        .extend(names.iter().cloned());
    app.resize(100, 13);
    app.handle_key(KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE));
    let shown = labels(form(&app));
    // FAR's `ScrY / 2`: 6 names on 13 rows, then the rest.
    assert_eq!(shown.len(), 1 + 1 + 6 + 1, "{shown:?}");
    assert_eq!(shown[8], "…and 7 more");
    let lit: Vec<bool> = (form(&app).items.iter())
        .filter(|item| matches!(item.control, Control::Label(_)))
        .map(|item| item.highlighted)
        .collect();
    assert_eq!(
        lit,
        [false, false, true, true, true, true, true, true, true]
    );
    // One file: highlighted only when it is not the one under the cursor.
    app.overlay = None;
    app.active_panel_mut().selected.clear();
    app.ask_delete(false, false);
    assert!(!form(&app).items.iter().any(|item| item.highlighted));
    app.overlay = None;
    app.active_panel_mut().selected.insert("05.txt".to_string());
    app.ask_delete(false, false);
    assert!(
        form(&app).items[1].highlighted,
        "05.txt, the cursor on 01.txt"
    );
}
