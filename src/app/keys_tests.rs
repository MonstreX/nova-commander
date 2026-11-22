//! Keys on the main screen.

use super::*;
use crate::app::{Overlay, Screen};
use crate::vfs::SortMode;

fn app_with_files() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    std::fs::write(temp.path().join("my notes.txt"), "hello").unwrap();
    let app = App::new(temp.path(), 100, 40, &crate::settings::Settings::default());
    (temp, app)
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

#[test]
fn esc_on_an_empty_command_line_hides_and_shows_the_panels() {
    let (_temp, mut app) = app_with_files();
    app.command_line.set("dir");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.command_line.is_empty(), "the first Esc clears");
    assert!(!app.panels_hidden());
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.panels_hidden());
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.panels[0].visible && app.panels[1].visible);
}

#[test]
fn shift_with_the_keypad_plus_selects_all_and_the_main_keyboards_is_typed() {
    let (_temp, mut app) = app_with_files();
    let plus = KeyEvent::new(KeyCode::Char('+'), KeyModifiers::SHIFT);
    app.handle_key(plus);
    assert_eq!(app.command_line.text(), "+", "the main keyboard's");
    assert!(app.panels[0].selected.is_empty());
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.handle_key(KeyEvent {
        state: ratatui::crossterm::event::KeyEventState::KEYPAD,
        ..plus
    });
    assert!(app.command_line.is_empty(), "the keypad's is not typed");
    assert!(!app.panels[0].selected.is_empty());
}

#[test]
fn typing_goes_to_command_line_and_esc_clears_it() {
    let (_temp, mut app) = app_with_files();
    press(&mut app, KeyCode::Char('d'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('I'), KeyModifiers::SHIFT);
    assert_eq!(app.command_line.text(), "dI");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.command_line.is_empty());
}

#[test]
fn enter_and_ctrl_page_up_navigate_directories() {
    let (temp, mut app) = app_with_files();
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.active_panel().path, temp.path().join("sub"));
    press(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
    assert_eq!(app.active_panel().path, temp.path());
}

