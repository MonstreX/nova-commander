//! FAR's quick search (`far/fastfind.cpp`, `FileList::FindPartName`): Alt+letter opens a small "Search"
//! box under the active panel; typed text moves the cursor to the first name starting with it
//! (wildcards allowed, a trailing `\` finds folders only); text that matches nothing is not taken; the
//! name found is centred in the panel. Ctrl+V / Shift+Ins paste (as much of the text as matches),
//! Backspace, Ctrl+Backspace and Ctrl+Y edit it; Ctrl+Enter / Ctrl+Shift+Enter go to the next /
//! previous match; any other key closes the box and does its usual job.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Overlay, Panel};
use crate::vfs;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuickSearch {
    pub text: String,
}

/// Alt+character (or Alt+Shift+character) on the panels starts a quick search with that character.
pub fn start(app: &mut App, key: KeyEvent) -> bool {
    let KeyCode::Char(ch) = key.code else {
        return false;
    };
    let alt_only =
        key.modifiers.contains(KeyModifiers::ALT) && !key.modifiers.contains(KeyModifiers::CONTROL);
    if !alt_only || ch.is_control() {
        return false;
    }
    let mut search = QuickSearch::default();
    type_char(app, &mut search, ch.to_lowercase().next().unwrap_or(ch));
    app.overlay = Some(Overlay::QuickSearch(search));
    true
}

/// The tree's search when it has the keys (`tree_panel::search`), else the active panel's.
fn find_in(app: &mut App, text: &str, next: bool, forward: bool) -> bool {
    if app.tree_has_keys() {
        return app.tree_find(text, next, forward);
    }
    find(app.active_panel_mut(), text, next, forward)
}

/// Adds `ch` if the longer text still matches a name (FAR keeps the last matching text).
fn type_char(app: &mut App, search: &mut QuickSearch, ch: char) {
    let mut text = format!("{}{ch}", search.text);
    // FAR drops a doubled trailing "**" and a leading quote.
    if text.ends_with("**") {
        text.pop();
    }
    let text = text.trim_start_matches('"').to_string();
    if find_in(app, &text, false, true) {
        search.text = text;
    }
}

/// The text edited (Backspace, Ctrl+Backspace, Ctrl+Y): searched for again from the cursor; one that
/// matches nothing goes back to what it was (FAR's `ProcessKey` on its edit line).
fn edited(app: &mut App, search: &mut QuickSearch, text: String) {
    if find_in(app, &text, false, true) {
        search.text = text;
    }
}

/// Ctrl+V / Shift+Ins: the clipboard's text added, then shortened from its end until a name matches
/// (FAR's `ProcessName`); nothing matching leaves the text as it was.
fn paste(app: &mut App, search: &mut QuickSearch) {
    let Ok(Some(pasted)) = app.clipboard.text() else {
        return;
    };
    // FAR's `unquote`: no quote stays.
    let mut text = format!("{}{pasted}", search.text).replace('"', "");
    while !text.is_empty() {
        if find_in(app, &text, false, true) {
            search.text = text;
            return;
        }
        text.pop();
    }
}

/// FAR's `FindPartName`: from the cursor (or the next entry when `next`) towards `forward`, wrapping.
fn find(panel: &mut Panel, text: &str, next: bool, forward: bool) -> bool {
    let count = panel.entries.len();
    if count == 0 {
        return false;
    }
    let (prefix, folders_only) = match text.strip_suffix('\\') {
        Some(prefix) => (prefix, true),
        None => (text, false),
    };
    let start = panel.cursor;
    let offsets = (0..count).map(|step| {
        let step = step + usize::from(next);
        if forward {
            (start + step) % count
        } else {
            (start + count * 2 - step) % count
        }
    });
    for index in offsets {
        let entry = &panel.entries[index];
        if !entry.is_parent
            && (!folders_only || entry.is_dir)
            && vfs::matches_prefix(prefix, &entry.name)
        {
            panel.cursor = index;
            panel.centre_cursor();
            return true;
        }
    }
    false
}

impl App {
    pub(super) fn handle_quick_search_key(&mut self, mut search: QuickSearch, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        // AltGr comes as Ctrl+Alt: its characters are typed, never taken for Ctrl's keys.
        let ctrl_only = ctrl && !shift && !key.modifiers.contains(KeyModifiers::ALT);
        let on_tree = self.tree_has_keys();
        match key.code {
            KeyCode::Esc | KeyCode::F(10) => {
                self.tree_search_closed(on_tree);
                return;
            }
            KeyCode::Enter if ctrl => {
                find_in(self, &search.text, true, !shift);
            }
            KeyCode::Char('v') if ctrl_only => paste(self, &mut search),
            KeyCode::Insert if shift && !ctrl => paste(self, &mut search),
            KeyCode::Char('y') if ctrl_only => edited(self, &mut search, String::new()),
            KeyCode::Backspace if ctrl => {
                let mut line = super::line_input::LineInput::with_text(&search.text);
                line.delete_word_left();
                edited(self, &mut search, line.text().to_string());
            }
            KeyCode::Backspace => {
                let mut text = search.text.clone();
                text.pop();
                edited(self, &mut search, text);
            }
            KeyCode::Char(ch) if !ctrl || key.modifiers.contains(KeyModifiers::ALT) => {
                let ch = ch.to_lowercase().next().unwrap_or(ch);
                type_char(self, &mut search, ch);
            }
            // FAR's tree: Enter closing the box takes the other panel to the folder found.
            KeyCode::Enter if on_tree && key.modifiers.is_empty() => {
                self.tree_enter();
                return;
            }
            _ => {
                // Any other key closes the box and is handled as usual (FAR's `m_KeyToProcess`).
                self.handle_key(key);
                self.tree_search_closed(on_tree);
                return;
            }
        }
        self.overlay = Some(Overlay::QuickSearch(search));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        for name in ["alpha.txt", "beta.txt", "bravo.md"] {
            std::fs::write(temp.path().join(name), "x").unwrap();
        }
        std::fs::create_dir(temp.path().join("build")).unwrap();
        let app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
        (temp, app)
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_key(KeyEvent::new(code, modifiers));
    }

