//! The editor's search (S2d.4c, S2d.6e; FAR's `Editor::DoSearchReplace` with
//! `GetSearchReplaceString`): F7's "Search" (the editor's dialog, `editor_search_form`) forward or
//! back from the cursor, Shift+F7 / Alt+F7 going on past what was found, "All" listing every match
//! (`editor_found`), "Word" / "Selection" filling "Search for"; the document looked through on a
//! worker (its edits from the table, the rest from the disk) with Esc; at an end FAR's question to go
//! on from the other one; found: the cursor at its start, the text marked until the next key (FAR's
//! defaults).

use super::dialog::{Dialog, FormAction};
use super::editor_search_form::{self as search_form, ALL, NEXT, PREVIOUS, SELECTION, TEXT, WORD};
use super::form::Form;
use super::viewer::search::{SearchParams, WRAP_YES, pattern, wrap_form};
use super::{App, Screen};
use crate::editor::view::Encoding;
use crate::editor::view::pieces::{DocFile, PieceTable};
use crate::editor::view::search::{self, Pattern, Range};
use crate::vfs::RandomRead;

/// Where a search reads the bytes the document did not change: the file it was read from, kept open
/// (R7 review F6: never what another program put at its path since, as the save), else the file at
/// its path.
pub(super) struct Source {
    pub path: std::path::PathBuf,
    pub held: Option<crate::jobs::file_reader::HeldFile>,
}

impl Source {
    pub(super) fn of(editor: &super::EditorScreen) -> Self {
        Self {
            path: editor.path.clone(),
            held: editor.text.held_file(),
        }
    }
}

/// The document read by offset on a worker: `doc`'s edits and what was read, the rest from `source`
/// (no file yet: nothing).
pub(super) fn doc_file(source: &Source, doc: PieceTable) -> std::io::Result<DocFile> {
    if let Some(held) = &source.held {
        let original = Box::new(crate::jobs::file_reader::HeldRead(held.clone()));
        return Ok(DocFile { doc, original });
    }
    let place = crate::vfs::VfsPath::local(&source.path);
    let original: Box<dyn RandomRead> =
        match crate::vfs::provider_for(&place).and_then(|provider| provider.open_random(&place)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Box::new(super::editor_actions::Empty)
            }
            Err(error) => return Err(error),
        };
    Ok(DocFile { doc, original })
}

/// The document looked through: the first match in `range`, until Esc.
fn run(
    source: &Source,
    doc: PieceTable,
    (encoding, pattern): (&Encoding, &Pattern),
    range: Range,
) -> std::io::Result<Option<(u64, u64)>> {
    let mut file = doc_file(source, doc)?;
    let stop = crate::vfs::stop::current();
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    let plain = |ch: char| ch;
    search::find(&mut file, (encoding, pattern), range, (&plain, &stopped))
}

/// What the editor looks for: the session's text (the editor has no Hex, FAR's editor parameters).
fn editor_pattern(params: &SearchParams) -> Result<Option<(Pattern, String)>, String> {
    pattern(&SearchParams {
        hex_mode: false,
        ..params.clone()
    })
}

