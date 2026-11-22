//! Centered modal dialogs: messages, confirmations, text input and lists.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear};

use super::theme::Buttons;
use super::{format, theme, vmenu};
use crate::app::{
    Cells, ControlKind, Dialog, DialogArea, DialogControl, FormAction, LineInput, ListAction,
};

const MIN_WIDTH: usize = 40;

/// What a dialog's draw tells: the cursor of an input dialog, and for the mouse where a list is and
/// where the dialog and its parts are.
#[derive(Default)]
pub struct Drawn {
    pub cursor: Option<Position>,
    pub menu: Option<crate::app::MenuArea>,
    pub dialog: Option<DialogArea>,
}

/// Draws the dialog.
pub fn draw(frame: &mut Frame, area: Rect, dialog: &Dialog) -> Drawn {
    // Options > Plugins: a menu as F11's (the owner's).
    if let Dialog::List {
        title,
        items,
        cursor,
        top,
        action: ListAction::Plugins(rows),
    } = dialog
    {
        let menu = super::plugins_list::draw(frame, area, title, (items, rows), (*cursor, top));
        return Drawn {
            menu,
            ..Drawn::default()
        };
    }
    if let Dialog::List {
        title,
        items,
        cursor,
        top,
        action,
    } = dialog
    {
        // The keys in the bottom frame need their room too.
        let keys = match action {
            ListAction::UserShells => Some(crate::app::SHELLS_KEYS),
            _ => None,
        };
        let mut sizing = items.clone();
        if let (Some(keys), Some(first)) = (keys, sizing.first_mut()) {
            let room = keys.chars().count() + 2;
            let length = first.chars().count();
            first.push_str(&" ".repeat(room.saturating_sub(length)));
        }
        let inner = frame_box(frame, area, title, &sizing, 0, false);
        let first = draw_list(frame.buffer_mut(), inner, items, (*cursor, top));
        // FAR's scroll bar on the right side of the box when the items do not all fit.
        let rows = usize::from(inner.height);
        if vmenu::shows_scrollbars() && items.len() > rows && rows >= 2 {
            let bar = super::form::scroll_bar(rows, first, first + rows, items.len());
            let x = inner.x + inner.width + 1;
            for (row, ch) in (0..inner.height).zip(bar) {
                let style = theme::dialog(false);
                frame
                    .buffer_mut()
                    .set_string(x, inner.y + row, ch.to_string(), style);
            }
        }
        if let Some(keys) = keys {
            bottom_title(frame.buffer_mut(), inner, keys);
        }
        // The list's frame is the dialog's: a row above and below the items, two columns aside.
        let frame = Rect::new(
            inner.x.saturating_sub(2),
            inner.y.saturating_sub(1),
            inner.width + 4,
            inner.height + 2,
        );
        return Drawn {
            menu: Some(super::menu_area(frame, frame, first)),
            ..Drawn::default()
        };
    }
    match dialog {
        Dialog::Form {
            form,
            action: FormAction::AttributesMore { main, .. },
        } => {
            // FAR's "More" stands over the Attributes dialog.
            super::form::draw(frame, area, main);
            form_drawn(super::form::draw(frame, area, form))
        }
        Dialog::Form { form, .. } => form_drawn(super::form::draw(frame, area, form)),
        _ => draw_other(frame, area, dialog),
    }
}

fn form_drawn((cursor, drawn): (Option<Position>, super::form::Drawn)) -> Drawn {
    Drawn {
        cursor,
        menu: drawn.list,
        dialog: Some(drawn.dialog),
    }
}

/// The dialog's box around `inner` (two columns and a row more on each side).
fn outside(inner: Rect) -> Cells {
    super::form::cells(Rect::new(
        inner.x.saturating_sub(2),
        inner.y.saturating_sub(1),
        inner.width + 4,
        inner.height + 2,
    ))
}

fn buttons_drawn(inner: Rect, buttons: Vec<Cells>) -> Drawn {
    let controls = buttons
        .into_iter()
        .enumerate()
        .map(|(index, cells)| DialogControl {
            cells,
            index,
            kind: ControlKind::Button,
        })
        .collect();
    Drawn {
        dialog: Some(DialogArea {
            frame: outside(inner),
            controls,
        }),
        ..Drawn::default()
    }
}

