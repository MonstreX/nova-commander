//! A drop-down field's open list (FAR's `Dialog::CalcComboBoxPos` and its `VMenu`): under the field
//! when the lines fit there or there is more room below than above, else over it; as many lines as
//! `CBoxMaxHeight` lets, scrolled to the cursor, with FAR's scroll bar on the right side of the box
//! when they do not all fit (`MakeScrollBarEx`); a separator's title and the bottom title (Find file's
//! "Space Ins") in the frame; a marked line's `√`. A list in the dialog (`DI_LISTBOX`) is drawn the
//! same way in its item's box, its title in the top frame, `&` shown as it is (FAR's default there).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::symbols;
use ratatui::widgets::{Block, Clear, Widget};

use super::{format, theme, vmenu};
use crate::app::form::{Item, LIST_ROWS, ListBox, ListLine, OpenList};

/// What an open list shows: a drop-down field's lines, or an edit line's history.
pub(super) struct Open<'a> {
    pub lines: &'a [ListLine],
    pub open: &'a OpenList,
    /// FAR's `DM_LISTSETTITLES` in the bottom frame.
    pub bottom_title: Option<&'a str>,
    /// FAR's `VMENU_SHOWAMPERSAND`: `&` is shown, no hotkey.
    pub plain: bool,
}

/// The open list of `item` in the dialog at `rect`. Returns where, for the mouse.
pub(super) fn draw_open_list(
    buffer: &mut Buffer,
    shown: &Open,
    (item, rect, area): (&Item, Rect, Rect),
) -> Option<crate::app::MenuArea> {
    let Open {
        lines,
        open,
        bottom_title,
        plain,
    } = *shown;
    let list = place(
        rect.x + item.x,
        rect.y + item.y,
        item.width,
        lines.len(),
        area,
    );
    if list.width < 3 || list.height < 3 {
        return None;
    }
    let frame = theme::look().menu_frame.set();
    let top = draw_boxed(buffer, (lines, open, plain), list, (false, frame));
    if let Some(title) = bottom_title {
        title_in(
            buffer,
            list,
            list.bottom() - 1,
            title,
            theme::dialog_list(false),
        );
    }
    Some(super::super::menu_area(list, list, top))
}

/// A list in the dialog in its item's box at `list`, its title in the top frame: a double box while it
/// has the focus, a single one without (FAR's `get_box_type` with `VMENU_LISTHASFOCUS`).
pub(super) fn draw_list_box(buffer: &mut Buffer, shown: &ListBox, list: Rect, focused: bool) {
    if list.width < 3 || list.height < 3 {
        return;
    }
    let frame = match theme::look().menu_frame.set() {
        set if !focused && set == symbols::border::DOUBLE => symbols::border::PLAIN,
        set => set,
    };
    let _ = draw_boxed(
        buffer,
        (&shown.lines, &shown.list, true),
        list,
        (true, frame),
    );
    if !shown.title.is_empty() {
        title_in(
            buffer,
            list,
            list.y,
            &shown.title,
            theme::dialog_listbox(false),
        );
    }
}

/// The lines in their box at `list`, scrolled to the cursor, in a list box's colours (`listbox`) or an
/// open list's; the first line shown.
fn draw_boxed(
    buffer: &mut Buffer,
    (lines, open, plain): (&[ListLine], &OpenList, bool),
    list: Rect,
    (listbox, frame_set): (bool, symbols::border::Set),
) -> usize {
    let rows = usize::from(list.height - 2);
    let top = if lines.is_empty() {
        0
    } else {
        crate::app::keep_in_view(&open.top, open.cursor, lines.len(), rows)
    };
    Clear.render(list, buffer);
    let colours = |selected: bool| match listbox {
        true => (
            theme::dialog_listbox(selected),
            theme::dialog_listbox(selected),
        ),
        false => (
            theme::dialog_list(selected),
            theme::dialog_list_hotkey(selected),
        ),
    };
    let frame = colours(false).0;
    let block = Block::bordered().border_set(frame_set).style(frame);
    block.render(list, buffer);
    let inner = usize::from(list.width - 2);
    for (row, (index, line)) in (0..rows).zip(lines.iter().enumerate().skip(top)) {
        let y = list.y + 1 + vmenu::to_u16(row);
        let selected = index == open.cursor;
        let styles = colours(selected);
        match line {
            ListLine::Separator | ListLine::Section(_) => {
                let text = theme::look().menu_frame.separator(usize::from(list.width));
                buffer.set_string(list.x, y, text, frame);
                if let ListLine::Section(title) = line {
                    title_in(buffer, list, y, title, frame);
                }
            }
            ListLine::Choice(label) => {
                draw_line(buffer, (list.x + 1, y), (inner, plain), ' ', label, styles)
            }
            ListLine::Grayed(label) => {
                let gray = theme::dialog_list_gray(selected);
                draw_line(
                    buffer,
                    (list.x + 1, y),
                    (inner, plain),
                    ' ',
                    label,
                    (gray, gray),
                );
            }
            ListLine::Toggle { label, checked } | ListLine::Marked { label, checked } => {
                let mark = if *checked { '√' } else { ' ' };
                draw_line(buffer, (list.x + 1, y), (inner, plain), mark, label, styles);
            }
        }
    }
    draw_scroll_bar(buffer, list, top, lines.len(), frame);
    top
}

