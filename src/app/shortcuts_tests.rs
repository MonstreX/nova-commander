//! Folder shortcuts' keys: Ctrl+Shift+0..9 adds, RCtrl+0..9 goes or puts the folder into a line.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::*;
use crate::app::console_input::RIGHT_CTRL;
use crate::app::fileops::tests::app_in;
use crate::app::{Overlay, ViewMode};

pub(in crate::app) fn right_ctrl(digit: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(digit),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: RIGHT_CTRL,
    }
}

fn ctrl_shift(digit: char) -> KeyEvent {
    KeyEvent::new(
        KeyCode::Char(digit),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )
}

/// A folder with `one` and `two` in it, the app in it.
pub(in crate::app) fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("one")).unwrap();
    std::fs::create_dir(temp.path().join("two")).unwrap();
    let app = app_in(temp.path());
    (temp, app)
}

pub(in crate::app) fn shortcut(folder: &Path) -> FolderShortcut {
    FolderShortcut {
        name: String::new(),
        folder: folder.display().to_string(),
    }
}

/// Ticks until the panel on `side` is in `folder`.
pub(in crate::app) fn wait_in(app: &mut App, side: usize, folder: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.panels[side].path.as_local() != Some(folder) {
        assert!(Instant::now() < deadline, "not in {}", folder.display());
        app.tick();
    }
}

#[test]
fn ctrl_shift_digit_adds_the_folder_at_the_end_and_rctrl_goes_back_there() {
    let (temp, mut app) = app();
    let one = temp.path().join("one");
    app.shortcuts.lists[3].push(shortcut(Path::new(r"C:\first")));
    app.panels[0]
        .change_dir(&VfsPath::local(one.clone()), None)
        .unwrap();
    wait_in(&mut app, 0, &one);
    app.handle_key(ctrl_shift('3'));
    assert_eq!(app.shortcuts.lists[3].len(), 2, "added after the first");
    assert_eq!(app.shortcuts.lists[3][1], shortcut(&one));

    app.shortcuts.lists[3].remove(0);
    app.panels[0]
        .change_dir(&VfsPath::local(temp.path().to_path_buf()), None)
        .unwrap();
    wait_in(&mut app, 0, temp.path());
    app.handle_key(right_ctrl('3'));
    wait_in(&mut app, 0, &one);
    assert!(app.overlay.is_none(), "one folder needs no menu");
}

/// What the console's key record of `virtual_key` with these Ctrl keys becomes.
fn translated(virtual_key: u16, left: bool, right: bool) -> Vec<KeyEvent> {
    use crate::app::console_input::{Input, Translator};
    use crate::platform::input::{ConsoleRecord, ControlKeys, KeyRecord};
    let record = ConsoleRecord::Key(KeyRecord {
        down: true,
        virtual_key,
        unit: 0,
        control: ControlKeys {
            left_ctrl: left,
            right_ctrl: right,
            ..ControlKeys::default()
        },
    });
    (Translator::default().translate(&record).into_iter())
        .filter_map(|input| match input {
            Input::Key(key) => Some(key),
            _ => None,
        })
        .collect()
}

#[test]
fn a_digit_under_the_right_ctrl_alone_is_told_from_the_left_ones_as_far_does() {
    let [right] = translated(0x37, false, true)[..] else {
        panic!("no key");
    };
    assert_eq!(right_ctrl_digit(&right), Some(7));
    for (left, right) in [(true, false), (true, true)] {
        let [key] = translated(0x37, left, right)[..] else {
            panic!("no key");
        };
        assert_eq!(right_ctrl_digit(&key), None);
        assert_eq!(key.code, KeyCode::Char('7'));
    }
    let [letter] = translated(0x46, false, true)[..] else {
        panic!("no key");
    };
    assert_eq!(letter.state, KeyEventState::NONE, "a letter is Ctrl's");
}

/// Alt+Shift and the key `virtual_key` typing `typed`, as the console reads it.
pub(in crate::app) fn alt_shift(virtual_key: u16, typed: char) -> KeyEvent {
    use crate::app::console_input::{Input, Translator};
    use crate::platform::input::{ConsoleRecord, ControlKeys, KeyRecord};
    let record = ConsoleRecord::Key(KeyRecord {
        down: true,
        virtual_key,
        unit: u16::try_from(u32::from(typed)).unwrap(),
        control: ControlKeys {
            shift: true,
            left_alt: true,
            ..ControlKeys::default()
        },
    });
    let keys: Vec<KeyEvent> = (Translator::default().translate(&record).into_iter())
        .filter_map(|input| match input {
            Input::Key(key) => Some(key),
            _ => None,
        })
        .collect();
    keys[0]
}

#[test]
fn alt_shift_digit_adds_as_well_where_the_terminal_keeps_ctrl_shift_digits() {
    let (temp, mut app) = app();
    // The Russian layout's Shift+2 types `"`; the key is what counts.
    app.handle_key(alt_shift(0x32, '"'));
    assert_eq!(app.shortcuts.lists[2], [shortcut(temp.path())]);
    assert!(app.overlay.is_none(), "no quick search");
    // The US layout's `"` is on the quote key: no shortcut there.
    let quote = alt_shift(0xDE, '"');
    assert_eq!(add_digit(&quote), None);
    assert_eq!(add_digit(&alt_shift(0x35, '%')), Some(5));
    assert_eq!(add_digit(&alt_shift(0x30, ')')), Some(0));
    assert_eq!(add_digit(&alt_shift(0x39, '(')), Some(9));
}

