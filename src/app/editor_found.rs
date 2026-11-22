//! F7's "All" in the editor (S2d.6e; FAR's `Editor::DoSearchReplace` with `find_all_list`): every
//! match looked for on a worker with Esc, then FAR's list "Occurrences: N, lines: M" low on the screen
//! (at most ten items): each match's line and its first character in columns, the line's text with
//! the match marked. Enter goes to the match and closes it, Ctrl+Enter goes to it and keeps it, F5
//! makes it as big as it needs, Shift+F5 hides or shows the columns (FAR's `ToggleFixedColumns`).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Overlay, PluginMenu, Screen};
use crate::editor::view::edit_view::Step;
use crate::editor::view::search::{self, FoundLine, Pattern};
use crate::plugin_host::windows::{MenuLine, MenuRequest};

/// The list's matches and how it is shown.
#[derive(Debug, Clone)]
pub struct FoundAll {
    pub found: Vec<FoundLine>,
    /// FAR's fixed columns of the line and the position shown (Shift+F5).
    pub columns: bool,
    /// F5: as big as it needs (FAR's `toggle_zoom`).
    pub zoomed: bool,
}

/// FAR's `SearchAllUseAltFileNameFormat` default: the files named `<name>.Filtered<.ext>`.
const ALT_NAMES: &str = "*.txt,*.log,*.md,*.csv,*.ini,*.cmd,*.map";

/// FAR's `GetSearchAllFileName`: `<name><.ext>.Filtered.txt` (`MEditSearchAllFileNameFormat`), or
/// `<name>.Filtered<.ext>` for a name of [`ALT_NAMES`] (`MEditSearchAllFileNameFormatAlt`).
fn filtered_name(path: &std::path::Path) -> std::path::PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    let name = name.unwrap_or_default();
    let mask = crate::vfs::FileMask::parse_with(ALT_NAMES, &|_| None, "");
    let alt = mask.is_some_and(|mask| mask.matches(&name));
    let (stem, ext) = match name.rfind('.').filter(|dot| *dot > 0) {
        Some(dot) => name.split_at(dot),
        None => (name.as_str(), ""),
    };
    let filtered = match alt {
        true => format!("{stem}.Filtered{ext}"),
        false => format!("{stem}{ext}.Filtered.txt"),
    };
    path.with_file_name(filtered)
}

/// The digits of `count` (FAR's `radix10_formatted_width`).
fn digits(count: u64) -> usize {
    count.max(1).ilog10() as usize + 1
}

impl FoundAll {
    /// The menu's lines and each one's marked characters.
    fn lines(&self) -> (Vec<MenuLine>, Vec<Option<(usize, usize)>>) {
        let line_width = digits(self.found.iter().map(|it| it.line + 1).max().unwrap_or(1));
        let pos_width = digits(
            self.found
                .iter()
                .map(|it| it.pos as u64 + 1)
                .max()
                .unwrap_or(1),
        );
        (self.found.iter())
            .map(|found| {
                let shown: String = (found.text.chars())
                    .map(|ch| if ch < ' ' { ' ' } else { ch })
                    .collect();
                let (text, before) = match self.columns {
                    true => (
                        format!(
                            "{:>line_width$}│{:>pos_width$}│{shown}",
                            found.line + 1,
                            found.pos + 1
                        ),
                        line_width + pos_width + 2,
                    ),
                    false => (shown, 0),
                };
                let line = MenuLine {
                    text,
                    ..MenuLine::default()
                };
                let from = before + found.pos;
                (line, Some((from, from + found.len)))
            })
            .unzip()
    }

    /// FAR's title: the matches and the lines they are on.
    fn title(&self) -> String {
        let mut lines: Vec<u64> = self.found.iter().map(|found| found.line).collect();
        lines.dedup();
        format!("Occurrences: {}, lines: {}", self.found.len(), lines.len())
    }

    /// The menu with the cursor on `selected`, low on a screen `height` rows high (FAR's `m_MenuY1`
    /// `ScrY - 20`) or, zoomed, as big as it needs.
    fn menu(self, selected: usize, height: u16) -> PluginMenu {
        let (lines, marks) = self.lines();
        let request = MenuRequest {
            title: self.title(),
            bottom: "Ctrl+Enter, F5, Shift+F5, Ctrl+Up, Ctrl+Down".to_string(),
            lines,
            selected,
            wrap: true,
            show_ampersand: true,
            y: (!self.zoomed).then(|| height.saturating_sub(21)),
            max_height: (!self.zoomed).then_some(10),
            ..MenuRequest::default()
        };
        let mut menu = PluginMenu::new(request);
        menu.marks = marks;
        menu.found_all = Some(Box::new(self));
        menu
    }
}

