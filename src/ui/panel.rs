//! A FAR Manager file panel (`FileList::ShowFileList`, `ShowList`, `ShowTotalSize`, `ShowSelectedSize`):
//! double frame with the path, column titles, the sort-mode letter, entries in columns and stripes,
//! the status line of the current entry, totals on the bottom frame and the selection size.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType};

use super::columns::{self, Column};
use super::highlight::{entry_styles, name_marks, status_cell};
use super::panel_cells::{CellLook, cell_in, fit_status_columns};
use super::panel_title::draw_title;
use super::size_format::{SizeStyle, format_size, group_digits};
use super::{format, theme};
use crate::app::ModeSpec;
use crate::app::{HighlightState, Highlighting, ListArea, Panel};
use crate::platform;
use crate::vfs::Entry;

/// Long-name marker drawn over the divider (`MPanelBracketsForLongName`).
const LONG_NAME_MARK: &str = "}";
/// FAR's `{` on the divider before a cut name kept to the right (`R`, `F`).
const LONG_NAME_LEFT_MARK: &str = "{";

use crate::app::NON_LIST_ROWS;

/// How the entries look: the panel has the focus (its cursor is shown), its highlighting when it is on,
/// and the time the groups' dates are counted from.
pub struct Look<'a> {
    pub focused: bool,
    pub highlighting: Option<&'a Highlighting>,
    pub now: std::time::SystemTime,
    /// The panel's view mode.
    pub mode: &'a ModeSpec,
}

/// Geometry of one panel draw.
struct Layout {
    area: Rect,
    columns: Vec<Column>,
    /// Columns per stripe; dividers between stripes are double lines.
    per_stripe: usize,
    /// Rows of entries per column.
    height: u16,
    list_top: u16,
}

impl Layout {
    /// The divider after each column, by its index: none after the last shown one and none by an
    /// empty column (FAR's negative width).
    fn dividers(&self) -> Vec<Option<(u16, bool)>> {
        let last_shown = self.columns.iter().rposition(|column| column.width > 0);
        let mut x = self.area.x + 1;
        let mut result = Vec::new();
        for (index, column) in self.columns.iter().enumerate() {
            if column.width == 0 || Some(index) >= last_shown {
                result.push(None);
                continue;
            }
            x += to_u16(column.width);
            let next = (index + 1..self.columns.len())
                .find(|at| self.columns[*at].width > 0)
                .unwrap_or(index + 1);
            result.push(Some((x, next / self.per_stripe != index / self.per_stripe)));
            x += 1;
        }
        result
    }

    fn column_x(&self, index: usize) -> u16 {
        let before: usize = (self.columns[..index].iter())
            .filter(|column| column.width > 0)
            .map(|column| column.width + 1)
            .sum();
        self.area.x + 1 + to_u16(before)
    }

    fn bottom(&self) -> u16 {
        self.area.y + self.area.height - 1
    }
}

/// Draws `panel` into `area`. `clock_x` is where the clock starts on the top screen row (FAR keeps the
/// title clear of it). Returns where the entries are, for the mouse.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    panel: &mut Panel,
    look: &Look,
    clock_x: u16,
) -> Option<ListArea> {
    let focused = look.focused;
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(theme::panel_border())
        .style(theme::panel());
    // The shell's output lies behind the panels: wipe it where the panel has nothing to show.
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);
    if area.width < 3 || area.height < 3 {
        return None;
    }
    let spec = look.mode;
    let text_width = usize::from(area.width - 2);
    let columns = columns::layout(&spec.panel, text_width);
    let (stripes, per_stripe) = columns::stripes(&columns);
    let layout = Layout {
        area,
        columns,
        per_stripe,
        height: area.height.saturating_sub(NON_LIST_ROWS),
        list_top: area.y + 2,
    };
    panel.set_view_metrics(usize::from(layout.height), stripes);
    let widths: Vec<usize> = layout.columns.iter().map(|column| column.width).collect();
    panel.set_drawn_columns(&spec.types, &widths);

    let buffer = frame.buffer_mut();
    let style = theme::panel_border();
    for (x, double) in layout.dividers().into_iter().flatten() {
        set_text(buffer, x, area.y, if double { "╦" } else { "╤" }, style);
    }
    draw_column_titles(buffer, &layout, spec);
    draw_rows(buffer, &layout, spec, panel, look);
    if layout.height > 0 {
        draw_status(buffer, &layout, spec, panel, look);
    }
    draw_totals(buffer, &layout, panel);
    draw_selected_info(buffer, &layout, panel);
    let mut letter = crate::app::mode_letter(panel.sort.mode, panel.sort.reverse).to_string();
    // FAR's `*` after the sort mode: a filter is on.
    if panel.filter_is_on() {
        letter.push('*');
    }
    if panel.sort.selected_first {
        letter.push('^');
    }
    set_text(
        buffer,
        area.x + 1,
        area.y + 1,
        &letter,
        theme::panel_header(),
    );
    // FAR's "Reading: N files" while a folder is read in the background.
    let title = panel.reading().map_or_else(
        || (panel.look.title.clone()).unwrap_or_else(|| panel.path.to_string()),
        |read| format!("Reading: {read} files"),
    );
    draw_title(buffer, area, &title, focused, clock_x);
    (layout.height > 0).then(|| ListArea {
        top: layout.list_top,
        rows: layout.height,
        stripe_lefts: (0..stripes)
            .map(|stripe| layout.column_x(stripe * layout.per_stripe))
            .collect(),
    })
}

