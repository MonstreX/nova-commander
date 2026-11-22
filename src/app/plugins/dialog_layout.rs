//! A plugin's dialog laid out as FAR's dialogs are (ADR-0016 §11): its items in rows from the top (an
//! item on the same row as the one before when the plugin asks, consecutive buttons in one row), each row
//! from the dialog's left margin, an edit line or a list without a width taking what the row leaves (a
//! history's `↓` after it), a list box its rows in its box; the buttons ending the dialog centred under a
//! separator. As wide as the widest row, at least 40 columns, at most the screen. A dialog the plugin
//! gives a size is placed: each item at its own coordinates (FAR's `X1`, `Y1`, `X2`, `Y2`).

use crate::app::LineInput;
use crate::app::form::{Control, EditHistory, Form, Item, ListBox, ListLine, Place};
use crate::app::masked_input::MaskedInput;
use crate::plugin_host::windows::{DialogItem, DialogRequest, ItemKind};

/// Where the rows start, and the columns the box and its margins take on both sides.
const LEFT: u16 = 5;
const MARGINS: u16 = 10;
/// The least columns for the items (FAR's dialogs are at least 40 wide).
const LEAST: u16 = 30;
/// The least columns an edit line or a list without a width is given in a row.
const FILL: u16 = 10;

/// The dialog's form and, for each of the plugin's items, the form's item showing it.
#[derive(Debug)]
pub struct Laid {
    pub form: Form,
    pub items: Vec<usize>,
}

/// `text`'s columns as shown: `&` marking a hotkey takes none, `&&` is one.
fn shown_width(text: &str) -> u16 {
    let mut columns = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' && chars.peek() == Some(&'&') {
            chars.next();
        } else if ch == '&' {
            continue;
        }
        columns += 1;
    }
    u16::try_from(columns).unwrap_or(u16::MAX)
}

/// Whether the item is as wide as it is given, or as the row lets it be: a line or a list.
fn is_field(item: &DialogItem) -> bool {
    match item.kind {
        ItemKind::Edit | ItemKind::Password | ItemKind::List | ItemKind::ListBox => true,
        ItemKind::Fixed => item.mask.is_empty(),
        _ => false,
    }
}

/// Whether the item takes the row's columns left when it has no width of its own.
fn fills(item: &DialogItem) -> bool {
    is_field(item) && item.width == 0
}

/// The most lines a list box shows: no screen has more rows.
const MOST_BOX_ROWS: u16 = 500;

/// The lines a list box shows: as the plugin says, else as many as it has up to 8 (FAR's combo box's).
fn box_rows(item: &DialogItem) -> u16 {
    match item.height {
        0 => u16::try_from(item.lines.len().clamp(1, crate::app::form::LIST_ROWS)).unwrap_or(1),
        rows => rows.min(MOST_BOX_ROWS),
    }
}

/// The rows an item takes: a list box its lines and its box.
fn item_rows(item: &DialogItem) -> u16 {
    match item.kind {
        ItemKind::ListBox => box_rows(item).saturating_add(2),
        _ => 1,
    }
}

/// The columns an item takes by itself (a filling one: [`FILL`]).
fn natural(item: &DialogItem) -> u16 {
    match item.kind {
        ItemKind::Text => shown_width(&item.text),
        ItemKind::Check | ItemKind::Radio | ItemKind::Button | ItemKind::Box => {
            shown_width(&item.text).saturating_add(4)
        }
        ItemKind::Separator => 0,
        ItemKind::Fixed if !item.mask.is_empty() => {
            u16::try_from(item.mask.chars().count()).unwrap_or(u16::MAX)
        }
        _ if fills(item) => {
            let longest = (item.lines.iter())
                .map(|line| shown_width(line).saturating_add(3))
                .max();
            longest.unwrap_or(0).max(FILL)
        }
        _ => item.width,
    }
}

/// The items in rows: a new row for each item but one on the same row as the one before or a button
/// after a button; a separator always a row of its own.
fn rows(items: &[DialogItem]) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let before = index.checked_sub(1).map(|before| &items[before]);
        let joins = before.is_some_and(|before| {
            let separator = |item: &DialogItem| item.kind == ItemKind::Separator;
            let buttons = before.kind == ItemKind::Button && item.kind == ItemKind::Button;
            !separator(before) && !separator(item) && (item.same_row || buttons)
        });
        match rows.last_mut() {
            Some(row) if joins => row.push(index),
            _ => rows.push(vec![index]),
        }
    }
    rows
}

