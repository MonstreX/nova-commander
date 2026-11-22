//! Panel keys that select files (FAR's `FileList::ProcessKey`): Shift+movement, the gray keypad
//! `+` `-` `*` with Shift/Ctrl/Alt, the Select/Deselect mask dialogs, and Ctrl+H.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

use super::App;

/// Handles a selection key; returns false for keys that are not selection keys.
pub fn handle_selection_key(app: &mut App, key: KeyEvent) -> bool {
    if let KeyCode::Char(ch @ ('+' | '-' | '*')) = key.code
        && key.state.contains(KeyEventState::KEYPAD)
    {
        gray_key(app, ch, key.modifiers);
        return true;
    }
    if key.modifiers != KeyModifiers::SHIFT {
        return false;
    }
    let panel = app.active_panel_mut();
    match key.code {
        KeyCode::Up | KeyCode::Down => panel.shift_step(key.code == KeyCode::Down),
        KeyCode::Home | KeyCode::End => panel.shift_to_edge(key.code == KeyCode::End),
        KeyCode::PageUp | KeyCode::PageDown => panel.shift_page(key.code == KeyCode::PageDown),
        KeyCode::Left | KeyCode::Right if panel.stripes() > 1 => {
            panel.shift_column(key.code == KeyCode::Right);
        }
        _ => return false,
    }
    true
}

/// FAR: Gray+ / Gray- ask for a mask, Gray* inverts; Shift: all / none; Ctrl: by the current
/// extension, Ctrl+Gray* inverts folders too; Alt: by the current name, Alt+Gray* inverts files.
fn gray_key(app: &mut App, ch: char, modifiers: KeyModifiers) {
    let side = app.active;
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let alt = modifiers.contains(KeyModifiers::ALT);
    let shift = modifiers.contains(KeyModifiers::SHIFT);
    let panel = app.active_panel_mut();
    let folders = panel.options.select_folders;
    match (ch, ctrl, alt, shift) {
        ('*', true, _, _) => panel.invert_selection(true),
        // FAR's `SELECT_INVERTFILES`.
        ('*', _, true, _) => panel.invert_selection(false),
        ('*', _, _, _) => panel.invert_selection(folders),
        ('+', false, false, true) => panel.select_all(),
        ('-', false, false, true) => panel.clear_selection(),
        (_, true, _, _) => {
            if let Some(mask) = panel.current_extension_mask() {
                app.select_group(side, ch == '+', &mask);
            }
        }
        (_, _, true, _) => {
            if let Some(mask) = panel.current_name_mask() {
                app.select_group(side, ch == '+', &mask);
            }
        }
        _ => app.ask_select_group(side, ch == '+'),
    }
}

impl App {
    /// Ctrl+H: hidden and system files on or off in both panels (FAR's global `ShowHidden`).
    pub(super) fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        let show = self.show_hidden;
        for panel in &mut self.panels {
            panel.set_show_hidden(show);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        for name in ["a.rs", "b.rs", "c.md"] {
            std::fs::write(temp.path().join(name), "x").unwrap();
        }
        let app = App::new(temp.path(), 100, 30, &crate::settings::Settings::default());
        (temp, app)
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_key(KeyEvent::new(code, modifiers));
    }

    #[test]
    fn shift_arrows_select_from_the_panel() {
        let (_temp, mut app) = app();
        press(&mut app, KeyCode::Down, KeyModifiers::SHIFT);
        press(&mut app, KeyCode::Down, KeyModifiers::SHIFT);
        assert_eq!(
            app.active_panel().selected.len(),
            1,
            "\"..\" is never selected"
        );
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        assert!(
            app.active_panel().shift_selection.is_none(),
            "a key without Shift ends it"
        );
    }

    #[test]
    fn ctrl_h_hides_hidden_files_in_both_panels() {
        let (temp, mut app) = app();
        let hidden = temp.path().join("secret.txt");
        std::fs::write(&hidden, "x").unwrap();
        let status = std::process::Command::new("attrib")
            .arg("+h")
            .arg(&hidden)
            .status()
            .unwrap();
        assert!(status.success());
        app.panels[0].reload();
        app.panels[1].reload();
        let names = |app: &App, side: usize| -> Vec<String> {
            app.panels[side]
                .entries
                .iter()
                .map(|e| e.name.clone())
                .collect()
        };
        assert!(names(&app, 0).contains(&"secret.txt".to_string()));
        press(&mut app, KeyCode::Char('h'), KeyModifiers::CONTROL);
        assert!(!app.show_hidden);
        assert!(!names(&app, 0).contains(&"secret.txt".to_string()));
        assert!(!names(&app, 1).contains(&"secret.txt".to_string()));
        press(&mut app, KeyCode::Char('h'), KeyModifiers::CONTROL);
        assert!(names(&app, 1).contains(&"secret.txt".to_string()));
    }
}