/// The columns' titles: a provider's mode's own, else the kinds'.
fn draw_column_titles(buffer: &mut Buffer, layout: &Layout, spec: &ModeSpec) {
    let y = layout.area.y + 1;
    for (index, column) in layout.columns.iter().enumerate() {
        if column.width == 0 {
            continue;
        }
        // FAR's `ColumnTitles[i]`: any title given replaces the kind's, an empty one too.
        let own = spec.titles.get(index).and_then(Option::as_ref);
        let title = format::center(
            own.map_or(column.kind.title(), String::as_str),
            column.width,
        );
        set_text(
            buffer,
            layout.column_x(index),
            y,
            &title,
            theme::panel_header(),
        );
    }
    for (x, double) in layout.dividers().into_iter().flatten() {
        set_text(
            buffer,
            x,
            y,
            if double { "║" } else { "│" },
            theme::panel_border(),
        );
    }
}

fn draw_rows(buffer: &mut Buffer, layout: &Layout, spec: &ModeSpec, panel: &Panel, look: &Look) {
    if let Some(error) = &panel.error {
        let width = usize::from(layout.area.width - 2);
        let text = format::fit(error, width);
        set_text(
            buffer,
            layout.area.x + 1,
            layout.list_top,
            &text,
            theme::error_text(),
        );
        return;
    }
    let dividers = layout.dividers();
    for row in 0..layout.height {
        // An entry's selection and highlighting are worked out once for all its cells (review R34).
        let mut shown: Option<(usize, EntryLook)> = None;
        for index in 0..layout.columns.len() {
            if layout.columns[index].width == 0 {
                continue;
            }
            let stripe = index / layout.per_stripe;
            let position = panel.offset + stripe * usize::from(layout.height) + usize::from(row);
            let place = CellPlace {
                index,
                y: layout.list_top + row,
                divider: dividers.get(index).copied().flatten(),
            };
            let entry = panel.entries.get(position).map(|entry| {
                if shown.as_ref().is_none_or(|(at, _)| *at != position) {
                    shown = Some((position, EntryLook::of(entry, position, panel, look)));
                }
                (entry, shown.as_ref().map(|(_, entry_look)| entry_look))
            });
            draw_cell(buffer, layout, spec, look, entry, &place);
        }
    }
}

/// What all the cells of one entry are drawn with.
struct EntryLook {
    selected: bool,
    /// The cursor is on it.
    cursor: bool,
    element: Option<crate::app::HighlightElement>,
}

impl EntryLook {
    fn of(entry: &Entry, position: usize, panel: &Panel, look: &Look) -> Self {
        Self {
            selected: panel.selected.contains(&entry.name),
            cursor: position == panel.cursor,
            element: (look.highlighting).and_then(|groups| groups.element(entry, look.now)),
        }
    }
}

struct CellPlace {
    index: usize,
    y: u16,
    /// The divider after the cell: x and whether it ends a stripe (double line).
    divider: Option<(u16, bool)>,
}

