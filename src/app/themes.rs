//! Options > Themes (ADR-0008): the chosen theme, the one shown while the list is open, and the user's
//! theme files (`<name>.json` in the themes folder). Drawing turns the name into colours (`ui::theme`).

use std::path::PathBuf;

use super::App;
use super::dialog::{Dialog, ListAction};

#[derive(Debug, Default)]
pub struct Themes {
    /// The theme chosen in Options > Themes (saved in the settings).
    pub chosen: String,
    /// The theme shown while the list is open: moving over a theme shows the program in it.
    pub preview: Option<String>,
    /// The built-in themes (the program gives them at start; drawing owns them).
    pub built_in: Vec<String>,
    /// The folder of the user's theme files.
    pub dir: Option<PathBuf>,
    /// What could not be read from the theme just shown (drawing reports it).
    pub problems: Vec<String>,
}

impl Themes {
    pub fn new(chosen: &str) -> Self {
        Self {
            chosen: chosen.to_string(),
            ..Self::default()
        }
    }

    /// The theme to draw with.
    pub fn shown(&self) -> &str {
        self.preview.as_deref().unwrap_or(&self.chosen)
    }

    /// The theme file `name`, if the user has one. Only plain names: never a path.
    pub fn read_file(&self, name: &str) -> Option<String> {
        if name.is_empty() || name.contains(['/', '\\', ':']) || name.starts_with('.') {
            return None;
        }
        let path = self.dir.as_ref()?.join(format!("{name}.json"));
        std::fs::read_to_string(path).ok()
    }

    /// The built-in themes, then the user's files.
    pub fn names(&self) -> Vec<String> {
        let mut names = self.built_in.clone();
        let files = self
            .dir
            .as_ref()
            .and_then(|dir| std::fs::read_dir(dir).ok());
        let mut own: Vec<String> = files
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let is_json = path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
                is_json.then(|| path.file_stem()?.to_str().map(str::to_string))?
            })
            .filter(|name| !names.contains(name))
            .collect();
        own.sort();
        names.append(&mut own);
        names
    }
}

impl App {
    pub(super) fn choose_theme(&mut self) {
        let items = self.themes.names();
        let cursor = items
            .iter()
            .position(|name| *name == self.themes.chosen)
            .unwrap_or(0);
        self.show_dialog(Dialog::List {
            title: "Themes".to_string(),
            items,
            cursor,
            top: Default::default(),
            action: ListAction::Theme,
        });
    }

    /// While the theme list is open, the theme under its cursor is shown.
    pub(super) fn preview_theme(&mut self, dialog: &Dialog) {
        if let Dialog::List {
            items,
            cursor,
            action: ListAction::Theme,
            ..
        } = dialog
        {
            self.themes.preview = items.get(*cursor).cloned();
        }
    }

    /// Problems drawing found in a theme, shown once.
    pub(super) fn report_theme_problems(&mut self) {
        if self.themes.problems.is_empty() || self.screen_busy() {
            return;
        }
        let lines = std::mem::take(&mut self.themes.problems);
        self.show_dialog(Dialog::error("Theme", lines));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_users_theme_files_come_after_the_built_in_ones() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("zebra.json"), "{}").unwrap();
        std::fs::write(temp.path().join("Mine.JSON"), r#"{"inherits": "dn"}"#).unwrap();
        std::fs::write(temp.path().join("far.json"), "{}").unwrap();
        std::fs::write(temp.path().join("notes.txt"), "").unwrap();
        let themes = Themes {
            built_in: vec!["far".to_string(), "dn".to_string()],
            dir: Some(temp.path().to_path_buf()),
            ..Themes::new("far")
        };
        assert_eq!(themes.names(), ["far", "dn", "Mine", "zebra"]);
        assert_eq!(
            themes.read_file("Mine").as_deref(),
            Some(r#"{"inherits": "dn"}"#)
        );
        assert_eq!(themes.read_file("dn"), None, "built in");
        assert_eq!(themes.read_file("../zebra"), None, "never a path");
    }

    #[test]
    fn options_themes_shows_the_theme_under_the_cursor_and_enter_keeps_it() {
        use super::super::command::Command;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::new(temp.path(), 100, 40, &crate::settings::Settings::default());
        app.themes.built_in = vec!["far".to_string(), "nc5".to_string(), "dn".to_string()];
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);

        app.execute(Command::ChooseTheme);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.themes.shown(), "nc5", "shown while moving");
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.themes.shown(), "far", "Esc: back to the chosen one");

        app.execute(Command::ChooseTheme);
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.themes.shown(), "dn");
        assert_eq!(app.to_settings().theme, "dn", "saved with the settings");
    }

    #[test]
    fn the_shown_theme_is_the_previewed_one_while_the_list_is_open() {
        let mut themes = Themes::new("far");
        assert_eq!(themes.shown(), "far");
        themes.preview = Some("dn".to_string());
        assert_eq!(themes.shown(), "dn");
    }
}
