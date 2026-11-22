//! The main screen drawn whole: panels, command line, key bar, overlays.

use super::*;
use crate::settings::Settings;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn render(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..height {
        for x in 0..width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn app(width: u16, height: u16) -> (tempfile::TempDir, App) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("folder")).unwrap();
    std::fs::write(temp.path().join("file.txt"), "text").unwrap();
    let app = App::new(temp.path(), width, height, &Settings::default());
    (temp, app)
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn main_screen_shows_far_layout() {
    let (_temp, mut app) = app(100, 30);
    // FAR's default Medium mode: two name columns split by a double line, status line below.
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("╔"));
    assert!(screen.contains("║"));
    assert!(screen.contains("Name"));
    assert!(screen.contains("  Up  "), "status line of the parent entry");
    assert!(screen.contains("file.txt"));
    assert!(screen.contains("Bytes: 4 B, files: 1, folders: 1"));
    assert!(screen.contains("1Help"));
    assert!(screen.contains("10Quit"));
    // Ctrl+3: Full mode with the size column.
    app.handle_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::CONTROL));
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("Size"));
    assert!(screen.contains("Folder"));
}

#[test]
fn menu_and_dialogs_render() {
    let (_temp, mut app) = app(100, 30);
    press(&mut app, KeyCode::F(9));
    assert!(render(&mut app, 100, 30).contains("Change drive"));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::F(7));
    assert!(render(&mut app, 100, 30).contains("Make folder"));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::F(1));
    assert!(render(&mut app, 100, 30).contains("switch panel"));
}

#[test]
fn viewer_and_editor_render() {
    let (_temp, mut app) = app(80, 25);
    app.active_panel_mut().end();
    press(&mut app, KeyCode::F(3));
    // Its blocks come from a thread of their own (ADR-0015): drawn as they come.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !render(&mut app, 80, 25).contains("text") {
        assert!(std::time::Instant::now() < deadline, "the text never came");
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        render(&mut app, 80, 25).contains("│t│"),
        "FAR's status line"
    );
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::F(4));
    assert!(render(&mut app, 80, 25).contains("Save"));
}

#[test]
fn tiny_and_odd_sizes_do_not_panic() {
    for (width, height) in [(1, 1), (10, 3), (20, 5), (39, 12), (200, 60)] {
        let (_temp, mut app) = app(width, height);
        render(&mut app, width, height);
        press(&mut app, KeyCode::F(9));
        render(&mut app, width, height);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::F(7));
        render(&mut app, width, height);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::F(10));
        render(&mut app, width, height);
    }
}

#[test]
fn detailed_mode_takes_the_whole_width_while_active() {
    let (_temp, mut app) = app(120, 30);
    app.handle_key(KeyEvent::new(KeyCode::Char('5'), KeyModifiers::CONTROL));
    let screen = render(&mut app, 120, 30);
    let top = screen.lines().next().unwrap_or_default();
    assert!(top.starts_with('╔'));
    assert_eq!(top.matches('╔').count(), 1, "one panel spans the screen");
    // "Allocated" cut to the 6-wide column, as FAR's fit_to_center does.
    assert!(screen.contains("Alloca"));
    assert!(screen.contains("Write"));
    assert!(screen.contains("Created"));
    assert!(screen.contains("Accessed"));
    assert!(screen.contains("Attr"));
    // The other panel keeps its own mode and is drawn again when it becomes active.
    press(&mut app, KeyCode::Tab);
    let screen = render(&mut app, 120, 30);
    assert_eq!(
        screen
            .lines()
            .next()
            .unwrap_or_default()
            .matches('╔')
            .count(),
        2
    );
}

#[test]
fn clock_sits_in_the_top_right_corner() {
    let (_temp, mut app) = app(100, 30);
    let screen = render(&mut app, 100, 30);
    let top: Vec<char> = screen.lines().next().unwrap_or_default().chars().collect();
    let clock: String = top[top.len() - 5..].iter().collect();
    assert_eq!(
        clock.chars().nth(2),
        Some(crate::platform::locale::date_format().time_separator)
    );
    assert!(
        clock.chars().filter(char::is_ascii_digit).count() == 4,
        "{clock}"
    );
}

