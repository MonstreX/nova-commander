//! The tree of the copy, move and link dialogs. The tree's root is each test's own folder (`TEST_ROOT`):
//! `a/b`, `a/c`, `d`, the panels in `a`, F5 on `b`.

use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::super::tests::{app, ctrl_t, press, settle as tree_settle};
use crate::app::dialog::{Dialog, FormAction};
use crate::app::form::{Control, Form};
use crate::app::transfer_dialog::{EXISTING, MULTIPLE_TARGETS, Operation, TARGET, TREE};
use crate::app::tree_panel::{FolderTree, Purpose};
use crate::app::{App, Overlay, Side};
use crate::vfs::VfsPath;

/// F5 (`move_`: F6) on `b`.
fn dialog(app: &mut App, move_: bool) {
    let a = app.active_panel().path.clone();
    app.active_panel_mut().change_dir(&a, Some("b")).unwrap();
    let key = if move_ { KeyCode::F(6) } else { KeyCode::F(5) };
    press(app, key, KeyModifiers::NONE);
    form(app);
}

fn form(app: &App) -> &Form {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Transfer { .. },
        })) => form,
        _ => panic!("no copy dialog"),
    }
}

fn form_mut(app: &mut App) -> &mut Form {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("no copy dialog"),
    }
}

/// "Process multiple destinations" on.
fn several_on(app: &mut App) {
    if let Some(Control::Check { checked, .. }) = form_mut(app).control_mut(MULTIPLE_TARGETS) {
        *checked = true;
    }
}

fn found(app: &App) -> &FolderTree {
    match &app.overlay {
        Some(Overlay::FolderTree(found)) => found,
        _ => panic!("no tree"),
    }
}

/// The tree opened with `modifiers` + F10, read, the cursor where it is going.
fn f10(app: &mut App, modifiers: KeyModifiers) {
    press(app, KeyCode::F(10), modifiers);
    settle(app);
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.pump_folder_tree();
        let tree = &found(app).tree;
        if !tree.reading && tree.waiting.is_none() {
            break;
        }
        assert!(Instant::now() < deadline, "the tree was not read");
        std::thread::sleep(Duration::from_millis(5));
    }
    let Some(Overlay::FolderTree(found)) = &mut app.overlay else {
        unreachable!()
    };
    found.tree.set_rows(29);
}

fn current(app: &App) -> String {
    let tree = &found(app).tree;
    let item = &tree.items[tree.cursor];
    let rest = item.path.strip_prefix(&tree.root).unwrap();
    rest.to_string_lossy().replace('\\', "/")
}

fn with_slash(path: &Path) -> String {
    format!("{}{MAIN_SEPARATOR}", path.display())
}

fn root(app: &App) -> PathBuf {
    found(app).tree.root.clone()
}

#[test]
fn f10_opens_the_active_panel_s_tree_and_the_folder_chosen_goes_to_the_line() {
    let (temp, mut app) = app();
    dialog(&mut app, false);
    let before = form(&app).clone();
    f10(&mut app, KeyModifiers::NONE);
    assert!(!found(&app).standalone(), "the dialog's window");
    assert!(matches!(found(&app).purpose, Purpose::Target(_)));
    assert_eq!(root(&app), temp.path());
    assert_eq!(current(&app), "a");
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let form = form(&app);
    let b = temp.path().join("a").join("b");
    assert_eq!(form.edit_text(TARGET), with_slash(&b));
    assert_eq!(form.focus, TARGET, "the line takes the focus");
    assert_eq!(form.items.len(), before.items.len());
    assert_eq!(
        app.active_panel().path,
        VfsPath::local(temp.path().join("a")),
        "no panel moved"
    );
}

#[test]
fn esc_and_f10_in_the_tree_give_the_dialog_back_as_it_was() {
    let (_temp, mut app) = app();
    dialog(&mut app, false);
    let before = form(&app).clone();
    for close in [KeyCode::Esc, KeyCode::F(10)] {
        f10(&mut app, KeyModifiers::NONE);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, close, KeyModifiers::NONE);
        assert_eq!(form(&app), &before, "{close:?}");
    }
}

#[test]
fn alt_f10_opens_the_passive_panel_s_folder_and_the_active_one_s_when_it_is_hidden() {
    let (temp, mut app) = app();
    let d = VfsPath::local(temp.path().join("d"));
    app.panels[Side::Right.index()]
        .change_dir(&d, None)
        .unwrap();
    dialog(&mut app, false);
    f10(&mut app, KeyModifiers::ALT);
    assert_eq!(current(&app), "d");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        form(&app).edit_text(TARGET),
        with_slash(&temp.path().join("d"))
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.toggle_panel(Side::Right);
    dialog(&mut app, false);
    f10(&mut app, KeyModifiers::ALT);
    assert_eq!(current(&app), "a", "the passive panel hidden");
}

#[test]
fn shift_f10_opens_on_the_line_s_folder_taken_where_the_copy_goes() {
    let (temp, mut app) = app();
    dialog(&mut app, false);
    form_mut(&mut app).set_edit_text(TARGET, "..\\d\\");
    f10(&mut app, KeyModifiers::SHIFT);
    assert_eq!(current(&app), "d");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    let whole = with_slash(&temp.path().join("a").join("c"));
    form_mut(&mut app).set_edit_text(TARGET, &format!("\"{whole}\""));
    f10(&mut app, KeyModifiers::SHIFT);
    assert_eq!(current(&app), "a/c", "quoted");
}

