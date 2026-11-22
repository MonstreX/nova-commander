//! The temporary panel's own keys (ADR-0016 §14, FAR's TmpPanel `ProcessKey` and key bar), its library as
//! it is shipped: the key bar's labels, F7 taking names off with the files left, Alt+Shift+F3 showing the
//! entry in the other panel, Alt+Shift+F12's menu and Alt+Shift+digit switching the common panels,
//! Alt+Shift+F2 saving the list, Ctrl+PgUp leaving for the file under the cursor (FAR's `ChangeDir`).

use ratatui::crossterm::event::{KeyEventKind, KeyEventState};

use super::*;
use crate::platform::input::Modifiers;

fn alt_shift(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT | KeyModifiers::SHIFT)
}

/// Alt+Shift and a digit key as the console tells it: the shifted character, the digit in the state.
fn alt_shift_digit(digit: u8, shifted: char) -> KeyEvent {
    KeyEvent::new_with_kind_and_state(
        KeyCode::Char(shifted),
        KeyModifiers::ALT | KeyModifiers::SHIFT,
        KeyEventKind::Press,
        // The console input's encoding: the digit plus one, three bits up (`alt_shift_digit`).
        KeyEventState::from_bits_retain((digit + 1) << 3),
    )
}

/// The temporary panel on the left holding `a.txt` and `sub`, active.
fn holding(name: &str) -> (tempfile::TempDir, tempfile::TempDir, App) {
    let (temp, files, mut app) = opened(name);
    for item in ["a.txt", "sub"] {
        point(&mut app, Side::Right, item);
        app.handle_key(key(KeyCode::Insert));
    }
    copy(&mut app, KeyCode::F(5));
    tick_until(&mut app, |app| {
        names(app, Side::Left).len() == 2 && app.overlay.is_none() && app.plugins.putting.is_none()
    });
    app.active = Side::Left;
    (temp, files, app)
}

#[test]
fn its_labels_are_on_the_key_bar_for_the_modifiers_held() {
    let (_temp, _files, app) = holding("tmpkeysone");
    let held = |alt: bool, shift: bool| Modifiers {
        shift,
        ctrl: false,
        alt,
    };
    assert_eq!(
        app.plugin_key_labels(held(false, false)),
        [(7, "Remove".to_string())]
    );
    let mut alt_shift = app.plugin_key_labels(held(true, true));
    alt_shift.sort();
    assert_eq!(
        alt_shift,
        [
            (2, "SavLst".to_string()),
            (3, "Goto".to_string()),
            (12, "Switch".to_string())
        ]
    );
    assert!(app.plugin_key_labels(held(true, false)).is_empty());
}

#[test]
fn f7_takes_the_selected_names_off_and_leaves_the_files() {
    let (_temp, files, mut app) = holding("tmpkeystwo");
    let a = files.path().join("a.txt");
    point(&mut app, Side::Left, &a.display().to_string());
    app.handle_key(key(KeyCode::F(7)));
    let sub = files.path().join("sub").display().to_string();
    tick_until(&mut app, |app| names(app, Side::Left) == [sub.clone()]);
    assert!(a.is_file(), "the file stays");
    assert!(app.overlay.is_none(), "not the program's F7");
}

#[test]
fn alt_shift_f3_shows_a_file_or_a_folder_in_the_other_panel() {
    let (temp, files, mut app) = holding("tmpkeysthree");
    let elsewhere = VfsPath::local(temp.path());
    app.panels[1].change_dir(&elsewhere, None).unwrap();
    let a = files.path().join("a.txt");
    point(&mut app, Side::Left, &a.display().to_string());
    app.handle_key(alt_shift(KeyCode::F(3)));
    let folder = VfsPath::local(files.path());
    tick_until(&mut app, |app| shows(app, Side::Right, &folder));
    let right = &app.panels[1];
    assert_eq!(
        right.current().map(|entry| entry.name.as_str()),
        Some("a.txt")
    );
    let sub = files.path().join("sub");
    point(&mut app, Side::Left, &sub.display().to_string());
    app.handle_key(alt_shift(KeyCode::F(3)));
    let inside = VfsPath::local(&sub);
    tick_until(&mut app, |app| shows(app, Side::Right, &inside));
}

