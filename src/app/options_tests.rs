//! The dialogs of Options and what their switches do.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::fileops::tests::app_in;
use crate::app::form::{Control, Form};
use crate::app::options::{MenuClick, OptionsPage};
use crate::app::options_pages::page;
use crate::app::{App, Command, Dialog, Overlay};

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "x").unwrap();
    let app = app_in(temp.path());
    (temp, app)
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn the_form(app: &App) -> &Form {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("no dialog"),
    }
}

/// The index of the check box labelled `label`.
fn check_box(app: &App, label: &str) -> usize {
    the_form(app)
        .items
        .iter()
        .position(
            |item| matches!(&item.control, Control::Check { label: found, .. } if found == label),
        )
        .unwrap()
}

/// Space on item `index`.
fn toggle(app: &mut App, index: usize) {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    form.focus_item(index);
    press(app, KeyCode::Char(' '));
}

/// The dialog's button called `label`, pressed.
fn press_button(app: &mut App, label: &str) {
    let index = the_form(app)
        .items
        .iter()
        .position(
            |item| matches!(&item.control, Control::Button { label: found, .. } if found == label),
        )
        .unwrap();
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    form.focus_item(index);
    press(app, KeyCode::Enter);
}

/// Opens `options`, flips the check box `label`, and leaves with `button`.
fn flip(app: &mut App, options: OptionsPage, label: &str, button: &str) {
    app.execute(Command::Options(options));
    let index = check_box(app, label);
    toggle(app, index);
    press_button(app, button);
    assert!(app.overlay.is_none());
}

#[test]
fn every_page_opens_with_its_title_and_ok_and_cancel_last() {
    let (_temp, mut app) = app();
    for options in [
        OptionsPage::System,
        OptionsPage::Panel,
        OptionsPage::Tree,
        OptionsPage::Interface,
        OptionsPage::Dialog,
        OptionsPage::Menu,
        OptionsPage::CommandLine,
        OptionsPage::AutoComplete,
        OptionsPage::InfoPanel,
        OptionsPage::Confirmations,
    ] {
        app.execute(Command::Options(options));
        let form = the_form(&app);
        assert_eq!(form.title, page(options).0);
        let last = &form.items[form.items.len() - 2..];
        assert!(matches!(&last[0].control, Control::Button { label, .. } if label == "OK"));
        assert!(matches!(&last[1].control, Control::Button { label, .. } if label == "Cancel"));
        app.overlay = None;
    }
}

#[test]
fn ok_takes_a_box_and_cancel_leaves_it() {
    let (_temp, mut app) = app();
    assert!(app.config.delete_settings.to_recycle_bin);
    flip(
        &mut app,
        OptionsPage::System,
        "&Delete to Recycle Bin",
        "Cancel",
    );
    assert!(app.config.delete_settings.to_recycle_bin);
    flip(
        &mut app,
        OptionsPage::System,
        "&Delete to Recycle Bin",
        "OK",
    );
    assert!(!app.config.delete_settings.to_recycle_bin);
}

#[test]
fn a_dialog_shows_the_values_the_program_has() {
    let (_temp, mut app) = app();
    app.prefs.confirm_exit = false;
    app.execute(Command::Options(OptionsPage::Confirmations));
    let index = check_box(&app, "E&xit");
    assert!(!the_form(&app).is_checked(index));
}

#[test]
fn hidden_files_go_for_both_panels() {
    let (_temp, mut app) = app();
    flip(
        &mut app,
        OptionsPage::Panel,
        "Show &hidden and system files",
        "OK",
    );
    assert!(!app.show_hidden);
}

#[test]
fn select_folders_and_reverse_sort_reach_both_panels() {
    let (_temp, mut app) = app();
    flip(&mut app, OptionsPage::Panel, "Select &folders", "OK");
    flip(
        &mut app,
        OptionsPage::Panel,
        "Allow re&verse sort modes",
        "OK",
    );
    for panel in &app.panels {
        assert!(panel.options.select_folders);
        assert!(!panel.options.allow_reverse);
    }
}

#[test]
fn without_reverse_sort_a_repeated_mode_keeps_its_direction() {
    let (_temp, mut app) = app();
    let mode = app.panels[0].sort.mode;
    app.panels[0].options.allow_reverse = false;
    let before = app.panels[0].sort.reverse;
    app.panels[0].set_sort(mode);
    assert_eq!(app.panels[0].sort.reverse, before);
    app.panels[0].options.allow_reverse = true;
    app.panels[0].set_sort(mode);
    assert_ne!(app.panels[0].sort.reverse, before);
}