#[test]
fn a_shell_still_gets_the_character_of_alt_shift_and_a_digit_key() {
    let key = alt_shift(0x31, '!');
    assert_eq!(key.code, KeyCode::Char('!'));
    assert_eq!(
        crate::app::terminal_keys::encode(key),
        Some(b"\x1b!".to_vec())
    );
}

#[test]
fn the_left_ctrl_and_a_digit_still_changes_the_mode() {
    let (temp, mut app) = app();
    app.shortcuts.lists[1].push(shortcut(&temp.path().join("one")));
    app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::CONTROL));
    assert_eq!(app.active_panel().view_mode, ViewMode::Brief);
    assert!(app.overlay.is_none());
    assert_eq!(app.active_panel().path.as_local(), Some(temp.path()));
}

#[test]
fn the_folders_variables_are_replaced_on_the_way() {
    let (temp, mut app) = app();
    app.environment
        .set("NC_SHORTCUT", &temp.path().display().to_string());
    app.shortcuts.lists[0].push(FolderShortcut {
        name: String::new(),
        folder: r"%NC_SHORTCUT%\two".to_string(),
    });
    app.handle_key(right_ctrl('0'));
    wait_in(&mut app, 0, &temp.path().join("two"));
}

#[test]
fn several_folders_or_none_are_a_list_to_choose_from() {
    let (temp, mut app) = app();
    let (one, two) = (temp.path().join("one"), temp.path().join("two"));
    app.shortcuts.lists[2] = vec![shortcut(&one), shortcut(&two)];
    app.handle_key(right_ctrl('2'));
    let Some(Overlay::Shortcuts(menu)) = &app.overlay else {
        panic!("no list");
    };
    assert_eq!(menu.title, "Folder shortcuts (2)");
    assert_eq!(menu.lines.len(), 2);
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.overlay.is_none());
    wait_in(&mut app, 0, &two);

    app.handle_key(right_ctrl('5'));
    let Some(Overlay::Shortcuts(menu)) = &app.overlay else {
        panic!("no empty list");
    };
    assert!(menu.lines.is_empty(), "Ins adds to it");
}

#[test]
fn on_the_info_panel_the_other_panel_goes() {
    let (temp, mut app) = app();
    let one = temp.path().join("one");
    app.shortcuts.lists[4].push(shortcut(&one));
    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
    if !app.info_is_active() {
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    assert!(app.info_is_active());
    let files = app.active.other().index();
    app.handle_key(right_ctrl('4'));
    wait_in(&mut app, files, &one);
}

#[test]
fn with_the_panel_hidden_the_folder_goes_into_the_command_line() {
    let (temp, mut app) = app();
    let one: PathBuf = temp.path().join("one");
    app.shortcuts.lists[6].push(shortcut(&one));
    app.panels[0].visible = false;
    app.panels[1].visible = false;
    app.command_line.insert_str("cd ");
    app.handle_key(right_ctrl('6'));
    assert_eq!(app.command_line.text(), format!("cd {}", one.display()));
    assert_eq!(app.active_panel().path.as_local(), Some(temp.path()));
}

#[test]
fn ctrl_shift_digit_on_the_info_panel_adds_nothing() {
    let (_temp, mut app) = app();
    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
    if !app.info_is_active() {
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    app.handle_key(ctrl_shift('7'));
    assert!(app.shortcuts.lists[7].is_empty());
}

fn make_dir_input(app: &App) -> String {
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &app.overlay else {
        panic!("no dialog");
    };
    form.edit_text(crate::app::make_dir_form::NAME).to_string()
}

#[test]
fn in_a_dialogs_line_the_folder_is_typed_and_a_list_comes_first_if_need_be() {
    let (temp, mut app) = app();
    let (one, two) = (temp.path().join("one"), temp.path().join("two"));
    app.shortcuts.lists[1].push(shortcut(&one));
    app.shortcuts.lists[2] = vec![shortcut(&one), shortcut(&two)];
    app.handle_key(KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE));
    app.handle_key(right_ctrl('1'));
    assert_eq!(make_dir_input(&app), one.display().to_string());

    app.handle_key(right_ctrl('2'));
    assert!(matches!(app.overlay, Some(Overlay::Shortcuts(_))));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(
        make_dir_input(&app),
        one.display().to_string(),
        "Esc gives the dialog back as it was"
    );

    app.handle_key(right_ctrl('2'));
    app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        make_dir_input(&app),
        format!("{}{}", one.display(), two.display())
    );
    assert_eq!(app.active_panel().path.as_local(), Some(temp.path()));
}

#[test]
fn the_lists_outlive_the_program_ten_of_them() {
    let (temp, mut app) = app();
    app.shortcuts.lists[9].push(FolderShortcut {
        name: "&Work".to_string(),
        folder: temp.path().display().to_string(),
    });
    let settings = app.to_settings();
    assert_eq!(settings.folder_shortcuts.len(), LISTS);
    let again = App::new(temp.path(), 100, 40, &settings);
    assert_eq!(again.shortcuts.lists, app.shortcuts.lists);

    let mut eleven = settings;
    eleven.folder_shortcuts.push(Vec::new());
    let cut = App::new(temp.path(), 100, 40, &eleven);
    assert_eq!(cut.shortcuts.lists.len(), LISTS);
}
