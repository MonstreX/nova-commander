use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::dialog::Dialog;
use crate::app::form::{Control, EditHistory, HistoryChange, HistoryLine, Item};
use crate::app::line_input::LineInput;
use crate::app::{Overlay, Side};
use crate::settings::history::{Entry, Kind};

fn app() -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.rs"), "x").unwrap();
    let app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
    (temp, app)
}

fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn form(app: &mut App) -> &mut Form {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("a form is up"),
    }
}

/// Gray+ with `mask` typed over the line, closed by `close`.
fn select(app: &mut App, mask: &str, close: KeyCode) {
    app.ask_select_group(Side::Left, true);
    form(app).set_edit_text(0, mask);
    key(app, close, KeyModifiers::NONE);
}

fn texts(app: &App, name: &str) -> Vec<String> {
    let lines = app.history.lists.dialog(name).iter();
    lines.map(|entry| entry.text.clone()).collect()
}

#[test]
fn a_button_adds_the_line_to_its_history_esc_does_not_and_the_list_has_it_newest_first() {
    let (_temp, mut app) = app();
    select(&mut app, "*.rs", KeyCode::Enter);
    select(&mut app, "*.md", KeyCode::Enter);
    select(&mut app, "*.txt", KeyCode::Esc);
    assert_eq!(texts(&app, "Masks"), ["*.rs", "*.md"]);
    app.ask_select_group(Side::Left, true);
    let history = form(&mut app).history(0).unwrap();
    let lines: Vec<&str> = (history.entries.iter().flatten())
        .map(|line| line.entry.text.as_str())
        .collect();
    assert_eq!(lines, ["*.md", "*.rs"]);
    assert!(
        history.entries.as_ref().unwrap()[0].day.len() >= 8,
        "its date"
    );
    // What the list does is kept even when Esc closes the dialog.
    key(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    key(&mut app, KeyCode::Insert, KeyModifiers::NONE);
    key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    key(&mut app, KeyCode::Delete, KeyModifiers::SHIFT);
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    let kept = app.history.lists.dialog("Masks");
    assert_eq!(texts(&app, "Masks"), ["*.md"]);
    assert!(kept[0].locked);
}

#[test]
fn with_the_dialogs_histories_off_there_is_no_list_and_nothing_is_added() {
    let (_temp, mut app) = app();
    app.prefs.dialogs.edit_history = false;
    select(&mut app, "*.rs", KeyCode::Enter);
    assert!(texts(&app, "Masks").is_empty());
    app.ask_select_group(Side::Left, true);
    assert!(!form(&mut app).history(0).unwrap().has_arrow());
}

#[test]
fn an_empty_last_history_line_takes_the_last_one_given() {
    let (_temp, mut app) = app();
    let line = || {
        let edit = Item::new(5, 2, Control::Edit(LineInput::default())).wide(20);
        let ok = Item::new(
            0,
            4,
            Control::Button {
                label: "OK".to_string(),
                default: true,
            },
        );
        Form::new(
            "Questions",
            40,
            7,
            vec![
                edit.with_history(EditHistory::new("UserVar0").use_last()),
                ok,
            ],
        )
    };
    let mut first = line();
    first.set_edit_text(0, "notes");
    app.keep_form_histories(&mut first, true);
    let mut again = line();
    app.give_histories(&mut again);
    assert_eq!(again.edit_text(0), "notes");
    assert!(
        again.history(0).unwrap().confirm_clear,
        "FAR's default asks before Del"
    );
}

fn click(app: &mut App, column: u16, row: u16) {
    use crate::app::console_input::{Mouse, MouseAction};
    use crate::platform::input::MouseButtons;
    for left in [true, false] {
        let buttons = MouseButtons {
            left,
            right: false,
            middle: false,
        };
        app.handle_mouse(Mouse {
            column,
            row,
            buttons,
            pressed: buttons,
            action: MouseAction::Button,
            modifiers: KeyModifiers::NONE,
        });
    }
}

#[test]
fn a_click_on_the_arrow_opens_the_history_and_one_on_a_line_puts_it_into_the_edit() {
    use crate::app::{Cells, ControlKind, DialogArea, DialogControl, MenuArea};
    let (_temp, mut app) = app();
    select(&mut app, "*.rs", KeyCode::Enter);
    select(&mut app, "*.md", KeyCode::Enter);
    app.ask_select_group(Side::Left, true);
    let cells = |x, y, width, height| Cells {
        x,
        y,
        width,
        height,
    };
    // The arrow as the renderer reports it: one cell after the line.
    app.mouse.dialog = Some(DialogArea {
        frame: cells(20, 8, 60, 8),
        controls: vec![
            DialogControl {
                cells: cells(25, 10, 45, 1),
                index: 0,
                kind: ControlKind::Edit { first: 0 },
            },
            DialogControl {
                cells: cells(70, 10, 1, 1),
                index: 0,
                kind: ControlKind::DropDown,
            },
        ],
    });
    click(&mut app, 70, 10);
    assert!(
        form(&mut app).open_history().is_some(),
        "the click opened it"
    );
    // Its list under the line: the date, "*.md", "*.rs".
    let list = cells(25, 11, 45, 5);
    app.mouse.menu = Some(MenuArea {
        frame: list,
        outer: list,
        first: 0,
    });
    click(&mut app, 30, 14);
    assert_eq!(form(&mut app).edit_text(0), "*.rs");
    assert!(form(&mut app).open_history().is_none());
}

#[test]
fn an_open_history_list_takes_ctrl_f5_and_the_line_keys_and_esc_closes_only_it() {
    let (_temp, mut app) = app();
    select(&mut app, "*.rs", KeyCode::Enter);
    app.ask_select_group(Side::Left, true);
    key(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert!(form(&mut app).open_history().is_some());
    key(&mut app, KeyCode::F(5), KeyModifiers::CONTROL);
    assert!(
        app.dialog_place.keys.is_none(),
        "not moved while the list is open"
    );
    key(&mut app, KeyCode::Char('['), KeyModifiers::CONTROL);
    assert_eq!(
        form(&mut app).edit_text(0),
        "*.rs",
        "no path put under the list"
    );
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(form(&mut app).open_history().is_none() && app.overlay.is_some());
}

#[test]
fn a_line_taken_out_leaves_the_cursor_on_a_line_not_a_date() {
    let (_temp, mut app) = app();
    let mut form = {
        app.ask_select_group(Side::Left, true);
        self::form(&mut app).clone()
    };
    let line = |text: &str, day: &str| HistoryLine {
        entry: Entry::new(text, Kind::Plain),
        day: day.to_string(),
    };
    let history = form.history_mut(0).unwrap();
    history.entries = Some(vec![line("a", "14"), line("b", "14"), line("c", "13")]);
    let mut clipboard = crate::app::clipboard::Memory::default();
    let mut press = |form: &mut Form, code, modifiers| {
        form.handle_key(KeyEvent::new(code, modifiers), &mut clipboard);
    };
    press(&mut form, KeyCode::Down, KeyModifiers::CONTROL);
    press(&mut form, KeyCode::Down, KeyModifiers::NONE);
    press(&mut form, KeyCode::Delete, KeyModifiers::SHIFT);
    // [-14-, a, -13-, c]: the cursor on `c`, not on the date.
    press(&mut form, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(form.edit_text(0), "c");
    press(&mut form, KeyCode::Down, KeyModifiers::CONTROL);
    press(&mut form, KeyCode::Insert, KeyModifiers::SHIFT);
    press(&mut form, KeyCode::Delete, KeyModifiers::CONTROL);
    let history = form.history(0).unwrap();
    assert!(
        history.changes.len() == 1,
        "Shift+Ins and Ctrl+Del do nothing: {:?}",
        history.changes
    );
}

#[test]
fn find_files_drive_and_a_wrong_mask_add_no_line_and_changes_are_kept_once() {
    let (_temp, mut app) = app();
    let mut find = {
        app.handle_key(KeyEvent::new(KeyCode::F(7), KeyModifiers::ALT));
        form(&mut app).clone()
    };
    app.overlay = None;
    find.set_edit_text(1, "*.zz");
    let drive = (find.items.iter())
        .position(
            |item| matches!(&item.control, Control::Button { label, .. } if label == "Dri&ve"),
        )
        .unwrap();
    assert!(!app.closes_for_good(&crate::app::FormAction::FindFile, &find, drive));
    let find_button = (find.items.iter())
        .position(|item| matches!(&item.control, Control::Button { label, .. } if label == "Find"))
        .unwrap();
    find.set_edit_text(1, "|");
    assert!(
        !app.closes_for_good(&crate::app::FormAction::FindFile, &find, find_button),
        "a mask with errors"
    );
    find.set_edit_text(1, "*.zz");
    assert!(app.closes_for_good(&crate::app::FormAction::FindFile, &find, find_button));
    find.history_mut(1)
        .unwrap()
        .changes
        .push(HistoryChange::Clear);
    app.keep_form_histories(&mut find, false);
    assert!(find.history(1).unwrap().changes.is_empty(), "done once");
}