fn draw_other(frame: &mut Frame, area: Rect, dialog: &Dialog) -> Drawn {
    match dialog {
        Dialog::Message {
            title,
            lines,
            error,
        } => {
            let inner = frame_box(frame, area, title, lines, button_rows(), *error);
            write_lines(frame.buffer_mut(), inner, lines, theme::dialog(*error));
            let buttons = draw_buttons(frame.buffer_mut(), inner, &["OK"], 0, *error);
            buttons_drawn(inner, buttons)
        }
        Dialog::Counting(count) => {
            super::transfer::draw_folder_count(frame, area, count);
            Drawn::default()
        }
        Dialog::Confirm {
            title,
            lines,
            buttons,
            focused,
            danger,
            ..
        } => {
            let inner = frame_box(frame, area, title, lines, button_rows(), *danger);
            write_lines(frame.buffer_mut(), inner, lines, theme::dialog(*danger));
            let buttons = draw_buttons(frame.buffer_mut(), inner, buttons, *focused, *danger);
            buttons_drawn(inner, buttons)
        }
        Dialog::Input {
            title,
            prompt,
            input,
            ..
        } => {
            let lines = vec![prompt.clone()];
            let inner = frame_box(frame, area, title, &lines, 2, false);
            write_lines(frame.buffer_mut(), inner, &lines, theme::dialog(false));
            let cursor = draw_input(frame.buffer_mut(), inner, input);
            let field = DialogControl {
                cells: super::form::cells(Rect::new(
                    inner.x,
                    (inner.y + inner.height).saturating_sub(1),
                    inner.width,
                    1,
                )),
                index: 0,
                kind: ControlKind::Edit {
                    first: input.shown_from(usize::from(inner.width)),
                },
            };
            Drawn {
                cursor,
                menu: None,
                dialog: Some(DialogArea {
                    frame: outside(inner),
                    controls: vec![field],
                }),
            }
        }
        Dialog::List { .. } | Dialog::Form { .. } => Drawn::default(),
    }
}

/// Clears and frames a centered box big enough for `lines` plus `extra_rows`; returns its inside.
fn frame_box(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    lines: &[String],
    extra_rows: usize,
    danger: bool,
) -> Rect {
    let content_width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let width = (content_width + 6).max(MIN_WIDTH);
    let height = lines.len() + extra_rows + 2;
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(area.width);
    let height = u16::try_from(height).unwrap_or(u16::MAX).min(area.height);
    let rect = super::dialog_place::placed(area, width, height);
    frame.render_widget(Clear, rect);
    let title_style = if danger {
        theme::dialog(true)
    } else {
        theme::dialog_title()
    };
    let block = Block::bordered()
        .border_set(theme::look().dialog_frame.set())
        .style(theme::dialog(danger))
        .title(Line::styled(format!(" {title} "), title_style).centered());
    frame.render_widget(block, rect);
    vmenu::draw_shadow(frame.buffer_mut(), rect, area);
    Rect::new(
        rect.x + 2,
        rect.y + 1,
        rect.width.saturating_sub(4),
        rect.height.saturating_sub(2),
    )
}

fn write_lines(buffer: &mut Buffer, inner: Rect, lines: &[String], style: Style) {
    for (row, line) in (0..inner.height).zip(lines) {
        buffer.set_stringn(
            inner.x,
            inner.y + row,
            line,
            usize::from(inner.width),
            style,
        );
    }
}

/// Rows under the text: a blank one and the buttons, and their shadow when buttons have one.
fn button_rows() -> usize {
    match theme::look().buttons {
        Buttons::Brackets => 2,
        Buttons::Shadow => 3,
    }
}

/// The buttons centred on the last inner row: FAR's `[ OK ]`, or Turbo Vision's ` OK ` with a shadow
/// (`▄` right of it, `▀` under it from its second column) on the row above the last; the letter after
/// an `&` in the hotkey's colour (FAR's `HiText`).
/// Returns where each button is, for the mouse.
fn draw_buttons(
    buffer: &mut Buffer,
    inner: Rect,
    buttons: &[&str],
    focused: usize,
    danger: bool,
) -> Vec<Cells> {
    let shadow = theme::look().buttons == Buttons::Shadow;
    let Some(y) = (inner.y + inner.height).checked_sub(1 + u16::from(shadow)) else {
        return Vec::new();
    };
    let mut placed = Vec::new();
    let labels: Vec<String> = buttons
        .iter()
        .map(|button| {
            if shadow {
                format!(" {button} ")
            } else {
                format!("[ {button} ]")
            }
        })
        .collect();
    let gap = 2 + usize::from(shadow);
    let total: usize = (labels.iter())
        .map(|label| super::vmenu::visible_length(label) + gap)
        .sum();
    let start = usize::from(inner.width).saturating_sub(total - 2) / 2;
    let mut x = inner.x + u16::try_from(start).unwrap_or(0);
    for (index, label) in labels.iter().enumerate() {
        let (normal, hotkey) = (
            theme::button(danger, index == focused),
            theme::button_hotkey(danger, index == focused),
        );
        let width = super::vmenu::hi_text(buffer, x, y, label, normal, hotkey) - x;
        placed.push(Cells {
            x,
            y,
            width,
            height: 1,
        });
        if shadow {
            let style = theme::button_shadow(danger);
            buffer.set_string(x + width, y, "▄", style);
            buffer.set_string(x + 1, y + 1, "▀".repeat(usize::from(width)), style);
        }
        x += width + u16::try_from(gap).unwrap_or(0);
    }
    placed
}

