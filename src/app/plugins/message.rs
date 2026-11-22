//! A plugin's message as FAR lays it out (`message.cpp` `MessageImpl`, ADR-0016 §15 E11): as wide as its
//! longest line, its title with a space each side or its buttons, at most the screen less ten columns; a
//! line starting with `\u{1}` (`\u{2}`) a separator (double) with the rest as its title in its middle, a
//! last one standing for the separator above the buttons; a line longer than that a read-only line that
//! scrolls (cut on the right in a window without buttons, FAR's `fit_to_center`); the system's error text
//! wrapped under a separator; the first button focused. A plugin's input line as FAR's `GetString`
//! (`stddlg.cpp`, E13): the prompt above a line 66 wide, Enter on it closing, or "OK" / "Cancel" under it.

use crate::app::LineInput;
use crate::app::form::{Control, EditHistory, Form, Item};
use crate::app::text_cut::{cut_right, truncate_right};
use crate::app::vmenu_keys::shown_chars;
use crate::plugin_host::windows::{InputRequest, MessageRequest};

/// FAR's separator marks at a line's start.
const SEPARATOR: char = '\u{1}';
const DOUBLE_SEPARATOR: char = '\u{2}';

/// The form of `message` titled `title` on a screen `columns` wide with room for `rows` lines.
pub(in crate::app) fn message_form(
    title: &str,
    message: &MessageRequest,
    columns: u16,
    rows: usize,
) -> Form {
    let mut lines: Vec<String> = message.lines.iter().take(rows).cloned().collect();
    let buttons_width = buttons_width(&message.buttons);
    // FAR's `MAX_MESSAGE_WIDTH`: the screen less ten columns, unless the buttons are wider.
    let most = usize::from(columns)
        .saturating_sub(11)
        .max(buttons_width)
        .max(1);
    let mut longest = (lines.iter().map(|line| line.chars().count()))
        .chain([buttons_width])
        .chain((!title.is_empty()).then(|| title.chars().count() + 2))
        .max()
        .unwrap_or(0)
        .min(most);
    if let Some(error) = &message.error {
        if !lines.is_empty() {
            lines.push(SEPARATOR.to_string());
        }
        for line in wrapped(error, most) {
            longest = longest.max(line.chars().count());
            lines.push(line);
        }
    }
    let width = longest + 10;
    let mut items: Vec<Item> = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let y = u16::try_from(row + 2).unwrap_or(u16::MAX);
        items.push(line_item(line, y, width, message));
    }
    let below = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let form_width = u16::try_from(width).unwrap_or(u16::MAX);
    if message.buttons.is_empty() {
        return with_warning(
            Form::new(&cut_right(title, longest), form_width, below + 2, items),
            message,
        );
    }
    // A last separator line stands for the one above the buttons (FAR's `StrSeparator`).
    let (buttons_y, height) = if lines.last().is_some_and(|line| separator(line).is_some()) {
        (below, below + 3)
    } else {
        items.push(Item::new(0, below, Control::Separator));
        (below + 1, below + 4)
    };
    let first_button = items.len();
    items.extend(message.buttons.iter().enumerate().map(|(index, label)| {
        Item::new(
            0,
            buttons_y,
            Control::Button {
                label: label.clone(),
                default: index == 0,
            },
        )
    }));
    let mut form = Form::new(&cut_right(title, longest), form_width, height, items);
    form.focus = first_button;
    with_warning(form, message)
}

/// The items of a plugin's input line: its edit line, and its "OK" with `FIB_BUTTONS`.
pub(in crate::app) const INPUT_LINE: usize = 1;
pub(in crate::app) const INPUT_OK: usize = 3;

