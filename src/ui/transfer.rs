//! FAR's progress dialogs of the background tasks: copy (`far/copy_progress.cpp`: the file and where it
//! goes, its bar, the totals with their bar, the time, the time left and the speed) and delete
//! (`far/delete.cpp` `delete_progress`: the object, the count, the total bar when counted) and attributes
//! (`far/setattr.cpp` `setarttr_progress`, so spelled there: the object changed now); under them
//! the buttons ADR-0010 adds: Background, Pause (Continue), Cancel. And the window of F3's count
//! (`far/stddlg.cpp` `dirinfo_progress`).

use std::time::Duration;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear};

use super::separator;
use super::size_format::group_digits;
use super::theme;
use super::vmenu;
use crate::app::form::{Control, Form, Item};
use crate::app::{
    AttributesProgress, BACKGROUND, CANCEL, DeleteProgress, FolderCount, PAUSE, Task, TaskKind,
    TaskStatus, TransferProgress as Progress,
};
use crate::platform;

/// FAR's `DlgW`; the height follows the rows, with two more for the buttons.
const WIDTH: u16 = 76;
/// The items' width: from column 5 to `DlgW - 6`.
const CANVAS: usize = WIDTH as usize - 10;
/// FAR's `dirinfo_progress` `DlgH`.
const COUNT_HEIGHT: u16 = 9;
/// What `make_progressbar` leaves for the percent after the bar.
const PERCENT_WIDTH: usize = 5;
/// The block going to and fro in a bar whose total is not known yet.
const BUSY_BLOCK: usize = 12;
/// How long the block stands on a column.
const BUSY_STEP: Duration = Duration::from_millis(50);

/// Draws the progress dialog; returns where it and its buttons are, for the mouse.
pub fn draw(frame: &mut Frame, area: Rect, task: &Task, focus: usize) -> crate::app::DialogArea {
    let elapsed = task.elapsed();
    // A paused task's block stands at the start of its bar.
    let moment = if task.status() == TaskStatus::Running {
        elapsed
    } else {
        Duration::ZERO
    };
    let (title, rows) = match task.kind {
        TaskKind::Delete => ("Delete", delete_lines(&task.deletion(), moment)),
        TaskKind::Attributes => ("Attributes", attributes_lines(&task.attributes())),
        TaskKind::Move => (
            "Rename/Move",
            lines(&task.progress(), true, (elapsed, moment)),
        ),
        // FAR shows links being made in its copy progress.
        TaskKind::Copy | TaskKind::Link => {
            ("Copy", lines(&task.progress(), false, (elapsed, moment)))
        }
    };
    draw_dialog(frame, area, title, rows, (task.is_paused(), focus))
}

/// F3's count, FAR's window to the column: what is scanned, a separator, the objects and the bytes.
pub fn draw_folder_count(frame: &mut Frame, area: Rect, count: &FolderCount) {
    let (rect, inside) = open_window(frame, area, "View", COUNT_HEIGHT);
    write_rows(frame.buffer_mut(), rect, inside, folder_count_lines(count));
}

fn folder_count_lines(count: &FolderCount) -> Vec<Row> {
    let separator = platform::locale::number_format().thousands_separator;
    vec![
        Row::Text("Scanning the folder".to_string()),
        // A dialog's text item is cut at its right end.
        Row::Text(count.folder.chars().take(CANVAS).collect()),
        Row::Separator(String::new()),
        Row::Text(plain_counter(
            "Files:",
            &group_digits(count.items, separator),
        )),
        Row::Text(plain_counter(
            "Bytes:",
            &group_digits(count.bytes, separator),
        )),
    ]
}

/// The box of a dialog `height` rows high, centred, with its shadow: the window's rectangle and the
/// frame's, inset as FAR's are.
fn open_window(frame: &mut Frame, area: Rect, title: &str, height: u16) -> (Rect, Rect) {
    let width = WIDTH.min(area.width);
    let height = height.min(area.height);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    let style = theme::dialog(false);
    frame.render_widget(Clear, rect);
    frame.render_widget(Block::new().style(style), rect);
    let inside = Rect::new(
        rect.x + 3,
        rect.y + 1,
        rect.width.saturating_sub(6),
        rect.height.saturating_sub(2),
    );
    let block = Block::bordered()
        .border_set(theme::look().dialog_frame.set())
        .style(style)
        .title(Line::styled(format!(" {title} "), theme::dialog_title()).centered());
    frame.render_widget(block, inside);
    vmenu::draw_shadow(frame.buffer_mut(), rect, area);
    (rect, inside)
}

