//! FAR's "Show scrollbar" on the panels: the files' bar beside their list, the tree's from under its top
//! frame (the mouse on them is `app::mouse_scroll_bar`'s).

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::settings::Settings;

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

/// The column `x` of a screen `render` gave, top to bottom.
fn column_of(screen: &str, x: usize) -> String {
    (screen.lines())
        .map(|line| line.chars().nth(x).unwrap_or(' '))
        .collect()
}

#[test]
fn show_scrollbar_draws_the_files_and_the_trees_bar_on_the_right_frame() {
    let temp = tempfile::tempdir().unwrap();
    for index in 0..60 {
        std::fs::create_dir(temp.path().join(format!("d{index:02}"))).unwrap();
    }
    let mut app = App::new(temp.path(), 80, 20, &Settings::default());
    let screen = render(&mut app, 80, 20);
    assert!(!column_of(&screen, 39).contains('▲'), "FAR's default: none");
    app.prefs.panel_scrollbar = true;
    let screen = render(&mut app, 80, 20);
    let files = column_of(&screen, 39);
    let list = app.mouse.areas[0].clone().unwrap().list.unwrap();
    let bar: String = (files.chars())
        .skip(usize::from(list.top))
        .take(usize::from(list.rows))
        .collect();
    assert!(
        bar.starts_with('▲') && bar.ends_with('▼') && bar.contains('█'),
        "{files}"
    );
    app.active = crate::app::Side::Right;
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        app.tick();
        let screen = render(&mut app, 80, 20);
        let tree = column_of(&screen, 39);
        // From under the top frame, the height less 4 (FAR's tree).
        let height = app.mouse.areas[0].as_ref().map_or(4, |area| area.height);
        let bar: String = tree.chars().skip(1).take(usize::from(height - 4)).collect();
        if bar.starts_with('▲') && bar.ends_with('▼') {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the tree's bar: {tree}"
        );
        std::thread::yield_now();
    }
}

#[test]
fn a_menu_longer_than_its_box_has_the_bar_on_its_right_side_unless_turned_off() {
    let texts: Vec<String> = (0..30).map(|n| format!("item {n}")).collect();
    let lines: Vec<vmenu::Line> = (texts.iter())
        .map(|text| vmenu::Line {
            label: text,
            key: "",
            check: None,
            separator: false,
            disabled: false,
        })
        .collect();
    let menu = vmenu::Menu {
        lines: &lines[5..13],
        selected: 0,
        title: "",
        bottom_title: "",
        scroll: Some((5, 30)),
    };
    let rect = ratatui::layout::Rect::new(0, 0, 20, 10);
    let side = |buffer: &ratatui::buffer::Buffer| -> String {
        (1..9)
            .map(|y| buffer[(19, y)].symbol().to_string())
            .collect()
    };
    let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 30, 12));
    let screen = buffer.area;
    vmenu::draw(&mut buffer, rect, &menu, screen);
    assert_eq!(
        side(&buffer),
        super::form::scroll_bar(8, 5, 13, 30)
            .into_iter()
            .collect::<String>()
    );
    vmenu::show_scrollbars(false);
    let mut plain = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 30, 12));
    vmenu::draw(&mut plain, rect, &menu, screen);
    vmenu::show_scrollbars(true);
    assert!(!side(&plain).contains('▲'), "{}", side(&plain));
}

#[test]
fn a_list_dialog_scrolls_to_its_cursor_and_shows_its_bar() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::new(temp.path(), 60, 12, &Settings::default());
    app.show_dialog(crate::app::Dialog::List {
        title: "Screens".to_string(),
        items: (0..30).map(|n| format!("screen {n}")).collect(),
        cursor: 25,
        top: Default::default(),
        action: crate::app::ListAction::Theme,
    });
    let screen = render(&mut app, 60, 12);
    assert!(
        screen.contains("screen 25") && !screen.contains("screen 3\n"),
        "{screen}"
    );
    assert!(screen.contains('▲') && screen.contains('▼'), "{screen}");
    let first = app.mouse.menu.unwrap().first;
    assert!(
        first > 0 && first <= 25,
        "the mouse knows the first line: {first}"
    );
}

#[test]
fn the_sort_menu_has_vmenu2s_margin_around_its_box() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = App::new(temp.path(), 100, 30, &Settings::default());
    app.execute(crate::app::Command::SortMenu(crate::app::Side::Left));
    let screen = render(&mut app, 100, 30);
    let area = app.mouse.menu.expect("the menu's place");
    assert_eq!(
        (area.outer.x + 2, area.outer.y + 1),
        (area.frame.x, area.frame.y)
    );
    let row: Vec<char> = screen
        .lines()
        .nth(usize::from(area.frame.y))
        .unwrap()
        .chars()
        .collect();
    assert_eq!(
        row[usize::from(area.outer.x)],
        ' ',
        "the margin: {}",
        screen
    );
    assert_eq!(row[usize::from(area.frame.x)], '╔', "{screen}");
}