fn row_width(items: &[DialogItem], row: &[usize]) -> u16 {
    let gaps = u16::try_from(row.len().saturating_sub(1)).unwrap_or(u16::MAX);
    row.iter()
        .map(|&index| natural(&items[index]))
        .fold(gaps, u16::saturating_add)
}

/// The plugin's dialog as a form on a screen `screen_width` columns wide: placed when it has a size.
pub fn lay_out(request: &DialogRequest, screen_width: u16) -> Laid {
    if let Some(size) = request.size {
        return placed(request, size);
    }
    let items = &request.items;
    let rows = rows(items);
    let widest = rows
        .iter()
        .map(|row| row_width(items, row))
        .max()
        .unwrap_or(0);
    let title = shown_width(&request.title).saturating_add(4);
    let most = screen_width.saturating_sub(MARGINS).max(1);
    let content = widest.max(LEAST).max(title).min(most);
    let mut placed: Vec<Item> = Vec::new();
    let mut map = vec![0; items.len()];
    let mut y = 2;
    for (number, row) in rows.iter().enumerate() {
        let buttons = row
            .iter()
            .all(|&index| items[index].kind == ItemKind::Button);
        let after_separator = number
            .checked_sub(1)
            .is_some_and(|before| items[rows[before][0]].kind == ItemKind::Separator);
        // FAR's dialogs: the buttons ending them under a separator.
        if buttons && number + 1 == rows.len() && number > 0 && !after_separator {
            placed.push(Item::new(0, y, Control::Separator));
            y = y.saturating_add(1);
        }
        let mut x = LEFT;
        for (at, &index) in row.iter().enumerate() {
            let item = &items[index];
            let later = row_width(items, &row[at + 1..]);
            let later = if at + 1 < row.len() {
                later.saturating_add(1)
            } else {
                0
            };
            let end = LEFT + content;
            let room = end.saturating_sub(x).saturating_sub(later);
            // A line or a list never wider than the dialog lets it be, a width given or not.
            let width = if fills(item) {
                room.max(1)
            } else if is_field(item) {
                natural(item).min(end.saturating_sub(x)).max(1)
            } else {
                natural(item)
            };
            map[index] = placed.len();
            let starts_group = item.group || index == 0 || items[index - 1].kind != ItemKind::Radio;
            // A history's `↓` stands after the line, in the width it was given.
            let width = match item.history {
                Some(_) if item.kind == ItemKind::Edit => width.saturating_sub(1).max(1),
                _ => width,
            };
            placed.push(item_at(item, (x, y), width, (buttons, starts_group)));
            x = x.saturating_add(width).saturating_add(1);
        }
        let rows = row.iter().map(|&index| item_rows(&items[index])).max();
        y = y.saturating_add(rows.unwrap_or(1));
    }
    finish(
        request,
        Form::new(
            &request.title,
            content + MARGINS,
            y.saturating_add(2),
            placed,
        ),
        map,
    )
}

/// The form's first focus and colours as the plugin asks.
fn finish(request: &DialogRequest, mut form: Form, items: Vec<usize>) -> Laid {
    if let Some(first) = request.items.iter().position(|item| item.focus) {
        form.focus_item(items[first]);
    }
    let form = if request.warning {
        form.warning()
    } else {
        form
    };
    Laid { form, items }
}

/// A dialog `width` x `height` with each item at its own coordinates (FAR's `X1`, `Y1`, `X2`, `Y2`
/// inside the dialog): a field from `X1` to `X2` (as wide as `width` says when `X2` is not after it), a
/// list box down to `Y2`, a text with `X1` -1 centred, a button at its own place.
fn placed(request: &DialogRequest, (width, height): (u16, u16)) -> Laid {
    // A plugin's numbers as they are: none of them may overflow.
    let column = |at: i64| u16::try_from(at.max(0)).unwrap_or(u16::MAX);
    let items: Vec<Item> = (request.items.iter())
        .map(|item| {
            let (x1, y1, x2, y2) = item.place;
            let (x1, y1, x2, y2) = (i64::from(x1), i64::from(y1), i64::from(x2), i64::from(y2));
            let wide = match x2 > x1 {
                true => column(x2 - x1 + 1),
                false => item.width.max(1),
            };
            let x = match (x1, item.kind) {
                (-1, ItemKind::Text) => width.saturating_sub(shown_width(&item.text)) / 2,
                _ => column(x1),
            };
            let mut item = item.clone();
            if item.kind == ItemKind::ListBox && y2 > y1 + 1 {
                item.height = column(y2 - y1 - 1).min(MOST_BOX_ROWS);
            }
            if item.kind == ItemKind::Box {
                item.height = column(y2 - y1 + 1).clamp(1, MOST_BOX_ROWS);
            }
            let starts_group = item.group;
            item_at(&item, (x, column(y1)), wide, (false, starts_group))
        })
        .collect();
    let map = (0..items.len()).collect();
    finish(
        request,
        Form::new(&request.title, width, height, items),
        map,
    )
}