impl App {
    /// F7 in the editor.
    pub(super) fn show_editor_search(&mut self) {
        if !matches!(&self.screen, Screen::Editor(_)) {
            return;
        }
        let form = search_form::form(&self.viewer_search);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::EditorSearch,
        });
    }

    pub(super) fn finish_editor_search(&mut self, form: &Form, button: Option<usize>) {
        if let Some(picker @ (WORD | SELECTION)) = button {
            return self.pick_into_search(form, picker == SELECTION);
        }
        self.viewer_search = search_form::read(form, &self.viewer_search);
        match button {
            Some(PREVIOUS) => self.editor_search_from(true, false),
            Some(NEXT) => self.editor_search_from(false, false),
            Some(ALL) => match editor_pattern(&self.viewer_search) {
                Ok(Some((pattern, shown))) => self.editor_search_all(pattern, shown),
                Ok(None) => {}
                Err(error) => {
                    let lines = vec![error, self.viewer_search.text.clone()];
                    self.show_dialog(Dialog::error("Error", lines));
                }
            },
            _ => {}
        }
    }

    /// FAR's "Word" / "Selection" (`DIF_BTNNOCLOSE`): the editor's text into "Search for", the dialog
    /// as it was otherwise.
    fn pick_into_search(&mut self, form: &Form, selection: bool) {
        let Screen::Editor(editor) = &self.screen else {
            return;
        };
        let picked = (editor.text.view())
            .and_then(|view| {
                let (line, cursor) = view.cursor_line().ok()?;
                let block = view.block_text().ok().flatten();
                Some(search_form::picked(
                    &line,
                    cursor,
                    block.as_deref(),
                    selection,
                ))
            })
            .unwrap_or_default();
        let mut form = form.clone();
        if let Some(edit) = form.edit_mut(TEXT) {
            edit.insert_str(&picked);
        }
        form.focus_item(TEXT);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::EditorSearch,
        });
    }

    /// The search from the cursor (on past it when going on), back or forward.
    pub(super) fn editor_search_from(&mut self, backward: bool, going_on: bool) {
        let Screen::Editor(editor) = &self.screen else {
            return;
        };
        let Some(view) = editor.text.view() else {
            return;
        };
        let Ok(pos) = view.cursor_pos() else {
            return;
        };
        let len = view.doc().size();
        let start = if going_on && !backward { pos + 1 } else { pos };
        let can_wrap = if backward { start < len } else { start > 0 };
        let range = Range {
            from: start,
            to: if backward { 0 } else { u64::MAX },
            backward,
        };
        self.editor_search_in(range, can_wrap);
    }

    fn editor_search_in(&mut self, range: Range, can_wrap: bool) {
        let Screen::Editor(editor) = &self.screen else {
            return;
        };
        let params: SearchParams = self.viewer_search.clone();
        let (Some(view), Some(doc), Ok(Some((pattern, shown)))) = (
            editor.text.view(),
            editor.text.document_to_read(),
            editor_pattern(&params),
        ) else {
            if let Err(error) = editor_pattern(&params) {
                self.show_dialog(Dialog::error("Error", vec![error, params.text]));
            }
            return;
        };
        let encoding = view.encoding().clone();
        let source = Source::of(editor);
        let line = format!("Searching for {shown}");
        self.wait_for_stoppable(vec![line], move || {
            let found = run(&source, doc, (&encoding, &pattern), range);
            let stopped = crate::vfs::stop::current().load(std::sync::atomic::Ordering::Relaxed);
            Box::new(move |app: &mut App| {
                if !stopped {
                    app.editor_found(found, (range, can_wrap), &shown);
                }
            })
        });
    }

    fn editor_found(
        &mut self,
        found: std::io::Result<Option<(u64, u64)>>,
        (range, can_wrap): (Range, bool),
        shown: &str,
    ) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        match found {
            Ok(Some((at, size))) => editor.text.show_found(at, size),
            Ok(None) if can_wrap => {
                let form = wrap_form(range.backward, shown);
                self.show_dialog(Dialog::Form {
                    form,
                    action: FormAction::EditorSearchWrap {
                        backward: range.backward,
                        start: range.from,
                    },
                });
            }
            Ok(None) => {
                let lines = vec!["Could not find the string".to_string(), shown.to_string()];
                self.show_dialog(Dialog::error("Search", lines));
            }
            Err(error) => self.show_error("Search", &error),
        }
    }

    /// The question at an end answered: Yes looks through the rest from the other end.
    pub(super) fn finish_editor_search_wrap(
        &mut self,
        button: Option<usize>,
        (backward, start): (bool, u64),
    ) {
        if button != Some(WRAP_YES) {
            return;
        }
        let range = match backward {
            false => Range {
                from: 0,
                to: start.saturating_add(1),
                backward,
            },
            true => Range {
                from: u64::MAX,
                to: start,
                backward,
            },
        };
        self.editor_search_in(range, false);
    }
}

#[cfg(test)]
#[path = "editor_search_tests.rs"]
mod tests;