/// FAR's `CalcComboBoxPos` for a field at `x`, `y`: the box of `count` lines (up to `CBoxMaxHeight`),
/// 21 columns at least.
fn place(x: u16, y: u16, width: u16, count: usize, area: Rect) -> Rect {
    let width = width.max(21);
    let height = vmenu::to_u16(count.min(LIST_ROWS) + 2);
    let below = area.bottom().saturating_sub(y + 1);
    let above = y.saturating_sub(area.y);
    let (top, height) = if below >= height || below > above {
        (y + 1, height.min(below))
    } else {
        let height = height.min(above);
        (y - height, height)
    };
    Rect::new(x, top, width, height).intersection(area)
}

/// " title " in the middle of row `y` of the frame.
fn title_in(buffer: &mut Buffer, list: Rect, y: u16, title: &str, style: Style) {
    let room = usize::from(list.width).saturating_sub(4);
    let shown: String = title.chars().take(room).collect();
    let length = shown.chars().count();
    let x = list.x + vmenu::to_u16((usize::from(list.width) - 2 - length) / 2);
    buffer.set_string(x, y, format!(" {shown} "), style);
}

fn draw_line(
    buffer: &mut Buffer,
    (x, y): (u16, u16),
    (width, plain): (usize, bool),
    mark: char,
    label: &str,
    (normal, hotkey): (Style, Style),
) {
    buffer.set_string(x, y, " ".repeat(width), normal);
    buffer.set_string(x, y, mark.to_string(), normal);
    let room = width.saturating_sub(1);
    if plain {
        buffer.set_stringn(x + 1, y, format::fit(label, room), room, normal);
        return;
    }
    let shown = vmenu::cut_visible(label, room);
    vmenu::hi_text(buffer, x + 1, y, &shown, normal, hotkey);
}

/// FAR's `ScrollBar` down the right side of the box, when the lines do not all fit.
fn draw_scroll_bar(buffer: &mut Buffer, list: Rect, top: usize, count: usize, style: Style) {
    let rows = usize::from(list.height - 2);
    if rows < 2 || rows >= count {
        return;
    }
    let x = list.right() - 1;
    for (row, ch) in scroll_bar(rows, top, top + rows, count)
        .into_iter()
        .enumerate()
    {
        buffer.set_string(x, list.y + 1 + vmenu::to_u16(row), ch.to_string(), style);
    }
}

/// FAR's `MakeScrollBarEx`: the arrows at the ends, the slider for lines `start..end` of `size`.
pub(in crate::ui) fn scroll_bar(length: usize, start: usize, end: usize, size: usize) -> Vec<char> {
    let mut bar = vec!['░'; length];
    if length < 2 {
        return Vec::new();
    }
    bar[0] = '▲';
    bar[length - 1] = '▼';
    let field = length - 2;
    if field == 0 {
        return bar;
    }
    if field == 1 {
        bar[1] = '█';
        return bar;
    }
    let end = end.min(size);
    let begin = slider_begin(start, size, field);
    let slider = rounded(end - start, size, field).max(1);
    let (mut begin, mut stop) = (begin, if end == size { field } else { begin + slider });
    if stop == field && end < size {
        stop -= 1;
        if begin > 1 {
            begin -= 1;
        }
    }
    for cell in &mut bar[1 + begin..1 + stop.max(begin)] {
        *cell = '█';
    }
    bar
}