#[test]
fn the_update_limit_is_the_number_when_its_box_is_on_and_none_when_off() {
    let (_temp, mut app) = app();
    app.execute(Command::Options(OptionsPage::Panel));
    let index = check_box(&app, "&Disable automatic panel update");
    toggle(&mut app, index);
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    form.set_edit_text(index + 2, "500");
    press_button(&mut app, "OK");
    assert_eq!(app.config.auto_update_limit, 500);
    flip(
        &mut app,
        OptionsPage::Panel,
        "&Disable automatic panel update",
        "OK",
    );
    assert_eq!(app.config.auto_update_limit, 0);
}

#[test]
fn a_menu_click_is_chosen_from_its_list() {
    let (_temp, mut app) = app();
    app.execute(Command::Options(OptionsPage::Menu));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    // The first list is the left button's.
    form.focus_item(1);
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    press_button(&mut app, "OK");
    assert_eq!(app.prefs.menu_left, MenuClick::Apply, "{:?}", app.prefs);
}

#[test]
fn the_info_panels_names_and_cd_parameters_are_chosen_and_saved() {
    use crate::platform::system::{ComputerNameFormat, UserNameFormat};
    let (_temp, mut app) = app();
    assert_eq!(
        app.config.info_options.names.computer,
        ComputerNameFormat::PhysicalNetBios,
        "FAR's default"
    );
    flip(
        &mut app,
        OptionsPage::InfoPanel,
        "Show &CD drive parameters",
        "OK",
    );
    assert!(!app.config.info_options.show_cd_info);
    app.execute(Command::Options(OptionsPage::InfoPanel));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    let lists: Vec<usize> = (form.items.iter().enumerate())
        .filter(|(_, item)| matches!(item.control, Control::DropDown { .. }))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(lists.len(), 2, "the computer's and the user's names");
    form.focus_item(lists[1]);
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    press_button(&mut app, "OK");
    assert_eq!(
        app.config.info_options.names.user,
        UserNameFormat::SamCompatible
    );
    let saved = app.to_settings();
    assert_eq!(
        (
            saved.info_panel.show_cd_info,
            saved.info_panel.user_name_format.as_str()
        ),
        (false, "samCompatible")
    );
    assert_eq!(
        crate::app::persist::info_options(&saved),
        app.config.info_options
    );
}

#[test]
fn the_switches_are_saved() {
    let (_temp, mut app) = app();
    app.prefs.clock = false;
    app.prefs.menu_right = MenuClick::Ignore;
    app.panels[0].options.highlight = false;
    let settings = app.to_settings();
    assert!(!settings.clock);
    assert_eq!(settings.menu_right_click, "ignore");
    assert!(!settings.highlight);
    let back = crate::app::options::Preferences::from_settings(&settings);
    assert_eq!(back, app.prefs);
}

#[test]
fn with_no_question_on_exit_f10_quits_at_once() {
    let (_temp, mut app) = app();
    app.prefs.confirm_exit = false;
    press(&mut app, KeyCode::F(10));
    assert!(app.should_quit());
}

#[test]
fn without_the_list_typing_opens_no_completion() {
    let (_temp, mut app) = app();
    app.prefs.completion_list = false;
    press(&mut app, KeyCode::Char('a'));
    assert!(app.completion.is_none());
    app.prefs.completion_list = true;
    press(&mut app, KeyCode::Char('b'));
    assert!(app.completion.is_some());
}

#[test]
fn a_click_outside_a_dialog_can_be_turned_off() {
    use crate::app::console_input::{Mouse, MouseAction};
    use crate::app::mouse_dialogs::DialogArea;
    use crate::app::mouse_menus::Cells;
    use crate::platform::input::MouseButtons;
    let (_temp, mut app) = app();
    app.prefs.click_outside_dialog = false;
    app.execute(Command::Quit);
    app.mouse.dialog = Some(DialogArea {
        frame: Cells {
            x: 20,
            y: 8,
            width: 40,
            height: 8,
        },
        controls: Vec::new(),
    });
    let left = MouseButtons {
        left: true,
        right: false,
        middle: false,
    };
    let outside = Mouse {
        column: 5,
        row: 2,
        buttons: left,
        pressed: left,
        action: MouseAction::Button,
        modifiers: KeyModifiers::NONE,
    };
    assert!(!app.handle_mouse(outside));
    assert!(app.overlay.is_some());
}

