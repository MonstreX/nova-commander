use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::fileops::tests::app_in;

fn entry(name: &str, group: &str) -> TypeEntry {
    TypeEntry {
        id: 0,
        name: name.to_string(),
        group: group.to_string(),
        description: name.to_uppercase(),
    }
}

#[test]
fn the_type_menu_has_the_choices_the_favourites_then_each_group() {
    let types = [entry("a", "One"), entry("b", "One"), entry("c", "Two")];
    let settings = SyntaxTypeSettings {
        favorites: vec!["b".to_string()],
        hotkeys: [("c".to_string(), "C".to_string())].into(),
    };
    let (lines, entries) = type_lines(&types, &settings);
    let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "&A Automatic detection",
            "&D Disable",
            "Favorites",
            "  b: B",
            "One",
            "  a: A",
            "Two",
            "&C c: C",
        ]
    );
    let separators: Vec<usize> = (lines.iter().enumerate())
        .filter(|(_, line)| line.separator)
        .map(|(at, _)| at)
        .collect();
    assert_eq!(separators, [2, 4, 6]);
    assert_eq!(entries[3], TypeLine::Favorite("b".to_string()));
    assert_eq!(entries[7], TypeLine::Type("c".to_string()));
    assert_eq!(line_of(&entries, Some("a")), 5);
    assert_eq!(line_of(&entries, None), 1, "no type: Disable's line");
    assert_eq!(choosable_from(&entries, 4), 5, "past a group's title");
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// Ticks until `done` holds of the editor.
fn until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done(app) {
        assert!(Instant::now() < deadline, "never came");
        app.tick();
        if let Screen::Editor(editor) = &mut app.screen {
            editor.text.room.set((40, 5));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn highlighted(app: &App) -> bool {
    matches!(&app.screen, Screen::Editor(editor) if editor.text.highlighted())
}

fn menu_texts(app: &App) -> Vec<String> {
    match &app.overlay {
        Some(Overlay::PluginMenu(menu)) => menu.lines.iter().map(|l| l.text.clone()).collect(),
        _ => Vec::new(),
    }
}

/// The line of the menu up that has `text`, by Down from its first.
fn choose(app: &mut App, text: &str) {
    let texts = menu_texts(app);
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("a menu");
    };
    let at = (texts.iter())
        .position(|line| line.contains(text))
        .unwrap_or_else(|| panic!("{text} in {texts:?}"));
    let from = menu.selected;
    let (key, steps) = match at >= from {
        true => (KeyCode::Down, at - from),
        false => (KeyCode::Up, from - at),
    };
    for _ in 0..steps {
        press(app, key);
    }
    press(app, KeyCode::Enter);
}

#[test]
fn colorer_in_the_editor_s_f11_disables_and_chooses_the_type() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("m.rs");
    std::fs::write(&path, "fn main() {}\r\n").unwrap();
    let mut app = app_in(temp.path());
    app.edit_file(&path, false);
    until(&mut app, highlighted);
    press(&mut app, KeyCode::F(11));
    choose(&mut app, ITEM);
    let texts = menu_texts(&app);
    assert_eq!(texts.len(), FULL.len(), "the full menu: {texts:?}");
    choose(&mut app, "List types");
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("Select syntax");
    };
    assert_eq!(menu.title, "Select syntax");
    assert!(
        menu.lines[menu.selected].text.contains("rust:"),
        "on the type taken: {}",
        menu.lines[menu.selected].text
    );
    choose(&mut app, "Disable");
    until(&mut app, |app| !highlighted(app));
    press(&mut app, KeyCode::F(11));
    choose(&mut app, ITEM);
    let texts = menu_texts(&app);
    assert_eq!(texts.len(), 4, "no type: the short menu {texts:?}");
    choose(&mut app, "List types");
    choose(&mut app, "Automatic detection");
    until(&mut app, highlighted);
}

/// The stand's case: after "Reload schema library" another file of another type is coloured too.
#[test]
fn after_a_reload_a_file_of_another_type_is_coloured() {
    let temp = tempfile::tempdir().unwrap();
    let rust = temp.path().join("m.rs");
    std::fs::write(&rust, "fn main() {}\r\n").unwrap();
    let c = temp.path().join("x.c");
    std::fs::write(&c, "int one(void) {\r\n  return 1;\r\n}\r\n").unwrap();
    let mut app = app_in(temp.path());
    app.edit_file(&rust, false);
    until(&mut app, highlighted);
    press(&mut app, KeyCode::F(11));
    choose(&mut app, ITEM);
    choose(&mut app, "Reload schema library");
    until(&mut app, highlighted);
    app.edit_file(&c, false);
    until(&mut app, highlighted);
}

#[test]
fn ins_makes_a_type_a_favourite_and_f4_gives_it_a_hotkey() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("m.rs");
    std::fs::write(&path, "fn main() {}\r\n").unwrap();
    let mut app = app_in(temp.path());
    app.edit_file(&path, false);
    until(&mut app, highlighted);
    app.show_type_menu(Some("rust"));
    press(&mut app, KeyCode::Insert);
    assert_eq!(app.syntax_types.favorites, ["rust"]);
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("the menu back");
    };
    assert_eq!(menu.selected, 3, "on it among the favourites");
    press(&mut app, KeyCode::F(4));
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(_))),
        "Define key"
    );
    press(&mut app, KeyCode::Char('r'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.syntax_types.hotkeys.get("rust").map(String::as_str),
        Some("R")
    );
    assert!(menu_texts(&app)[3].starts_with("&R rust:"));
    press(&mut app, KeyCode::Delete);
    assert!(app.syntax_types.favorites.is_empty(), "Del takes it back");
}