fn slider_begin(start: usize, size: usize, field: usize) -> usize {
    let mut begin = rounded(start, size, field).max(usize::from(start > 0));
    if begin == field {
        begin -= 1;
    }
    begin
}

/// FAR's `round(ToPercent(nom, den, field * 10) / 10.0)`.
fn rounded(nom: usize, den: usize, field: usize) -> usize {
    if nom == 0 || den == 0 {
        return 0;
    }
    let tenths = if nom == den {
        field * 10
    } else {
        nom * field * 10 / den
    };
    (tenths + 5) / 10
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(length: usize, start: usize, size: usize) -> String {
        scroll_bar(length, start, start + length, size)
            .into_iter()
            .collect()
    }

    #[test]
    fn the_scroll_bar_is_fars() {
        assert_eq!(bar(8, 0, 16), "▲███░░░▼");
        assert_eq!(bar(8, 8, 16), "▲░░░███▼");
        assert_eq!(bar(8, 4, 16), "▲░░███░▼");
        assert_eq!(
            bar(8, 1, 100),
            "▲░█░░░░▼",
            "off the top: a gap after the arrow"
        );
        assert_eq!(bar(8, 91, 100), "▲░░░░█░▼", "off the end: a gap before it");
        assert_eq!(bar(3, 0, 10), "▲█▼");
        assert!(scroll_bar(1, 0, 1, 10).is_empty());
    }

    #[test]
    fn a_long_list_shows_eight_lines_scrolled_to_the_cursor() {
        use crate::app::form::{Control, Form, OpenList};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut lines = vec![ListLine::Section("Part".to_string())];
        lines.extend((0..20).map(|n| ListLine::Marked {
            label: format!("page {n}"),
            checked: n == 11,
        }));
        let list = Control::DropDown {
            lines,
            chosen: 1,
            open: Some(OpenList::at(13)),
            caption: None,
            bottom_title: Some("Space Ins".to_string()),
        };
        let form = Form::new("List", 40, 6, vec![Item::new(5, 2, list).wide(24)]);
        let mut terminal = Terminal::new(TestBackend::new(60, 24)).unwrap();
        let mut drawn = None;
        terminal
            .draw(|frame| drawn = Some(super::super::draw(frame, frame.area(), &form).1))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (0..24)
            .map(|y| (0..60).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        let shown: Vec<&String> = (rows.iter())
            .filter(|row| row.contains("page ") && !row.contains("↓"))
            .collect();
        assert_eq!(shown.len(), LIST_ROWS, "{rows:#?}");
        assert!(
            shown.iter().any(|row| row.contains("√page 11")),
            "the mark: {rows:#?}"
        );
        assert!(
            shown.iter().any(|row| row.contains("page 12")),
            "the cursor's line"
        );
        assert!(
            !rows.iter().any(|row| row.contains("Part")),
            "scrolled past the section"
        );
        assert!(
            rows.iter().any(|row| row.contains(" Space Ins ")),
            "{rows:#?}"
        );
        let area = drawn
            .and_then(|drawn| drawn.list)
            .expect("the list's place");
        assert_eq!(area.first, 6, "the cursor on the last line shown");
        let x = usize::from(area.frame.x + area.frame.width - 1);
        let bar: String = (rows[usize::from(area.frame.y) + 1..])
            .iter()
            .take(LIST_ROWS)
            .map(|row| row.chars().nth(x).unwrap())
            .collect();
        assert_eq!(bar, "▲░░██░░▼", "{rows:#?}");
    }

    #[test]
    fn the_list_goes_up_when_there_is_more_room_above() {
        let screen = Rect::new(0, 0, 80, 25);
        assert_eq!(place(5, 10, 30, 100, screen), Rect::new(5, 11, 30, 10));
        assert_eq!(place(5, 20, 30, 100, screen), Rect::new(5, 10, 30, 10));
        assert_eq!(
            place(5, 20, 30, 2, screen),
            Rect::new(5, 21, 30, 4),
            "two lines fit below"
        );
        assert_eq!(
            place(5, 1, 30, 100, Rect::new(0, 0, 80, 6)),
            Rect::new(5, 2, 30, 4)
        );
    }
}