/// The F9 menu as FAR draws it (`HMenu::ShowMenu`, `VMenu`, `DropShadow`).
#[test]
fn f9_menu_matches_far_layout() {
    let (_temp, mut app) = app(100, 30);
    press(&mut app, KeyCode::F(9));
    let mut terminal = ratatui::Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let row = |y: u16| -> String { (0..100).map(|x| buffer[(x, y)].symbol()).collect() };

    assert!(row(0).starts_with("    Left    Files    Commands    Options    Right    "));
    assert_eq!(buffer[(0, 0)].style().bg, Some(ratatui::style::Color::Cyan));
    // The Left menu opens at its title's x (2).
    assert_eq!(buffer[(2, 1)].symbol(), "╔");
    assert!(row(3).contains("║√ Medium"), "{}", row(3));
    assert!(row(8).contains("╟────"), "separator row: {}", row(8));
    // The bottom frame and the shadow: two columns right and one row below.
    let bottom = (1..30)
        .find(|&y| buffer[(2, y)].symbol() == "╚")
        .unwrap_or(0);
    let right = (2..100)
        .find(|&x| buffer[(x, 1)].symbol() == "╗")
        .unwrap_or(0);
    let shadow = buffer[(right + 1, 2)].style();
    assert_eq!(shadow.fg, Some(ratatui::style::Color::DarkGray));
    assert_eq!(shadow.bg, Some(ratatui::style::Color::Black));
    assert_eq!(
        buffer[(4, bottom + 1)].style().bg,
        Some(ratatui::style::Color::Black)
    );
}

#[test]
fn held_modifiers_switch_the_key_bar_labels() {
    let (_temp, mut app) = app(100, 30);
    let bottom = |app: &mut App| {
        render(app, 100, 30)
            .lines()
            .nth(29)
            .unwrap_or_default()
            .to_string()
    };
    assert!(bottom(&mut app).starts_with("1Help"));
    let ctrl = crate::platform::input::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    assert!(app.set_held_modifiers(ctrl), "a change asks for a redraw");
    assert!(!app.set_held_modifiers(ctrl), "the same state does not");
    let line = bottom(&mut app);
    assert!(line.starts_with("1Left"), "{line}");
    assert!(line.contains("3Name"));
    app.set_held_modifiers(crate::platform::input::Modifiers::default());
    assert!(bottom(&mut app).contains("2UserMn"));
}

#[test]
fn quick_search_box_sits_on_the_active_panel_bottom_frame() {
    let (_temp, mut app) = app(100, 30);
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT));
    let mut terminal = ratatui::Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let row = |y: u16| -> String { (0..100).map(|x| buffer[(x, y)].symbol()).collect() };
    // FAR: the box's top is the panel's bottom frame (row 27); it covers the command line and key bar.
    assert!(row(27).contains("╔══════ Search ══════╗"), "{}", row(27));
    assert!(row(28).contains("║ f"), "{}", row(28));
    assert_eq!(
        buffer[(9, 27)].symbol(),
        "╔",
        "nine columns into the left panel"
    );
    assert_eq!(
        buffer[(11, 28)].style().bg,
        Some(ratatui::style::Color::Cyan)
    );
}

#[test]
fn sort_menu_is_a_far_popup_with_direction_marks() {
    let (_temp, mut app) = app(100, 30);
    app.handle_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::CONTROL));
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains(" Sort by "));
    assert!(screen.contains("▲ Name"), "current mode, ascending");
    assert!(screen.contains("Write time"));
    assert!(screen.contains("Shift+F12"));
    assert!(screen.contains("√ Show directories first"));
    assert!(screen.contains(" + - * "));
}

#[test]
fn the_right_menu_is_moved_left_to_stay_on_a_narrow_screen() {
    let (_temp, mut app) = app(60, 25);
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    press(&mut app, KeyCode::F(9));
    let screen = render(&mut app, 60, 25);
    let line = screen.lines().nth(1).unwrap_or_default();
    // The menu ends at ScrX - 4 = 55, so "╗" is the 56th character.
    assert_eq!(line.chars().nth(55), Some('╗'), "{line}");
}

