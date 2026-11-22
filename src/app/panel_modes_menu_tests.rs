//! Options > File panel modes: the list, its keys and the dialog.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;
use crate::app::{Command, Overlay};

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn list(app: &App) -> &EditList {
    let Some(Overlay::EditList(list)) = &app.overlay else {
        panic!("no list: {:?}", app.overlay.is_some());
    };
    list
}

fn form(app: &mut App) -> &mut Form {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    form
}

/// Presses the dialog's button at `at`.
fn press_button(app: &mut App, at: usize) {
    form(app).focus_item(at);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    app.tick();
}

fn open(app: &mut App) {
    app.execute(Command::PanelModes);
}

#[test]
fn the_list_has_fars_modes_in_its_order_and_its_cursor_on_the_panels_mode() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    open(&mut app);
    let shown = list(&app);
    assert_eq!(shown.title, "Edit panel modes");
    assert_eq!(shown.bottom, "Ins, Del, F4, Ctrl+Enter, Ctrl+Shift+Enter");
    assert_eq!(
        shown.lines,
        [
            "&Brief",
            "&Medium",
            "&Full",
            "&Wide",
            "Detai&led",
            "&Alternative full"
        ]
    );
    assert!(shown.separators.is_empty() && shown.hotkeys);
    assert_eq!(shown.selected, 1, "Medium, the panel's");
    app.overlay = None;
    app.panels[0].set_view_mode(ViewMode::AlternativeFull);
    open(&mut app);
    assert_eq!(list(&app).selected, 5);
}

#[test]
fn f4_changes_a_mode_and_reset_brings_fars_back() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    open(&mut app);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    let shown = form(&mut app);
    assert_eq!(shown.title, "Brief");
    assert_eq!(
        (shown.edit_text(TYPES), shown.edit_text(WIDTHS)),
        ("N,N,N", "0,0,0")
    );
    assert_eq!(shown.edit_text(STATUS_TYPES), "NR,S,D,T");
    assert!(shown.is_checked(FLAGS + 1), "Align file extensions");
    assert!(!shown.items[RESET].disabled);
    shown.set_edit_text(NAME, "Two");
    shown.set_edit_text(TYPES, "N,S");
    shown.set_edit_text(WIDTHS, "0,6");
    if let Some(Control::Check { checked, .. }) = shown.control_mut(FLAGS + 3) {
        *checked = true;
    }
    press_button(&mut app, OK);
    let brief = app.panel_modes.kept().brief.clone();
    assert_eq!(
        (brief.name.as_str(), brief.columns.as_str()),
        ("Two", "N,S")
    );
    assert!(brief.folder_upper_case);
    assert_eq!(app.panel_modes.spec(ViewMode::Brief).panel.len(), 2);
    assert_eq!(
        list(&app).lines[0],
        "Two",
        "the list back with the mode's name"
    );
    assert_eq!(list(&app).selected, 0);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(form(&mut app).title, "Two");
    press_button(&mut app, RESET);
    assert_eq!(
        app.panel_modes.kept().brief,
        crate::settings::PanelModes::far_defaults().brief
    );
    assert_eq!(list(&app).lines[0], "&Brief");
}

/// Ins on the line under the cursor and OK with `name` and `types`.
fn insert(app: &mut App, name: &str, types: &str) {
    press(app, KeyCode::Insert, KeyModifiers::NONE);
    let shown = form(app);
    assert_eq!(shown.title, "", "a new mode's box has no title");
    assert_eq!(shown.edit_text(TYPES), "");
    shown.set_edit_text(NAME, name);
    shown.set_edit_text(TYPES, types);
    press_button(app, OK);
}

#[test]
fn ins_adds_the_users_modes_after_a_separator_and_ctrl_enter_shows_one() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    open(&mut app);
    insert(&mut app, "Mine", "N,X");
    let shown = list(&app);
    assert_eq!(shown.lines.len(), 8);
    assert_eq!(shown.separators, [6]);
    assert_eq!(shown.lines[7], "Mine");
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    assert_eq!(list(&app).selected, 7);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(list(&app).selected, 5, "the separator is passed over");
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
    assert_eq!(app.panels[0].view_mode, ViewMode::Custom(0));
    assert_eq!(list(&app).selected, 7, "the list stays");
    press(
        &mut app,
        KeyCode::Enter,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(app.panels[1].view_mode, ViewMode::Custom(0));
    let kinds: Vec<_> = (app.panel_modes.spec(ViewMode::Custom(0)).panel.iter())
        .map(|column| column.kind)
        .collect();
    assert_eq!(
        kinds,
        [
            crate::app::ColumnKind::Name,
            crate::app::ColumnKind::Extension
        ]
    );
}

#[test]
fn a_new_mode_goes_before_the_users_mode_under_the_cursor() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    open(&mut app);
    insert(&mut app, "Mine", "N,X");
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert_eq!(form(&mut app).title, "Mine");
    assert!(
        form(&mut app).items[RESET].disabled,
        "a user's mode has no FAR's"
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.tick();
    // The right panel on a plugin's panel came from Mine.
    came_from_view(&mut app, 1, ViewMode::Custom(0));
    insert(&mut app, "Before", "N");
    let names: Vec<&str> = (app.panel_modes.kept().custom.iter())
        .map(|mode| mode.name.as_str())
        .collect();
    assert_eq!(names, ["Before", "Mine"], "before the one under the cursor");
    assert_eq!(
        app.panels[0].view_mode,
        ViewMode::Custom(1),
        "a panel keeps showing its mode"
    );
    let saved = app.to_settings();
    assert_eq!(
        app.panels[1].return_view(),
        Some(ViewMode::Custom(1)),
        "the mode to go back to moves with it"
    );
    assert_eq!(saved.left_view, "custom-2");
    assert_eq!(saved.panel_modes.unwrap().custom.len(), 2);
}

