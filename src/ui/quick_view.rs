//! FAR's quick view panel (`qview.cpp`, `QuickView::DisplayObject`): a double box titled "Quick view",
//! a separator above the last row with the file's type in its middle, the item's name on the last row.
//! A folder: `Folder "path"`, the link it is, and FAR's rows of its count from the fifth row, the
//! numbers marked `~` in the highlight colour until the count ends; the disk's rows (cluster, slack,
//! MFT overhead) after a blank row, for a folder on a disk. A file: its contents from the first row to
//! the separator in the viewer's colours (FAR's `Viewer` in the box, 2c step 3b), laid out by the view
//! core as the room the drawing records asks.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType};

use super::info_panel::size_text;
use super::theme;
use crate::app::text_cut::{truncate_left, truncate_path};
use crate::app::{FileText, FolderCounted as Counted, FolderLink, QuickView, Showing, Shown};
use crate::platform::files::LinkKind;
use crate::platform::locale::{NumberFormat, group_digits};

const TITLE: &str = " Quick view ";
/// FAR's `MQuickView*` labels, padded to the longest and a space (`ColumnSize`).
const LABELS: [&str; 7] = [
    "Folders:",
    "Files:",
    "Files size:",
    "Allocated size:",
    "Cluster size:",
    "Files slack:",
    "MFT overhead:",
];

pub fn draw(frame: &mut Frame, area: Rect, view: &QuickView, focused: bool) {
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(theme::panel_border())
        .style(theme::panel());
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);
    if area.width < 6 || area.height < 5 {
        return;
    }
    let buffer = frame.buffer_mut();
    let x = area.x + area.width.saturating_sub(width_of(TITLE)) / 2;
    set_text(buffer, x, area.y, TITLE, theme::panel_title(focused));
    let inner = usize::from(area.width - 2);
    let separator = area.bottom() - 3;
    let line = format!("╟{}╢", "─".repeat(inner));
    set_text(buffer, area.x, separator, &line, theme::panel_border());
    let Some(shown) = &view.shown else {
        return;
    };
    let name = shown
        .path
        .file_name()
        .unwrap_or_else(|| shown.path.to_string());
    let name: String = name.chars().take(inner).collect();
    set_text(buffer, area.x + 1, area.bottom() - 2, &name, theme::panel());
    match &shown.what {
        Showing::File { type_name, text } => {
            if let Some(type_name) = type_name {
                let label = truncate_left(&format!(" {type_name} "), inner);
                let x = area.x + area.width.saturating_sub(width_of(&label)) / 2;
                set_text(buffer, x, separator, &label, theme::selected_info());
            }
            let rows = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 4);
            draw_text(buffer, rows, text);
        }
        Showing::Folder { seen, .. } => {
            let numbers = crate::platform::locale::number_format();
            let lines = folder_lines(shown, seen, numbers, inner);
            draw_lines(buffer, area, &lines);
        }
    }
}

/// What a viewer or an editor shows while it reads its file with nothing to show yet (R7 review F9: a
/// share that stopped answering left it blank).
pub(super) fn reading_notice(reading: std::time::Duration) -> String {
    format!("Reading the file\u{2026} {} s", reading.as_secs())
}

/// A file's rows (FAR's `QView->SetPosition` inside the box above the separator), the viewer's colour
/// filling the room; why it cannot be shown in the panel's.
pub(super) fn draw_text(buffer: &mut Buffer, rows: Rect, text: &FileText) {
    text.room.set((rows.width, rows.height));
    if let Some(error) = &text.failed {
        let line: String = error.chars().take(usize::from(rows.width)).collect();
        set_text(buffer, rows.x, rows.y, &line, theme::panel());
        return;
    }
    let Some(screen) = text.screen(rows.width, rows.height) else {
        if let Some(reading) = text.reading_for() {
            let line: String = reading_notice(reading)
                .chars()
                .take(usize::from(rows.width))
                .collect();
            set_text(buffer, rows.x, rows.y, &line, theme::text_body());
        }
        return;
    };
    let width = usize::from(rows.width);
    let blank = " ".repeat(width);
    for y in 0..rows.height {
        let row = screen.rows.get(usize::from(y)).map_or("", String::as_str);
        // A wide character takes two columns: the row stops at the box's edge.
        buffer.set_stringn(rows.x, rows.y + y, &blank, width, theme::text_body());
        buffer.set_stringn(rows.x, rows.y + y, row, width, theme::text_body());
        // What a search found (FAR's `COL_VIEWERSELECTEDTEXT`).
        if let Some(Some((from, to))) = screen.marks.get(usize::from(y)) {
            let from = u16::try_from(*from).unwrap_or(u16::MAX).min(rows.width);
            let to = u16::try_from(*to).unwrap_or(u16::MAX).min(rows.width);
            let marked = Rect::new(rows.x + from, rows.y + y, to.saturating_sub(from), 1);
            buffer.set_style(marked, theme::text_selected());
        }
    }
}

