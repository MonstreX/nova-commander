//! FAR's vertical menu (`VMenu`, `far/vmenu.cpp`) as used by the F9 pull-downs and pop-up menus:
//! a short double box in Menu colours with an optional title and bottom title in the frame, a
//! check-mark column, `&` hotkeys, accelerator keys aligned after the longest label
//! (`DecorateItemsWithHotkeys`), `╟─╢` separators and a shadow (`DropShadow`).

use std::cell::Cell;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use super::theme;

/// Service columns: left box, check mark, left scroll marker, right scroll marker, right box
/// (`menu_layout::get_service_area_size`).
const SERVICE_COLUMNS: usize = 5;
/// Around a title: box, space, ..., space, box and one column each side (`get_title_service_area_size`).
const TITLE_SERVICE_COLUMNS: usize = 6;

thread_local! {
    /// FAR's `ShowMenuScrollbar` as the frame being drawn has it (`ui::draw` sets it from the settings).
    static SCROLLBAR: Cell<bool> = const { Cell::new(true) };
}

/// Whether menus and lists longer than their box show FAR's scroll bar.
pub fn shows_scrollbars() -> bool {
    SCROLLBAR.with(Cell::get)
}

pub fn show_scrollbars(on: bool) {
    SCROLLBAR.with(|shown| shown.set(on));
}

/// One menu line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// Label with `&` before the hotkey letter.
    pub label: &'a str,
    pub key: &'a str,
    pub check: Option<char>,
    pub separator: bool,
    /// Shown in grey, never chosen (FAR's `MIF_DISABLE`).
    pub disabled: bool,
}

/// A menu to draw: its lines, the highlighted one, the titles in the frame.
pub struct Menu<'a> {
    pub lines: &'a [Line<'a>],
    pub selected: usize,
    pub title: &'a str,
    pub bottom_title: &'a str,
    /// A menu showing part of its lines: the first one shown and how many there are, for its scroll
    /// bar (FAR's `TopPos` and `GetShowItemCount`).
    pub scroll: Option<(usize, usize)>,
}

impl Menu<'_> {
    /// Each line's text with its accelerator key at one column after the longest label.
    pub fn texts(&self) -> Vec<String> {
        let label_width = self
            .lines
            .iter()
            .filter(|line| !line.separator)
            .map(|line| visible_length(line.label))
            .max()
            .unwrap_or(0);
        self.lines
            .iter()
            .map(|line| {
                if line.separator || line.key.is_empty() {
                    return line.label.to_string();
                }
                let padding = label_width + 1 - visible_length(line.label);
                format!("{}{}{}", line.label, " ".repeat(padding), line.key)
            })
            .collect()
    }

    /// FAR's `GetNaturalMenuWidth`: the widest text plus the service columns, or the titles.
    pub fn natural_width(&self) -> usize {
        let text = self
            .texts()
            .iter()
            .map(|text| visible_length(text))
            .max()
            .unwrap_or(0);
        let title = self
            .title
            .chars()
            .count()
            .max(self.bottom_title.chars().count());
        (text + SERVICE_COLUMNS).max(if title == 0 {
            0
        } else {
            title + TITLE_SERVICE_COLUMNS
        })
    }

    pub fn height(&self) -> usize {
        self.lines.len() + 2
    }
}

/// Draws `menu` in `rect` and its shadow (clipped to `area`).
pub fn draw(buffer: &mut Buffer, rect: Rect, menu: &Menu, area: Rect) {
    if rect.width < 3 || rect.height < 3 {
        return;
    }
    draw_menu(buffer, rect, menu);
    draw_shadow(buffer, rect, area);
}

/// FAR's `VMenu2` with its full box: the menu two columns and one row inside a margin of the menu's
/// colour, the shadow around the margin. `outer` is the whole of it.
pub fn draw_in_margin(buffer: &mut Buffer, outer: Rect, menu: &Menu, area: Rect) {
    if outer.width < 7 || outer.height < 5 {
        return;
    }
    ratatui::widgets::Clear.render(outer, buffer);
    buffer.set_style(outer, theme::menu_text());
    let inner = Rect::new(outer.x + 2, outer.y + 1, outer.width - 4, outer.height - 2);
    draw_menu(buffer, inner, menu);
    draw_shadow(buffer, outer, area);
}