#[test]
fn view_modes_are_in_the_panel_menus() {
    let (_temp, mut app) = app(100, 30);
    press(&mut app, KeyCode::F(9));
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("Brief"));
    assert!(screen.contains("Alternative full"));
    assert!(screen.contains("Ctrl+0"));
    // Down to "Full" (Brief, Medium, Full) and Enter.
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.active_panel().view_mode, crate::app::ViewMode::Full);
}

#[test]
fn ctrl_o_hides_the_panels_over_the_user_screen_and_brings_them_back() {
    let (temp, mut app) = app(100, 30);
    let ctrl_o = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
    app.handle_key(ctrl_o);
    assert_eq!(app.main_view, MainView::Panels);
    let screen = render(&mut app, 100, 30);
    assert!(!screen.contains("╔"));
    assert!(screen.contains("1Help"), "FAR keeps the key bar");
    assert!(screen.contains(&format!("{}>", temp.path().display())));
    app.handle_key(ctrl_o);
    assert!(render(&mut app, 100, 30).contains("╔"));
}

#[test]
fn the_guest_shell_fills_the_console_between_the_programs_own_rows() {
    let (_temp, mut app) = app(100, 30);
    let alt_shift_t = KeyEvent::new(KeyCode::Char('T'), KeyModifiers::ALT | KeyModifiers::SHIFT);
    app.handle_key(alt_shift_t);
    assert_eq!(app.main_view, MainView::Guest);
    let screen = render(&mut app, 100, 30);
    assert!(!screen.contains("╔"));
    assert!(!screen.contains("1Help"));
    let lines: Vec<&str> = screen.lines().collect();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(
        buffer[(0, 0)].bg,
        ratatui::style::Color::Reset,
        "no colour left of the clock"
    );
    assert_eq!(buffer[(99, 0)].bg, theme::clock().bg.unwrap_or_default());
    assert_eq!(
        lines[0].trim().len(),
        5,
        "only the clock on top: {}",
        lines[0]
    );
    assert_eq!(
        lines[0].trim_end().chars().nth_back(2),
        Some(':'),
        "the clock"
    );
    let shell = app.guest.as_ref().expect("the guest shell runs");
    assert!(lines[29].contains(shell.shell().title()), "{}", lines[29]);
    assert!(lines[29].contains("Alt+Shift+T Panels"), "{}", lines[29]);
    assert_eq!(
        shell.screen().size().0.get(),
        28,
        "two rows are the program's"
    );
    app.handle_key(alt_shift_t);
    assert_eq!(app.main_view, MainView::Panels);
    assert!(render(&mut app, 100, 30).contains("1Help"));
}

#[test]
fn panels_fill_the_height_down_to_the_command_line() {
    let (_temp, mut app) = app(100, 30);
    let screen = render(&mut app, 100, 30);
    let lines: Vec<&str> = screen.lines().collect();
    assert!(lines[0].starts_with('╔'));
    assert!(
        lines[27].starts_with('╚'),
        "bottom frame right above the command line"
    );
    assert!(lines[29].starts_with("1Help"));
    // Ctrl+Up uncovers one terminal line between the panels and the command line.
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL));
    let screen = render(&mut app, 100, 30);
    assert!(screen.lines().nth(26).unwrap_or_default().starts_with('╚'));
}

#[test]
fn a_panel_hides_the_shell_output_behind_it_even_where_it_lists_nothing() {
    // "§": a character the random temporary folder name in the panel title never has.
    let (_temp, mut app) = app(60, 20);
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            for y in 0..area.height {
                frame
                    .buffer_mut()
                    .set_string(0, y, "§".repeat(60), ratatui::style::Style::new());
            }
            let look = panel::Look {
                focused: true,
                highlighting: None,
                now: std::time::SystemTime::now(),
                mode: app.panel_modes.spec(app.panels[0].view_mode),
            };
            panel::draw(frame, area, &mut app.panels[0], &look, 60);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(buffer.content().iter().all(|cell| cell.symbol() != "§"));
}