/// Draws the input field on the last inner row; `None` when the console is too small for it.
fn draw_input(buffer: &mut Buffer, inner: Rect, input: &LineInput) -> Option<Position> {
    if inner.is_empty() {
        return None;
    }
    let y = inner.y + inner.height - 1;
    let width = usize::from(inner.width);
    let skip = input.shown_from(width);
    let visible: String = input.text().chars().skip(skip).collect();
    buffer.set_stringn(
        inner.x,
        y,
        format::fit(&visible, width),
        width,
        theme::input_field(),
    );
    let row = Rect::new(inner.x, y, inner.width, 1);
    super::bars::mark_selection(buffer, row, input, skip, theme::input_field_selected());
    let cursor_x = u16::try_from(input.cursor() - skip).unwrap_or(0);
    Some(Position::new(inner.x + cursor_x, y))
}

/// " keys " centred in the frame row under `inner`, as far as it fits.
fn bottom_title(buffer: &mut Buffer, inner: Rect, text: &str) {
    let room = usize::from(inner.width);
    let shown: String = format!(" {text} ").chars().take(room).collect();
    let width = u16::try_from(shown.chars().count()).unwrap_or(inner.width);
    let x = inner.x + (inner.width - width) / 2;
    buffer.set_stringn(x, inner.bottom(), shown, room, theme::dialog_title());
}

/// The items from the first one shown, which follows the cursor (FAR's `TopPos`); returns that one.
fn draw_list(
    buffer: &mut Buffer,
    inner: Rect,
    items: &[String],
    (cursor, top): (usize, &std::cell::Cell<usize>),
) -> usize {
    let width = usize::from(inner.width);
    let first = match items.len() {
        0 => 0,
        count => crate::app::keep_in_view(top, cursor, count, usize::from(inner.height)),
    };
    for (row, (index, item)) in (0..inner.height).zip(items.iter().enumerate().skip(first)) {
        // Dialog.List.Text.Selected: white on black.
        let style = if index == cursor {
            theme::menu_highlight()
        } else {
            theme::dialog(false)
        };
        buffer.set_stringn(
            inner.x,
            inner.y + row,
            format::fit(item, width),
            width,
            style,
        );
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    fn render(dialog: &Dialog) -> (Buffer, Vec<String>) {
        let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, frame.area(), dialog);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let rows = (0..12)
            .map(|y| (0..60).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        (buffer, rows)
    }

    #[test]
    fn buttons_are_drawn_as_the_theme_says() {
        let dialog = Dialog::error("Copy", vec!["Cannot copy".to_string()]);
        let (_, far) = render(&dialog);
        assert!(far.iter().any(|row| row.contains("[ OK ]")), "{far:#?}");

        theme::set_current(theme::resolve("nc5", &|_| None));
        let (buffer, rows) = render(&dialog);
        theme::set_current(theme::Theme::far());
        let y = rows.iter().position(|row| row.contains(" OK ▄")).unwrap();
        let x = rows[y].find(" OK ").unwrap();
        let x = u16::try_from(rows[y][..x].chars().count()).unwrap();
        let y = u16::try_from(y).unwrap();
        assert_eq!(buffer[(x, y)].bg, Color::LightYellow, "the focused button");
        assert_eq!(buffer[(x, y)].fg, Color::Red);
        assert_eq!(
            rows[usize::from(y) + 1].matches('▀').count(),
            4,
            "{rows:#?}"
        );
    }

    #[test]
    fn the_plugins_list_is_a_menu_with_its_states_in_brackets_at_the_right_under_its_keys() {
        use crate::app::{PluginRow, PluginState};
        let row = |name: &str, state| PluginRow {
            name: name.to_string(),
            state,
        };
        let rows = vec![
            row("emenu", PluginState::Loaded),
            row("filecase", PluginState::Disabled),
            row("probe", PluginState::Loaded),
        ];
        let dialog = Dialog::List {
            title: "Plugins".to_string(),
            items: (rows.iter())
                .map(|row| format!("{:<10}   {}", row.name, row.state.shown()))
                .collect(),
            cursor: 2,
            top: std::cell::Cell::default(),
            action: ListAction::Plugins(rows),
        };
        let (buffer, shown) = render(&dialog);
        assert!(
            shown
                .iter()
                .any(|row| row.contains(crate::app::PLUGINS_KEYS)),
            "{shown:#?}"
        );
        let at = |text: &str| {
            let y = shown.iter().position(|row| row.contains(text)).unwrap();
            let byte = shown[y].find(text).unwrap();
            (shown[y][..byte].chars().count(), y)
        };
        let (name, y) = at("emenu");
        let (state, y_state) = at("[  loaded  ]");
        let (disabled, y_disabled) = at("[ disabled ]");
        assert_eq!(y, y_state, "on its line");
        assert_eq!(state, disabled, "one column");
        let cell =
            |x: usize, y: usize| &buffer[(u16::try_from(x).unwrap(), u16::try_from(y).unwrap())];
        assert_eq!(
            cell(name, y).bg,
            theme::menu_text().bg.unwrap(),
            "the menus' colours"
        );
        assert_eq!(cell(state, y).fg, cell(name, y).fg, "loaded: as the text");
        assert_ne!(cell(disabled, y_disabled).fg, cell(name, y).fg, "off: grey");
    }
}
