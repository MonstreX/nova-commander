//! The editor's keys of the code editing mode (ours, owner 2026-11-22; `editor::view::edit_code`):
//! Shift+Tab, Ctrl+/ with the file type's line comment, Alt+Up / Alt+Down. With the mode off they are
//! FAR's (Alt+Up / Alt+Down a vertical block's moves, the others nothing).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::editing::{EditorAction, EditorScreen};
use super::plugins::{MenuOutcome, PluginMenu};
use super::{App, Overlay, Screen};
use crate::editor::view::edit_view::Step;
use crate::plugin_host::windows::{MenuLine, MenuRequest};

/// The line comments by Colorer's type names, the names apart by blanks (none: the type has no line
/// comment, or none known).
const LINE_COMMENTS: [(&str, &str); 9] = [
    (
        "//",
        "c cpp cpp-qt cppsql csharp dart go groovy haxe idl java jsnet jScript jsx kotlin scala solidity swift zig rust typescript php fsharp gleam qml pascal protobuf verilog squirrel jenkinsfile javacc prisma hcl terraform nesc graphviz",
    ),
    (
        "#",
        "python cython perl ruby elixir julia nix tcltk shell shell-bash shell-posix makefile yaml yml toml cmake powershell r dockerfile dotenv gitignore gitattributes awk gdscript config meson qmake scons graphql nsi iss",
    ),
    ("--", "lua luau sql mysql haskell ada moonscript vhdl sml"),
    ("'", "vbasic vbnet vbScript"),
    (";", "asm asm80 asasm ini lisp reg avr z80 picasm AutoIt"),
    ("%", "TeX erlang matlab postscript m4"),
    ("!", "fortran"),
    ("\\", "forth"),
    ("REM", "Batch"),
];

/// The line comment of Colorer's type `name`.
fn line_comment(name: &str) -> Option<&'static str> {
    (LINE_COMMENTS.iter())
        .find(|(_, names)| (names.split_whitespace()).any(|known| known.eq_ignore_ascii_case(name)))
        .map(|(token, _)| *token)
}

impl EditorScreen {
    /// The code mode's own keys as the view's steps; none: the mode is off or the key is not one.
    pub(super) fn code_key(&self, key: &KeyEvent) -> Option<Step> {
        if !self.settings.code_editing {
            return None;
        }
        match (key.code, key.modifiers) {
            (KeyCode::BackTab, _) => Some(Step::Outdent),
            (KeyCode::Up, KeyModifiers::ALT) => Some(Step::MoveLines(true)),
            (KeyCode::Down, KeyModifiers::ALT) => Some(Step::MoveLines(false)),
            (KeyCode::Char('/'), KeyModifiers::CONTROL) => {
                let token = self.text.syntax().taken().and_then(line_comment)?;
                Some(Step::ToggleComment(token.to_string()))
            }
            (KeyCode::Char('d' | 'D'), modifiers)
                if modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT =>
            {
                Some(Step::Duplicate)
            }
            _ => None,
        }
    }

    /// The code mode's own actions: Ctrl+Space's words.
    pub(super) fn code_action(&self, key: &KeyEvent) -> Option<EditorAction> {
        let space = key.code == KeyCode::Char(' ') && key.modifiers == KeyModifiers::CONTROL;
        (self.settings.code_editing && space).then_some(EditorAction::Complete)
    }

    /// F2 / Shift+F10 in the code mode with "Clean line ends on save": the lines cleaned first (the
    /// save waits for the step, as for every one before it).
    pub(super) fn before_save(&mut self, action: &EditorAction) {
        let saves = matches!(action, EditorAction::Save | EditorAction::SaveAndClose);
        if saves
            && self.settings.code_editing
            && self.settings.trim_on_save
            && !self.text.is_locked()
        {
            self.text.press(Step::CleanUp);
        }
    }
}

impl App {
    /// Ctrl+Space: the words completing the one before the cursor in a menu under it; none: nothing.
    pub(super) fn show_completion(&mut self) {
        let Screen::Editor(editor) = &self.screen else {
            return;
        };
        let Some(view) = editor.text.view() else {
            return;
        };
        let (typed, words) = view.completions();
        if words.is_empty() {
            return;
        }
        let (width, height) = editor.text.room.get();
        let (x, y) = editor.text.origin.get();
        let cursor = view.screen(usize::from(width), usize::from(height)).cursor;
        let at = |(row, col): (usize, usize)| {
            let col = u16::try_from(col.saturating_sub(typed)).unwrap_or(0);
            (
                x.saturating_add(col),
                y.saturating_add(u16::try_from(row + 1).unwrap_or(0)),
            )
        };
        let request = MenuRequest {
            title: String::new(),
            lines: (words.into_iter())
                .map(|text| MenuLine {
                    text,
                    ..MenuLine::default()
                })
                .collect(),
            show_ampersand: true,
            x: cursor.map(|cursor| at(cursor).0),
            y: cursor.map(|cursor| at(cursor).1),
            max_height: Some(10),
            ..MenuRequest::default()
        };
        let mut menu = PluginMenu::new(request);
        menu.completion = Some(typed);
        self.overlay = Some(Overlay::PluginMenu(Box::new(menu)));
    }

    /// A key in Ctrl+Space's menu: Enter puts the word in place of the characters typed.
    pub(super) fn completion_key(&mut self, mut menu: Box<PluginMenu>, key: KeyEvent) {
        let typed = menu.completion.unwrap_or(0);
        match menu.handle_key(key) {
            MenuOutcome::Stay => self.overlay = Some(Overlay::PluginMenu(menu)),
            MenuOutcome::Esc => {}
            MenuOutcome::Chosen { item, .. } => {
                let word = item
                    .and_then(|item| menu.lines.get(item))
                    .map(|line| line.text.clone());
                if let (Some(word), Screen::Editor(editor)) = (word, &mut self.screen) {
                    editor.text.press(Step::Complete(typed, word));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_comments_by_the_type() {
        assert_eq!(line_comment("rust"), Some("//"));
        assert_eq!(line_comment("python"), Some("#"));
        assert_eq!(line_comment("lua"), Some("--"));
        assert_eq!(line_comment("batch"), Some("REM"), "the case aside");
        assert_eq!(line_comment("html"), None, "no line comment");
    }
}
