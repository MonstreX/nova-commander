//! The viewer's search (FAR's `Viewer::ShowSearchReplaceDialog` with `GetSearchReplaceString`, and
//! `DoSearchReplace`): F7's "Search" - the text or its bytes (Text / Hex, the one turned into the other
//! through the code page shown), "Case sensitive", "Whole words", "Fuzzy search", "Regular
//! expressions", "Find previous" / "Find next"; Shift+F7 and Space go on forward, Alt+F7 back, from
//! what was found (else from the screen's start, back from its end). The file is looked through in
//! the background (`editor::view::search`), "Searching for" shown with Esc; at the end FAR's "End of the
//! document reached" asks whether to go on from the beginning (back: from the end); what is found is
//! selected and shown - where it is when on the screen, else a quarter of the screen from the top.

use super::Viewer;
use crate::app::delete::{Line, message_form};
use crate::app::find::dialog::{hex_text, parse_hex};
use crate::app::form::{Control, EditHistory, Form, Item};
use crate::app::line_input::LineInput;
use crate::app::masked_input::MaskedInput;
use crate::editor::view::search::{self, Pattern, Range};
use crate::editor::view::{Encoding, Move};
use crate::jobs::find::text::{decode_bytes, encode_text};

/// FAR's dialog's items (`dlg_*`, the ones a viewer shows).
const RADIO_TEXT: usize = 0;
const RADIO_HEX: usize = 1;
const TEXT: usize = 3;
const HEX: usize = 4;
const CASE: usize = 6;
const WORDS: usize = 7;
const FUZZY: usize = 8;
const REGEX: usize = 9;
const PREVIOUS: usize = 11;
const NEXT: usize = 12;
/// FAR's `HexMask(64)`.
const HEX_BYTES: usize = 64;

/// What the dialog was left with, kept for the session's viewers (FAR's shared search parameters).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchParams {
    pub text: String,
    /// The editor's "Replace with".
    pub replace: String,
    pub hex: String,
    pub hex_mode: bool,
    pub case_sensitive: bool,
    pub whole_words: bool,
    pub fuzzy: bool,
    pub regex: bool,
}

/// A search's state in a viewer (FAR's `SelectPos`, `SelectSize`): what was found, its first byte
/// and its bytes.
#[derive(Debug, Clone, Copy, Default)]
pub struct Searched {
    pub found: Option<(u64, u64)>,
}

fn radio(x: u16, label: &str, checked: bool, group: bool) -> Item {
    let label = label.to_string();
    Item::new(
        x,
        2,
        Control::Radio {
            label,
            checked,
            group,
        },
    )
}

fn check(x: u16, y: u16, label: &str, checked: bool) -> Item {
    let label = label.to_string();
    Item::new(x, y, Control::Check { label, checked })
}

fn button(label: &str, default: bool) -> Item {
    let label = label.to_string();
    Item::new(0, 9, Control::Button { label, default })
}

/// FAR's `GetSearchReplaceString` for the viewer (76 by 12; "Find previous" and "Find next").
pub fn form(params: &SearchParams) -> Form {
    let mut text = LineInput::with_text(&params.text);
    text.select_all();
    let mask = vec!["HH"; HEX_BYTES].join(" ");
    let items = vec![
        radio(17, "&Text", !params.hex_mode, true),
        radio(27, "&Hex", params.hex_mode, false),
        Item::new(5, 2, Control::Text("Search for".to_string())),
        Item::new(5, 3, Control::Edit(text))
            .wide(66)
            .with_history(EditHistory::new("SearchText").use_last()),
        Item::new(5, 3, Control::Masked(MaskedInput::new(&mask, &params.hex))).wide(66),
        Item::new(0, 4, Control::Separator),
        check(5, 5, "&Case sensitive", params.case_sensitive),
        check(5, 6, "&Whole words", params.whole_words),
        check(5, 7, "Fu&zzy search", params.fuzzy),
        check(39, 5, "R&egular expressions", params.regex),
        Item::new(0, 8, Control::Separator),
        button("Find pre&vious", false),
        button("Find next", true),
        button("Cancel", false),
    ];
    let mut form = Form::new("Search", 76, 12, items);
    enable(&mut form, params.hex_mode);
    // FAR's `DIF_FOCUS` on the line shown.
    form.focus_item(if params.hex_mode { HEX } else { TEXT });
    form
}

/// FAR's `DM_SHOWITEM` / `DM_ENABLE` of Text and Hex, "Fuzzy search" off with a regular expression.
fn enable(form: &mut Form, hex: bool) {
    form.items[TEXT].hidden = hex;
    form.items[HEX].hidden = !hex;
    for index in [CASE, WORDS, REGEX] {
        form.items[index].disabled = hex;
    }
    form.items[FUZZY].disabled = hex || form.is_checked(REGEX);
}