#[test]
fn shift_f10_with_several_destinations_takes_the_first() {
    let (temp, mut app) = app();
    dialog(&mut app, false);
    several_on(&mut app);
    form_mut(&mut app).set_edit_text(TARGET, " ; ..\\d, a");
    f10(&mut app, KeyModifiers::SHIFT);
    assert_eq!(current(&app), "d");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let d = with_slash(&temp.path().join("d"));
    assert_eq!(
        form(&app).edit_text(TARGET),
        format!(" ; ..\\d, a;{d}"),
        "added after a ;"
    );
}

#[test]
fn shift_f10_on_an_empty_line_does_nothing() {
    let (_temp, mut app) = app();
    dialog(&mut app, false);
    form_mut(&mut app).set_edit_text(TARGET, "");
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    assert_eq!(form(&app).edit_text(TARGET), "", "the dialog stays");
}

#[test]
fn several_destinations_add_the_folder_quoted_when_it_has_a_separator() {
    let (temp, mut app) = app();
    std::fs::create_dir(temp.path().join("a").join("x,y")).unwrap();
    dialog(&mut app, false);
    several_on(&mut app);
    form_mut(&mut app).set_edit_text(TARGET, "");
    f10(&mut app, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle(&mut app);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(current(&app), "a/x,y");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let folder = with_slash(&temp.path().join("a").join("x,y"));
    assert_eq!(
        form(&app).edit_text(TARGET),
        format!("\"{folder}\""),
        "nothing before it"
    );
}

#[test]
fn a_move_s_line_takes_the_folder_alone() {
    let (temp, mut app) = app();
    dialog(&mut app, true);
    f10(&mut app, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let c = with_slash(&temp.path().join("a").join("c"));
    assert_eq!(form(&app).edit_text(TARGET), c);
}

#[test]
fn the_tree_button_is_f10_and_other_modifiers_are_not() {
    let (_temp, mut app) = app();
    dialog(&mut app, false);
    press(&mut app, KeyCode::F(10), KeyModifiers::CONTROL);
    form(&app);
    let Some(Overlay::Dialog(dialog)) = app.overlay.take() else {
        unreachable!()
    };
    app.finish_dialog(dialog, crate::app::dialog::DialogOutcome::Button(TREE));
    settle(&mut app);
    assert_eq!(current(&app), "a");
}

#[test]
fn with_the_tree_turned_off_there_is_no_tree() {
    let (_temp, mut app) = app();
    app.prefs.tree.turn_off_completely = true;
    dialog(&mut app, false);
    assert!(form(&app).items[TREE].hidden);
    press(&mut app, KeyCode::F(10), KeyModifiers::NONE);
    press(&mut app, KeyCode::F(10), KeyModifiers::ALT);
    assert!(!matches!(app.overlay, Some(Overlay::FolderTree(_))));
}

#[test]
fn the_link_dialog_has_the_tree_too() {
    let (temp, mut app) = app();
    let a = app.active_panel().path.clone();
    app.active_panel_mut().change_dir(&a, Some("b")).unwrap();
    press(&mut app, KeyCode::F(6), KeyModifiers::ALT);
    assert!(!form(&app).items[Operation::Link.tree_button()].disabled);
    f10(&mut app, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        form(&app).edit_text(TARGET),
        with_slash(&temp.path().join("a"))
    );
}

#[test]
fn an_open_drop_down_list_keeps_f10() {
    let (_temp, mut app) = app();
    dialog(&mut app, false);
    form_mut(&mut app).focus_item(EXISTING);
    press(&mut app, KeyCode::Down, KeyModifiers::ALT);
    let open = |app: &App| {
        matches!(
            form(app).control(EXISTING),
            Some(Control::DropDown { open: Some(_), .. })
        )
    };
    assert!(open(&app), "Alt+Down opens the list");
    press(&mut app, KeyCode::F(10), KeyModifiers::NONE);
    assert!(matches!(app.overlay, Some(Overlay::Dialog(_))), "no tree");
}

#[test]
fn alt_f10_takes_the_cursor_of_a_tree_on_the_passive_side() {
    let (_temp, mut app) = app();
    ctrl_t(&mut app);
    tree_settle(&mut app);
    let tree = app.tree_panel.as_mut().unwrap();
    tree.move_cursor(|tree| tree.items.len(), None);
    dialog(&mut app, false);
    f10(&mut app, KeyModifiers::ALT);
    assert_eq!(
        current(&app),
        "d",
        "the tree's cursor, not its files panel's folder"
    );
}

#[test]
fn f10_waits_for_a_slow_disk_under_the_dialog_and_esc_gives_it_back() {
    // ADR-0019, review R22: the target's disk asked off the screen's thread, the dialog under the wait.
    use crate::app::waiting::{finish_waiting, hold_next_work, shows_stopped};
    let (_temp, mut app) = app();
    dialog(&mut app, false);
    let before = form(&app).clone();
    let release = hold_next_work();
    press(&mut app, KeyCode::F(10), KeyModifiers::NONE);
    assert!(app.is_waiting());
    assert_eq!(form(&app), &before, "the dialog under the wait");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(shows_stopped(&app));
    drop(release);
    assert!(!app.pump_waiting(), "its late end is not done");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(form(&app), &before, "the dialog back as it was");
    let release = hold_next_work();
    press(&mut app, KeyCode::F(10), KeyModifiers::NONE);
    let _ = release.send(());
    finish_waiting(&mut app);
    assert!(
        matches!(app.overlay, Some(Overlay::FolderTree(_))),
        "the tree opened"
    );
}