/// The panel of `side` titled `title` (read again after a switch).
fn titled(app: &App, side: Side, title: &str) -> bool {
    let panel = &app.panels[side.index()];
    panel.reading().is_none() && panel.look.title.as_deref() == Some(title)
}

#[test]
fn alt_shift_digits_and_the_menu_switch_the_common_panels() {
    // 5d-3b: FAR's one global current common panel (`SwitchToPanel`): the panel stays on its one
    // instance and is read again, its title the current's number (before: an instance a panel).
    let (_temp, _files, mut app) = holding("tmpkeysfour");
    app.handle_key(alt_shift_digit(1, '!'));
    tick_until(&mut app, |app| {
        titled(app, Side::Left, "Temporary panel [1]")
    });
    assert!(names(&app, Side::Left).is_empty(), "panel 1 is empty");
    app.handle_key(alt_shift(KeyCode::F(12)));
    tick_until(&mut app, |app| app.overlay.is_some());
    app.handle_key(key(KeyCode::Char('0')));
    tick_until(&mut app, |app| {
        titled(app, Side::Left, "Temporary panel [0]")
    });
    assert_eq!(names(&app, Side::Left).len(), 2, "panel 0 kept its names");
}

#[test]
fn alt_shift_f2_saves_the_list_where_it_is_asked() {
    let (_temp, files, mut app) = holding("tmpkeysfive");
    app.handle_key(alt_shift(KeyCode::F(2)));
    tick_until(&mut app, |app| app.overlay.is_some());
    app.handle_key(key(KeyCode::Enter));
    let saved = files.path().join("panel0.temp");
    tick_until(&mut app, |_| saved.is_file());
    let text = std::fs::read_to_string(&saved).unwrap();
    let a = files.path().join("a.txt").display().to_string();
    let sub = files.path().join("sub").display().to_string();
    assert_eq!(
        text,
        // Put in the order the other panel showed them, the folder first (review R41).
        format!("\u{feff}{sub}\r\n{a}\r\n"),
        "UTF-8 with its BOM, CR LF"
    );
}

#[test]
fn a_name_typed_without_a_folder_is_saved_where_the_panel_was_opened_from() {
    // FAR's `GetFullPath` takes its current folder, which a plugin's panel never changes: the folder the
    // temporary panel was opened from (the test's), not the other panel's offered in the line (D9).
    let (temp, files, mut app) = holding("tmpkeysten");
    app.handle_key(alt_shift(KeyCode::F(2)));
    tick_until(&mut app, |app| app.overlay.is_some());
    app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
    for ch in "mine.temp".chars() {
        app.handle_key(key(KeyCode::Char(ch)));
    }
    app.handle_key(key(KeyCode::Enter));
    let saved = temp.path().join("mine.temp");
    tick_until(&mut app, |_| saved.is_file());
    assert!(
        !files.path().join("mine.temp").exists(),
        "not in the other panel's folder"
    );
}

#[test]
fn ctrl_pgup_on_a_real_name_goes_back_with_the_cursor_on_that_name_there() {
    let (temp, files, mut app) = holding("tmpkeyssix");
    // FAR's `ChangeDir` with `OPIF_REALNAMES`: back where the panel came from (the test's folder), the
    // cursor on the entry of the same name there (`PopPrevData`).
    std::fs::write(temp.path().join("a.txt"), "the same name").unwrap();
    let a = files.path().join("a.txt");
    point(&mut app, Side::Left, &a.display().to_string());
    app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL));
    let came_from = VfsPath::local(temp.path());
    tick_until(&mut app, |app| shows(app, Side::Left, &came_from));
    let left = &app.panels[0];
    assert_eq!(
        left.current().map(|entry| entry.name.as_str()),
        Some("a.txt")
    );
}