/// After a key in the dialog (FAR's `DN_BTNCLICK` of Text and Hex): the other line shown with the same
/// thing in it, turned through the code page `page`.
pub fn follow(form: &mut Form, page: u32) {
    let hex = form.is_checked(RADIO_HEX);
    if hex != form.items[TEXT].hidden {
        if hex {
            let bytes = encode_text(page, form.edit_text(TEXT));
            form.set_masked_text(HEX, &hex_text(&bytes, " "));
        } else {
            let bytes = parse_hex(&form.masked_text(HEX));
            form.set_edit_text(TEXT, &decode_bytes(page, &bytes));
        }
    }
    enable(form, hex);
}

/// What the dialog says.
pub fn read(form: &Form) -> SearchParams {
    SearchParams {
        text: form.edit_text(TEXT).to_string(),
        replace: String::new(),
        hex: form.masked_text(HEX).trim_end().to_string(),
        hex_mode: form.is_checked(RADIO_HEX) && !form.is_checked(RADIO_TEXT),
        case_sensitive: form.is_checked(CASE),
        whole_words: form.is_checked(WORDS),
        fuzzy: form.is_checked(FUZZY),
        regex: form.is_checked(REGEX),
    }
}

/// The dialog's button: back, forward, or neither (Cancel, Esc).
pub fn direction(button: Option<usize>) -> Option<bool> {
    match button {
        Some(PREVIOUS) => Some(true),
        Some(NEXT) => Some(false),
        _ => None,
    }
}

/// What to look for, and how FAR's messages show it (the text quoted, the bytes as hex); an error of
/// the regular expression otherwise.
pub fn pattern(params: &SearchParams) -> Result<Option<(Pattern, String)>, String> {
    if params.hex_mode {
        let bytes = parse_hex(&params.hex);
        let shown = hex_text(&bytes, " ");
        return Ok((!bytes.is_empty()).then_some((Pattern::Bytes(bytes), shown)));
    }
    if params.text.is_empty() {
        return Ok(None);
    }
    if params.regex {
        // FAR matches a line at a time: `^` and `$` at each line's ends, `\r\n` one break (review A6).
        let built = regex::RegexBuilder::new(&params.text)
            .case_insensitive(!params.case_sensitive)
            .multi_line(true)
            .crlf(true)
            .build()
            .map_err(|error| error.to_string())?;
        return Ok(Some((Pattern::Regex(built), params.text.clone())));
    }
    let pattern = Pattern::Text {
        text: params.text.clone(),
        case_sensitive: params.case_sensitive,
        whole_words: params.whole_words,
        fuzzy: params.fuzzy,
    };
    Ok(Some((pattern, format!("\"{}\"", params.text))))
}

/// FAR's fuzzy folding of one character.
fn fold(ch: char) -> char {
    let folded = crate::platform::locale::fuzzy_fold(ch.encode_utf8(&mut [0; 4]));
    folded.chars().next().unwrap_or(ch)
}

/// The search done on the worker: the first match in `range` of the viewer's file, until Esc.
fn run(
    path: &std::path::Path,
    (encoding, pattern): (&Encoding, &Pattern),
    range: Range,
) -> std::io::Result<Option<(u64, u64)>> {
    let place = crate::vfs::VfsPath::local(path);
    let mut file = crate::vfs::provider_for(&place)?.open_random(&place)?;
    let stop = crate::vfs::stop::current();
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    search::find(file.as_mut(), (encoding, pattern), range, (&fold, &stopped))
}

impl Viewer {
    /// Where a search starts (FAR's `DoSearchReplace`): on from what was found (its next byte, back
    /// from its last), else the screen's start (back: its end).
    fn search_start(&self, backward: bool, going_on: bool) -> u64 {
        match (going_on, self.searched.found) {
            (true, Some((at, size))) if backward => at + size.saturating_sub(1),
            (true, Some((at, _))) => at + 1,
            _ => {
                let (begin, end) = self.text.span();
                if backward { end } else { begin }
            }
        }
    }

    /// What was found selected and shown: where it is when it is on the screen, else from its row a
    /// quarter of the screen from the top (FAR's `SelectText` and `Up(FromTop)`).
    pub(super) fn show_found(&mut self, at: u64, size: u64) {
        self.searched = Searched {
            found: Some((at, size)),
        };
        self.text.select(Some((at, size)));
        let (begin, end) = self.text.span();
        if at >= begin && at + size <= end {
            return;
        }
        self.text.press(Move::Show(at));
        let (_, height) = self.text.room.get();
        for _ in 0..(usize::from(height).saturating_sub(1) / 4) {
            self.text.press(Move::Up);
        }
    }
}

/// The question at the file's end (FAR's `MViewSearchEod` / `MViewSearchBod` with Yes and Cancel).
pub fn wrap_form(backward: bool, shown: &str) -> Form {
    let (reached, go_on) = match backward {
        false => (
            "End of the document reached",
            "Continue from the beginning of the document?",
        ),
        true => (
            "Begin of the document reached",
            "Continue from the end of the document?",
        ),
    };
    let lines = [reached, go_on, shown].map(|line| Line::Centered(line.to_string()));
    message_form("Search", &lines, &["Yes", "Cancel"])
}

