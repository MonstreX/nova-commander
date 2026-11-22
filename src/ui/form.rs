//! Draws FAR's dialog with items (`app::form`): the margin, the double box with its title, the items at
//! FAR's coordinates, the centred button group and an open drop-down list on top.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear};

#[path = "form_list.rs"]
mod list;

pub(in crate::ui) use list::scroll_bar;

use super::theme::{self, Buttons};
use super::{bars, format, vmenu};
use crate::app::form::{Control, Form, Item, ListLine, Place};
use crate::app::masked_input::MaskedInput;
use crate::app::{Cells, ControlKind, DialogArea, DialogControl, LineInput};

/// Draws `form` centred in `area`; returns the cursor position of the focused item.
/// Draws the form; returns the cursor and where the form and its parts are, for the mouse.
pub fn draw(frame: &mut Frame, area: Rect, form: &Form) -> (Option<Position>, Drawn) {
    let shadow_buttons = theme::look().buttons == Buttons::Shadow;
    // A button's shadow takes the row under the buttons: the box grows by one row.
    let height = form.height + u16::from(shadow_buttons);
    let width = form.width.min(area.width);
    let height = height.min(area.height);
    let rect = super::dialog_place::placed(area, width, height);
    frame.render_widget(Clear, rect);
    frame.render_widget(Block::new().style(theme::dialog(form.warning)), rect);
    let inside = Rect::new(
        rect.x + 3,
        rect.y + 1,
        rect.width.saturating_sub(6),
        rect.height.saturating_sub(2),
    );
    let title_style = if form.warning {
        theme::dialog(true)
    } else {
        theme::dialog_title()
    };
    let block = Block::bordered()
        .border_set(theme::look().dialog_frame.set())
        .style(theme::dialog(form.warning))
        .title(Line::styled(format!(" {} ", form.title), title_style).centered());
    frame.render_widget(block, inside);
    vmenu::draw_shadow(frame.buffer_mut(), rect, area);
    let buffer = frame.buffer_mut();
    let mut cursor = None;
    let mut controls = Vec::new();
    for (index, item) in form.items.iter().enumerate() {
        // A form larger than the screen: what lies past it is not drawn.
        if item.hidden || matches!(item.control, Control::Button { .. }) || !within(item, rect) {
            continue;
        }
        let focused = index == form.focus;
        let at = Position::new(rect.x + item.x, rect.y + item.y);
        let position = draw_item(buffer, item, at, inside, (focused, form.warning));
        if focused {
            cursor = position;
        }
        if !item.disabled {
            controls.extend(control_of(index, item, at));
            // FAR's click on a history's `↓` opens it (as Alt+Down there).
            if item
                .history
                .as_ref()
                .is_some_and(|history| history.has_arrow())
            {
                let cells = cells(Rect::new(at.x + item.width, at.y, 1, 1));
                let kind = ControlKind::DropDown;
                controls.push(DialogControl { cells, index, kind });
            }
        } else if let Some(field) = field_of(item, at) {
            // FAR's `COL_DIALOGEDITDISABLED`.
            buffer.set_style(field, theme::input_field_disabled());
        }
    }
    controls.extend(draw_buttons(buffer, form, rect, shadow_buttons));
    let (list, question) = draw_front_list(buffer, form, (rect, area));
    // Del's question in front of the history list; the place the dialog took stays its own.
    if let Some(question) = question {
        let taken = super::dialog_place::taken();
        draw(frame, area, question);
        super::dialog_place::set_taken(taken);
    }
    let dialog = DialogArea {
        frame: cells(rect),
        controls,
    };
    (cursor, Drawn { dialog, list })
}

/// The focused item's open list, a drop-down field's or an edit line's history, on top; where it is
/// and the question in front of a history list.
fn draw_front_list<'a>(
    buffer: &mut Buffer,
    form: &'a Form,
    (rect, area): (Rect, Rect),
) -> (Option<crate::app::MenuArea>, Option<&'a Form>) {
    let mut list = None;
    let mut question = None;
    match form.items.get(form.focus) {
        Some(
            item @ Item {
                control:
                    Control::DropDown {
                        lines,
                        open: Some(open),
                        bottom_title,
                        ..
                    },
                ..
            },
        ) => {
            let shown = list::Open {
                lines,
                open,
                bottom_title: bottom_title.as_deref(),
                plain: false,
            };
            list = list::draw_open_list(buffer, &shown, (item, rect, area));
        }
        Some(
            item @ Item {
                history: Some(history),
                ..
            },
        ) if history.open.is_some() => {
            let lines = history.lines().lines;
            if let Some(open) = &history.open {
                // FAR's history menu shows a line's `&` (`VMENU_SHOWAMPERSAND`).
                let shown = list::Open {
                    lines: &lines,
                    open,
                    bottom_title: None,
                    plain: true,
                };
                list = list::draw_open_list(buffer, &shown, (item, rect, area));
            }
            question = history.question.as_deref();
        }
        _ => {}
    }
    (list, question)
}

