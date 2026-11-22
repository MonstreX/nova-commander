//! The code page menu as FAR's: its sections and lines, the favourites in and out, Ctrl+H's mode, the
//! user's names; the menu in the viewer.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::{App, Overlay, Screen};

fn texts(settings: &CodePageSettings) -> Vec<String> {
    let (lines, entries) = lines(settings);
    (lines.iter().zip(&entries))
        .map(|(line, entry)| match entry.kind {
            Kind::Separator => format!("-- {}", line.text),
            _ => line.text.clone(),
        })
        .collect()
}

/// A code page of "Other" on this system, if it has one.
fn other_page() -> Option<u32> {
    let (_, entries) = lines(&CodePageSettings::default());
    (entries.iter())
        .find(|entry| entry.kind == Kind::Normal)
        .and_then(|entry| entry.page)
}

#[test]
fn the_menu_is_far_s() {
    let shown = texts(&CodePageSettings::default());
    assert_eq!(shown[..2], ["Automatic detection", "-- System"]);
    assert!(
        shown[2].starts_with(&format!("{:<5}│ ", ansi_code_page())),
        "{}",
        shown[2]
    );
    assert!(
        shown[3].starts_with(&format!("{:<5}│ ", oem_code_page())),
        "{}",
        shown[3]
    );
    assert_eq!(
        shown[4..8],
        [
            "-- Unicode",
            "65001│ UTF-8",
            "1200 │ UTF-16 (Little endian)",
            "1201 │ UTF-16 (Big endian)"
        ]
    );
    if cfg!(windows) {
        assert_eq!(shown[8], "-- Other");
        assert!(
            !shown.iter().any(|line| line.starts_with("65000")),
            "no UTF-7"
        );
        let numbers: Vec<u32> = shown[9..]
            .iter()
            .filter_map(|line| line.split('│').next()?.trim().parse().ok())
            .collect();
        assert!(
            numbers.windows(2).all(|pair| pair[0] < pair[1]),
            "by number"
        );
    }
    let request = request(&CodePageSettings::default(), Some(UTF8), 30);
    assert_eq!(request.title, "Code pages");
    assert_eq!(request.bottom, "Ctrl+H Ins Del F4");
    assert_eq!(request.selected, 5, "the cursor on the code page shown");
    assert_eq!(request.max_height, Some(26));
}

/// The menu's hotkeys leave its separators' titles as they are.
#[test]
fn the_separators_keep_their_titles() {
    let menu = crate::app::PluginMenu::new(request(&CodePageSettings::default(), None, 30));
    let titles: Vec<&str> = (menu.lines.iter())
        .filter(|line| line.separator)
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(titles[..2], ["System", "Unicode"]);
}

#[test]
fn favourites_come_and_go_with_the_cursor_after_them() {
    let Some(page) = other_page() else {
        return;
    };
    let mut settings = CodePageSettings::default();
    let next = add_favorite(&mut settings, page);
    assert_eq!(settings.favorites, [page]);
    assert_ne!(next, Some(page), "the cursor on the next of Other");
    let shown = texts(&settings);
    let favorites = shown
        .iter()
        .position(|line| line == "-- Favorites")
        .unwrap();
    assert!(shown[favorites + 1].starts_with(&format!("{page:<5}│ ")));
    assert!(shown.iter().position(|line| line == "-- Other").unwrap() > favorites);
    assert_eq!(add_favorite(&mut settings, page), None, "not of Other");
    assert_eq!(
        remove_favorite(&mut settings, page),
        Some(page),
        "no other favourite"
    );
    assert!(settings.favorites.is_empty());
    assert!(!texts(&settings).contains(&"-- Favorites".to_string()));
    assert_eq!(remove_favorite(&mut settings, page), None);
}

#[test]
fn ctrl_h_hides_other_and_ins_does_nothing_then() {
    let Some(page) = other_page() else {
        return;
    };
    let mut settings = CodePageSettings {
        favorites: vec![page],
        menu_mode: true,
        ..CodePageSettings::default()
    };
    let shown = texts(&settings);
    assert!(!shown.contains(&"-- Other".to_string()));
    assert!(shown.contains(&"-- Favorites".to_string()));
    let request = request(&settings, None, 30);
    assert_eq!(
        (request.title.as_str(), request.bottom.as_str()),
        ("Code pages *", "Ctrl+H Del F4")
    );
    settings.favorites.clear();
    assert_eq!(add_favorite(&mut settings, page), None);
}

#[test]
fn a_user_s_name_is_marked_and_taken_back() {
    let Some(page) = other_page() else {
        return;
    };
    let mut settings = CodePageSettings::default();
    renamed(&mut settings, page, Some("Mine"));
    let line = format!("{page:<5}│*Mine");
    assert!(texts(&settings).contains(&line), "{:?}", texts(&settings));
    assert_eq!(shown_name(&line), "Mine");
    renamed(&mut settings, page, Some(""));
    assert!(settings.names.is_empty(), "an empty name takes it back");
    renamed(&mut settings, page, Some("Mine"));
    renamed(&mut settings, page, None);
    assert!(settings.names.is_empty(), "Reset");
    let form = rename_form("Cyrillic");
    assert_eq!(form.edit_text(NAME_EDIT), "Cyrillic");
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn shown_page(app: &mut App) -> Option<u32> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        app.tick();
        if let Screen::Viewer(viewer) = &app.screen
            && let Some(page) = viewer.text().page_number()
        {
            return Some(page);
        }
        assert!(std::time::Instant::now() < deadline, "never opened");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// Shift+F8 in the viewer: the menu on the code page shown; Enter on UTF-16 shows it so; "Automatic
/// detection" tells it again; F4 renames over the menu, which comes back after.
#[test]
fn the_viewer_s_shift_f8_chooses_the_code_page() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("v.txt"), "plain text\n").unwrap();
    let mut app = crate::app::fileops::tests::app_in(temp.path());
    app.view_file(&temp.path().join("v.txt"), false);
    assert_eq!(shown_page(&mut app), Some(ansi_code_page()));
    press(&mut app, KeyCode::F(8), KeyModifiers::SHIFT);
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("no menu");
    };
    assert!(menu.code_pages && menu.title == "Code pages");
    assert_eq!(menu.selected, 2, "on ANSI");
    // OEM, the separator passed over, UTF-8, UTF-16 (Little endian).
    for _ in 0..3 {
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    assert_eq!(
        shown_page(&mut app),
        Some(UTF16_LE),
        "UTF-16 (Little endian)"
    );
    press(&mut app, KeyCode::F(8), KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while shown_page(&mut app) != Some(ansi_code_page()) {
        assert!(std::time::Instant::now() < deadline, "not told again");
    }
    let Some(page) = other_page() else {
        return;
    };
    app.show_code_page_menu(Some(page));
    press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
    assert!(
        matches!(&app.overlay, Some(Overlay::Dialog(_))),
        "the rename dialog"
    );
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.code_pages.names.contains_key(&page.to_string()));
    app.tick();
    let Some(Overlay::PluginMenu(menu)) = &app.overlay else {
        panic!("the menu not back");
    };
    assert!(menu.code_pages);
    // The menu's own hotkeys (`VMENU_AUTOHIGHLIGHT`) put an `&` into its lines.
    let line = menu.lines[menu.selected].text.replace('&', "");
    assert!(line.starts_with(&format!("{page:<5}│*")), "{line}");
}
