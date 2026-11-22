//! The operations on the tree's folder (FAR's `TreeList::ProcessKey`): what each key takes and where it
//! goes. Each test's tree has its temporary folder for its root.

use std::path::Path;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::super::tests::{app, current, on_tree, press};
use crate::app::dialog::{Dialog, FormAction};
use crate::app::form::Control;
use crate::app::menu_commands_tests::run_from_menu;
use crate::app::mouse_drag_tests::asked;
use crate::app::transfer_dialog::Operation;
use crate::app::{App, Command, Overlay};

/// The tree has the keys with its cursor on `d`.
fn on_d(app: &mut App) {
    on_tree(app);
    press(app, KeyCode::End, KeyModifiers::NONE);
    assert_eq!(current(app), "d");
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

/// The texts of the dialog that came up.
fn texts(app: &App) -> Vec<String> {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no dialog");
    };
    (form.items.iter())
        .filter_map(|item| match &item.control {
            Control::Text(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        press(app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
}

#[test]
fn f5_and_f6_take_the_folder_under_the_cursor_to_the_other_panel() {
    let (temp, mut app) = app();
    on_d(&mut app);
    let other = format!("{}\\", shown(&temp.path().join("a")));
    press(&mut app, KeyCode::F(5), KeyModifiers::NONE);
    let d = shown(&temp.path().join("d"));
    assert_eq!(
        asked(&app),
        (Operation::Copy, vec![d.clone()], other.clone())
    );
    app.overlay = None;
    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    assert_eq!(asked(&app), (Operation::Move, vec![d.clone()], other));
    let prompt = texts(&app)[0].clone();
    // FAR's tree names its folder by its whole path, cut at its end where the line ends.
    let start: String = d.chars().take(20).collect();
    assert!(
        prompt.starts_with(&format!("Rename or move \"{start}")),
        "{prompt}"
    );
    app.overlay = None;
    press(&mut app, KeyCode::F(6), KeyModifiers::ALT);
    assert_eq!(asked(&app).0, Operation::Link);
}

#[test]
fn the_other_panel_in_the_tree_s_folder_offers_the_folder_s_whole_path() {
    let (temp, mut app) = app();
    on_tree(&mut app);
    assert_eq!(current(&app), "a");
    press(&mut app, KeyCode::F(5), KeyModifiers::NONE);
    let a = shown(&temp.path().join("a"));
    assert_eq!(asked(&app), (Operation::Copy, vec![a.clone()], a));
}

#[test]
fn shift_f5_and_shift_f6_offer_the_folder_s_own_path() {
    let (temp, mut app) = app();
    on_d(&mut app);
    let d = shown(&temp.path().join("d"));
    press(&mut app, KeyCode::F(5), KeyModifiers::SHIFT);
    assert_eq!(asked(&app), (Operation::Copy, vec![d.clone()], d.clone()));
    app.overlay = None;
    press(&mut app, KeyCode::F(6), KeyModifiers::SHIFT);
    assert_eq!(asked(&app), (Operation::Move, vec![d.clone()], d));
}

#[test]
fn a_name_alone_goes_beside_the_folder_the_dialog_was_opened_for() {
    let (temp, mut app) = app();
    on_d(&mut app);
    press(&mut app, KeyCode::F(5), KeyModifiers::SHIFT);
    // The cursor goes elsewhere while the dialog is up (the other panel's folder read in).
    app.tree_panel.as_mut().unwrap().cursor = 2;
    press(&mut app, KeyCode::Home, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    type_text(&mut app, "copy");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let id = app.shown_task.as_ref().expect("the copy's task").id;
    let target = app.tasks.get(id).unwrap().target.clone();
    assert_eq!(target, temp.path().join("copy"), "beside d, not beside a/b");
}

#[test]
fn the_root_is_neither_copied_nor_deleted() {
    let (_temp, mut app) = app();
    on_tree(&mut app);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    assert_eq!(current(&app), "");
    for (code, modifiers) in [
        (KeyCode::F(5), KeyModifiers::NONE),
        (KeyCode::F(6), KeyModifiers::NONE),
        (KeyCode::F(6), KeyModifiers::ALT),
        (KeyCode::F(5), KeyModifiers::SHIFT),
        (KeyCode::F(6), KeyModifiers::SHIFT),
        (KeyCode::F(8), KeyModifiers::NONE),
        (KeyCode::Delete, KeyModifiers::SHIFT),
    ] {
        press(&mut app, code, modifiers);
        assert!(app.overlay.is_none(), "{code:?} {modifiers:?}");
    }
}

#[test]
fn f8_and_shift_del_ask_to_delete_the_folder_under_the_cursor() {
    let (temp, mut app) = app();
    app.config.delete_settings.confirm = true;
    on_d(&mut app);
    for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
        let code = if modifiers.is_empty() {
            KeyCode::F(8)
        } else {
            KeyCode::Delete
        };
        press(&mut app, code, modifiers);
        let Some(Overlay::Dialog(Dialog::Form {
            action: FormAction::Delete { request, .. },
            ..
        })) = &app.overlay
        else {
            panic!("no question for {code:?}");
        };
        let paths: Vec<String> = (request.items.iter())
            .map(|item| item.path.to_string())
            .collect();
        assert_eq!(paths, [shown(&temp.path().join("d"))]);
        assert!(request.items[0].entry.is_dir);
        app.overlay = None;
        app.work.delete_count = None;
    }
    assert!(temp.path().join("d").is_dir(), "only asked");
}

#[test]
fn f4_and_ctrl_a_give_the_attributes_of_the_folder_root_too() {
    let (temp, mut app) = app();
    on_d(&mut app);
    for (code, modifiers) in [
        (KeyCode::F(4), KeyModifiers::NONE),
        (KeyCode::Char('a'), KeyModifiers::CONTROL),
    ] {
        press(&mut app, code, modifiers);
        let Some(Overlay::Dialog(Dialog::Form {
            action: FormAction::Attributes(session),
            ..
        })) = &app.overlay
        else {
            panic!("no Attributes for {code:?}");
        };
        let paths: Vec<_> = session.targets.iter().map(|target| &target.path).collect();
        assert_eq!(paths, [&temp.path().join("d")]);
        app.overlay = None;
    }
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert!(
        matches!(
            &app.overlay,
            Some(Overlay::Dialog(Dialog::Form {
                action: FormAction::Attributes(_),
                ..
            }))
        ),
        "the root's own"
    );
}

#[test]
fn a_held_f8_or_shift_del_deletes_nothing_more() {
    let (temp, mut app) = app();
    app.config.delete_settings.confirm = false;
    on_d(&mut app);
    for (code, modifiers) in [
        (KeyCode::F(8), KeyModifiers::NONE),
        (KeyCode::Delete, KeyModifiers::SHIFT),
    ] {
        app.handle_key(KeyEvent::new_with_kind(
            code,
            modifiers,
            KeyEventKind::Repeat,
        ));
        assert!(app.shown_task.is_none(), "{code:?}");
    }
    assert!(temp.path().join("d").is_dir());
}

#[test]
fn a_hidden_tree_takes_no_key_nor_the_menu_s_items() {
    let (temp, mut app) = app();
    app.config.delete_settings.confirm = false;
    on_d(&mut app);
    app.execute(Command::TogglePanels);
    assert!(!app.tree_has_keys());
    for (code, modifiers) in [
        (KeyCode::F(8), KeyModifiers::NONE),
        (KeyCode::Delete, KeyModifiers::SHIFT),
        (KeyCode::F(5), KeyModifiers::NONE),
        (KeyCode::Char('a'), KeyModifiers::CONTROL),
    ] {
        press(&mut app, code, modifiers);
        assert!(
            app.overlay.is_none() && app.shown_task.is_none(),
            "{code:?}"
        );
    }
    for command in [Command::Delete, Command::Copy, Command::Attributes] {
        run_from_menu(&mut app, command);
        assert!(
            app.overlay.is_none() && app.shown_task.is_none(),
            "{command:?}"
        );
    }
    assert!(temp.path().join("d").is_dir());
}

#[test]
fn f7_makes_the_folder_in_the_tree_s_folder_not_in_the_panel_s() {
    let (temp, mut app) = app();
    on_d(&mut app);
    press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
    let d = temp.path().join("d");
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(Dialog::Form {
            action: crate::app::dialog::FormAction::MakeDir(Some(folder)),
            ..
        })) if *folder == d),
        "F7's question, for d as the key was pressed"
    );
    // The cursor goes elsewhere while the question is up.
    app.tree_panel.as_mut().unwrap().cursor = 1;
    type_text(&mut app, "new");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let made = temp.path().join("d").join("new");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !made.is_dir()
        || app
            .tree_panel
            .as_ref()
            .is_some_and(|tree| tree.stale.is_none())
    {
        assert!(Instant::now() < deadline, "the folder was not made");
        app.pump_waiting();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!temp.path().join("a").join("new").exists());
    assert_eq!(current(&app), "a", "where the test put it");
}

#[test]
fn the_files_menu_does_on_the_tree_what_its_keys_do() {
    let (temp, mut app) = app();
    on_d(&mut app);
    run_from_menu(&mut app, Command::Copy);
    assert_eq!(asked(&app).1, [shown(&temp.path().join("d"))]);
    app.overlay = None;
    run_from_menu(&mut app, Command::MakeDir);
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: FormAction::MakeDir(_),
            ..
        }))
    ));
    app.overlay = None;
    run_from_menu(&mut app, Command::Attributes);
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: FormAction::Attributes(_),
            ..
        }))
    ));
    app.overlay = None;
    app.config.delete_settings.confirm = true;
    run_from_menu(&mut app, Command::Delete);
    assert!(matches!(
        app.overlay,
        Some(Overlay::Dialog(Dialog::Form {
            action: FormAction::Delete { .. },
            ..
        }))
    ));
    app.overlay = None;
    app.work.delete_count = None;
}

#[test]
fn a_target_with_spaces_at_an_end_is_quoted() {
    use crate::app::transfer_dialog::quote_target;
    assert_eq!(quote_target(r"R:\d ".to_string(), true), "\"R:\\d \"");
    assert_eq!(quote_target(r"R:\a,b".to_string(), false), "\"R:\\a,b\"");
    assert_eq!(quote_target(r"R:\a,b".to_string(), true), r"R:\a,b");
}
