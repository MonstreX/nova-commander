//! What the items of the menu bar do (`Options::ShellOptions`).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::fileops::tests::app_in;
use super::menu::MENUS;
use super::{App, Command, Dialog, MenuItem, Overlay, Side, ViewMode};

fn app_with_files() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.txt"] {
        std::fs::write(temp.path().join(name), "x").unwrap();
    }
    let app = app_in(temp.path());
    (temp, app)
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn selected_count(app: &App) -> usize {
    app.panels[0].selected.len()
}

/// Where a command stands in the menu bar.
fn place_of(command: Command) -> (usize, usize) {
    MENUS
        .iter()
        .enumerate()
        .find_map(|(menu, bar)| {
            let item = bar.items.iter().position(
                |item| matches!(item, MenuItem::Item { command: found, .. } if *found == command),
            )?;
            Some((menu, item))
        })
        .unwrap()
}

#[test]
fn the_group_items_are_the_gray_keys() {
    let (_temp, mut app) = app_with_files();
    app.execute(Command::SelectGroup);
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: crate::app::dialog::FormAction::SelectGroup { .. },
            ..
        }))
    ));
    app.overlay = None;
    app.execute(Command::UnselectGroup);
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: crate::app::dialog::FormAction::SelectGroup { .. },
            ..
        }))
    ));
}

#[test]
fn invert_and_restore_selection_work_on_the_active_panel() {
    let (_temp, mut app) = app_with_files();
    app.execute(Command::InvertSelection);
    assert_eq!(selected_count(&app), 2);
    app.panels[0].clear_selection();
    assert_eq!(selected_count(&app), 0);
    app.execute(Command::RestoreSelection);
    assert_eq!(selected_count(&app), 2);
}

#[test]
fn the_selection_items_leave_an_active_info_panel_alone() {
    let (_temp, mut app) = app_with_files();
    app.execute(Command::InfoPanel(Side::Left));
    assert!(app.info_is_active());
    app.execute(Command::InvertSelection);
    assert_eq!(selected_count(&app), 0);
    app.execute(Command::SelectGroup);
    assert!(app.overlay.is_none());
}

#[test]
fn the_info_panel_item_shows_it_on_that_side_and_moves_it() {
    let (_temp, mut app) = app_with_files();
    app.panels[1].visible = false;
    app.execute(Command::InfoPanel(Side::Right));
    assert_eq!(app.info_side(), Some(Side::Right));
    assert!(app.panels[1].visible, "a hidden side comes back");
    app.execute(Command::InfoPanel(Side::Right));
    assert_eq!(app.info_side(), Some(Side::Right), "it stays");
    app.execute(Command::InfoPanel(Side::Left));
    assert_eq!(app.info_side(), Some(Side::Left));
}

#[test]
fn a_view_mode_from_the_menu_makes_that_side_files_again() {
    let (_temp, mut app) = app_with_files();
    app.execute(Command::InfoPanel(Side::Right));
    app.execute(Command::SetViewMode(Side::Right, ViewMode::Brief));
    assert_eq!(app.info_side(), None);
    assert_eq!(app.panels[1].view_mode, ViewMode::Brief);
}

#[test]
fn shift_f10_opens_the_menu_bar_at_the_item_chosen_last() {
    let (_temp, mut app) = app_with_files();
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    assert!(
        matches!(app.overlay, Some(Overlay::Menu(state)) if state.menu == 0 && state.item == 0),
        "nothing chosen yet: as F9"
    );
    app.overlay = None;
    press(&mut app, KeyCode::F(9), KeyModifiers::NONE);
    // The Commands menu: its "Panels On/Off" by its hotkey.
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('p'), KeyModifiers::NONE);
    assert!(app.panels_hidden());
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    let (menu, item) = place_of(Command::TogglePanels);
    assert!(
        matches!(app.overlay, Some(Overlay::Menu(state)) if state.menu == menu && state.item == item)
    );
}

#[test]
fn f9_opens_the_right_menu_only_when_the_right_panel_is_active_and_shown() {
    let (_temp, mut app) = app_with_files();
    app.active = Side::Right;
    press(&mut app, KeyCode::F(9), KeyModifiers::NONE);
    assert!(matches!(app.overlay, Some(Overlay::Menu(state)) if state.menu == MENUS.len() - 1));
    app.overlay = None;
    app.panels[1].visible = false;
    press(&mut app, KeyCode::F(9), KeyModifiers::NONE);
    assert!(matches!(app.overlay, Some(Overlay::Menu(state)) if state.menu == 0));
}

/// Runs the item of `command` from the menu bar, as Enter on it.
pub(super) fn run_from_menu(app: &mut App, command: Command) {
    let (menu, item) = place_of(command);
    app.overlay = Some(Overlay::Menu(super::MenuState {
        menu,
        item,
        no_tree: false,
        provider_modes: [0; 2],
    }));
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

#[test]
fn file_items_do_nothing_for_files_behind_an_info_panel() {
    let (_temp, mut app) = app_with_files();
    app.panels[0].cursor = 1;
    app.execute(Command::InfoPanel(Side::Left));
    for command in [
        Command::Delete,
        Command::Copy,
        Command::Move,
        Command::Link,
        Command::Attributes,
        Command::MakeDir,
    ] {
        run_from_menu(&mut app, command);
        assert!(app.overlay.is_none(), "{command:?}");
    }
}

#[test]
fn file_items_do_nothing_with_the_active_panel_hidden_but_a_folder_can_be_made() {
    let (_temp, mut app) = app_with_files();
    app.panels[0].cursor = 1;
    app.panels[0].visible = false;
    for command in [Command::Delete, Command::Copy, Command::Attributes] {
        run_from_menu(&mut app, command);
        assert!(app.overlay.is_none(), "{command:?}");
    }
    run_from_menu(&mut app, Command::MakeDir);
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: super::dialog::FormAction::MakeDir(_),
            ..
        }))
    ));
}

