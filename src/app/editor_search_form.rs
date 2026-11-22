//! FAR's "Search" dialog in the editor (`Editor::ShowSearchReplaceDialog` with
//! `GetSearchReplaceString`; S2d.6e): no Hex (the editor's parameters have none), "W&ord" and
//! "Se&lection" on the right of the first row putting the word at the cursor or the block's text
//! (else the line) into "Search for", the four options, "Find previous", "Find next", "All", "Cancel".

use crate::app::form::{Control, EditHistory, Form, Item, Place};
use crate::app::line_input::LineInput;
use crate::app::viewer::search::SearchParams;

/// FAR's dialog's items (`dlg_*`, the ones the editor shows).
pub const WORD: usize = 0;
pub const SELECTION: usize = 1;
pub const TEXT: usize = 3;
const CASE: usize = 5;
const WORDS: usize = 6;
const FUZZY: usize = 7;
const REGEX: usize = 8;
pub const PREVIOUS: usize = 10;
pub const NEXT: usize = 11;
pub const ALL: usize = 12;

fn check(x: u16, y: u16, label: &str, checked: bool) -> Item {
    let label = label.to_string();
    Item::new(x, y, Control::Check { label, checked })
}

fn button(x: u16, y: u16, label: &str, default: bool) -> Item {
    let label = label.to_string();
    Item::new(x, y, Control::Button { label, default })
}

/// The dialog with the session's parameters (FAR's 76 by 12, `YFix` 2 without "Replace with").
pub fn form(params: &SearchParams) -> Form {
    let mut text = LineInput::with_text(&params.text);
    text.select_all();
    let items = vec![
        // FAR's `WordButtonX1` and `SelectionButtonX1`: right-aligned to the box's inner edge.
        button(49, 2, "W&ord", false).placed(Place::At),
        button(58, 2, "Se&lection", false).placed(Place::At),
        Item::new(5, 2, Control::Text("&Search for".to_string())),
        Item::new(5, 3, Control::Edit(text))
            .wide(66)
            .with_history(EditHistory::new("SearchText").use_last()),
        Item::new(0, 4, Control::Separator),
        check(5, 5, "&Case sensitive", params.case_sensitive),
        check(5, 6, "&Whole words", params.whole_words),
        check(5, 7, "Fu&zzy search", params.fuzzy),
        check(39, 5, "R&egular expressions", params.regex),
        Item::new(0, 8, Control::Separator),
        button(0, 9, "Find pre&vious", false),
        button(0, 9, "Find next", true),
        button(0, 9, "&All", false),
        button(0, 9, "Cancel", false),
    ];
    let mut form = Form::new("Search", 76, 12, items);
    follow(&mut form);
    form.focus_item(TEXT);
    form
}

/// FAR's `DN_BTNCLICK` of "Regular expressions": "Fuzzy search" off with it.
pub fn follow(form: &mut Form) {
    form.items[FUZZY].disabled = form.is_checked(REGEX);
}

/// The parameters the dialog was left with; the viewer's Hex and the replace text as they were.
pub fn read(form: &Form, old: &SearchParams) -> SearchParams {
    SearchParams {
        text: form.edit_text(TEXT).to_string(),
        case_sensitive: form.is_checked(CASE),
        whole_words: form.is_checked(WORDS),
        fuzzy: form.is_checked(FUZZY),
        regex: form.is_checked(REGEX),
        ..old.clone()
    }
}

/// FAR's `Picker`: the word at the cursor (`FindWordInString` with `WordDiv`, the blanks and the
/// breaks), or the block's text on its first line, else the cursor's line (`selection`).
pub fn picked(line: &[char], cursor: usize, block: Option<&str>, selection: bool) -> String {
    if selection {
        return match block {
            Some(block) => block.lines().next().unwrap_or_default().to_string(),
            None => line.iter().collect(),
        };
    }
    if line.is_empty() || cursor > line.len() {
        return String::new();
    }
    let divider =
        |ch: &char| ch.is_whitespace() || crate::editor::view::search::WORD_DIVIDERS.contains(*ch);
    let begin = (line[..cursor].iter())
        .rposition(divider)
        .map_or(0, |at| at + 1);
    let end = (line[cursor..].iter())
        .position(divider)
        .map_or(line.len(), |at| cursor + at);
    line[begin..end.max(begin)].iter().collect()
}