/// The rows from the window's second one down, as many as its frame holds.
fn write_rows(buffer: &mut Buffer, rect: Rect, inside: Rect, rows: Vec<Row>) {
    let style = theme::dialog(false);
    for (row, line) in (2u16..).zip(rows) {
        if row >= rect.height.saturating_sub(2) {
            break;
        }
        match line {
            Row::Text(text) => buffer.set_string(rect.x + 5, rect.y + row, text, style),
            Row::Separator(title) => {
                separator::draw(
                    buffer,
                    inside,
                    rect.y + row,
                    (false, &title, None),
                    theme::dialog(false),
                );
            }
        }
    }
}

/// The dialog's box, its rows from the second one down, a separator and the buttons.
fn draw_dialog(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    mut lines: Vec<Row>,
    buttons: (bool, usize),
) -> crate::app::DialogArea {
    let shadow_buttons = theme::look().buttons == theme::Buttons::Shadow;
    lines.push(Row::Separator(String::new()));
    let buttons_row = 2 + u16::try_from(lines.len()).unwrap_or(0);
    let full_height = buttons_row + 3;
    let (rect, inside) = open_window(frame, area, title, full_height + u16::from(shadow_buttons));
    let buffer = frame.buffer_mut();
    write_rows(buffer, rect, inside, lines);
    let (paused, focus) = buttons;
    let form = button_form(paused, focus, buttons_row, full_height);
    let controls = super::form::draw_buttons(buffer, &form, rect, shadow_buttons);
    crate::app::DialogArea {
        frame: super::form::cells(rect),
        controls,
    }
}

/// The buttons as a form's centred group, so they look as every dialog's buttons do.
fn button_form(paused: bool, focus: usize, row: u16, height: u16) -> Form {
    let button = |label: &str, default| Control::Button {
        label: label.to_string(),
        default,
    };
    let mut items = vec![Item::new(0, 0, Control::Separator); 3];
    items[BACKGROUND] = Item::new(0, row, button("&Background", true));
    let pause = if paused { "C&ontinue" } else { "&Pause" };
    items[PAUSE] = Item::new(0, row, button(pause, false));
    items[CANCEL] = Item::new(0, row, button("&Cancel", false));
    let mut form = Form::new("", WIDTH, height, items);
    form.focus = focus;
    form
}

enum Row {
    Text(String),
    /// A line across the box with a title in its middle.
    Separator(String),
}

/// The rows from the second one down, as FAR lays them out. `times`: how long the task runs, and the
/// moment of the bar's block.
fn lines(progress: &Progress, move_: bool, times: (Duration, Duration)) -> Vec<Row> {
    let (elapsed, moment) = times;
    let file_percent = match progress.verified {
        Some(compared) => percent(compared, progress.file_size),
        None => percent(progress.file_done, progress.file_size),
    };
    let total_bar = if progress.totals_known {
        let (done, total) = progress.amounts();
        bar(percent(done, total))
    } else {
        busy_bar(moment)
    };
    let files_total = progress.files_total.max(progress.files_done);
    let bytes_total = progress.bytes_total.max(progress.bytes_done);
    vec![
        Row::Text(heading(progress, move_).to_string()),
        Row::Text(fit_path(&progress.source.to_string())),
        Row::Text("to".to_string()),
        Row::Text(fit_path(&progress.target.to_string())),
        Row::Text(bar(file_percent)),
        Row::Separator("Total".to_string()),
        Row::Text(counter(
            "Files:",
            progress.files_done,
            files_total,
            progress.totals_known,
        )),
        Row::Text(counter(
            "Bytes:",
            progress.bytes_done,
            bytes_total,
            progress.totals_known,
        )),
        Row::Text(total_bar),
        Row::Separator(String::new()),
        Row::Text(stats(progress, elapsed)),
    ]
}

fn heading(progress: &Progress, move_: bool) -> &'static str {
    match (progress.verified, move_) {
        (Some(_), _) => "Verifying the file",
        (None, true) => "Moving the file",
        (None, false) => "Copying the file",
    }
}