#[test]
fn select_folders_reaches_the_gray_star_and_the_menus_invert() {
    let (temp, mut app) = app();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    app.panels[0].reload();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !app.panels[0]
        .entries
        .iter()
        .any(|entry| entry.name == "sub")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the folder never came"
        );
        app.tick();
    }
    let star = KeyEvent {
        state: ratatui::crossterm::event::KeyEventState::KEYPAD,
        ..KeyEvent::new(KeyCode::Char('*'), KeyModifiers::NONE)
    };
    app.handle_key(star);
    assert!(!app.panels[0].selected.contains("sub"), "folders stay out");
    app.panels[0].clear_selection();
    app.panels[0].options.select_folders = true;
    app.handle_key(star);
    assert!(app.panels[0].selected.contains("sub"));
    app.panels[0].clear_selection();
    app.execute(Command::InvertSelection);
    assert!(app.panels[0].selected.contains("sub"));
    app.panels[0].clear_selection();
    app.handle_key(KeyEvent {
        modifiers: KeyModifiers::ALT,
        ..star
    });
    assert!(!app.panels[0].selected.contains("sub"), "Alt: files only");
}

#[test]
fn what_is_no_number_leaves_the_update_limit_as_it_was() {
    let (_temp, mut app) = app();
    app.config.auto_update_limit = 7;
    app.execute(Command::Options(OptionsPage::Panel));
    let index = check_box(&app, "&Disable automatic panel update");
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    form.set_edit_text(index + 2, "5x");
    press_button(&mut app, "OK");
    assert_eq!(app.config.auto_update_limit, 7);
}

#[test]
fn every_new_key_goes_through_the_settings_file_and_back() {
    let (_temp, mut app) = app();
    app.prefs.clock = false;
    app.prefs.save_history = false;
    app.prefs.save_folders_history = false;
    app.prefs.save_view_history = false;
    app.prefs.confirm_history_clear = false;
    app.prefs.click_outside_dialog = false;
    app.prefs.menu_left = MenuClick::Ignore;
    app.prefs.menu_middle = MenuClick::Cancel;
    app.prefs.completion_list = false;
    app.prefs.confirm_interrupt = false;
    app.prefs.confirm_exit = false;
    app.prefs.confirm_connection = false;
    app.prefs.confirm_subst = false;
    app.prefs.confirm_virtual = false;
    app.prefs.confirm_hotplug = false;
    app.panels[0].options.select_folders = true;
    app.panels[0].options.allow_reverse = false;
    let settings = app.to_settings();
    let text = serde_json::to_string(&settings).unwrap();
    let back: crate::settings::Settings = serde_json::from_str(&text).unwrap();
    assert_eq!(back, settings);
    assert_eq!(
        crate::app::options::Preferences::from_settings(&back),
        app.prefs
    );
    assert!(back.select_folders && !back.allow_reverse_sort);
}

#[test]
fn tree_settings_take_auto_change_and_the_least_number_of_folders() {
    let (_temp, mut app) = app();
    app.execute(Command::Options(OptionsPage::Tree));
    let index = check_box(&app, "&Auto change folder");
    toggle(&mut app, index);
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    let label = &form.items[index + 1];
    let number = &form.items[index + 2];
    assert!(matches!(&label.control, Control::Text(text) if text == "&Minimum number of folders:"));
    assert_eq!(
        (number.y, number.width),
        (label.y, 3),
        "FAR's field beside its label"
    );
    assert_eq!(form.edit_text(index + 2), "4");
    form.set_edit_text(index + 2, "10");
    press_button(&mut app, "OK");
    assert!(app.prefs.tree.auto_change_folder);
    assert_eq!(app.prefs.tree.min_tree_count, 10);
    app.execute(Command::Options(OptionsPage::Tree));
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    form.set_edit_text(index + 2, "many");
    press_button(&mut app, "OK");
    assert_eq!(app.prefs.tree.min_tree_count, 10, "no number: as it was");
}

