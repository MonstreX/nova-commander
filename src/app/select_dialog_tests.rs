use super::*;
use crate::app::Overlay;
use crate::app::filters::set_mark;
use crate::vfs::filter::Mark;

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    for name in ["a.rs", "b.rs", "c.md"] {
        std::fs::write(temp.path().join(name), "x").unwrap();
    }
    std::fs::create_dir(temp.path().join("src")).unwrap();
    let app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    (temp, app)
}

/// The open Select dialog's title and form.
fn shown(app: &App) -> (String, Form) {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("the dialog is open");
    };
    (form.title.clone(), form.clone())
}

/// The open dialog closed by `button` with `mask` typed.
fn close(app: &mut App, mask: &str, button: Option<usize>, (side, on): (Side, bool)) {
    let (_, mut form) = shown(app);
    if let Control::Edit(input) = &mut form.items[MASK].control {
        input.set(mask);
    }
    app.overlay = None;
    app.finish_select_form(&form, button, (side, on));
}

fn selected(app: &App) -> Vec<String> {
    app.active_panel().selected.iter().cloned().collect()
}

#[test]
fn select_dialog_remembers_the_mask_and_has_ok_filter_cancel() {
    let (_temp, mut app) = app();
    app.ask_select_group(Side::Left, true);
    let (title, form) = shown(&app);
    assert_eq!(title, "Select");
    assert_eq!(form.edit_text(MASK), "*.*");
    let labels: Vec<&str> = (form.items.iter())
        .filter_map(|item| match &item.control {
            Control::Button { label, .. } => Some(label.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(labels, ["Ok", "&Filter", "Cancel"]);
    close(&mut app, "*.rs", Some(OK), (Side::Left, true));
    assert_eq!(selected(&app), ["a.rs", "b.rs"]);
    app.ask_select_group(Side::Left, false);
    close(&mut app, "a*", Some(OK), (Side::Left, false));
    assert_eq!(selected(&app), ["b.rs"]);
    app.ask_select_group(Side::Left, false);
    let (title, form) = shown(&app);
    assert_eq!(title, "Deselect");
    assert_eq!(form.edit_text(MASK), "a*");
    close(&mut app, "*", None, (Side::Left, false));
    assert_eq!(selected(&app), ["b.rs"], "Cancel selects nothing");
}

#[test]
fn a_mask_with_errors_says_so_and_the_dialog_comes_back() {
    let (_temp, mut app) = app();
    app.ask_select_group(Side::Left, true);
    close(&mut app, "/[/", Some(OK), (Side::Left, true));
    let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
        panic!("a warning is shown");
    };
    assert_eq!(lines, &[MASK_ERRORS.to_string()]);
    assert!(selected(&app).is_empty());
    app.overlay = None;
    assert!(app.resume_dialogs_behind());
    let (title, form) = shown(&app);
    assert_eq!(title, "Select");
    assert_eq!(form.edit_text(MASK), "/[/", "as it was typed");
}

#[test]
fn filter_selects_by_the_select_areas_filters_once_their_menu_closes() {
    let (_temp, mut app) = app();
    // The "Folders" filter marked for Select only: the folders are what it takes.
    set_mark(&mut app.filters.folders, Area::Select, Mark::Include);
    app.ask_select_group(Side::Left, true);
    close(&mut app, "*.*", Some(FILTER), (Side::Left, true));
    assert!(
        matches!(app.overlay, Some(Overlay::Filters(_))),
        "the filters menu"
    );
    assert!(selected(&app).is_empty(), "nothing before the menu closes");
    app.handle_key(ratatui::crossterm::event::KeyEvent::from(
        ratatui::crossterm::event::KeyCode::Esc,
    ));
    assert_eq!(
        selected(&app),
        ["src"],
        "the folder, as the filter takes it"
    );
    assert!(app.filters.then_select.is_none());
}