/// Where a form and its parts are, for the mouse.
pub struct Drawn {
    pub dialog: DialogArea,
    /// An open drop-down list.
    pub list: Option<crate::app::MenuArea>,
}

pub(super) fn cells(rect: Rect) -> Cells {
    Cells {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

/// What part of the form item `index` is under the mouse, as FAR's `Dialog::ProcessMouse` hits it.
fn control_of(index: usize, item: &Item, at: Position) -> Option<DialogControl> {
    let row = |width: u16, kind| DialogControl {
        cells: Cells {
            x: at.x,
            y: at.y,
            width,
            height: 1,
        },
        index,
        kind,
    };
    Some(match &item.control {
        Control::Edit(input) => {
            let first = edit_first(input, item.width);
            row(item.width, ControlKind::Edit { first })
        }
        Control::Masked(input) => row(masked_width(item, input), ControlKind::Masked),
        // The mark and the label (FAR: up to `X1 + HiStrlen + 4`).
        Control::Check { label, .. }
        | Control::Tristate { label, .. }
        | Control::Radio { label, .. } => row(4 + visible_width(label), ControlKind::Toggle),
        Control::DropDown { .. } => row(item.width, ControlKind::DropDown),
        // The lines inside the box, from the first shown.
        Control::ListBox(list) => DialogControl {
            cells: Cells {
                x: at.x + 1,
                y: at.y + 1,
                width: item.width.saturating_sub(2),
                height: list.rows,
            },
            index,
            kind: ControlKind::ListBox {
                top: list.list.top.get(),
            },
        },
        // FAR's `DN_CONTROLINPUT` reaches a dialog's own rules for a text too.
        Control::Text(text) if !text.is_empty() => row(visible_width(text), ControlKind::Text),
        _ => return None,
    })
}

/// The cells of an edit line, a masked one or a list's field.
fn field_of(item: &Item, at: Position) -> Option<Rect> {
    let width = match &item.control {
        Control::Edit(_) | Control::DropDown { .. } => item.width,
        Control::Masked(input) => masked_width(item, input),
        _ => return None,
    };
    Some(Rect::new(at.x, at.y, width, 1))
}

/// A masked line is as wide as its item says (FAR's `X2 - X1 + 1`), else as its mask.
fn masked_width(item: &Item, input: &MaskedInput) -> u16 {
    if item.width > 0 {
        item.width
    } else {
        u16::try_from(input.width()).unwrap_or(u16::MAX)
    }
}

/// The first character an edit line `width` columns wide shows (FAR's `LeftPos`).
fn edit_first(input: &LineInput, width: u16) -> usize {
    input.shown_from(usize::from(width))
}

/// One item that is not a button; returns where the cursor goes when it has the focus.
fn draw_item(
    buffer: &mut Buffer,
    item: &Item,
    at: Position,
    inside: Rect,
    (focused, warning): (bool, bool),
) -> Option<Position> {
    let (normal, hotkey) = if item.disabled {
        (theme::dialog_disabled(), theme::dialog_disabled())
    } else {
        (theme::dialog(warning), theme::dialog_hotkey(warning))
    };
    // A label stops at the box's right side (a plugin's may be longer than the dialog).
    let room = usize::from(inside.right().saturating_sub(1).saturating_sub(at.x));
    match &item.control {
        Control::Text(text) => {
            let shown = vmenu::cut_visible(text, room);
            vmenu::hi_text(buffer, at.x, at.y, &shown, normal, hotkey);
            None
        }
        Control::Label(text) => {
            let style = if item.highlighted { hotkey } else { normal };
            buffer.set_string(at.x, at.y, text, style);
            None
        }
        Control::Separator => {
            let line = theme::look()
                .dialog_frame
                .separator(usize::from(inside.width));
            buffer.set_string(inside.x, at.y, line, theme::dialog(warning));
            None
        }
        Control::TitledSeparator { .. } => {
            super::separator::draw_item(buffer, inside, at, &item.control, theme::dialog(warning));
            None
        }
        Control::Edit(input) if item.password => {
            // The same length and cursor, every character a star.
            let mut shown = LineInput::default();
            shown.set_with_cursor(&"*".repeat(input.text().chars().count()), input.cursor());
            Some(draw_edit(buffer, at, item.width, &shown))
        }
        Control::Edit(input) => {
            // FAR's `ItemHasDropDownArrow`: a history's `↓` after the line, in the dialog's colour.
            if item
                .history
                .as_ref()
                .is_some_and(|history| history.has_arrow())
            {
                buffer.set_string(at.x + item.width, at.y, "↓", normal);
            }
            Some(draw_edit(buffer, at, item.width, input))
        }
        Control::Check { .. } | Control::Tristate { .. } | Control::Radio { .. } => {
            let label_room = room.saturating_sub(4);
            draw_toggle(buffer, &item.control, at, label_room, (normal, hotkey))
        }
        Control::Masked(input) => {
            let width = usize::from(masked_width(item, input));
            let style = theme::input_field();
            buffer.set_stringn(at.x, at.y, format::fit(&input.text(), width), width, style);
            let cursor = u16::try_from(input.cursor()).unwrap_or(0);
            Some(Position::new(at.x + cursor, at.y))
        }
        Control::VRule(_) | Control::HRule(_) => {
            draw_rule(buffer, &item.control, at, theme::dialog(warning));
            None
        }
        Control::Box { title, rows } => {
            let style = if item.disabled {
                normal
            } else {
                theme::dialog(warning)
            };
            let title = title.replace('&', "");
            super::separator::draw_box(buffer, at, (item.width, *rows), &title, style);
            None
        }
        Control::DropDown { .. } => {
            draw_drop_down(buffer, item, at);
            focused.then_some(at)
        }
        Control::ListBox(list) => {
            let rows = list.rows.saturating_add(2);
            // A box past the screen's bottom (a tall list, a screen made smaller) drawn as far as it goes.
            let place = Rect::new(at.x, at.y, item.width, rows).intersection(*buffer.area());
            list::draw_list_box(buffer, list, place, focused);
            None
        }
        Control::Button { .. } => None,
    }
}

/// A drop-down field: its chosen line (or its caption) and the `↓`.
fn draw_drop_down(buffer: &mut Buffer, item: &Item, at: Position) {
    let Control::DropDown {
        lines,
        chosen,
        caption,
        ..
    } = &item.control
    else {
        return;
    };
    let text = match (caption, lines.get(*chosen)) {
        (Some(caption), _) => caption.clone(),
        (None, Some(ListLine::Choice(label) | ListLine::Marked { label, .. })) => {
            label.replace('&', "")
        }
        _ => String::new(),
    };
    let width = usize::from(item.width.saturating_sub(1));
    let style = theme::input_field();
    buffer.set_stringn(at.x, at.y, format::fit(&text, width), width, style);
    buffer.set_string(at.x + item.width.saturating_sub(1), at.y, "↓", style);
}

/// A check box, a three-state one or a radio button: its mark and its label cut to `room` columns.
fn draw_toggle(
    buffer: &mut Buffer,
    control: &Control,
    at: Position,
    room: usize,
    styles: (Style, Style),
) -> Option<Position> {
    let (mark, label) = match control {
        Control::Check { label, checked } => (if *checked { "[x] " } else { "[ ] " }, label),
        Control::Tristate { label, state, .. } => {
            let mark = match state {
                Some(true) => "[x] ",
                Some(false) => "[ ] ",
                None => "[?] ",
            };
            (mark, label)
        }
        Control::Radio { label, checked, .. } => (if *checked { "(•) " } else { "( ) " }, label),
        _ => return None,
    };
    Some(draw_marked(
        buffer,
        at,
        mark,
        &vmenu::cut_visible(label, room),
        styles,
    ))
}

/// A check box or a radio button: its mark and its label; the cursor goes inside the mark.
fn draw_marked(
    buffer: &mut Buffer,
    at: Position,
    mark: &str,
    label: &str,
    (normal, hotkey): (Style, Style),
) -> Position {
    buffer.set_string(at.x, at.y, mark, normal);
    vmenu::hi_text(buffer, at.x + 4, at.y, label, normal, hotkey);
    Position::new(at.x + 1, at.y)
}

/// FAR's user separators: a line down joined to the separators at its ends, or a line across from
/// it to the box.
fn draw_rule(buffer: &mut Buffer, control: &Control, at: Position, style: Style) {
    let frame = theme::look().dialog_frame;
    match control {
        Control::VRule(rows) => {
            let (top, line, bottom) = frame.inner_vertical();
            let last = rows.saturating_sub(1);
            for row in 0..*rows {
                let ch = match row {
                    0 => top,
                    row if row == last => bottom,
                    _ => line,
                };
                buffer.set_string(at.x, at.y + row, ch.to_string(), style);
            }
        }
        Control::HRule(columns) => {
            let (left, line, right) = frame.inner_branch();
            let inner = usize::from(columns.saturating_sub(2));
            let text = format!("{left}{}{right}", line.to_string().repeat(inner));
            buffer.set_string(at.x, at.y, text, style);
        }
        _ => {}
    }
}

/// An edit line `width` columns wide showing the end of a long text; returns the cursor position.
pub fn draw_edit(buffer: &mut Buffer, at: Position, width: u16, input: &LineInput) -> Position {
    let columns = usize::from(width);
    let skip = edit_first(input, width);
    let visible: String = input.text().chars().skip(skip).collect();
    buffer.set_stringn(
        at.x,
        at.y,
        format::fit(&visible, columns),
        columns,
        theme::input_field(),
    );
    let row = Rect::new(at.x, at.y, width, 1);
    bars::mark_selection(buffer, row, input, skip, theme::input_field_selected());
    let cursor = u16::try_from(input.cursor() - skip).unwrap_or(0);
    Position::new(at.x + cursor, at.y)
}

/// FAR's `DIF_CENTERGROUP`: the buttons of a row centred together, one column apart (between the
/// margins of [`Place::Between`]; a button [`Place::At`] stands at its own column); FAR's
/// `[ Cancel ]` and `{ Copy }` for the default one, or a theme's buttons with a shadow.
/// Returns where each button that is not disabled is, for the mouse.
pub(super) fn draw_buttons(
    buffer: &mut Buffer,
    form: &Form,
    rect: Rect,
    shadow: bool,
) -> Vec<DialogControl> {
    let buttons: Vec<(usize, &Item)> = form
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            // A button's shadow takes the row under it: that row is on the screen too.
            !item.hidden
                && matches!(item.control, Control::Button { .. })
                && within(item, rect)
                && item.y + u16::from(shadow) < rect.height
        })
        .collect();
    let gap = 1 + u16::from(shadow);
    let mut controls = Vec::new();
    let mut rows: Vec<(u16, Place)> = Vec::new();
    for &(index, item) in &buttons {
        if item.place == Place::At {
            let text = button_text(item, shadow);
            let at = Position::new(rect.x + item.x, rect.y + item.y);
            let end = draw_button(buffer, form, (index, item, &text), at, shadow);
            if !item.disabled {
                controls.push(button_control(index, at, end));
            }
        } else if !rows.contains(&(item.y, item.place)) {
            rows.push((item.y, item.place));
        }
    }
    for (y, place) in rows {
        let row: Vec<(usize, &Item, String)> = buttons
            .iter()
            .filter(|(_, item)| item.y == y && item.place == place)
            .map(|&(index, item)| (index, item, button_text(item, shadow)))
            .collect();
        let total: u16 = row
            .iter()
            .map(|(_, _, text)| visible_width(text) + gap)
            .sum::<u16>()
            .saturating_sub(gap);
        let (left, right) = match place {
            Place::Between(left, right) => (left, right),
            Place::Centre | Place::At => (0, 0),
        };
        let room = rect.width.saturating_sub(left + right);
        let mut x = rect.x + left + room.saturating_sub(total) / 2;
        for (index, item, text) in row {
            let at = Position::new(x, rect.y + y);
            let end = draw_button(buffer, form, (index, item, &text), at, shadow);
            if !item.disabled {
                controls.push(button_control(index, at, end));
            }
            x = end + gap;
        }
    }
    controls
}