/// One cell and the divider after it. Dividers inside a stripe take the row's background (the cursor
/// bar runs through them); a cut name puts FAR's `}` on the divider.
fn draw_cell(
    buffer: &mut Buffer,
    layout: &Layout,
    spec: &ModeSpec,
    look: &Look,
    entry: Option<(&Entry, Option<&EntryLook>)>,
    place: &CellPlace,
) {
    let column = &layout.columns[place.index];
    let mut x = layout.column_x(place.index);
    let stripe_end = place.divider.is_none_or(|(_, double)| double);
    let divider_symbol = if stripe_end { "║" } else { "│" };
    let Some((entry, Some(entry_look))) = entry else {
        if let Some((divider_x, _)) = place.divider {
            set_text(
                buffer,
                divider_x,
                place.y,
                divider_symbol,
                theme::panel_border(),
            );
        }
        return;
    };
    let selected = entry_look.selected;
    let state = HighlightState::of(selected, look.focused && entry_look.cursor);
    let element = &entry_look.element;
    let (style, mark_style) = entry_styles(element.as_ref(), state);
    let mut shown_column = *column;
    // FAR's selection mark, then the highlighting mark in its own colour, then the name.
    let mark = element.as_ref().map_or("", |element| element.mark.as_str());
    if let Some(marks) = name_marks(column, selected, mark) {
        for (text, style) in [(marks.selection, style), (marks.mark, mark_style)] {
            if let Some(text) = text {
                set_text(buffer, x, place.y, text, style);
                x += to_u16(Span::raw(text).width());
            }
        }
        shown_column = marks.rest;
    }
    let mut shown = cell_in(
        entry,
        &shown_column,
        selected,
        &CellLook::of(spec, look.now),
        false,
    );
    // A plugin's mode whose stripes start with its own column (the process list's friendly names): ".."
    // there, as the name column would have it, not an empty row.
    let first_of_stripe = place.index.is_multiple_of(layout.per_stripe);
    if entry.is_parent && first_of_stripe && matches!(column.kind, columns::ColumnKind::Custom(_)) {
        let width = shown_column.width;
        shown.text = format!("{:<width$.width$}", "..");
    }
    set_text(buffer, x, place.y, &shown.text, style);
    let long_name = shown.too_long
        && matches!(
            column.kind,
            columns::ColumnKind::Name | columns::ColumnKind::Extension
        );
    let border = |stripe_edge: bool| {
        if stripe_edge {
            theme::panel_border()
        } else {
            theme::panel_border().bg(style.bg.unwrap_or_default())
        }
    };
    let kept_right = column.flags.right_align || column.flags.right_align_force;
    let long_left = long_name && kept_right && column.kind == columns::ColumnKind::Name;
    if long_left {
        let left_x = layout.column_x(place.index).saturating_sub(1);
        let left_style = border(place.index.is_multiple_of(layout.per_stripe));
        set_text(buffer, left_x, place.y, LONG_NAME_LEFT_MARK, left_style);
    }
    let symbol = match (long_name && !long_left, place.divider) {
        (true, _) => LONG_NAME_MARK,
        (false, Some(_)) => divider_symbol,
        (false, None) => return,
    };
    let symbol_style = border(stripe_end);
    set_text(
        buffer,
        x + to_u16(shown_column.width),
        place.y,
        symbol,
        symbol_style,
    );
}

/// Separator `╟──┴──╢` above the status line and the status line of the current entry.
fn draw_status(buffer: &mut Buffer, layout: &Layout, spec: &ModeSpec, panel: &Panel, look: &Look) {
    let area = layout.area;
    let separator_y = layout.bottom() - 2;
    let style = theme::panel_border();
    let line = format!("╟{}╢", "─".repeat(usize::from(area.width - 2)));
    set_text(buffer, area.x, separator_y, &line, style);
    for (x, double) in layout.dividers().into_iter().flatten() {
        set_text(
            buffer,
            x,
            separator_y,
            if double { "╨" } else { "┴" },
            style,
        );
    }
    let Some(entry) = panel.current() else {
        return;
    };
    let width = usize::from(area.width - 2);
    // A link's target, read for this one entry only (the listing does not read it).
    let mut entry = entry.clone();
    if entry.points_elsewhere()
        && let Some(dir) = panel.path.as_local()
    {
        entry.link_target = platform::files::link_target(&dir.join(&entry.name))
            .ok()
            .map(|target| target.display().to_string());
    }
    let entry = &entry;
    let mut columns = columns::layout(&spec.status, width);
    columns.retain(|column| column.width > 0);
    fit_status_columns(entry, &mut columns);
    let selected = panel.selected.contains(&entry.name);
    // FAR's status line has the highlighting mark too, in the panel's colour.
    let element = (look.highlighting).and_then(|groups| groups.element(entry, look.now));
    let mark = element.as_ref().map_or("", |element| element.mark.as_str());
    let text: Vec<String> = columns
        .iter()
        .map(|column| status_cell(entry, column, selected, mark))
        .collect();
    let text = format::fit(&text.join(" "), width);
    set_text(
        buffer,
        area.x + 1,
        layout.bottom() - 1,
        &text,
        theme::panel(),
    );
}

/// "Bytes: 1.21 M, files: 12, folders: 3" centred on the bottom frame.
fn draw_totals(buffer: &mut Buffer, layout: &Layout, panel: &Panel) {
    let totals = panel.totals();
    let (bytes, files, folders) = (totals.bytes, totals.files, totals.folders);
    let size = format_size(
        bytes,
        10,
        false,
        SizeStyle {
            float: true,
            show_unit: true,
            ..SizeStyle::default()
        },
        platform::locale::number_format(),
    );
    let text = format!("Bytes: {}, files: {files}, folders: {folders}", size.trim());
    draw_centered(buffer, layout, layout.bottom(), &text, theme::total_info());
}