#[test]
fn ctrl_page_up_at_a_root_opens_the_drive_menu_when_set_so() {
    let (temp, mut app) = app_with_files();
    let root = temp.path().ancestors().last().unwrap().to_path_buf();
    // At the root without reading it: only the key's choice is looked at.
    app.active_panel_mut().path = crate::vfs::VfsPath::local(&root);
    app.prefs.ctrl_pg_up_changes_drive = 0;
    press(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
    assert!(app.overlay.is_none(), "off: nothing");
    assert_eq!(app.active_panel().path, root);
    app.prefs.ctrl_pg_up_changes_drive = 1;
    press(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
    crate::app::waiting::finish_waiting(&mut app);
    assert!(
        matches!(app.overlay, Some(crate::app::Overlay::DriveMenu(_))),
        "FAR's default: the drive menu"
    );
    assert_eq!(app.active_panel().path, root);
}

#[test]
fn backspace_only_edits_the_command_line() {
    let (temp, mut app) = app_with_files();
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    // Empty line: nothing happens, the panel stays in the folder.
    press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(app.active_panel().path, temp.path().join("sub"));
    assert!(app.command_line.is_empty());
    app.command_line.set("dir");
    press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(app.command_line.text(), "di");
}

#[test]
fn left_right_move_the_panel_when_the_line_is_empty() {
    let (_temp, mut app) = app_with_files();
    app.active_panel_mut().set_view_metrics(10, 1);
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(
        app.active_panel().cursor,
        2,
        "one column down, clamped to the last entry"
    );
    press(&mut app, KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(app.active_panel().cursor, 0);
}

#[test]
fn left_right_edit_a_typed_line_only_in_single_stripe_views() {
    let (_temp, mut app) = app_with_files();
    app.command_line.set("dir");
    // Full-like view (one stripe): arrows edit the command line.
    app.active_panel_mut().set_view_metrics(10, 1);
    press(&mut app, KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(app.command_line.cursor(), 2);
    assert_eq!(app.active_panel().cursor, 0);
    // Medium (two stripes): arrows move the panel, Ctrl+S / Ctrl+D edit the line (FAR).
    app.active_panel_mut().set_view_metrics(10, 2);
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(app.active_panel().cursor, 2);
    assert_eq!(app.command_line.cursor(), 2);
    press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert_eq!(app.command_line.cursor(), 1);
    press(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);
    assert_eq!(app.command_line.cursor(), 2);
    assert_eq!(app.command_line.text(), "dir");
}

#[test]
fn a_hidden_active_panel_leaves_keys_to_the_command_line() {
    let (_temp, mut app) = app_with_files();
    press(&mut app, KeyCode::F(2), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::F(1), KeyModifiers::CONTROL);
    assert!(app.panels_hidden());
    app.command_line.set("dir x");
    let cursor = app.active_panel().cursor;
    press(&mut app, KeyCode::Left, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(5), KeyModifiers::NONE);
    assert_eq!(app.command_line.cursor(), 4, "Left edits the line");
    assert_eq!(
        app.active_panel().cursor,
        cursor,
        "Down does not move the panel"
    );
    assert!(app.overlay.is_none(), "F5 does nothing without a panel");
    press(&mut app, KeyCode::Char('a'), KeyModifiers::ALT);
    assert_eq!(app.command_line.text(), "dir x", "Alt+letter types nothing");
    // With both panels hidden Ctrl+Left moves by words even on an empty line.
    app.command_line.take();
    press(&mut app, KeyCode::Left, KeyModifiers::CONTROL);
    assert_eq!(app.width_decrement, 0);
    // The screen keys still work: Ctrl+P needs a shown active panel, Ctrl+F2 shows one.
    press(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(app.panels_hidden());
    press(&mut app, KeyCode::F(2), KeyModifiers::CONTROL);
    assert_eq!(app.active, Side::Right);
    press(&mut app, KeyCode::Left, KeyModifiers::CONTROL);
    assert_eq!(app.width_decrement, 1, "Ctrl+Left moves the split again");
}

#[test]
fn with_both_panels_hidden_up_and_down_walk_the_history() {
    let (_temp, mut app) = app_with_files();
    app.push_history("ping one");
    app.push_history("ping two");
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert!(app.command_line.is_empty(), "a shown panel takes Up");
    press(&mut app, KeyCode::F(2), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert!(app.command_line.is_empty(), "one panel is still shown");
    press(&mut app, KeyCode::F(1), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(app.command_line.text(), "ping one");
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(app.command_line.text(), "ping two");
}

#[test]
fn shift_f9_asks_to_save_the_setup() {
    let (_temp, mut app) = app_with_files();
    assert!(!app.take_save_request());
    press(&mut app, KeyCode::F(9), KeyModifiers::SHIFT);
    assert!(
        matches!(app.overlay, Some(Overlay::Dialog(_))),
        "Shift+F9 does not open the menu: FAR's question (R7 review D9)"
    );
    assert!(!app.take_save_request(), "not before the answer");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.take_save_request());
    assert!(!app.take_save_request(), "the request is taken once");
    assert!(app.overlay.is_none());
}

#[test]
fn ctrl_f12_sort_menu_starts_on_the_current_mode_and_sets_the_choice() {
    let (_temp, mut app) = app_with_files();
    app.active_panel_mut().set_sort(SortMode::Extension);
    press(&mut app, KeyCode::F(12), KeyModifiers::CONTROL);
    let Some(Overlay::Popup(popup)) = &app.overlay else {
        panic!("the sort menu is a pop-up menu");
    };
    assert_eq!(popup.items[popup.selected].label, "E&xtension");
    assert_eq!(popup.items[popup.selected].check, Some('▲'));
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.active_panel().sort.mode, SortMode::Modified);
    assert!(
        app.active_panel().sort.reverse,
        "write time starts newest first"
    );
    // '-' / '+' choose the direction explicitly.
    press(&mut app, KeyCode::F(12), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('+'), KeyModifiers::NONE);
    assert!(!app.active_panel().sort.reverse);
}

#[test]
fn ctrl_f_keys_sort_and_repeat_reverses_shift_f12_selected_first() {
    let (_temp, mut app) = app_with_files();
    press(&mut app, KeyCode::F(7), KeyModifiers::CONTROL);
    assert_eq!(app.active_panel().sort.mode, SortMode::Unsorted);
    press(&mut app, KeyCode::F(6), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::F(6), KeyModifiers::CONTROL);
    assert_eq!(app.active_panel().sort.mode, SortMode::Size);
    assert!(!app.active_panel().sort.reverse);
    press(&mut app, KeyCode::F(12), KeyModifiers::SHIFT);
    assert!(app.active_panel().sort.selected_first);
}

#[test]
fn ctrl_digits_switch_view_modes() {
    let (_temp, mut app) = app_with_files();
    press(&mut app, KeyCode::Char('1'), KeyModifiers::CONTROL);
    assert_eq!(app.active_panel().view_mode, ViewMode::Brief);
    press(&mut app, KeyCode::Char('0'), KeyModifiers::CONTROL);
    assert_eq!(app.active_panel().view_mode, ViewMode::AlternativeFull);
    press(&mut app, KeyCode::Char('7'), KeyModifiers::CONTROL);
    assert_eq!(app.active_panel().view_mode, ViewMode::AlternativeFull);
    assert!(app.command_line.is_empty());
}

#[test]
fn cd_in_command_line_changes_panel_directory() {
    let (temp, mut app) = app_with_files();
    app.command_line.set("cd sub");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.active_panel().path, temp.path().join("sub"));
    assert!(app.command_line.is_empty());
}

#[test]
fn tab_switches_panels_and_f9_opens_menu_of_active_side() {
    let (_temp, mut app) = app_with_files();
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.active, Side::Right);
    press(&mut app, KeyCode::F(9), KeyModifiers::NONE);
    assert!(matches!(app.overlay, Some(Overlay::Menu(state)) if state.menu == 4));
}

#[test]
fn f3_opens_viewer_and_esc_returns() {
    let (_temp, mut app) = app_with_files();
    app.active_panel_mut().end();
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Viewer(_)));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(app.screen, Screen::Panels));
}

