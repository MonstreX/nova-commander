use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::*;

/// A disabled edit line and a masked line wider than its mask.
fn form() -> Form {
    let edit = Item::new(5, 2, Control::Edit(LineInput::with_text("ab")))
        .wide(6)
        .disabled(true);
    let masked = Item::new(5, 3, Control::Masked(MaskedInput::new("9999", "12"))).wide(8);
    Form::new("Fields", 30, 7, vec![edit, masked])
}

#[test]
fn a_disabled_field_has_fars_disabled_edit_colours_and_a_masked_one_its_width() {
    let mut terminal = Terminal::new(TestBackend::new(30, 7)).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, frame.area(), &form());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let colours = |x: u16, y: u16| (Some(buffer[(x, y)].fg), Some(buffer[(x, y)].bg));
    let of = |style: Style| (style.fg, style.bg);
    for x in 5..11 {
        assert_eq!(colours(x, 2), of(theme::input_field_disabled()), "x {x}");
    }
    assert_eq!(colours(12, 3), of(theme::input_field()), "past the mask");
    assert_ne!(colours(13, 3), of(theme::input_field()), "past the field");
}

#[test]
fn a_form_larger_than_the_screen_draws_what_fits() {
    let mut items: Vec<Item> = (0..100)
        .map(|line| Item::new(5, 2 + line, Control::Label(format!("line {line}"))))
        .collect();
    items.push(Item::new(60, 3, Control::Label("far right".to_string())));
    items.push(Item::new(0, 102, Control::Separator));
    let ok = Control::Button {
        label: "OK".to_string(),
        default: true,
    };
    items.push(Item::new(0, 103, ok.clone()));
    items.push(Item::new(50, 4, ok).placed(Place::At));
    let form = Form::new("Long", 80, 106, items);
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, frame.area(), &form);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row = |y: u16| (0..40).map(|x| buffer[(x, y)].symbol()).collect::<String>();
    assert!(row(2).contains("line 0"), "{}", row(2));
    assert!(
        !(0..10).any(|y| row(y).contains("OK")),
        "the buttons are past the screen"
    );
}

/// The buttons drawn in a box of `height` rows: a button on its last row, another on the row above.
fn buttons_in(height: u16, shadow: bool) -> Vec<usize> {
    let button = |label: &str| Control::Button {
        label: label.to_string(),
        default: false,
    };
    let form = Form::new(
        "B",
        30,
        height,
        vec![
            Item::new(0, height - 2, button("Above")),
            Item::new(0, height - 1, button("Last")),
        ],
    );
    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 20));
    let rect = Rect::new(2, 2, 30, height);
    let drawn = draw_buttons(&mut buffer, &form, rect, shadow);
    drawn.iter().map(|control| control.index).collect()
}

#[test]
fn a_password_line_shows_a_star_for_each_character() {
    let line = Item::new(5, 2, Control::Edit(LineInput::with_text("pässwörd")))
        .wide(12)
        .password();
    let form = Form::new("P", 30, 5, vec![line]);
    let mut terminal = Terminal::new(TestBackend::new(30, 5)).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, frame.area(), &form);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row: String = (0..30).map(|x| buffer[(x, 2)].symbol()).collect();
    assert!(row.contains("******** "), "{row}");
    assert!(!row.contains('s'), "{row}");
}

#[test]
fn a_button_whose_shadow_falls_past_the_box_is_not_drawn() {
    assert_eq!(buttons_in(8, false), [0, 1], "no shadow: the last row");
    assert_eq!(buttons_in(8, true), [0], "its shadow under the box");
}

#[test]
fn what_starts_right_of_the_box_is_not_drawn() {
    let past = Item::new(35, 3, Control::Label("past".to_string()));
    let inside = Item::new(5, 3, Control::Label("inside".to_string()));
    let button = Item::new(
        33,
        4,
        Control::Button {
            label: "Right".to_string(),
            default: false,
        },
    )
    .placed(Place::At);
    let form = Form::new("Narrow", 30, 7, vec![past, inside, button]);
    // The box from column 15 to 44: its right is on the screen.
    let mut terminal = Terminal::new(TestBackend::new(60, 7)).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, frame.area(), &form);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let screen: String = (0..7)
        .flat_map(|y| (0..60).map(move |x| (x, y)))
        .map(|cell| buffer[cell].symbol())
        .collect();
    assert!(screen.contains("inside"), "{screen}");
    assert!(!screen.contains("past"), "{screen}");
    assert!(!screen.contains("Right"), "{screen}");
}