impl App {
    /// "All": every match of the session's search on a worker, then the list.
    pub(super) fn editor_search_all(&mut self, pattern: Pattern, shown: String) {
        let Screen::Editor(editor) = &self.screen else {
            return;
        };
        let (Some(view), Some(doc)) = (editor.text.view(), editor.text.document_to_read()) else {
            return;
        };
        let (encoding, floor) = (view.encoding().clone(), view.window().2);
        let source = super::editor_search::Source::of(editor);
        let line = format!("Searching for {shown}");
        self.wait_for_stoppable(vec![line], move || {
            let found = super::editor_search::doc_file(&source, doc).and_then(|mut file| {
                let stop = crate::vfs::stop::current();
                let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
                let plain = |ch: char| ch;
                let all = search::find_all(&mut file, (&encoding, &pattern), (&plain, &stopped))?;
                search::found_lines(&mut file, (&encoding, floor), &all, &stopped)
            });
            Box::new(move |app: &mut App| match found {
                Ok(found) if found.is_empty() => {
                    let lines = vec!["Could not find the string".to_string(), shown];
                    app.show_dialog(super::Dialog::error("Search", lines));
                }
                Ok(found) => {
                    let all = FoundAll {
                        found,
                        columns: true,
                        zoomed: false,
                    };
                    let menu = all.menu(0, app.height);
                    app.overlay = Some(Overlay::PluginMenu(Box::new(menu)));
                }
                Err(error) => app.show_error("Search", &error),
            })
        });
    }

    /// A key in the list.
    pub(super) fn found_all_key(&mut self, mut menu: Box<PluginMenu>, key: KeyEvent) {
        use super::plugins::MenuOutcome;
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        let shift = key.modifiers == KeyModifiers::SHIFT;
        let Some(mut all) = menu.found_all.take() else {
            return;
        };
        let selected = menu.selected;
        match key.code {
            KeyCode::Enter if ctrl => self.go_to_match(all.found.get(selected)),
            // FAR's `ProcessKeyInternal` of the editor under the list.
            KeyCode::Up | KeyCode::Down if ctrl => {
                if let Screen::Editor(editor) = &mut self.screen {
                    let up = key.code == KeyCode::Up;
                    let step = if up { Step::ScrollUp } else { Step::ScrollDown };
                    editor.text.press(step);
                }
            }
            // FAR's F4 (every item) and Alt+F4 (those the filter shows: the list has no filter yet).
            KeyCode::F(4) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::ALT => {
                return self.found_to_new_editor(&all, selected);
            }
            KeyCode::F(5) if key.modifiers.is_empty() => all.zoomed = !all.zoomed,
            KeyCode::F(5) if shift => all.columns = !all.columns,
            _ => {
                menu.found_all = Some(all);
                match menu.handle_key(key) {
                    MenuOutcome::Stay => self.overlay = Some(Overlay::PluginMenu(menu)),
                    MenuOutcome::Esc => {}
                    MenuOutcome::Chosen { item, .. } => {
                        let all = menu.found_all.take();
                        let found = all.as_ref().and_then(|all| all.found.get(item?));
                        self.go_to_match(found);
                    }
                }
                return;
            }
        }
        let mut again = all.menu(selected, self.height);
        again.top = menu.top;
        self.overlay = Some(Overlay::PluginMenu(Box::new(again)));
    }

    /// FAR's `SaveFoundItemsToNewEditor`: the lines found, each once, in a new editor window on a file
    /// not there yet named after this one ([`filtered_name`]), in this one's code page; the cursor at
    /// the item chosen, its match marked with "Select found".
    fn found_to_new_editor(&mut self, all: &FoundAll, selected: usize) {
        let Screen::Editor(editor) = &self.screen else {
            return;
        };
        let path = filtered_name(&editor.path);
        let page = editor.text.page_number();
        let select = editor.text.options().select_found;
        let (mut text, mut lines, mut at) = (String::new(), None, None);
        for (index, found) in all.found.iter().enumerate() {
            if lines.map(|(line, _)| line) != Some(found.line) {
                text.push_str(&found.text);
                text.push('\n');
                lines = Some((found.line, lines.map_or(0, |(_, count)| count + 1)));
            }
            if index == selected {
                at = lines.map(|(_, count)| (count, found));
            }
        }
        self.open_new_file(&path);
        let Screen::Editor(new) = &mut self.screen else {
            return;
        };
        // FAR's `FFILEEDIT_EPHEMERAL`: F2 asks for the name, an existing file is asked about.
        new.saving.ephemeral = true;
        new.text.pump();
        new.text.set_page(page);
        new.text.press(Step::Paste(text));
        let Some((line, found)) = at else {
            return new.text.press(Step::GoTo(0, 0));
        };
        new.text.press(Step::GoTo(line, found.pos));
        for _ in 0..found.len * usize::from(select) {
            new.text.press(Step::Select(Box::new(Step::Right)));
        }
    }

    /// FAR's `SelectFoundPattern`: the editor's cursor at the match, as a search leaves it.
    fn go_to_match(&mut self, found: Option<&FoundLine>) {
        if let (Screen::Editor(editor), Some(found)) = (&mut self.screen, found) {
            editor.text.show_found(found.at, found.size);
        }
    }
}

#[cfg(test)]
#[path = "editor_found_tests.rs"]
mod tests;