/// A line of the folder's part: its row from the top of the box, a label and a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Line {
    pub row: u16,
    pub label: String,
    pub value: String,
    /// The count has not ended: FAR's highlight colour.
    pub unfinished: bool,
}

/// FAR's rows of a folder (`QuickView::DisplayObject` with a directory scan).
/// `inner`: the box's inside width, the folder's path cut to fit (FAR's `truncate_path`).
pub(super) fn folder_lines(
    shown: &Shown,
    counted: &Counted,
    numbers: NumberFormat,
    inner: usize,
) -> Vec<Line> {
    let text = |row, label: &str| Line {
        row,
        label: label.to_string(),
        value: String::new(),
        unfinished: false,
    };
    let room = inner.saturating_sub("Folder".len() + 5);
    let path = truncate_path(&shown.path.to_string(), room);
    let mut lines = vec![text(2, &format!("Folder \"{path}\""))];
    if let Some(link) = &counted.link {
        lines.push(text(3, &link_line(link, inner)));
    }
    let info = &counted.info;
    let size = |bytes| size_text(bytes, numbers);
    let mut values = vec![
        (4, info.folders.to_string()),
        (5, info.files.to_string()),
        (6, size(info.bytes)),
        (
            7,
            with_percent(size(info.allocated), info.allocated, info.bytes),
        ),
    ];
    if let Some(cluster) = info.cluster {
        let slack = with_percent(size(info.slack), info.slack, info.allocated);
        let overhead = with_percent(size(info.mft_overhead), info.mft_overhead, info.allocated);
        values.push((9, group_digits(cluster, numbers.thousands_separator)));
        values.push((10, slack));
        values.push((11, overhead));
    }
    let label_of = |row: u16| match row {
        4..=7 => LABELS[usize::from(row - 4)],
        _ => LABELS[usize::from(row - 5)],
    };
    let prefix = if counted.over { "" } else { "~" };
    lines.extend(values.into_iter().map(|(row, value)| Line {
        row,
        label: label_of(row).to_string(),
        value: format!("{prefix}{value}"),
        unfinished: !counted.over,
    }));
    lines
}

/// FAR's second row of a folder that is a link: `Junction "C:\x"`, `Symlink "..."`, `Volume "..."`
/// for a junction to a volume's root, the target cut as FAR cuts it to fit `inner`
/// (`truncate_path`); other kinds by their tag's name.
fn link_line(link: &FolderLink, inner: usize) -> String {
    let target = link.target.as_deref().unwrap_or_default();
    let kind = match link.kind {
        LinkKind::Junction if target.starts_with(r"\\?\Volume{") => "Volume",
        LinkKind::Junction => "Junction",
        LinkKind::Symlink => "Symlink",
        kind => {
            return match kind.name() {
                Some(name) => format!("Reparse point {name}"),
                None => "Unknown reparse point".to_string(),
            };
        }
    };
    let target = truncate_path(target, inner.saturating_sub(kind.len() + 5));
    format!("{kind} \"{target}\"")
}

/// FAR's `{} ({}%)` with `ToPercent`.
fn with_percent(text: String, value: u64, base: u64) -> String {
    format!("{text} ({}%)", to_percent(value, base))
}

/// FAR's `ToPercent`: 0 for nothing, whole percents rounded down.
pub(super) fn to_percent(value: u64, base: u64) -> u128 {
    if value == 0 || base == 0 {
        return 0;
    }
    u128::from(value) * 100 / u128::from(base)
}

/// The folder's lines from two columns in, cut a column before the frame and above the separator
/// (FAR's `PrintText`).
fn draw_lines(buffer: &mut Buffer, area: Rect, lines: &[Line]) {
    let column = LABELS.iter().map(|label| label.len()).max().unwrap_or(0) + 1;
    let room = usize::from(area.width.saturating_sub(4));
    for line in lines {
        let y = area.y + line.row;
        if y > area.bottom().saturating_sub(4) {
            continue;
        }
        let x = area.x + 2;
        if line.value.is_empty() {
            let text: String = line.label.chars().take(room).collect();
            set_text(buffer, x, y, &text, theme::panel());
            continue;
        }
        let label = format!("{:<column$}", line.label);
        let label: String = label.chars().take(room).collect();
        set_text(buffer, x, y, &label, theme::panel());
        let left = room.saturating_sub(label.chars().count());
        let value: String = line.value.chars().take(left).collect();
        let style: Style = if line.unfinished {
            theme::panel_highlight()
        } else {
            theme::panel_info()
        };
        set_text(buffer, x + width_of(&label), y, &value, style);
    }
}

fn width_of(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

fn set_text(buffer: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    let area = buffer.area;
    if y >= area.bottom() || x >= area.right() {
        return;
    }
    buffer.set_stringn(x, y, text, usize::from(area.right() - x), style);
}

#[cfg(test)]
#[path = "quick_view_tests.rs"]
mod tests;