fn button_control(index: usize, at: Position, end: u16) -> DialogControl {
    DialogControl {
        cells: Cells {
            x: at.x,
            y: at.y,
            width: end - at.x,
            height: 1,
        },
        index,
        kind: ControlKind::Button,
    }
}

/// One button; returns the column where it ends.
fn draw_button(
    buffer: &mut Buffer,
    form: &Form,
    (index, item, text): (usize, &Item, &str),
    at: Position,
    shadow: bool,
) -> u16 {
    let focused = index == form.focus;
    let (normal, hotkey) = if item.disabled {
        (theme::dialog_disabled(), theme::dialog_disabled())
    } else {
        (
            theme::button(form.warning, focused),
            theme::button_hotkey(form.warning, focused),
        )
    };
    let end = vmenu::hi_text(buffer, at.x, at.y, text, normal, hotkey);
    if shadow {
        let style = theme::button_shadow(form.warning);
        let width = end - at.x;
        buffer.set_string(end, at.y, "▄", style);
        buffer.set_string(at.x + 1, at.y + 1, "▀".repeat(usize::from(width)), style);
    }
    end
}

fn button_text(item: &Item, shadow: bool) -> String {
    let Control::Button { label, default } = &item.control else {
        return String::new();
    };
    match (shadow, default) {
        (true, _) => format!(" {label} "),
        (false, true) => format!("{{ {label} }}"),
        (false, false) => format!("[ {label} ]"),
    }
}

/// Whether `item` starts inside the form's box as the screen lets it be drawn.
fn within(item: &Item, rect: Rect) -> bool {
    item.y < rect.height && item.x < rect.width
}

fn visible_width(text: &str) -> u16 {
    u16::try_from(vmenu::visible_length(text)).unwrap_or(u16::MAX)
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "form_cut_tests.rs"]
mod cut_tests;