#[test]
fn the_update_limit_is_disabled_while_its_box_is_off_and_the_history_box_is_there() {
    let (_temp, mut app) = app();
    app.config.auto_update_limit = 0;
    app.execute(Command::Options(OptionsPage::Panel));
    let index = check_box(&app, "&Disable automatic panel update");
    assert!(the_form(&app).items[index + 2].disabled, "FAR's LinkFlags");
    toggle(&mut app, index);
    assert!(!the_form(&app).items[index + 2].disabled);
    assert!(
        !the_form(&app).items[index + 1].disabled,
        "its label is not linked"
    );
    press(&mut app, KeyCode::Esc);
    flip(
        &mut app,
        OptionsPage::Dialog,
        "&History in dialog edit controls",
        "OK",
    );
    assert!(!app.prefs.dialogs.edit_history);
    assert!(!app.to_settings().dialog_switches.edit_history);
}

#[test]
fn system_settings_keep_finished_tasks_as_long_as_asked_and_it_is_written() {
    let (_temp, mut app) = app();
    app.execute(Command::Options(OptionsPage::System));
    let form = the_form(&app);
    let label = (form.items.iter())
        .position(|item| matches!(&item.control, Control::Text(text) if text == "Keep finished &tasks, seconds:"))
        .expect("the field");
    assert_eq!(form.edit_text(label + 1), "30", "ADR-0010's default");
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    form.set_edit_text(label + 1, "5");
    press_button(&mut app, "OK");
    assert_eq!(app.tasks.keep_done, std::time::Duration::from_secs(5));
    assert_eq!(app.to_settings().keep_finished_tasks, 5);
}

/// FAR's three-state "Use Ctrl+PgUp to change drive": shown off, on and `[?]` for 0, 1 and 2, Space
/// going round them, OK taking the state shown.
#[test]
fn ctrl_pg_up_is_a_three_state_box() {
    let (_temp, mut app) = app();
    let label = "Use Ctrl+Pg&Up to change drive";
    let state = |app: &App| {
        let form = the_form(app);
        let index = (form.items.iter())
            .position(|item| {
                matches!(&item.control, Control::Tristate { label: found, .. } if found == label)
            })
            .unwrap();
        (index, form.tristate(index))
    };
    for (value, shown) in [(0, Some(false)), (1, Some(true)), (2, None)] {
        app.prefs.ctrl_pg_up_changes_drive = value;
        app.execute(Command::Options(OptionsPage::Interface));
        assert_eq!(state(&app).1, shown, "{value} shown");
        press_button(&mut app, "OK");
        assert_eq!(app.prefs.ctrl_pg_up_changes_drive, value, "{value} kept");
    }
    // Space from [?] (2, left by the loop above): off, on, [?].
    for after in [0, 1, 2] {
        app.execute(Command::Options(OptionsPage::Interface));
        let index = state(&app).0;
        toggle(&mut app, index);
        press_button(&mut app, "OK");
        assert_eq!(app.prefs.ctrl_pg_up_changes_drive, after);
    }
}

/// S2d.6b: FAR's "Save file position" and "Save view mode" - a file opened again where it was left.
#[test]
fn a_viewed_file_opens_again_where_it_was_left() {
    let (temp, mut app) = app();
    let path = temp.path().join("long.txt");
    let lines: String = (1..=200).map(|n| format!("line {n}\n")).collect();
    std::fs::write(&path, lines).unwrap();
    app.positions = Some(temp.path().join("positions.json"));
    let first_row = |app: &mut App| -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let crate::app::Screen::Viewer(viewer) = &app.screen {
                viewer.text().room.set((20, 5));
            }
            app.tick();
            if let crate::app::Screen::Viewer(viewer) = &app.screen
                && let Some(screen) = viewer.text().screen(20, 5)
                && let Some(row) = screen.rows.first()
            {
                return row.clone();
            }
            assert!(std::time::Instant::now() < deadline, "not shown");
        }
    };
    app.view_file(&path, false);
    first_row(&mut app);
    for _ in 0..10 {
        press(&mut app, KeyCode::PageDown);
    }
    let left_at = first_row(&mut app);
    assert_ne!(left_at, "line 1");
    press(&mut app, KeyCode::Esc);
    app.view_file(&path, false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while first_row(&mut app) != left_at {
        assert!(
            std::time::Instant::now() < deadline,
            "not where it was left"
        );
    }
}

#[path = "options_local_tests.rs"]
mod local_tests;