/// After a key in a dialog: the viewer's search dialog's Text / Hex followed.
pub fn follow_dialog(dialog: &mut super::super::Dialog) {
    match dialog {
        super::super::Dialog::Form {
            form,
            action: super::super::dialog::FormAction::ViewerSearch(page),
        } => follow(form, *page),
        super::super::Dialog::Form {
            form,
            action: super::super::dialog::FormAction::EditorSearch,
        } => crate::app::editor_search_form::follow(form),
        _ => {}
    }
}

/// The wrap question's "Yes".
pub const WRAP_YES: usize = 4;

impl super::super::App {
    /// F7: FAR's "Search" dialog.
    pub(in crate::app) fn show_viewer_search(&mut self) {
        let super::super::Screen::Viewer(viewer) = &self.screen else {
            return;
        };
        let page = viewer.text.page_number().unwrap_or(65001);
        let form = form(&self.viewer_search);
        let action = super::super::dialog::FormAction::ViewerSearch(page);
        self.show_dialog(super::super::Dialog::Form { form, action });
    }

    /// The dialog answered: the search back or forward from the screen.
    pub(in crate::app) fn finish_viewer_search(&mut self, form: &Form, button: Option<usize>) {
        let replace = std::mem::take(&mut self.viewer_search.replace);
        self.viewer_search = SearchParams {
            replace,
            ..read(form)
        };
        if let Some(backward) = direction(button) {
            self.viewer_search_from(backward, false);
        }
    }

    /// Shift+F7 / Space (forward), Alt+F7 (back): on from what was found.
    pub(in crate::app) fn viewer_search_from(&mut self, backward: bool, going_on: bool) {
        let super::super::Screen::Viewer(viewer) = &self.screen else {
            return;
        };
        let start = viewer.search_start(backward, going_on);
        let len = viewer.text.facts().map_or(0, |facts| facts.len);
        // From the very end it reached the search does not ask to go round.
        let can_wrap = if backward { start < len } else { start > 0 };
        let range = match backward {
            true => Range {
                from: start,
                to: 0,
                backward,
            },
            false => Range {
                from: start,
                to: u64::MAX,
                backward,
            },
        };
        self.viewer_search_in(range, can_wrap);
    }

    /// The search over `range`; at the end without a match the wrap question when `can_wrap` (the
    /// search did not start at the end it reached).
    fn viewer_search_in(&mut self, range: Range, can_wrap: bool) {
        let super::super::Screen::Viewer(viewer) = &self.screen else {
            return;
        };
        let (Some(encoding), Ok(Some((pattern, shown)))) =
            (viewer.text.encoding(), pattern(&self.viewer_search))
        else {
            if let Err(error) = pattern(&self.viewer_search) {
                let lines = vec![error, self.viewer_search.text.clone()];
                self.show_dialog(super::super::Dialog::error("Error", lines));
            }
            return;
        };
        let path = viewer.path.clone();
        let hex = self.viewer_search.hex_mode;
        let searching = if hex {
            "Searching for bytes"
        } else {
            "Searching for"
        };
        let line = format!("{searching} {shown}");
        self.wait_for_stoppable(vec![line], move || {
            let found = run(&path, (&encoding, &pattern), range);
            // Esc's stop: nothing more is said or asked.
            let stopped = crate::vfs::stop::current().load(std::sync::atomic::Ordering::Relaxed);
            Box::new(move |app: &mut super::super::App| {
                if !stopped {
                    app.viewer_found(found, (range, can_wrap), (&shown, hex));
                }
            })
        });
    }

    /// A search's end: the match shown, or the wrap question, or FAR's "Could not find".
    fn viewer_found(
        &mut self,
        found: std::io::Result<Option<(u64, u64)>>,
        (range, can_wrap): (Range, bool),
        (shown, hex): (&str, bool),
    ) {
        let super::super::Screen::Viewer(viewer) = &mut self.screen else {
            return;
        };
        match found {
            Ok(Some((at, size))) => viewer.show_found(at, size),
            Ok(None) if can_wrap => {
                let form = wrap_form(range.backward, shown);
                let action = super::super::dialog::FormAction::ViewerSearchWrap {
                    backward: range.backward,
                    start: range.from,
                };
                self.show_dialog(super::super::Dialog::Form { form, action });
            }
            Ok(None) => {
                let what = if hex {
                    "Could not find the bytes"
                } else {
                    "Could not find the string"
                };
                let lines = vec![what.to_string(), shown.to_string()];
                self.show_dialog(super::super::Dialog::error("Search", lines));
            }
            Err(error) => self.show_error("Search", &error),
        }
    }

    /// The wrap question answered: Yes looks through the rest, from the other end to where it began.
    pub(in crate::app) fn finish_viewer_search_wrap(
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
        self.viewer_search_in(range, false);
    }
}

#[cfg(test)]
#[path = "viewer_search_tests.rs"]
mod tests;