/// The form's item for a plugin's item at column `x` of row `y`; a button in a row of buttons centred
/// with them; a radio button starting a group when the plugin says so or the item before it is none (FAR's
/// consecutive radio buttons are one group).
fn item_at(
    item: &DialogItem,
    (x, y): (u16, u16),
    width: u16,
    (in_buttons, starts_group): (bool, bool),
) -> Item {
    let label = item.text.clone();
    let placed = match item.kind {
        ItemKind::Text => Item::new(x, y, Control::Text(label)),
        ItemKind::Edit => edit(item, (x, y), width),
        ItemKind::Fixed if !item.mask.is_empty() => {
            Item::new(x, y, Control::Masked(MaskedInput::new(&item.mask, &label)))
        }
        ItemKind::Fixed => edit(item, (x, y), width),
        ItemKind::ListBox => {
            let lines = item.lines.iter().cloned().map(ListLine::Choice).collect();
            let cursor = usize::try_from(item.state).unwrap_or(0);
            let mut list = ListBox::new(&label, lines, cursor, box_rows(item));
            list.no_close = item.no_close;
            let placed = Item::new(x, y, Control::ListBox(list)).wide(width);
            // Enter on it closes the dialog (the default button, else it), as FAR's; with
            // `DIF_LISTNOCLOSE` Enter does nothing there (`form_listbox`).
            if item.no_close {
                placed
            } else {
                placed.closes()
            }
        }
        ItemKind::Password => edit(item, (x, y), width).password(),
        ItemKind::Check if item.three_state => {
            let state = match item.state {
                1 => Some(true),
                2 => None,
                _ => Some(false),
            };
            let control = Control::Tristate {
                label,
                state,
                three: true,
            };
            Item::new(x, y, control)
        }
        ItemKind::Check => Item::new(
            x,
            y,
            Control::Check {
                label,
                checked: item.state == 1,
            },
        ),
        ItemKind::Radio => Item::new(
            x,
            y,
            Control::Radio {
                label,
                checked: item.state == 1,
                group: starts_group,
            },
        ),
        ItemKind::List => Item::new(x, y, list(item)).wide(width),
        ItemKind::Box => {
            let rows = item.height.max(1);
            Item::new(x, y, Control::Box { title: label, rows }).wide(width)
        }
        ItemKind::Separator if label.is_empty() => Item::new(0, y, Control::Separator),
        // FAR's `DIF_SEPARATOR` with a text: its title in the middle, or at its column when `X1` is given.
        ItemKind::Separator => {
            let left = item.place.0 > 0;
            let control = Control::TitledSeparator {
                double: false,
                title: label,
                left,
            };
            Item::new(if left { x } else { 0 }, y, control)
        }
        ItemKind::Button => {
            let control = Control::Button {
                label,
                default: item.default,
            };
            let button = Item::new(x, y, control);
            if in_buttons {
                button
            } else {
                button.placed(Place::At)
            }
        }
    };
    placed.disabled(item.disabled).hidden(item.hidden)
}

/// An edit line with its history when it has one.
fn edit(item: &DialogItem, (x, y): (u16, u16), width: u16) -> Item {
    let line = Item::new(x, y, Control::Edit(LineInput::with_text(&item.text))).wide(width);
    let line = if item.read_only {
        line.read_only()
    } else {
        line
    };
    match &item.history {
        Some(name) if item.kind == ItemKind::Edit => {
            let history = EditHistory::new(name);
            line.with_history(match item.use_last_history {
                true => history.use_last(),
                false => history,
            })
        }
        _ => line,
    }
}

fn list(item: &DialogItem) -> Control {
    let last = item.lines.len().saturating_sub(1);
    Control::DropDown {
        lines: item.lines.iter().cloned().map(ListLine::Choice).collect(),
        chosen: usize::try_from(item.state).unwrap_or(0).min(last),
        open: None,
        caption: None,
        bottom_title: None,
    }
}

#[cfg(test)]
#[path = "dialog_layout_tests.rs"]
mod tests;