/// The selection size, in bytes with grouped digits, on the status separator (or the bottom frame).
fn draw_selected_info(buffer: &mut Buffer, layout: &Layout, panel: &Panel) {
    if panel.selected.is_empty() {
        return;
    }
    let selected = panel.selected_totals();
    // `…` while a marked folder is still being counted (owner: a total that grows).
    let mark = if selected.counting { "…" } else { "" };
    let (bytes, files, folders) = (
        selected.totals.bytes,
        selected.totals.files,
        selected.totals.folders,
    );
    let numbers = platform::locale::number_format();
    let available = usize::from(layout.area.width).saturating_sub(4);
    let message = |size: String| format!("Bytes: {size}{mark}, files: {files}, folders: {folders}");
    let mut text = message(group_digits(bytes, numbers.thousands_separator));
    if text.chars().count() > available {
        let plain = bytes.to_string();
        let size = if plain.len() <= 6 {
            plain
        } else {
            let style = SizeStyle {
                show_unit: true,
                ..SizeStyle::default()
            };
            format_size(bytes, 6, false, style, numbers)
                .trim()
                .to_string()
        };
        text = message(size).chars().take(available).collect();
    }
    draw_centered(
        buffer,
        layout,
        layout.bottom() - 2,
        &text,
        theme::selected_info(),
    );
}

fn draw_centered(buffer: &mut Buffer, layout: &Layout, y: u16, text: &str, style: Style) {
    let available = usize::from(layout.area.width).saturating_sub(4);
    let text: String = text.chars().take(available).collect();
    let length = text.chars().count();
    let x = layout.area.x + 1 + to_u16((available - length) / 2);
    set_text(buffer, x, y, &format!(" {text} "), style);
}

pub(super) fn set_text(buffer: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    let area = buffer.area;
    if y >= area.bottom() || x >= area.right() {
        return;
    }
    let width = usize::from(area.right() - x);
    buffer.set_stringn(x, y, text, width, style);
}

pub(super) fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// FAR's `ScrollBar` at column `x` from row `y`, `length` rows for items `top..` of `count`: drawn only
/// when they do not all fit (`ScrollBarRequired`).
pub(super) fn draw_bar_at(
    buffer: &mut Buffer,
    (x, y): (u16, u16),
    length: usize,
    (top, count): (usize, usize),
) {
    if length < 2 || count <= length {
        return;
    }
    let style = super::theme::panel_scrollbar();
    for (row, ch) in super::form::scroll_bar(length, top, top + length, count)
        .into_iter()
        .enumerate()
    {
        buffer.set_string(x, y + super::vmenu::to_u16(row), ch.to_string(), style);
    }
}

#[cfg(test)]
#[path = "panel_provider_tests.rs"]
mod provider_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::text_cut::truncate_path;
    use crate::vfs::VfsPath;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::{Duration, Instant};

    #[test]
    fn a_folder_read_in_the_background_shows_far_s_reading_count_in_the_title() {
        let temp = tempfile::tempdir().unwrap();
        let mut panel = Panel::new(VfsPath::local(temp.path()));
        panel.set_read_wait(Duration::ZERO);
        panel
            .change_dir(&crate::vfs::testing::endless(), None)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while panel.reading() != Some(2) && Instant::now() < deadline {
            panel.poll();
            std::thread::yield_now();
        }
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        let modes = crate::app::PanelModes::default();
        terminal
            .draw(|frame| {
                let look = Look {
                    focused: true,
                    highlighting: None,
                    now: std::time::SystemTime::now(),
                    mode: modes.spec(panel.view_mode),
                };
                draw(frame, frame.area(), &mut panel, &look, 40);
            })
            .unwrap();
        let top: String = (0..40)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
            .collect();
        assert!(top.contains("Reading: 2 files"), "{top}");
    }

    #[test]
    fn long_unc_paths_keep_the_share() {
        let path = r"\\server\share\projects\novacmd";
        assert_eq!(truncate_path(path, 23), r"\\server\share\…novacmd");
        assert_eq!(truncate_path(path, 0), "");
    }

    #[test]
    fn long_paths_keep_root_and_end() {
        assert_eq!(truncate_path("C:\\short", 20), "C:\\short");
        assert_eq!(
            truncate_path("C:\\Users\\me\\projects\\novacmd", 16),
            "C:\\…ects\\novacmd"
        );
        assert_eq!(
            truncate_path("C:\\Users\\me\\projects\\novacmd", 16)
                .chars()
                .count(),
            16
        );
    }
}