    fn current(app: &App) -> &str {
        app.active_panel()
            .current()
            .map_or("", |entry| entry.name.as_str())
    }

    fn text(app: &App) -> Option<&str> {
        match &app.overlay {
            Some(Overlay::QuickSearch(search)) => Some(&search.text),
            _ => None,
        }
    }

    #[test]
    fn alt_letter_finds_and_typing_narrows() {
        let (_temp, mut app) = app();
        press(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
        assert_eq!(text(&app), Some("b"));
        assert_eq!(current(&app), "build", "folders come first");
        press(&mut app, KeyCode::Char('r'), KeyModifiers::NONE);
        assert_eq!(current(&app), "bravo.md");
        // "brz" matches nothing: the 'z' is not taken.
        press(&mut app, KeyCode::Char('z'), KeyModifiers::NONE);
        assert_eq!(text(&app), Some("br"));
        assert_eq!(current(&app), "bravo.md");
    }

    #[test]
    fn ctrl_enter_cycles_and_other_keys_close() {
        let (_temp, mut app) = app();
        press(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
        press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
        assert_eq!(current(&app), "beta.txt");
        press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
        assert_eq!(current(&app), "bravo.md");
        press(
            &mut app,
            KeyCode::Enter,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        assert_eq!(current(&app), "beta.txt");
        // Down closes the search and moves the cursor as usual.
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(text(&app), None);
        assert_eq!(current(&app), "bravo.md");
    }

    #[test]
    fn trailing_backslash_finds_folders_only_and_esc_closes() {
        let (_temp, mut app) = app();
        press(&mut app, KeyCode::Char('a'), KeyModifiers::ALT);
        assert_eq!(current(&app), "alpha.txt");
        press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('b'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('\\'), KeyModifiers::NONE);
        assert_eq!(current(&app), "build");
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(text(&app), None);
        assert!(app.command_line.is_empty());
    }

    #[test]
    fn a_paste_takes_as_much_of_its_text_as_matches() {
        let (_temp, mut app) = app();
        app.clipboard.set_text("\"ravo.zip\"").unwrap();
        press(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
        press(&mut app, KeyCode::Char('v'), KeyModifiers::CONTROL);
        assert_eq!(text(&app), Some("bravo."), "cut back to what still matches");
        assert_eq!(current(&app), "bravo.md");
        app.clipboard.set_text("zzz").unwrap();
        press(&mut app, KeyCode::Insert, KeyModifiers::SHIFT);
        assert_eq!(text(&app), Some("bravo."), "nothing matching: as it was");
    }

    #[test]
    fn backspace_ctrl_backspace_and_ctrl_y_edit_the_text_and_search_again() {
        let (_temp, mut app) = app();
        press(&mut app, KeyCode::Char('a'), KeyModifiers::ALT);
        for ch in "lpha.t".chars() {
            press(&mut app, KeyCode::Char(ch), KeyModifiers::NONE);
        }
        assert_eq!(text(&app), Some("alpha.t"));
        press(&mut app, KeyCode::Backspace, KeyModifiers::CONTROL);
        assert_eq!(text(&app), Some("alpha."), "the word after the dot");
        press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(text(&app), Some("alpha"));
        press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
        assert_eq!(text(&app), Some(""), "Ctrl+Y empties it, the box stays");
        assert_eq!(current(&app), "alpha.txt");
        press(&mut app, KeyCode::Char('b'), KeyModifiers::NONE);
        assert_eq!(current(&app), "beta.txt", "searched from the cursor on");
    }

    #[test]
    fn the_name_found_is_centred() {
        let temp = tempfile::tempdir().unwrap();
        for at in 0..60 {
            std::fs::write(temp.path().join(format!("f{at:02}")), "x").unwrap();
        }
        let mut app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
        app.active_panel_mut().set_view_metrics(20, 1);
        press(&mut app, KeyCode::Char('f'), KeyModifiers::ALT);
        for ch in "40".chars() {
            press(&mut app, KeyCode::Char(ch), KeyModifiers::NONE);
        }
        let panel = app.active_panel();
        assert_eq!(current(&app), "f40");
        // FAR: the top at the cursor less half the panel's height (its 20 rows and 5 of frame).
        assert_eq!(panel.cursor - panel.offset, 12);
    }
}
