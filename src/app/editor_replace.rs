//! The editor's replace (S2d.4d; FAR's `Editor::DoSearchReplace` in replace mode): Ctrl+F7's "Replace"
//! dialog (`GetSearchReplaceString` with "Replace with", "Replace previous" / "Replace next"), each match
//! from the cursor shown marked with FAR's question "Replace "x" with "y"" - Replace, All (the rest
//! without asking), Skip, Cancel; the document looked through on a worker with Esc.

use super::delete::{Line, message_form};
use super::dialog::{Dialog, FormAction};
use super::editing::EditorAction;
use super::form::{Control, EditHistory, Form, Item};
use super::line_input::LineInput;
use super::viewer::search::{SearchParams, pattern};
use super::{App, Screen};
use crate::editor::view::edit_view::Step;
use crate::editor::view::search::{self, Range};
use crate::vfs::RandomRead;

/// The dialog's items.
const TEXT: usize = 1;
const REPLACE: usize = 3;
const CASE: usize = 5;
const WORDS: usize = 6;
const REGEX: usize = 7;
const PREVIOUS: usize = 9;
const NEXT: usize = 10;
/// The question's buttons (after its four lines and a separator).
const ASK_REPLACE: usize = 5;
const ASK_ALL: usize = 6;
const ASK_SKIP: usize = 7;

fn check(x: u16, y: u16, label: &str, checked: bool) -> Item {
    let label = label.to_string();
    Item::new(x, y, Control::Check { label, checked })
}

fn button(label: &str, default: bool) -> Item {
    let label = label.to_string();
    Item::new(0, 11, Control::Button { label, default })
}

/// FAR's "Replace" dialog (76 by 12 without the hex line and the picker).
fn form(params: &SearchParams) -> Form {
    let mut text = LineInput::with_text(&params.text);
    text.select_all();
    let edit = |input: LineInput, y: u16, history: &str| {
        Item::new(5, y, Control::Edit(input))
            .wide(66)
            .with_history(EditHistory::new(history).use_last())
    };
    let items = vec![
        Item::new(5, 2, Control::Text("&Search for".to_string())),
        edit(text, 3, "SearchText"),
        Item::new(5, 4, Control::Text("&Replace with".to_string())),
        edit(LineInput::with_text(&params.replace), 5, "ReplaceText"),
        Item::new(0, 6, Control::Separator),
        check(5, 7, "&Case sensitive", params.case_sensitive),
        check(5, 8, "&Whole words", params.whole_words),
        check(39, 7, "R&egular expressions", params.regex),
        Item::new(0, 10, Control::Separator),
        button("Replace pre&vious", false),
        button("Replace next", true),
        button("Cancel", false),
    ];
    let mut form = Form::new("Replace", 76, 14, items);
    form.focus_item(TEXT);
    form
}

/// A match and its text, found by the worker.
type Found = Option<(u64, u64, String)>;

/// The next match of the session's search from `range`, with its text, on the worker.
fn run(
    source: &super::editor_search::Source,
    file: (
        crate::editor::view::pieces::PieceTable,
        crate::editor::view::Encoding,
    ),
    pattern: &search::Pattern,
    range: Range,
) -> std::io::Result<Found> {
    let (doc, encoding) = file;
    let mut file = super::editor_search::doc_file(source, doc)?;
    let stop = crate::vfs::stop::current();
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    let plain = |ch: char| ch;
    let found = search::find(&mut file, (&encoding, pattern), range, (&plain, &stopped))?;
    let Some((at, size)) = found else {
        return Ok(None);
    };
    let mut bytes = vec![0; usize::try_from(size).unwrap_or(0)];
    let mut done = 0;
    while done < bytes.len() {
        match file.read_at(at + done as u64, &mut bytes[done..])? {
            0 => break,
            read => done += read,
        }
    }
    let mut text = String::new();
    let mut rest = &bytes[..done];
    while !rest.is_empty() {
        let (ch, len) = encoding.decode(rest, false).unwrap_or(('\u{FFFD}', 1));
        text.push(ch);
        rest = &rest[len.max(1).min(rest.len())..];
    }
    Ok(Some((at, size, text)))
}

/// "All"'s matches from `range` on, found in one pass over one document, the last first (each next
/// one looked for past the one before it, as FAR goes on past what it replaced); none left when
/// stopped.
fn run_all(
    source: &super::editor_search::Source,
    file: (
        crate::editor::view::pieces::PieceTable,
        crate::editor::view::Encoding,
    ),
    pattern: &search::Pattern,
    range: Range,
) -> std::io::Result<Vec<(u64, u64)>> {
    let (doc, encoding) = file;
    let mut file = super::editor_search::doc_file(source, doc)?;
    let stop = crate::vfs::stop::current();
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    let plain = |ch: char| ch;
    // The document read once (one search after another read a window each), the matches from the
    // cursor on, or before it going back.
    let all = search::find_all(&mut file, (&encoding, pattern), (&plain, &stopped))?;
    if stopped() {
        return Ok(Vec::new());
    }
    let mut found: Vec<(u64, u64)> = (all.into_iter())
        .filter(|&(at, size)| match range.backward {
            false => at >= range.from,
            true => at + size <= range.from,
        })
        .collect();
    // The last first: each replaced before the ones ahead of it move.
    found.sort_unstable_by(|one, other| other.0.cmp(&one.0));
    Ok(found)
}

/// The bytes `text` takes in the editor's document.
fn encoded_len(editor: &super::EditorScreen, text: &str) -> u64 {
    (editor.text.view()).map_or(text.len() as u64, |view| {
        view.encoding().encode(text).len() as u64
    })
}