/// FAR's "Deleting" rows: what is deleted, the count; with the totals counted (`DelShowTotal`) the
/// count is `done / total` and the total bar follows.
fn delete_lines(progress: &DeleteProgress, moment: Duration) -> Vec<Row> {
    let mut rows = vec![
        Row::Text("Deleting the file or folder".to_string()),
        Row::Text(fit_name(&progress.current)),
        Row::Separator(String::new()),
    ];
    if progress.total == 0 {
        let separator = platform::locale::number_format().thousands_separator;
        let value = group_digits(progress.done, separator);
        rows.push(Row::Text(plain_counter("Files:", &value)));
    } else {
        let total = progress.total.max(progress.done);
        rows.push(Row::Text(counter(
            "Files:",
            progress.done,
            total,
            progress.totals_known,
        )));
        rows.push(Row::Text(if progress.totals_known {
            bar(percent(progress.done, total))
        } else {
            busy_bar(moment)
        }));
    }
    rows
}

/// FAR's `setattr_progress`: what is changed now, nothing counted.
fn attributes_lines(progress: &AttributesProgress) -> Vec<Row> {
    vec![
        Row::Text("Setting file attributes for".to_string()),
        Row::Text(fit_name(&progress.current)),
    ]
}

/// FAR's `make_progressbar`: `█` done, `░` to go, ` 35%` at the end.
fn bar(percent: u64) -> String {
    let label = format!(" {percent:3}%");
    let size = CANVAS - PERCENT_WIDTH;
    let done = usize::try_from(percent.min(100)).unwrap_or(0) * size / 100;
    format!("{}{}{label}", "█".repeat(done), "░".repeat(size - done))
}

/// The total bar while the total is still being counted (owner's): against a total that grows a percent
/// would jump back and forth, so a block goes to and fro instead, with no percent after it.
fn busy_bar(moment: Duration) -> String {
    let size = CANVAS - PERCENT_WIDTH;
    let at = busy_position(size - BUSY_BLOCK, moment);
    format!(
        "{}{}{}{}",
        "░".repeat(at),
        "█".repeat(BUSY_BLOCK),
        "░".repeat(size - BUSY_BLOCK - at),
        " ".repeat(PERCENT_WIDTH)
    )
}

/// Where the block stands at `moment`, of `room` + 1 places: to the end and back.
pub(super) fn busy_position(room: usize, moment: Duration) -> usize {
    if room == 0 {
        return 0;
    }
    let steps = usize::try_from(moment.as_millis() / BUSY_STEP.as_millis()).unwrap_or(0);
    let step = steps % (2 * room);
    step.min(2 * room - step)
}

fn percent(done: u64, total: u64) -> u64 {
    if total == 0 {
        return if done == 0 { 0 } else { 100 };
    }
    (u128::from(done.min(total)) * 100 / u128::from(total)) as u64
}

/// FAR's `FormatCounter`: the label, then `done / total` to the right; `…` after a total still being
/// counted (the owner's addition: the count runs beside the operation, never before it).
fn counter(label: &str, done: u64, total: u64, known: bool) -> String {
    let separator = platform::locale::number_format().thousands_separator;
    let more = if known { "" } else { "…" };
    let value = format!(
        "{} / {}{more}",
        group_digits(done, separator),
        group_digits(total, separator)
    );
    let label = format!("{label:7}");
    let width = CANVAS - 5 - label.len();
    format!("{label}{value:>width$}")
}

/// FAR's `FormatCounter` with no total: the label, then the value to the right.
fn plain_counter(label: &str, value: &str) -> String {
    let label = format!("{label:7}");
    let width = CANVAS - 5 - label.len();
    format!("{label}{value:>width$}")
}

/// `Time: 00:00:03   Remaining: 00:00:12   5.2 MB/s`, spread over the line as FAR does.
fn stats(progress: &Progress, elapsed: Duration) -> String {
    let seconds = elapsed.as_secs_f64();
    let time = format!("Time: {}", hms(elapsed.as_secs()));
    let left = if progress.totals_known && progress.bytes_done > 0 && seconds > 0.0 {
        let to_go = progress.bytes_total.saturating_sub(progress.bytes_done) as f64;
        let rest = seconds * to_go / progress.bytes_done as f64;
        format!("Remaining: {}", hms(rest as u64))
    } else {
        "Remaining: --:--:--".to_string()
    };
    let speed = if seconds >= 1.0 {
        speed(progress.bytes_done as f64 / seconds)
    } else {
        String::new()
    };
    let used = time.len() + 1 + left.len() + 1 + speed.len();
    let room = CANVAS.saturating_sub(used);
    let (first, second) = (room / 2, room - room / 2);
    let line = format!(
        "{time}{} {left}{} {speed}",
        " ".repeat(first),
        " ".repeat(second)
    );
    line.chars().take(CANVAS).collect()
}