#[test]
fn del_takes_a_users_mode_away_at_once_and_its_panel_shows_the_mode_before() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    open(&mut app);
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(list(&app).lines.len(), 6, "FAR's own modes stay");
    insert(&mut app, "One", "N");
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    insert(&mut app, "Zero", "N");
    // The users' modes: Zero, One; the panels show each.
    app.panels[0].set_view_mode(ViewMode::Custom(0));
    app.panels[1].set_view_mode(ViewMode::Custom(1));
    came_from_view(&mut app, 0, ViewMode::Custom(1));
    came_from_view(&mut app, 1, ViewMode::Custom(0));
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(list(&app).lines[list(&app).selected], "Zero");
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert!(
        matches!(app.overlay, Some(Overlay::EditList(_))),
        "no question"
    );
    assert_eq!(app.panel_modes.custom_count(), 1);
    assert_eq!(app.panels[0].view_mode, ViewMode::AlternativeFull);
    assert_eq!(app.panels[1].view_mode, ViewMode::Custom(0), "One, still");
    assert_eq!(
        [app.panels[0].return_view(), app.panels[1].return_view()],
        [Some(ViewMode::Custom(0)), Some(ViewMode::AlternativeFull)],
        "the modes to go back to as the shown ones"
    );
    assert_eq!(list(&app).selected, 5, "past the separator");
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(
        list(&app).lines.len(),
        6,
        "no separator without a user's mode"
    );
    assert_eq!(app.panels[1].view_mode, ViewMode::AlternativeFull);
    assert_eq!(app.panels[0].return_view(), Some(ViewMode::AlternativeFull));
}

#[test]
fn a_hotkey_opens_its_modes_dialog() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    open(&mut app);
    press(&mut app, KeyCode::Char('w'), KeyModifiers::NONE);
    assert_eq!(form(&mut app).title, "Wide");
    assert_eq!(form(&mut app).edit_text(TYPES), "N,S");
}

#[test]
fn the_users_modes_are_named_custom_in_the_settings() {
    assert_eq!(ViewMode::Custom(0).id(), "custom-1");
    assert_eq!(ViewMode::from_id("custom-3"), ViewMode::Custom(2));
    assert_eq!(ViewMode::from_id("custom-0"), ViewMode::Medium);
    let modes = crate::app::PanelModes::default();
    assert_eq!(
        modes.spec(ViewMode::Custom(4)),
        modes.spec(ViewMode::Medium),
        "a mode that is not there shows Medium"
    );
}

#[test]
fn reset_in_a_new_modes_dialog_adds_fars_mode_under_the_cursor() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    open(&mut app);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Insert, KeyModifiers::NONE);
    assert!(!form(&mut app).items[RESET].disabled);
    press_button(&mut app, RESET);
    assert_eq!(
        app.panel_modes.kept().custom,
        [crate::settings::PanelModes::far_defaults().full],
        "Full was under the cursor"
    );
}

#[test]
fn ctrl_enter_on_an_info_panel_keeps_the_mode_for_its_files() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    app.execute(Command::InfoPanel(app.active.other()));
    assert_eq!(app.info_side(), Some(app.active.other()));
    open(&mut app);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    press(
        &mut app,
        KeyCode::Enter,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(
        app.info_side(),
        Some(app.active.other()),
        "still the info panel"
    );
    assert_eq!(
        app.panels[app.active.other().index()].view_mode,
        ViewMode::Brief
    );
}

#[test]
fn a_title_keeps_a_doubled_ampersand() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let mut mode = crate::settings::PanelModes::far_defaults().wide;
    mode.name = "Me && &you".to_string();
    app.panel_modes.set(ViewMode::Wide, mode);
    app.edit_panel_mode(Some(ViewMode::Wide), ViewMode::Wide);
    assert_eq!(form(&mut app).title, "Me & you");
}

#[test]
fn a_saved_users_mode_that_is_not_there_is_medium_and_the_options_menu_has_the_item() {
    let temp = tempfile::tempdir().unwrap();
    let settings = crate::settings::Settings {
        left_view: "custom-3".to_string(),
        right_view: "custom-1".to_string(),
        panel_modes: Some(crate::settings::PanelModes {
            custom: vec![crate::settings::PanelMode::default()],
            ..crate::settings::PanelModes::far_defaults()
        }),
        ..crate::settings::Settings::default()
    };
    let app = App::new(temp.path(), 100, 40, &settings);
    assert_eq!(app.panels[0].view_mode, ViewMode::Medium);
    assert_eq!(app.panels[1].view_mode, ViewMode::Custom(0));
    let options = (crate::app::MENUS.iter())
        .find(|menu| menu.title.contains("Options"))
        .unwrap();
    let items: Vec<(&str, Command)> = (options.items.iter())
        .filter_map(|item| match item {
            crate::app::MenuItem::Item { label, command, .. } => Some((*label, *command)),
            crate::app::MenuItem::Separator => None,
        })
        .collect();
    let at = (items.iter())
        .position(|(label, _)| *label == "File panel &modes")
        .unwrap();
    assert_eq!(items[at - 1].0, "Co&nfirmations");
    assert_eq!(items[at].1, Command::PanelModes);
}

/// The panel `side` as if on a plugin's panel entered with the view `view` (what comes back as it leaves).
fn came_from_view(app: &mut App, side: usize, view: ViewMode) {
    let panel = &mut app.panels[side];
    let entered = crate::app::panel::Entered {
        from: panel.path.clone(),
        view,
        sort: panel.sort,
        host: None,
    };
    panel.plugin_stack_mut().push(entered);
}