fn draw_menu(buffer: &mut Buffer, rect: Rect, menu: &Menu) {
    draw_box(buffer, rect);
    draw_title(buffer, rect, rect.y, menu.title);
    draw_title(buffer, rect, rect.bottom() - 1, menu.bottom_title);
    let texts = menu.texts();
    let rows = usize::from(rect.height - 2);
    for (row, (index, line)) in menu.lines.iter().enumerate().take(rows).enumerate() {
        let y = rect.y + 1 + to_u16(row);
        if line.separator {
            let text = theme::look().menu_frame.separator(usize::from(rect.width));
            set_text(buffer, rect.x, y, &text, theme::menu_frame());
            // A separator with a title (FAR's history dates).
            draw_title(buffer, rect, y, line.label);
        } else {
            draw_item(
                buffer,
                rect,
                y,
                &texts[index],
                line.check,
                (index == menu.selected, line.disabled),
            );
        }
    }
    if let Some((top, count)) = menu.scroll
        && shows_scrollbars()
        && count > rows
        && rows >= 2
    {
        // FAR's `ScrollBar` on the box's right side, in its colour.
        let bar = super::form::scroll_bar(rows, top, top + rows, count);
        for (row, ch) in bar.into_iter().enumerate() {
            let y = rect.y + 1 + to_u16(row);
            set_text(
                buffer,
                rect.right() - 1,
                y,
                &ch.to_string(),
                theme::menu_frame(),
            );
        }
    }
}

fn draw_box(buffer: &mut Buffer, rect: Rect) {
    let lines = theme::look().menu_frame.set();
    let block = ratatui::widgets::Block::bordered()
        .border_set(lines)
        .border_style(theme::menu_frame())
        .style(theme::menu_text());
    ratatui::widgets::Clear.render(rect, buffer);
    block.render(rect, buffer);
}

/// `VMenu::DrawTitles`: " title " centred in the frame row.
fn draw_title(buffer: &mut Buffer, rect: Rect, y: u16, title: &str) {
    if title.is_empty() {
        return;
    }
    let max = usize::from(rect.width).saturating_sub(4);
    let shown: String = title.chars().take(max).collect();
    let length = shown.chars().count();
    let x = rect.x + to_u16((usize::from(rect.width) - 2 - length) / 2);
    set_text(buffer, x, y, &format!(" {shown} "), theme::menu_text());
}

/// One item between the box sides: check mark, scroll-marker column, text with its hotkey letter,
/// scroll-marker column. The selection bar covers all of it.
fn draw_item(
    buffer: &mut Buffer,
    rect: Rect,
    y: u16,
    text: &str,
    check: Option<char>,
    (selected, disabled): (bool, bool),
) {
    let normal = match (selected, disabled) {
        (true, false) => theme::menu_highlight(),
        (false, false) => theme::menu_text(),
        (selected, true) => theme::menu_disabled(selected),
    };
    let inner = usize::from(rect.width - 2);
    set_text(buffer, rect.x + 1, y, &" ".repeat(inner), normal);
    if let Some(check) = check {
        set_text(buffer, rect.x + 1, y, &check.to_string(), normal);
    }
    let shown = cut_visible(text, inner.saturating_sub(3));
    hi_text(
        buffer,
        rect.x + 3,
        y,
        &shown,
        normal,
        if disabled {
            normal
        } else {
            theme::menu_hotkey(selected)
        },
    );
}

/// FAR's `HiText`: the first `&` marks the next character, drawn in `highlight`; `&&` is one `&`.
/// Returns the x after the text.
pub fn hi_text(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    text: &str,
    normal: Style,
    highlight: Style,
) -> u16 {
    let mut x = x;
    for (ch, marked) in shown_chars(text) {
        let style = if marked { highlight } else { normal };
        set_text(buffer, x, y, &ch.to_string(), style);
        x += 1;
    }
    x
}

/// The characters `text` shows, each with whether it is the hotkey (FAR's `HiTextBase`).
fn shown_chars(text: &str) -> Vec<(char, bool)> {
    crate::app::shown_chars(text)
}

/// `&` written so that it shows as itself (FAR's `escape_ampersands`).
pub fn escape_ampersands(text: &str) -> String {
    text.replace('&', "&&")
}

/// The shadow two columns to the right and one row below (`DropShadow`): dark grey on black in FAR.
pub fn draw_shadow(buffer: &mut Buffer, rect: Rect, area: Rect) {
    let shadow = theme::shadow();
    let right = rect.right();
    let bottom = rect.bottom();
    for y in rect.y + 1..=bottom {
        let from = if y == bottom { rect.x + 2 } else { right };
        for x in from..right + 2 {
            if x < area.right()
                && y < area.bottom()
                && let Some(cell) = buffer.cell_mut((x, y))
            {
                cell.set_style(shadow);
            }
        }
    }
}

/// Text without the `&` marker counts this many columns.
pub fn visible_length(text: &str) -> usize {
    shown_chars(text).len()
}