#[test]
fn hidden_panels_split_and_key_bar_follow_the_far_keys() {
    let (_temp, mut app) = app(100, 30);
    let ctrl = |app: &mut App, code: KeyCode| {
        app.handle_key(KeyEvent::new(code, KeyModifiers::CONTROL));
    };
    // Ctrl+Left: the left panel one column narrower, the right one starts a column earlier.
    ctrl(&mut app, KeyCode::Left);
    let screen = render(&mut app, 100, 30);
    let top: Vec<char> = screen.lines().next().unwrap_or_default().chars().collect();
    assert_eq!(top[48], '╗');
    assert_eq!(top[49], '╔');
    // Ctrl+F1: the left half shows what lies behind (the shell, blank here).
    ctrl(&mut app, KeyCode::F(1));
    let screen = render(&mut app, 100, 30);
    let top: Vec<char> = screen.lines().next().unwrap_or_default().chars().collect();
    assert_eq!(top[0], ' ');
    assert_eq!(top[49], '╔');
    // Ctrl+B: no key bar, the command line moves to the last row.
    ctrl(&mut app, KeyCode::Char('b'));
    let screen = render(&mut app, 100, 30);
    let lines: Vec<&str> = screen.lines().collect();
    assert!(!screen.contains("1Help"));
    assert!(lines[29].contains('>'), "the command line on the last row");
    assert!(lines[28].contains('╚'));
}

#[test]
fn f5_shows_fars_copy_dialog() {
    let (_temp, mut app) = app(100, 30);
    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::F(5));
    let screen = render(&mut app, 100, 30);
    let rows: Vec<&str> = screen.lines().collect();
    let top = rows.iter().position(|row| row.contains(" Copy ")).unwrap();
    let expected = [
        "Copy \"file.txt\" to:",
        "",
        "",
        "Access rights: (•) Default ( ) Copy ( ) Inherit",
        "",
        "Already existing files: Ask",
        "[ ] Preserve all timestamps",
        "[ ] Copy contents of symbolic links",
        "[ ] Process multiple destinations",
        "",
        "[ ] Use filter",
        "",
        "{ Copy } [ F10-Tree ] [ Filter ] [ Cancel ]",
    ];
    for (offset, text) in expected.iter().enumerate() {
        if !text.is_empty() {
            let row = rows[top + 1 + offset];
            assert!(row.contains(text), "row {offset}: {row:?}");
        }
    }
    assert!(rows[top + 3].contains("╟───"), "{}", rows[top + 3]);
}

#[test]
fn the_panels_are_where_the_mouse_finds_them() {
    let (_temp, mut app) = app(100, 30);
    render(&mut app, 100, 30);
    let left = app.mouse.areas[0].clone().unwrap();
    assert_eq!((left.x, left.y, left.width, left.height), (0, 0, 50, 28));
    let list = left.list.unwrap();
    assert_eq!((list.top, list.rows), (2, 23));
    assert_eq!(list.stripe_lefts.len(), app.panels[0].stripes());
    assert_eq!(list.stripe_lefts[0], 1);
    assert_eq!(app.mouse.areas[1].clone().unwrap().x, 50);
    // Hidden panels are nowhere.
    app.toggle_panels();
    render(&mut app, 100, 30);
    assert_eq!(app.mouse.areas, [None, None]);
}