/// The screen's rows after drawing `form` on 60 x 20.
fn rows_of(form: &Form) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
    terminal
        .draw(|frame| drop(draw(frame, frame.area(), form)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..20)
        .map(|y| (0..60).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn a_history_has_its_arrow_after_the_line_and_its_list_shows_dates_and_ampersands() {
    use crate::app::form::{EditHistory, HistoryLine};
    use crate::settings::history::{Entry, Kind};
    let line = |text: &str, day: &str| HistoryLine {
        entry: Entry::new(text, Kind::Plain),
        day: day.to_string(),
    };
    let mut history = EditHistory::new("Copy");
    history.entries = Some(vec![line("a&b", "14.11.2026"), line("c", "13.11.2026")]);
    let edit = Item::new(5, 2, Control::Edit(LineInput::with_text("x"))).wide(10);
    let mut form = Form::new("Copy", 30, 6, vec![edit.with_history(history)]);
    let rows = rows_of(&form);
    let row = rows.iter().find(|row| row.contains('x')).unwrap();
    let x = row.chars().position(|ch| ch == 'x').unwrap();
    assert_eq!(row.chars().nth(x + 10), Some('↓'), "{row}");
    form.handle_key(
        ratatui::crossterm::event::KeyEvent::new(KeyCode::Down, KeyModifiers::ALT),
        &mut crate::app::clipboard::Memory::default(),
    );
    let rows = rows_of(&form);
    let text = rows.join("\n");
    assert!(
        text.contains(" 14.11.2026 ") && text.contains(" 13.11.2026 "),
        "{text}"
    );
    assert!(text.contains("a&b"), "shown as it is: {text}");
    form.history_mut(0).unwrap().confirm_clear = true;
    form.handle_key(
        ratatui::crossterm::event::KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
        &mut crate::app::clipboard::Memory::default(),
    );
    let text = rows_of(&form).join("\n");
    assert!(
        text.contains("All records in the history will be deleted"),
        "{text}"
    );
}

#[test]
fn a_list_box_draws_its_lines_in_its_box_under_its_title_with_the_cursor_line_marked() {
    use crate::app::form::ListBox;
    let lines = ["&One", "Two", "Three", "Four"]
        .iter()
        .map(|line| ListLine::Choice((*line).to_string()))
        .collect();
    let list = ListBox::new("Box", lines, 3, 2);
    let form = Form::new(
        "Lists",
        30,
        8,
        vec![Item::new(5, 2, Control::ListBox(list)).wide(14)],
    );
    let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
    let mut drawn = None;
    terminal
        .draw(|frame| drawn = Some(draw(frame, frame.area(), &form).1))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row = |y: u16| -> String { (5..19).map(|x| buffer[(x, y)].symbol()).collect() };
    assert!(row(2).contains(" Box "), "{}", row(2));
    assert!(
        row(2).starts_with('╔'),
        "focused: FAR's double box {}",
        row(2)
    );
    assert!(
        row(3).contains("Three") && row(4).contains("Four"),
        "scrolled to the cursor"
    );
    assert!(row(3).contains("Three") && !row(3).contains("&"));
    let colours = |y: u16| (buffer[(7, y)].fg, buffer[(7, y)].bg);
    let of = |style: Style| (style.fg.unwrap(), style.bg.unwrap());
    assert_eq!(
        colours(4),
        of(theme::dialog_listbox(true)),
        "the cursor line"
    );
    assert_eq!(colours(3), of(theme::dialog_listbox(false)));
    let control = (drawn.unwrap().dialog.controls.into_iter())
        .find(|control| matches!(control.kind, ControlKind::ListBox { .. }))
        .unwrap();
    assert_eq!(control.kind, ControlKind::ListBox { top: 2 });
    assert_eq!(
        (control.cells.x, control.cells.y, control.cells.height),
        (6, 3, 2)
    );
}

#[test]
fn a_list_box_without_the_focus_has_a_single_box() {
    use crate::app::form::ListBox;
    let list = ListBox::new("", vec![ListLine::Choice("One".to_string())], 0, 1);
    let line = Item::new(5, 5, Control::Edit(LineInput::with_text(""))).wide(10);
    let mut form = Form::new(
        "Lists",
        30,
        8,
        vec![Item::new(5, 2, Control::ListBox(list)).wide(14), line],
    );
    form.focus_item(1);
    let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, frame.area(), &form);
        })
        .unwrap();
    assert_eq!(terminal.backend().buffer()[(5, 2)].symbol(), "┌");
}

#[test]
fn a_list_box_past_the_screens_bottom_is_drawn_as_far_as_it_goes() {
    use crate::app::form::ListBox;
    let lines = (0..40)
        .map(|line| ListLine::Choice(format!("line {line}")))
        .collect();
    let list = ListBox::new("Tall", lines, 30, 40);
    let form = Form::new(
        "Lists",
        30,
        8,
        vec![Item::new(5, 2, Control::ListBox(list)).wide(14)],
    );
    let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, frame.area(), &form);
        })
        .unwrap();
    let row: String = (5..19)
        .map(|x| terminal.backend().buffer()[(x, 3)].symbol())
        .collect();
    assert!(row.contains("line"), "{row}");
}