impl App {
    /// Ctrl+F7 in the editor.
    pub(super) fn show_editor_replace(&mut self) {
        let form = form(&self.viewer_search);
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::EditorReplace,
        });
    }

    pub(super) fn finish_editor_replace(&mut self, form: &Form, button: Option<usize>) {
        let backward = match button {
            Some(PREVIOUS) => true,
            Some(NEXT) => false,
            _ => return,
        };
        let params = &mut self.viewer_search;
        params.text = form.edit_text(TEXT).to_string();
        params.replace = form.edit_text(REPLACE).to_string();
        params.hex_mode = false;
        params.fuzzy = false;
        params.case_sensitive = form.is_checked(CASE);
        params.whole_words = form.is_checked(WORDS);
        params.regex = form.is_checked(REGEX);
        let Screen::Editor(editor) = &self.screen else {
            return;
        };
        let Some(Ok(pos)) = editor.text.view().map(|view| view.cursor_pos()) else {
            return;
        };
        self.replace_from(pos, backward, true);
    }

    /// "All" from `pos` on: the matches found in one pass on the worker, then replaced in one step (R7
    /// review F2: one search after another through the waiting's end, a stack frame and a copy of the
    /// document each, the screen never drawn meanwhile).
    pub(super) fn replace_all_from(&mut self, pos: u64, backward: bool) {
        let Screen::Editor(editor) = &self.screen else {
            return;
        };
        let params = self.viewer_search.clone();
        let (Some(view), Some(doc), Ok(Some((pattern, shown)))) = (
            editor.text.view(),
            editor.text.document_to_read(),
            pattern(&params),
        ) else {
            return;
        };
        let file = (doc, view.encoding().clone());
        let source = super::editor_search::Source::of(editor);
        let range = Range {
            from: pos,
            to: if backward { 0 } else { u64::MAX },
            backward,
        };
        let line = format!("Replacing {shown}");
        self.wait_for_stoppable(vec![line], move || {
            let found = run_all(&source, file, &pattern, range);
            Box::new(move |app: &mut App| match found {
                Ok(found) if found.is_empty() => {}
                Ok(found) => {
                    let with = app.viewer_search.replace.clone();
                    if let Screen::Editor(editor) = &mut app.screen {
                        editor.text.press(Step::ReplaceAll(found, with, backward));
                    }
                }
                Err(error) => app.show_error("Replace", &error),
            })
        });
    }

    /// The next match from `pos` back or forward; `first`: nothing found at all is said. Once the
    /// steps asked before are done (R7 review F3: the next match was found in a copy without the
    /// replacement still waiting for its blocks).
    pub(super) fn replace_from(&mut self, pos: u64, backward: bool, first: bool) {
        let Screen::Editor(editor) = &self.screen else {
            return;
        };
        let params = self.viewer_search.clone();
        let (Some(view), Some(doc), Ok(Some((pattern, shown)))) = (
            editor.text.view(),
            editor.text.document_to_read(),
            pattern(&params),
        ) else {
            return;
        };
        let file = (doc, view.encoding().clone());
        let source = super::editor_search::Source::of(editor);
        let range = Range {
            from: pos,
            to: if backward { 0 } else { u64::MAX },
            backward,
        };
        self.wait_for_stoppable(vec![format!("Searching for {shown}")], move || {
            let found = run(&source, file, &pattern, range);
            let stopped = crate::vfs::stop::current().load(std::sync::atomic::Ordering::Relaxed);
            Box::new(move |app: &mut App| {
                if !stopped {
                    app.replace_found(found, backward, (first, &shown));
                }
            })
        });
    }

    fn replace_found(
        &mut self,
        found: std::io::Result<Found>,
        backward: bool,
        (first, shown): (bool, &str),
    ) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        match found {
            Ok(Some((at, size, text))) => {
                editor.text.show_found(at, size);
                let quote = |text: &str| format!("\"{text}\"");
                let with = self.viewer_search.replace.clone();
                let lines = [quote(&text), quote(&with)];
                let lines = [
                    Line::Centered("Replace".to_string()),
                    Line::Centered(lines[0].clone()),
                    Line::Centered("with".to_string()),
                    Line::Centered(lines[1].clone()),
                ];
                let buttons = ["&Replace", "&All", "&Skip", "&Cancel"];
                let form = message_form("Replace", &lines, &buttons);
                self.show_dialog(Dialog::Form {
                    form,
                    action: FormAction::EditorReplaceAsk { at, size, backward },
                });
            }
            Ok(None) if first => {
                let lines = vec!["Could not find the string".to_string(), shown.to_string()];
                self.show_dialog(Dialog::error("Search", lines));
            }
            Ok(None) => {}
            Err(error) => self.show_error("Replace", &error),
        }
    }

    /// FAR's question answered: Replace (and on), All (and the rest), Skip (on), Cancel.
    pub(super) fn answer_editor_replace(
        &mut self,
        button: Option<usize>,
        (at, size, backward): (u64, u64, bool),
    ) {
        let Screen::Editor(editor) = &mut self.screen else {
            return;
        };
        let with = self.viewer_search.replace.clone();
        let replaced = if backward {
            at
        } else {
            at + encoded_len(editor, &with)
        };
        // The next search once the steps before it are done (the editor's pending action).
        let on = |pos: u64, all: bool| EditorAction::ReplaceFrom { pos, backward, all };
        match button {
            Some(ASK_REPLACE) => {
                editor.text.press(Step::Replace(at, size, with, false));
                self.handle_editor_action(on(replaced, false));
            }
            // This match and the rest, in one pass from it.
            Some(ASK_ALL) => {
                self.handle_editor_action(on(if backward { at + size } else { at }, true))
            }
            Some(ASK_SKIP) => {
                let next = if backward { at } else { at + size.max(1) };
                self.handle_editor_action(on(next, false));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "editor_replace_tests.rs"]
mod tests;
