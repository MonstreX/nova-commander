//! "Use filter" and Filter of Find file: the filters menu of Find file's marks over the dialog, and
//! the search through them (FAR's `FFT_FINDFILE`); every search stays in the test's own folder.

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::dialog::{FILTER, USE_FILTER};
use super::tests::{app, form, found_names, press, results, until};
use super::*;
use crate::app::filters::Area as FilterArea;
use crate::app::filters_menu::FilterItem;

fn use_filter(app: &mut App) {
    if let Some(Control::Check { checked, .. }) = form(app).control_mut(USE_FILTER) {
        *checked = true;
    }
}

#[test]
fn use_filter_enables_filter_which_opens_find_files_filters_menu() {
    let (_temp, mut app) = app();
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    assert!(!form(&mut app).is_checked(USE_FILTER));
    assert!(form(&mut app).items[FILTER].disabled, "FAR's automation");
    press(&mut app, KeyCode::Char('u'), KeyModifiers::ALT);
    assert!(form(&mut app).is_checked(USE_FILTER));
    assert!(!form(&mut app).items[FILTER].disabled);
    press(&mut app, KeyCode::Char('e'), KeyModifiers::ALT);
    let Some(Overlay::Filters(menu)) = &mut app.overlay else {
        panic!("no filters menu");
    };
    assert_eq!(menu.area, FilterArea::FindFile);
    let at = (menu.lines.iter())
        .position(|line| line.item == FilterItem::Type("*.txt".to_string()))
        .expect("the panel's types");
    menu.selected = at;
    press(&mut app, KeyCode::Char('-'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.tick();
    assert!(
        form(&mut app).is_checked(USE_FILTER),
        "the dialog back as it was"
    );
    let kind = &app.filters.types[0];
    assert_eq!(
        (kind.mask.as_str(), kind.marks.find_file.as_str()),
        ("*.txt", "-")
    );
    assert!(kind.marks.left_panel.is_empty(), "the panel's marks stay");
}

#[test]
fn the_search_goes_through_find_files_filter_and_says_so() {
    let (_temp, mut app) = app();
    let marks = crate::settings::FilterMarks {
        find_file: "-".to_string(),
        ..crate::settings::FilterMarks::default()
    };
    app.filters.types.push(crate::settings::TypeFilter {
        mask: "*.txt".to_string(),
        marks,
    });
    press(&mut app, KeyCode::F(7), KeyModifiers::ALT);
    use_filter(&mut app);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    until(&mut app, |app| results(app).finalized);
    assert_eq!(found_names(&app), ["sub", "c.rs"]);
    assert_eq!(results(&app).form.title, "Find file: * (using filter)");
    assert!(app.to_settings().find.use_filter, "saved");
    // The panel is not filtered by Find file's marks.
    assert_eq!(app.panels[0].entries.len(), 3);
}

#[test]
fn a_panel_that_shows_no_files_gives_no_types_to_finds_menu() {
    let (_temp, mut app) = app();
    let side = app.active;
    app.panels[side.index()].visible = false;
    app.open_area_filters_menu(FilterArea::FindFile);
    let Some(Overlay::Filters(menu)) = &app.overlay else {
        panic!("no filters menu");
    };
    assert!(
        !(menu.lines.iter()).any(|line| matches!(line.item, FilterItem::Type(_))),
        "{:?}",
        menu.lines
    );
}
