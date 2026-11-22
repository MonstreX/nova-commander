use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::clipboard::Memory;
use crate::app::form::{Control, Form, FormOutcome, Item};
use crate::app::line_input::LineInput;
use crate::settings::history::{Entry, Kind};

fn line(text: &str, day: &str, locked: bool) -> HistoryLine {
    HistoryLine {
        entry: Entry {
            locked,
            ..Entry::new(text, Kind::Plain)
        },
        day: day.to_string(),
    }
}

/// An edit line with the history "Masks", newest first: two lines of one day, one of the day before.
fn form(text: &str) -> Form {
    let mut history = EditHistory::new("Masks").use_last();
    history.entries = Some(vec![
        line("*.md", "14.11.2026", false),
        line("*.rs", "14.11.2026", true),
        line("*.txt", "13.11.2026", false),
    ]);
    history.confirm_clear = false;
    let edit = Item::new(5, 2, Control::Edit(LineInput::with_text(text)))
        .wide(30)
        .with_history(history);
    let ok = Item::new(
        0,
        4,
        Control::Button {
            label: "OK".to_string(),
            default: true,
        },
    );
    Form::new("Find file", 40, 7, vec![edit, ok])
}

fn press(form: &mut Form, code: KeyCode, modifiers: KeyModifiers) -> FormOutcome {
    form.handle_key(KeyEvent::new(code, modifiers), &mut Memory::default())
}

fn open(form: &mut Form) {
    press(form, KeyCode::Down, KeyModifiers::CONTROL);
    assert!(form.open_history().is_some(), "Ctrl+Down opens the history");
}

#[test]
fn the_list_has_a_date_before_each_day_and_the_cursor_on_the_newest_line() {
    let mut form = form("");
    open(&mut form);
    let history = form.open_history().unwrap();
    let shown = history.lines();
    let texts: Vec<String> = (shown.lines.iter())
        .map(|line| match line {
            ListLine::Section(day) => format!("-{day}-"),
            ListLine::Marked { label, checked } => {
                format!("{label}{}", if *checked { "√" } else { "" })
            }
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        texts,
        ["-14.11.2026-", "*.md", "*.rs√", "-13.11.2026-", "*.txt"]
    );
    assert_eq!(history.open.as_ref().unwrap().cursor, 1, "off the date");
}

#[test]
fn enter_and_tab_put_the_line_into_the_edit_and_esc_leaves_it() {
    let mut form = form("typed");
    open(&mut form);
    press(&mut form, KeyCode::Down, KeyModifiers::NONE);
    press(&mut form, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(
        press(&mut form, KeyCode::Enter, KeyModifiers::NONE),
        FormOutcome::Stay
    );
    assert_eq!(
        (form.edit_text(0), form.open_history()),
        ("*.txt", None),
        "over the date"
    );
    open(&mut form);
    press(&mut form, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(form.edit_text(0), "*.txt");
    open(&mut form);
    press(&mut form, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(form.edit_text(0), "*.md", "FAR's Tab there is Enter");
    assert_eq!(
        press(&mut form, KeyCode::Esc, KeyModifiers::NONE),
        FormOutcome::Cancel
    );
}

#[test]
fn ins_locks_shift_del_takes_out_an_unlocked_line_and_del_the_unlocked_ones() {
    let mut form = form("");
    open(&mut form);
    press(&mut form, KeyCode::Insert, KeyModifiers::NONE);
    press(&mut form, KeyCode::Down, KeyModifiers::NONE);
    press(&mut form, KeyCode::Delete, KeyModifiers::SHIFT);
    let history = form.history(0).unwrap();
    assert_eq!(
        history.entries.as_ref().unwrap().len(),
        3,
        "a locked line stays"
    );
    press(&mut form, KeyCode::Down, KeyModifiers::NONE);
    press(&mut form, KeyCode::Delete, KeyModifiers::SHIFT);
    let texts = |form: &Form| -> Vec<String> {
        (form.history(0).unwrap().entries.iter().flatten())
            .map(|line| line.entry.text.clone())
            .collect()
    };
    assert_eq!(texts(&form), ["*.md", "*.rs"]);
    press(&mut form, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(texts(&form), ["*.md", "*.rs"], "both locked now");
    let changes = &form.history(0).unwrap().changes;
    assert!(
        matches!(&changes[..], [
        HistoryChange::Lock(md),
        HistoryChange::Remove(txt),
        HistoryChange::Clear,
    ] if md.locked && md.text == "*.md" && txt.text == "*.txt"),
        "{changes:?}"
    );
}

#[test]
fn del_asks_first_while_clearing_is_confirmed_and_an_emptied_list_closes() {
    let mut form = form("");
    form.history_mut(0).unwrap().confirm_clear = true;
    if let Some(entries) = &mut form.history_mut(0).unwrap().entries {
        entries
            .iter_mut()
            .for_each(|line| line.entry.locked = false);
    }
    open(&mut form);
    press(&mut form, KeyCode::Delete, KeyModifiers::NONE);
    assert!(form.open_history().unwrap().question.is_some());
    press(&mut form, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(
        form.history(0).unwrap().entries.as_ref().unwrap().len(),
        3,
        "Esc: nothing goes"
    );
    press(&mut form, KeyCode::Delete, KeyModifiers::NONE);
    press(&mut form, KeyCode::Enter, KeyModifiers::NONE);
    let history = form.history(0).unwrap();
    assert!(history.entries.as_ref().unwrap().is_empty() && history.open.is_none());
    press(&mut form, KeyCode::Down, KeyModifiers::ALT);
    assert!(form.open_history().is_none(), "nothing to open");
}

#[test]
fn ctrl_ins_copies_the_line_and_the_last_line_fills_an_empty_edit() {
    let mut form = form("");
    open(&mut form);
    let mut clipboard = Memory::default();
    let key = KeyEvent::new(KeyCode::Insert, KeyModifiers::CONTROL);
    form.handle_key(key, &mut clipboard);
    assert_eq!(clipboard.contents().text.as_deref(), Some("*.md"));
    assert!(
        !form.history(0).unwrap().entries.as_ref().unwrap()[0]
            .entry
            .locked
    );
    form.fill_last_history();
    assert_eq!(form.edit_text(0), "*.md");
    let mut typed = self::form("mine");
    typed.fill_last_history();
    assert_eq!(typed.edit_text(0), "mine", "only an empty line");
}

#[test]
fn a_read_only_line_or_one_without_histories_opens_nothing() {
    let mut form = form("");
    form.items[0].read_only = true;
    press(&mut form, KeyCode::Down, KeyModifiers::CONTROL);
    assert!(form.open_history().is_none());
    let mut off = self::form("");
    off.history_mut(0).unwrap().entries = None;
    assert!(!off.history(0).unwrap().has_arrow());
    press(&mut off, KeyCode::Down, KeyModifiers::CONTROL);
    assert!(off.open_history().is_none());
}