/// Cuts `text` to `width` visible characters, keeping the `&` marker.
pub fn cut_visible(text: &str, width: usize) -> String {
    let mut result = String::new();
    for (ch, marked) in shown_chars(text).into_iter().take(width) {
        // The hotkey `&` written `&&&`, an `&` shown `&&`.
        if marked {
            result.push('&');
        }
        if ch == '&' {
            result.push('&');
        }
        result.push(ch);
    }
    result
}

pub fn set_text(buffer: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    let area = buffer.area;
    if y >= area.bottom() || x >= area.right() {
        return;
    }
    buffer.set_stringn(x, y, text, usize::from(area.right() - x), style);
}

pub fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line<'a>(label: &'a str, key: &'a str) -> Line<'a> {
        Line {
            label,
            key,
            check: None,
            separator: false,
            disabled: false,
        }
    }

    #[test]
    fn titles_widen_the_menu() {
        let lines = [line("&Name", "Ctrl+F3")];
        let menu = Menu {
            lines: &lines,
            selected: 0,
            title: "",
            bottom_title: "",
            scroll: None,
        };
        assert_eq!(menu.natural_width(), "Name Ctrl+F3".len() + SERVICE_COLUMNS);
        let titled = Menu {
            title: "A rather long title here",
            ..menu
        };
        assert_eq!(titled.natural_width(), 24 + TITLE_SERVICE_COLUMNS);
    }

    #[test]
    fn titles_are_centred_in_the_frame() {
        let lines = [line("&One", ""), line("&Two", "")];
        let menu = Menu {
            lines: &lines,
            selected: 1,
            title: "Sort by",
            bottom_title: "+ - *",
            scroll: None,
        };
        let rect = Rect::new(0, 0, 20, 4);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 30, 8));
        let area = buffer.area;
        draw(&mut buffer, rect, &menu, area);
        let row = |y: u16| -> String { (0..20).map(|x| buffer[(x, y)].symbol()).collect() };
        assert_eq!(row(0), "╔════ Sort by ═════╗");
        assert_eq!(row(3), "╚═════ + - * ══════╝");
        assert_eq!(
            buffer[(3, 2)].style().bg,
            Some(ratatui::style::Color::Black),
            "selected row"
        );
    }

    #[test]
    fn a_disabled_line_is_grey_without_its_hotkeys_colour() {
        let grey = Line {
            disabled: true,
            ..line("&Grey", "")
        };
        let lines = [line("&One", ""), grey, grey];
        let menu = Menu {
            lines: &lines,
            selected: 2,
            title: "",
            bottom_title: "",
            scroll: None,
        };
        let mut buffer = Buffer::empty(Rect::new(0, 0, 30, 8));
        let area = buffer.area;
        draw(&mut buffer, Rect::new(0, 0, 20, 5), &menu, area);
        let colours = |x: u16, y: u16| {
            let style = buffer[(x, y)].style();
            (style.fg, style.bg)
        };
        let of = |style: Style| (style.fg, style.bg);
        assert_eq!(buffer[(3, 2)].symbol(), "G");
        assert_eq!(
            colours(3, 1),
            of(theme::menu_hotkey(false)),
            "a line's hotkey"
        );
        assert_eq!(colours(4, 1), of(theme::menu_text()));
        assert_eq!(colours(3, 2), of(theme::menu_disabled(false)), "no hotkey");
        assert_eq!(colours(5, 2), of(theme::menu_disabled(false)));
        assert_eq!(
            colours(3, 3),
            of(theme::menu_disabled(true)),
            "under the cursor"
        );
        assert_eq!(colours(5, 3), of(theme::menu_disabled(true)));
        assert_ne!(
            of(theme::menu_disabled(true)),
            of(theme::menu_disabled(false))
        );
        assert_ne!(
            of(theme::menu_disabled(false)),
            of(theme::menu_hotkey(false))
        );
        assert_ne!(of(theme::menu_disabled(true)), of(theme::menu_highlight()));
    }

    #[test]
    fn a_double_ampersand_is_one_and_marks_nothing() {
        assert_eq!(visible_length("R&&D"), 3);
        assert_eq!(visible_length("&Copy"), 4);
        assert_eq!(cut_visible("R&&D &x", 4), "R&&D ");
        assert_eq!(cut_visible("a&bc", 2), "a&b");
        // FAR's `HiTextBase` (`interf.cpp`'s test "1&&&2"): an odd run marks, `&&&` the hotkey `&`.
        assert_eq!(
            shown_chars("R&&&D"),
            [('R', false), ('&', true), ('D', false)]
        );
        assert_eq!(cut_visible("R&&&D", 2), "R&&&");
        assert_eq!(
            visible_length("1&2&3"),
            3,
            "a lone `&` after the hotkey dropped"
        );
        assert_eq!(escape_ampersands("a&b"), "a&&b");
    }
}