#[test]
fn sort_modes_on_the_info_panel_is_its_own_menu() {
    let (_temp, mut app) = app_with_files();
    app.execute(Command::InfoPanel(Side::Left));
    app.execute(Command::SortMenu(Side::Left));
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Popup(popup)) if popup.title != "Sort by"
    ));
}

#[test]
fn sort_modes_of_a_hidden_side_opens_nothing() {
    let (_temp, mut app) = app_with_files();
    app.panels[1].visible = false;
    app.execute(Command::SortMenu(Side::Right));
    assert!(app.overlay.is_none());
}

#[test]
fn restore_selection_reaches_a_hidden_panel_as_in_far() {
    let (_temp, mut app) = app_with_files();
    app.execute(Command::InvertSelection);
    app.panels[0].clear_selection();
    app.panels[0].visible = false;
    app.execute(Command::RestoreSelection);
    assert_eq!(selected_count(&app), 2);
}

#[test]
fn a_view_mode_shows_a_hidden_side_that_held_the_info_panel() {
    let (_temp, mut app) = app_with_files();
    app.execute(Command::InfoPanel(Side::Right));
    app.panels[1].visible = false;
    app.execute(Command::SetViewMode(Side::Right, ViewMode::Brief));
    assert!(app.panels[1].visible);
}

#[test]
fn shift_f10_goes_to_the_item_chosen_with_enter() {
    let (_temp, mut app) = app_with_files();
    run_from_menu(&mut app, Command::SwapPanels);
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    let (menu, item) = place_of(Command::SwapPanels);
    assert!(
        matches!(app.overlay, Some(Overlay::Menu(state)) if state.menu == menu && state.item == item)
    );
}

#[test]
fn a_panels_menu_lists_its_providers_modes_after_detailed() {
    let (_temp, mut app) = app_with_files();
    let mut modes = vec![None; 10];
    modes[6] = Some(crate::vfs::PanelMode::default());
    modes[8] = Some(crate::vfs::PanelMode::default());
    app.panels[0].look.modes = modes.into();
    let state = app.fresh_menu(super::MenuState::for_side(Side::Left));
    assert_eq!(state.provider_modes, [0b101, 0]);
    let labels: Vec<&str> = (state.items().iter())
        .filter_map(|item| match item {
            MenuItem::Item { label, .. } => Some(*label),
            MenuItem::Separator => None,
        })
        .take(8)
        .collect();
    assert_eq!(
        labels,
        [
            "&Brief",
            "&Medium",
            "&Full",
            "&Wide",
            "Detai&led",
            "&Descriptions",
            "File own&ers",
            "&Alternative full"
        ]
    );
    let right = app.fresh_menu(super::MenuState::for_side(Side::Right));
    assert_eq!(
        right.items().len(),
        MENUS[right.menu].items.len(),
        "none on the right"
    );
    assert!(
        state
            .items()
            .iter()
            .any(|item| item.command()
                == Some(Command::SetViewMode(Side::Left, ViewMode::Provider(8)))),
        "Ctrl+8's item shows the mode"
    );
}

#[test]
fn shift_f10_finds_the_item_run_last_again_when_the_items_changed() {
    let (_temp, mut app) = app_with_files();
    let mut modes = vec![None; 10];
    modes[6] = Some(crate::vfs::PanelMode::default());
    modes[7] = Some(crate::vfs::PanelMode::default());
    app.panels[0].look.modes = modes.into();
    // "Change drive", the left menu's last item, run with the plugin's two modes listed.
    let mut state = app.fresh_menu(super::MenuState::for_side(Side::Left));
    state.item = state.items().len() - 1;
    assert_eq!(
        state.items()[state.item].command(),
        Some(Command::ChangeDrive(Side::Left))
    );
    app.remembered.last_menu_item = Some(state);
    app.panels[0].look.modes = Vec::new().into();
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    let Some(Overlay::Menu(shown)) = &app.overlay else {
        panic!("the menu");
    };
    assert_eq!(
        shown.items()[shown.item].command(),
        Some(Command::ChangeDrive(Side::Left)),
        "the same item, not an index past the end"
    );
}

/// R7 review D9: Shift+F9 asks FAR's question first; Cancel saves nothing, Save does.
#[test]
fn save_setup_asks_first() {
    let (_temp, mut app) = app_with_files();
    press(&mut app, KeyCode::F(9), KeyModifiers::SHIFT);
    let Some(Overlay::Dialog(Dialog::Confirm { title, lines, .. })) = &app.overlay else {
        panic!("the question");
    };
    assert_eq!(title, "Save setup");
    assert_eq!(lines, &["Do you wish to save", "current setup?"]);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(!app.take_save_request(), "cancelled");
    press(&mut app, KeyCode::F(9), KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    assert!(app.take_save_request(), "saved");
}