fn hms(seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

fn speed(bytes_per_second: f64) -> String {
    let units = ["B/s", "KB/s", "MB/s", "GB/s", "TB/s"];
    let mut value = bytes_per_second;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < units.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value:.0} {}", units[unit])
    } else {
        format!("{value:.1} {}", units[unit])
    }
}

/// A long path keeps its root and its end (FAR's `copy_progress::SetNames`: `truncate_path`).
fn fit_path(path: &str) -> String {
    crate::app::text_cut::truncate_path(path, CANVAS)
}

/// The delete and attributes progress's name: as a dialog's text cuts it (FAR's `delete_progress` and
/// `setattr_progress` give it whole to a `DI_TEXT`; the dialog's `fit_to_left` cuts its end).
fn fit_name(name: &str) -> String {
    crate::app::text_cut::truncate_right(name, CANVAS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn bars_counters_and_stats_fill_fars_canvas() {
        assert_eq!(bar(0).chars().count(), CANVAS);
        assert!(bar(50).starts_with(&"█".repeat(30)) && bar(50).ends_with("  50%"));
        assert!(bar(100).ends_with(" 100%") && !bar(100).contains('░'));
        let files = counter("Files:", 3, 10, true);
        assert_eq!(files.chars().count(), CANVAS - 5);
        assert!(files.starts_with("Files: ") && files.ends_with("3 / 10"));
        let counting = counter("Files:", 3, 10, false);
        assert_eq!(counting.chars().count(), CANVAS - 5);
        assert!(counting.ends_with("3 / 10…"), "{counting}");
        assert_eq!(percent(5, 0), 100);
        assert_eq!(percent(1, 3), 33);
        let progress = Progress {
            bytes_done: 2 * 1024 * 1024,
            bytes_total: 4 * 1024 * 1024,
            totals_known: true,
            ..Progress::default()
        };
        let line = stats(&progress, Duration::from_secs(2));
        assert!(line.starts_with("Time: 00:00:02"), "{line}");
        assert!(line.contains("Remaining: 00:00:02"), "{line}");
        assert!(line.ends_with("1.0 MB/s"), "{line}");
        assert_eq!(line.chars().count(), CANVAS);
    }

    fn texts(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Text(text) => text.clone(),
                Row::Separator(_) => "--".to_string(),
            })
            .collect()
    }

    #[test]
    fn while_a_copy_is_read_back_the_dialog_says_so_and_the_bar_shows_that() {
        let progress = Progress {
            file_size: 100,
            file_done: 100,
            verified: Some(25),
            ..Progress::default()
        };
        let rows = texts(&lines(&progress, true, (Duration::ZERO, Duration::ZERO)));
        assert_eq!(rows[0], "Verifying the file");
        assert!(rows[4].ends_with("  25%"), "{}", rows[4]);
    }

    #[test]
    fn a_bar_with_no_total_yet_moves_to_and_fro_in_its_own_room() {
        let at = |steps: u32| busy_position(3, BUSY_STEP * steps);
        let path: Vec<usize> = (0..8).map(at).collect();
        assert_eq!(path, [0, 1, 2, 3, 2, 1, 0, 1]);
        assert_eq!(busy_position(0, BUSY_STEP * 5), 0);

        let size = CANVAS - PERCENT_WIDTH;
        let room = size - BUSY_BLOCK;
        for steps in 0..u32::try_from(2 * room).unwrap() {
            let bar = busy_bar(BUSY_STEP * steps);
            assert_eq!(bar.chars().count(), CANVAS);
            assert_eq!(bar.matches('█').count(), BUSY_BLOCK, "{bar}");
            assert!(bar.ends_with("░     ") || bar.ends_with("█     "), "{bar}");
            assert!(!bar.contains('%'), "{bar}");
        }
        assert_ne!(busy_bar(BUSY_STEP), busy_bar(BUSY_STEP * 2), "it moves");
    }

    #[test]
    fn the_total_bar_moves_while_counted_and_gives_a_percent_after() {
        let total_bar = |known| {
            let progress = Progress {
                bytes_done: 50,
                bytes_total: 100,
                totals_known: known,
                ..Progress::default()
            };
            let moment = BUSY_STEP * 4;
            let rows = texts(&lines(&progress, false, (Duration::ZERO, moment)));
            rows[8].clone()
        };
        assert_eq!(total_bar(false), busy_bar(BUSY_STEP * 4));
        assert!(total_bar(true).ends_with("  50%"), "{}", total_bar(true));
    }

    fn folder_count_screen(count: &FolderCount) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(100, 25)).unwrap();
        terminal
            .draw(|frame| draw_folder_count(frame, frame.area(), count))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..25)
            .map(|y| (0..100).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    /// FAR's `dirinfo_progress`: 76 by 9, the frame from (3, 1) to (72, 7), the items at column 5.
    #[test]
    fn the_folder_counts_window_is_fars_to_the_column() {
        let screen = folder_count_screen(&FolderCount {
            folder: "work".to_string(),
            items: 7,
            bytes: 12,
        });
        let (left, top) = (12, 8);
        let row = |y: usize| -> String { screen[top + y].chars().skip(left).take(76).collect() };
        let double = |line: &str| format!("   ║ {line:<66} ║   ");
        let top_edge: String = row(1).chars().skip(3).take(70).collect();
        assert!(
            top_edge.starts_with("╔═") && top_edge.ends_with("═╗"),
            "{top_edge}"
        );
        assert!(top_edge.contains(" View "), "{top_edge}");
        assert_eq!(top_edge.chars().count(), 70);
        assert_eq!(row(2), double("Scanning the folder"));
        assert_eq!(row(3), double("work"));
        assert_eq!(row(4), format!("   ╟{}╢   ", "─".repeat(68)));
        assert_eq!(row(5), double(&format!("Files: {:>54}", "7")));
        assert_eq!(row(6), double(&format!("Bytes: {:>54}", "12")));
        let bottom: String = row(7).chars().skip(3).take(70).collect();
        assert_eq!(bottom, format!("╚{}╝", "═".repeat(68)));
        assert_eq!(row(8).trim(), "");
    }

    #[test]
    fn a_long_folder_name_is_cut_at_the_right_as_a_dialogs_text() {
        let lines = texts(&folder_count_lines(&FolderCount {
            folder: "x".repeat(CANVAS + 9),
            items: 0,
            bytes: 0,
        }));
        assert_eq!(lines[1], "x".repeat(CANVAS));
    }

    #[cfg(windows)]
    #[test]
    fn a_long_path_keeps_its_root_in_the_copy_and_its_start_in_the_delete_progress() {
        let long = format!(r"C:\{}\end.txt", "x".repeat(CANVAS));
        let progress = Progress {
            source: crate::vfs::VfsPath::local(&long),
            target: crate::vfs::VfsPath::local(r"D:\short"),
            ..Progress::default()
        };
        let rows = texts(&lines(&progress, false, (Duration::ZERO, Duration::ZERO)));
        let tail_room = CANVAS - 4;
        let tail: String = long
            .chars()
            .skip(long.chars().count() - tail_room)
            .collect();
        assert_eq!(rows[1], format!(r"C:\…{tail}"), "the root, then the end");
        assert_eq!(rows[3], r"D:\short");
        let deleting = DeleteProgress {
            current: long.clone(),
            ..DeleteProgress::default()
        };
        let rows = texts(&delete_lines(&deleting, Duration::ZERO));
        let start: String = long.chars().take(CANVAS - 1).collect();
        assert_eq!(rows[1], format!("{start}…"), "the end cut off");
    }

    #[test]
    fn the_deleting_rows_count_and_show_the_total_bar_only_when_counted() {
        let mut progress = DeleteProgress {
            current: r"C:\work\old.txt".to_string(),
            done: 7,
            ..DeleteProgress::default()
        };
        let rows = texts(&delete_lines(&progress, Duration::ZERO));
        assert_eq!(
            rows[..3],
            ["Deleting the file or folder", r"C:\work\old.txt", "--"]
        );
        assert_eq!(rows.len(), 4, "no total bar without the totals");
        assert!(
            rows[3].starts_with("Files: ") && rows[3].ends_with(" 7"),
            "{}",
            rows[3]
        );

        progress.total = 14;
        let rows = texts(&delete_lines(&progress, Duration::ZERO));
        assert!(rows[3].ends_with("7 / 14…"), "still counting: {}", rows[3]);
        assert_eq!(
            rows[4],
            busy_bar(Duration::ZERO),
            "no percent of a growing total"
        );
        progress.totals_known = true;
        let rows = texts(&delete_lines(&progress, Duration::ZERO));
        assert!(rows[3].ends_with("7 / 14"), "{}", rows[3]);
        assert!(rows[4].ends_with("  50%"), "{}", rows[4]);
    }
}