/// FAR's `GetString` for `input`: a box 76 wide, the prompt (cut to 66) above the line, the line taking
/// Enter (`DIF_DEFAULTBUTTON`) unless "OK" and "Cancel" are under it; stars for a password.
pub(in crate::app) fn input_form(input: &InputRequest) -> Form {
    let prompt = truncate_right(&input.prompt, 66);
    let prompt = if input.no_ampersand {
        Control::Text(prompt)
    } else {
        Control::Label(prompt)
    };
    let mut line = Item::new(5, 3, Control::Edit(LineInput::with_text(&input.text))).wide(66);
    if input.password {
        line = line.password();
    } else if let Some(name) = &input.history {
        // FAR's `GetString`: `DIF_HISTORY`, and `DIF_USELASTHISTORY` but with `FIB_NOUSELASTHISTORY`.
        let history = EditHistory::new(name);
        line = line.with_history(match input.no_last_history {
            true => history,
            false => history.use_last(),
        });
    }
    if !input.buttons {
        line = line.closes();
    }
    let mut items = vec![Item::new(5, 2, prompt), line];
    if input.buttons {
        items.push(Item::new(0, 4, Control::Separator));
        for (label, default) in [("OK", true), ("Cancel", false)] {
            let label = label.to_string();
            items.push(Item::new(0, 5, Control::Button { label, default }));
        }
    }
    let height = if input.buttons { 8 } else { 6 };
    let mut form = Form::new(&input.title, 76, height, items);
    form.focus = INPUT_LINE;
    form
}

fn with_warning(form: Form, message: &MessageRequest) -> Form {
    if message.warning {
        form.warning()
    } else {
        form
    }
}

/// The buttons' row as FAR counts it: "[ label ]" each, a space between.
fn buttons_width(buttons: &[String]) -> usize {
    let sum: usize = (buttons.iter())
        .map(|label| shown_chars(label).len() + 5)
        .sum();
    sum.saturating_sub(1)
}

/// A separator line: whether double, and its title.
fn separator(line: &str) -> Option<(bool, &str)> {
    if let Some(title) = line.strip_prefix(SEPARATOR) {
        Some((false, title))
    } else {
        line.strip_prefix(DOUBLE_SEPARATOR)
            .map(|title| (true, title))
    }
}

/// One line of the message at row `y` of a box `width` wide.
fn line_item(line: &str, y: u16, width: usize, message: &MessageRequest) -> Item {
    if let Some((double, title)) = separator(line) {
        let title = title.to_string();
        // FAR's `MessageImpl`: `X1` is 5 for every line of a message from the left; without buttons the
        // title is centred all the same (FAR draws that window itself).
        let left = message.left_align && !message.buttons.is_empty();
        let x = if left { 5 } else { 0 };
        return Item::new(
            x,
            y,
            Control::TitledSeparator {
                double,
                title,
                left,
            },
        );
    }
    let room = width - 10;
    let length = line.chars().count();
    if length > room {
        if message.buttons.is_empty() {
            // FAR's `fit_to_center` / `fit_to_left`: cut by `truncate_right`, `…` last.
            return Item::new(5, y, Control::Label(truncate_right(line, room)));
        }
        // FAR's `DI_EDIT` with `DIF_READONLY`: the whole line, scrolled.
        let wide = u16::try_from(room).unwrap_or(u16::MAX);
        return Item::new(5, y, Control::Edit(LineInput::with_text(line)))
            .wide(wide)
            .read_only();
    }
    let x = if message.left_align {
        5
    } else {
        5 + (room - length) / 2
    };
    Item::new(
        u16::try_from(x).unwrap_or(5),
        y,
        Control::Label(line.to_string()),
    )
}

/// `text` in lines of at most `width` characters, broken between words (FAR's `wrapped_text`); a word
/// longer than a line broken where the line ends.
pub(in crate::app) fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split(' ').filter(|word| !word.is_empty()) {
        let mut word: Vec<char> = word.chars().collect();
        let used = line.chars().count();
        if used > 0 && used + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        while word.len() > width {
            let rest = word.split_off(width);
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            lines.push(word.iter().collect());
            word = rest;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.extend(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