/// F3's viewer and F4's editor read only what they show (S2d.1a, S2d.4b): both open at once, nothing
/// waited for, even with a disk slow to give the file (ADR-0019, review R8).
#[test]
fn f3_and_f4_open_at_once() {
    use crate::app::waiting::hold_next_work;
    let (_temp, mut app) = app_with_files();
    app.active_panel_mut().end();
    let release = hold_next_work();
    press(&mut app, KeyCode::F(3), KeyModifiers::NONE);
    assert!(!app.is_waiting(), "F3: nothing waited for");
    assert!(matches!(app.screen, Screen::Viewer(_)), "F3: at once");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert!(!app.is_waiting(), "F4: nothing waited for");
    assert!(matches!(app.screen, Screen::Editor(_)), "F4: at once");
    drop(release);
}

/// R7 review D8: F4 and Alt+F4 on a folder open its attributes, as Ctrl+A (FAR's `FileList`).
#[test]
fn f4_on_a_folder_opens_its_attributes() {
    use crate::app::dialog::{Dialog, FormAction};
    let (_temp, mut app) = app_with_files();
    let at = |app: &App| app.active_panel().current().map(|entry| entry.name.clone());
    while at(&app).as_deref() != Some("sub") {
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    for modifiers in [KeyModifiers::NONE, KeyModifiers::ALT] {
        press(&mut app, KeyCode::F(4), modifiers);
        assert!(
            matches!(
                &app.overlay,
                Some(Overlay::Dialog(Dialog::Form {
                    action: FormAction::Attributes(_),
                    ..
                }))
            ),
            "{modifiers:?}"
        );
        assert!(matches!(app.screen, Screen::Panels));
        app.overlay = None;
    }
}

/// Ticks until the editor's keys are done and no work is waited for.
fn settle_editor(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        app.tick();
        let busy = matches!(&app.screen, Screen::Editor(editor) if editor.pending.is_some());
        if !busy && !app.is_waiting() {
            return;
        }
        assert!(std::time::Instant::now() < deadline, "the editor's work");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn f4_edit_and_save_writes_the_file() {
    let (temp, mut app) = app_with_files();
    app.active_panel_mut().end();
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('!'), KeyModifiers::SHIFT);
    press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
    settle_editor(&mut app);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    settle_editor(&mut app);

    let saved = std::fs::read_to_string(temp.path().join("my notes.txt")).unwrap();
    assert_eq!(saved, "!hello");
    assert!(matches!(app.screen, Screen::Panels));
}

#[test]
fn closing_modified_editor_asks_before_discarding() {
    let (temp, mut app) = app_with_files();
    app.active_panel_mut().end();
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    settle_editor(&mut app);
    assert!(matches!(app.overlay, Some(Overlay::Dialog(_))));

    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(matches!(app.screen, Screen::Panels));
    let content = std::fs::read_to_string(temp.path().join("my notes.txt")).unwrap();
    assert_eq!(content, "hello");
}

#[test]
fn the_command_line_selects_and_uses_the_clipboard_beside_the_panel() {
    let (_temp, mut app) = app_with_files();
    let memory = crate::app::clipboard::Memory::default();
    app.clipboard = Box::new(memory.clone());
    press(&mut app, KeyCode::Char('3'), KeyModifiers::CONTROL);
    assert_eq!(app.active_panel().stripes(), 1, "Full: one stripe");
    app.command_line.set("echo one");

    press(&mut app, KeyCode::Left, KeyModifiers::SHIFT);
    press(
        &mut app,
        KeyCode::Left,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(app.command_line.selected_text().as_deref(), Some("one"));
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(memory.contents().text.as_deref(), Some("one"));
    press(
        &mut app,
        KeyCode::Home,
        KeyModifiers::ALT | KeyModifiers::SHIFT,
    );
    assert_eq!(app.command_line.selected_text().as_deref(), Some("echo "));
    press(&mut app, KeyCode::Insert, KeyModifiers::SHIFT);
    assert_eq!(app.command_line.text(), "oneone");

    // The panel keeps Shift+Del and Ctrl+A; Ctrl+Y and Ctrl+Backspace edit the line.
    app.command_line.select_all();
    press(&mut app, KeyCode::Delete, KeyModifiers::SHIFT);
    assert_eq!(app.command_line.text(), "oneone");
    press(&mut app, KeyCode::Backspace, KeyModifiers::CONTROL);
    assert!(app.command_line.is_empty(), "the block, then the word");
    app.command_line.set("dir x");
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert_eq!(app.command_line.selection(), None);

    // With the panels hidden they go to the line too.
    press(&mut app, KeyCode::F(2), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::F(1), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Delete, KeyModifiers::SHIFT);
    assert!(app.command_line.is_empty());
    assert_eq!(memory.contents().text.as_deref(), Some("dir x"));
    press(&mut app, KeyCode::Char('v'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert!(app.command_line.is_empty());
}

#[test]
fn alt_f6_opens_fars_link_dialog_on_a_folder_with_a_junction_chosen() {
    let (_temp, mut app) = app_with_files();
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);

    press(&mut app, KeyCode::F(6), KeyModifiers::ALT);

    let Some(Overlay::Dialog(crate::app::dialog::Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no dialog");
    };
    assert_eq!(form.title, "Link");
    assert_eq!(
        form.chosen(crate::app::transfer_dialog::LINK_TYPE),
        1,
        "directory junction"
    );
}