#[test]
fn the_switch_goes_by_the_global_current_both_panels_show_it_and_the_view_stays() {
    // 5d-3b: FAR's `SwitchToPanel` and `ProcessPanelSwitchMenu` go by the one global current, which
    // every temporary panel shows (`TmpClass.cpp:753-813`); before, ours went by the panel pressed on.
    let (_temp, _files, mut app) = holding("tmpkeysseven");
    app.open_disk_for_test(0, 0, Side::Right.index());
    tick_until(&mut app, |app| {
        titled(app, Side::Right, "Temporary panel [0]")
    });
    app.active = Side::Right;
    app.handle_key(alt_shift_digit(1, '!'));
    tick_until(&mut app, |app| {
        titled(app, Side::Right, "Temporary panel [1]")
            && titled(app, Side::Left, "Temporary panel [1]")
    });
    assert!(names(&app, Side::Left).is_empty(), "both show panel 1");
    app.active = Side::Left;
    app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::CONTROL));
    app.handle_key(alt_shift(KeyCode::F(12)));
    tick_until(&mut app, |app| app.overlay.is_some());
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("the switch menu");
    };
    assert_eq!(menu.title, "Available temporary panels");
    assert_eq!(menu.lines[0].text, "&0. Total files: 2");
    assert_eq!(menu.lines[1].text, "&1. Total files: 0");
    assert_eq!(menu.selected, 1, "the global current");
    // TmpPanel's `FMENU_WRAPMODE`: Up on the first panel goes to the last.
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Up));
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("the switch menu");
    };
    assert_eq!(menu.selected, 9);
    app.handle_key(key(KeyCode::Char('0')));
    tick_until(&mut app, |app| {
        titled(app, Side::Left, "Temporary panel [0]")
            && titled(app, Side::Right, "Temporary panel [0]")
    });
    assert_eq!(names(&app, Side::Right).len(), 2);
    assert_eq!(
        app.panels[0].view_mode,
        ViewMode::Medium,
        "TmpPanel's switch keeps the view"
    );
}

#[test]
fn savlst_saves_nothing_from_an_empty_panel_or_on_esc() {
    let (_temp, files, mut app) = holding("tmpkeyseight");
    app.handle_key(alt_shift(KeyCode::F(2)));
    tick_until(&mut app, |app| app.overlay.is_some());
    app.handle_key(key(KeyCode::Esc));
    app.handle_key(alt_shift_digit(5, '%'));
    tick_until(&mut app, |app| {
        titled(app, Side::Left, "Temporary panel [5]")
    });
    app.handle_key(alt_shift(KeyCode::F(2)));
    for _ in 0..50 {
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(app.overlay.is_none(), "an empty panel asks nothing");
    assert!(
        !files.path().join("panel0.temp").exists(),
        "Esc: nothing written"
    );
}

#[test]
fn an_answer_for_a_panel_that_left_the_plugin_is_let_be() {
    let (temp, _files, mut app) = holding("tmpkeysnine");
    let answer = crate::plugin_host::KeyAnswer {
        go: Some(crate::plugin_host::KeyGo::Open("3".to_string())),
        ..crate::plugin_host::KeyAnswer::default()
    };
    let disk = VfsPath::local(temp.path());
    app.panels[0].change_dir(&disk, None).unwrap();
    app.plugin_key_done("tmpkeysnine", 0, 0, answer);
    assert_eq!(app.panels[0].path, disk, "the disk's panel stays");
}

#[test]
fn f7_has_a_quick_view_beside_it_show_the_entry_now_under_the_cursor() {
    let (_temp, files, mut app) = holding("tmpkeysqv");
    app.execute(crate::app::Command::QuickView(Side::Right));
    let a = files.path().join("a.txt");
    point(&mut app, Side::Left, &a.display().to_string());
    let shown = |app: &App| {
        (app.quick_view.as_ref())
            .and_then(|view| view.shown.as_ref())
            .map(|shown| shown.path.to_string())
            .unwrap_or_default()
    };
    tick_until(&mut app, |app| shown(app).ends_with("a.txt"));
    // TmpPanel's F7 (`TmpClass.cpp:651-657`): the quick view of the passive panel shows its file again.
    app.handle_key(key(KeyCode::F(7)));
    tick_until(&mut app, |app| shown(app).ends_with("sub"));
    assert!(a.is_file(), "the file stays");
}