/// The text inside the reported frame on the row of item `index`.
fn menu_row(app: &App, buffer: &ratatui::buffer::Buffer, index: usize) -> String {
    let area = app.mouse.menu.unwrap();
    let frame = area.frame;
    let y = frame.y + 1 + u16::try_from(index - area.first).unwrap();
    (frame.x + 1..frame.x + frame.width - 1)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

#[test]
fn the_mouse_finds_the_items_of_a_popup_and_a_list_where_they_are_drawn() {
    let (_temp, mut app) = app(100, 30);
    app.execute(crate::app::Command::SortMenu(crate::app::Side::Left));
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let Some(Overlay::Popup(popup)) = &app.overlay else {
        panic!("no sort menu");
    };
    let label = popup.items[1].label.replace('&', "");
    let row = menu_row(&app, terminal.backend().buffer(), 1);
    assert!(row.contains(&label), "{row} / {label}");
    app.overlay = None;
    app.execute(crate::app::Command::Screens);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let row = menu_row(&app, terminal.backend().buffer(), 0);
    assert!(row.contains("Panels"), "{row}");
    let area = app.mouse.menu.unwrap();
    assert_eq!(area.frame, area.outer);
}

/// The text the screen shows in a control's cells.
fn control_text(buffer: &ratatui::buffer::Buffer, cells: crate::app::Cells) -> String {
    (cells.x..cells.x + cells.width)
        .map(|x| buffer[(x, cells.y)].symbol())
        .collect()
}

#[test]
fn the_mouse_finds_the_buttons_and_fields_of_dialogs_where_they_are_drawn() {
    let (_temp, mut app) = app(100, 30);
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    app.execute(crate::app::Command::Quit);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let area = app.mouse.dialog.clone().unwrap();
    let texts: Vec<String> = area
        .controls
        .iter()
        .map(|control| control_text(terminal.backend().buffer(), control.cells))
        .collect();
    assert_eq!(texts.len(), 2);
    assert!(
        texts[0].contains("Yes") && texts[1].contains("No"),
        "{texts:?}"
    );
    assert!(
        area.controls
            .iter()
            .all(|c| area.frame.contains(c.cells.x, c.cells.y))
    );
    // A form: the question before deleting the file (to the Recycle Bin: Move).
    app.overlay = None;
    let file = app.panels[0]
        .entries
        .iter()
        .position(|entry| entry.name == "file.txt")
        .unwrap();
    app.panels[0].cursor = file;
    app.execute(crate::app::Command::Delete);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let area = app.mouse.dialog.clone().unwrap();
    let buttons: Vec<String> = area
        .controls
        .iter()
        .filter(|control| control.kind == crate::app::ControlKind::Button)
        .map(|control| control_text(terminal.backend().buffer(), control.cells))
        .collect();
    assert!(
        buttons
            .iter()
            .any(|text| text.contains("Delete") || text.contains("Move")),
        "{buttons:?}"
    );
    assert!(
        buttons.iter().any(|text| text.contains("Cancel")),
        "{buttons:?}"
    );
}

#[test]
fn a_button_standing_at_its_own_column_is_found_too() {
    use crate::app::form::{Control, Form, Item, Place};
    let (_temp, mut app) = app(100, 30);
    let mut more = Item::new(
        5,
        3,
        Control::Button {
            label: "More".to_string(),
            default: false,
        },
    );
    more.place = Place::At;
    app.show_dialog(crate::app::Dialog::Form {
        form: Form::new("Test", 40, 7, vec![more]),
        action: crate::app::FormAction::DriveColumns {
            side: crate::app::Side::Left,
            selected: 0,
        },
    });
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let area = app.mouse.dialog.clone().unwrap();
    assert_eq!(area.controls.len(), 1);
    let text = control_text(terminal.backend().buffer(), area.controls[0].cells);
    assert!(text.contains("More"), "{text}");
}

#[test]
fn the_menu_checks_the_view_or_the_info_panel_of_its_side() {
    let (_temp, mut app) = app(100, 30);
    app.execute(crate::app::Command::OpenMenu);
    let row_of = |text: &str, wanted: &str| -> String {
        text.lines()
            .find(|line| line.contains(wanted))
            .unwrap_or_default()
            .to_string()
    };
    let screen = render(&mut app, 100, 30);
    assert!(row_of(&screen, "Medium").contains('√'), "{screen}");
    app.overlay = None;
    app.execute(crate::app::Command::InfoPanel(crate::app::Side::Left));
    app.execute(crate::app::Command::OpenMenu);
    let screen = render(&mut app, 100, 30);
    assert!(row_of(&screen, "Info panel").contains('√'), "{screen}");
    assert!(!row_of(&screen, "Medium").contains('√'), "{screen}");
}

#[test]
fn the_copy_dialog_waits_under_its_tree() {
    use crate::app::tree_panel::{FolderTree, Purpose, TreePanel};
    let (temp, mut app) = app(100, 30);
    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::F(5));
    let Some(Overlay::Dialog(dialog)) = app.overlay.take() else {
        panic!("no copy dialog")
    };
    let tree = TreePanel::showing(crate::app::Side::Left, temp.path(), vec![]);
    let mut found = FolderTree::showing(tree, "");
    found.purpose = Purpose::Target(Box::new(dialog));
    app.overlay = Some(Overlay::FolderTree(Box::new(found)));
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains(" Find folder "), "{screen}");
    let rows: Vec<&str> = screen.lines().collect();
    let prompt = rows.iter().find(|row| row.contains("Access rights:"));
    assert!(prompt.is_some(), "the dialog's left part shows: {screen}");
}
